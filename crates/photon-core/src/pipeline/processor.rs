//! Pipeline orchestration - wires together all processing stages.

use std::path::Path;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use crate::config::Config;
use crate::embedding::EmbeddingEngine;
use crate::error::{PipelineError, Result};
use crate::tagging::label_bank::LabelBank;
use crate::tagging::neighbors::NeighborExpander;
use crate::tagging::progressive::ProgressiveEncoder;
use crate::tagging::relevance::{Pool, RelevanceTracker};
use crate::tagging::seed::SeedSelector;
use crate::tagging::text_encoder::SigLipTextEncoder;
use crate::tagging::{TagScorer, Vocabulary};
use crate::types::ProcessedImage;

use super::decode::{format_to_string, ImageDecoder};
use super::discovery::{DiscoveredFile, FileDiscovery};
use super::hash::Hasher;
use super::metadata::MetadataExtractor;
use super::thumbnail::ThumbnailGenerator;
use super::validate::Validator;

/// Options for controlling image processing behavior.
#[derive(Debug, Clone, Default)]
pub struct ProcessOptions {
    /// Skip thumbnail generation
    pub skip_thumbnail: bool,
    /// Skip perceptual hash generation
    pub skip_perceptual_hash: bool,
    /// Skip embedding generation
    pub skip_embedding: bool,
    /// Skip zero-shot tagging
    pub skip_tagging: bool,
}

/// The main image processor that orchestrates the full pipeline.
pub struct ImageProcessor {
    decoder: ImageDecoder,
    thumbnail_gen: ThumbnailGenerator,
    validator: Validator,
    discovery: FileDiscovery,
    embedding_engine: Option<Arc<EmbeddingEngine>>,
    tag_scorer: Option<Arc<RwLock<TagScorer>>>,
    relevance_tracker: Option<RwLock<RelevanceTracker>>,
    embed_timeout_ms: u64,
    /// Sweep interval: run pool transitions every N images.
    sweep_interval: u64,
    /// Whether neighbor expansion is enabled (from config).
    neighbor_expansion: bool,
}

impl ImageProcessor {
    /// Create a new image processor with the given configuration.
    pub fn new(config: &Config) -> Self {
        Self {
            decoder: ImageDecoder::new(config.limits.clone()),
            thumbnail_gen: ThumbnailGenerator::new(config.thumbnail.clone()),
            validator: Validator::new(config.limits.clone()),
            discovery: FileDiscovery::new(config.processing.clone()),
            embedding_engine: None,
            tag_scorer: None,
            relevance_tracker: None,
            embed_timeout_ms: config.limits.embed_timeout_ms,
            sweep_interval: 1000,
            neighbor_expansion: config.tagging.relevance.neighbor_expansion,
        }
    }

    /// Load the embedding model. Call this before processing if you want
    /// embedding vectors in the output.
    ///
    /// If the model files are not available, returns an error. You can check
    /// availability first with [`EmbeddingEngine::model_exists`].
    pub fn load_embedding(&mut self, config: &Config) -> Result<()> {
        let engine = EmbeddingEngine::load(&config.embedding, &config.model_dir())?;
        self.embedding_engine = Some(Arc::new(engine));
        Ok(())
    }

    /// Check whether the embedding engine is loaded.
    pub fn has_embedding(&self) -> bool {
        self.embedding_engine.is_some()
    }

    /// Load the tagging system (vocabulary + label bank + scorer).
    ///
    /// On first run with progressive encoding enabled (default), this encodes
    /// a seed set of ~2K high-value terms synchronously (~30s), then spawns a
    /// background task to encode the remaining ~66K terms in chunks — swapping
    /// in progressively larger scorers as they become available.
    ///
    /// On subsequent runs, loads the cached label bank instantly.
    ///
    /// If relevance pruning is enabled, also creates or loads the relevance
    /// tracker for the three-pool system.
    ///
    /// Follows the same opt-in pattern as `load_embedding()`.
    pub fn load_tagging(&mut self, config: &Config) -> Result<()> {
        let vocab_dir = config.vocabulary_dir();
        let taxonomy_dir = config.taxonomy_dir();
        let model_dir = config.model_dir();

        // Load vocabulary
        let vocabulary = Vocabulary::load(&vocab_dir)?;

        if vocabulary.is_empty() {
            tracing::warn!(
                "Vocabulary is empty at {:?}. Run `photon models download` to install vocabulary files.",
                vocab_dir
            );
            return Ok(());
        }

        // Load or build label bank
        let label_bank_path = taxonomy_dir.join("label_bank.bin");
        let vocab_hash = vocabulary.content_hash();

        if LabelBank::exists(&label_bank_path)
            && LabelBank::cache_valid(&label_bank_path, &vocab_hash)
        {
            // FAST PATH: Load cached label bank (subsequent runs)
            let label_bank = LabelBank::load(&label_bank_path, vocabulary.len())?;

            // Load or create relevance tracker
            if config.tagging.relevance.enabled {
                self.load_relevance_tracker(config, &vocabulary)?;
            }

            let scorer = TagScorer::new(vocabulary, label_bank, config.tagging.clone());
            self.tag_scorer = Some(Arc::new(RwLock::new(scorer)));
        } else if config.tagging.progressive.enabled {
            // PROGRESSIVE PATH: Encode seed, background-encode rest
            if LabelBank::exists(&label_bank_path) {
                tracing::info!("Vocabulary changed — rebuilding label bank cache...");
            }

            if !SigLipTextEncoder::model_exists(&model_dir) {
                tracing::warn!(
                    "Text encoder not found. Run `photon models download` to enable tagging."
                );
                return Ok(());
            }

            // Guard: progressive encoding requires an active tokio runtime
            if tokio::runtime::Handle::try_current().is_err() {
                tracing::warn!("No tokio runtime — falling back to blocking encode for tagging");
                return self.load_tagging_blocking(
                    config,
                    vocabulary,
                    &label_bank_path,
                    &vocab_hash,
                );
            }

            let text_encoder = Arc::new(SigLipTextEncoder::new(&model_dir)?);

            let seed_path = vocab_dir.join("seed_terms.txt");
            let seed_indices = SeedSelector::select(
                &vocabulary,
                &seed_path,
                config.tagging.progressive.seed_size,
            );

            // Initialize the scorer slot with an empty scorer, then swap in the seed
            let scorer_slot = Arc::new(RwLock::new(TagScorer::new(
                Vocabulary::empty(),
                LabelBank::empty(),
                config.tagging.clone(),
            )));
            self.tag_scorer = Some(Arc::clone(&scorer_slot));

            std::fs::create_dir_all(&taxonomy_dir).map_err(|e| PipelineError::Model {
                message: format!("Failed to create taxonomy dir {:?}: {}", taxonomy_dir, e),
            })?;

            let seed_scorer = ProgressiveEncoder::start(
                vocabulary,
                text_encoder,
                config.tagging.clone(),
                Arc::clone(&scorer_slot),
                seed_indices,
                label_bank_path,
                vocab_hash,
                config.tagging.progressive.chunk_size,
            )?;

            // Install the seed scorer
            {
                let mut lock = scorer_slot
                    .write()
                    .expect("TagScorer lock poisoned during seed installation");
                *lock = seed_scorer;
            }

            // Note: relevance tracker not loaded for progressive path —
            // pool assignments only make sense once the full label bank is cached.
        } else {
            // BLOCKING PATH (legacy): Encode all terms synchronously
            return self.load_tagging_blocking(config, vocabulary, &label_bank_path, &vocab_hash);
        }

        Ok(())
    }

    /// Blocking fallback for load_tagging — encodes all terms synchronously.
    fn load_tagging_blocking(
        &mut self,
        config: &Config,
        vocabulary: Vocabulary,
        label_bank_path: &Path,
        vocab_hash: &str,
    ) -> Result<()> {
        let model_dir = config.model_dir();
        let taxonomy_dir = config.taxonomy_dir();

        if !SigLipTextEncoder::model_exists(&model_dir) {
            tracing::warn!(
                "Text encoder not found. Run `photon models download` to enable tagging."
            );
            return Ok(());
        }

        let text_encoder = SigLipTextEncoder::new(&model_dir)?;
        let bank = LabelBank::encode_all(&vocabulary, &text_encoder, 64)?;
        std::fs::create_dir_all(&taxonomy_dir).map_err(|e| PipelineError::Model {
            message: format!("Failed to create taxonomy dir {:?}: {}", taxonomy_dir, e),
        })?;
        bank.save(label_bank_path, vocab_hash)?;

        // Load or create relevance tracker
        if config.tagging.relevance.enabled {
            self.load_relevance_tracker(config, &vocabulary)?;
        }

        self.tag_scorer = Some(Arc::new(RwLock::new(TagScorer::new(
            vocabulary,
            bank,
            config.tagging.clone(),
        ))));
        Ok(())
    }

    /// Load or create the relevance tracker.
    fn load_relevance_tracker(&mut self, config: &Config, vocabulary: &Vocabulary) -> Result<()> {
        let taxonomy_dir = config.taxonomy_dir();
        let relevance_path = taxonomy_dir.join("relevance.json");

        let tracker = if relevance_path.exists() {
            match RelevanceTracker::load(
                &relevance_path,
                vocabulary,
                config.tagging.relevance.clone(),
            ) {
                Ok(tracker) => {
                    let (active, warm, cold) = tracker.pool_counts();
                    tracing::info!(
                        "Loaded relevance data: {} active, {} warm, {} cold",
                        active,
                        warm,
                        cold
                    );
                    tracker
                }
                Err(e) => {
                    tracing::warn!("Failed to load relevance data: {e} — starting fresh");
                    let encoded_mask = vec![true; vocabulary.len()];
                    RelevanceTracker::new(
                        vocabulary.len(),
                        &encoded_mask,
                        config.tagging.relevance.clone(),
                    )
                }
            }
        } else {
            tracing::info!(
                "Relevance pruning enabled — initializing with {} active terms",
                vocabulary.len()
            );
            let encoded_mask = vec![true; vocabulary.len()];
            RelevanceTracker::new(
                vocabulary.len(),
                &encoded_mask,
                config.tagging.relevance.clone(),
            )
        };

        self.relevance_tracker = Some(RwLock::new(tracker));
        Ok(())
    }

    /// Check whether the tagging system is loaded.
    pub fn has_tagging(&self) -> bool {
        self.tag_scorer.is_some()
    }

    /// Save relevance tracking data to disk.
    ///
    /// Call this at the end of a batch processing run.
    pub fn save_relevance(&self, config: &Config) -> Result<()> {
        if let (Some(scorer_lock), Some(tracker_lock)) = (&self.tag_scorer, &self.relevance_tracker)
        {
            let scorer = scorer_lock.read().expect("TagScorer lock poisoned");
            let tracker = tracker_lock.read().expect("RelevanceTracker lock poisoned");
            let taxonomy_dir = config.taxonomy_dir();
            std::fs::create_dir_all(&taxonomy_dir).map_err(|e| PipelineError::Model {
                message: format!("Failed to create taxonomy dir {:?}: {}", taxonomy_dir, e),
            })?;
            let path = taxonomy_dir.join("relevance.json");
            tracker.save(&path, scorer.vocabulary())?;
            let (active, warm, cold) = tracker.pool_counts();
            tracing::info!(
                "Saved relevance data: {} active, {} warm, {} cold ({} images processed)",
                active,
                warm,
                cold,
                tracker.images_processed()
            );
        }
        Ok(())
    }

    /// Process a single image through the full pipeline.
    ///
    /// Returns a `ProcessedImage` with all available data.
    /// The `embedding` field will be empty if the embedding model is not loaded
    /// or if `skip_embedding` is set.
    pub async fn process(&self, path: &Path) -> Result<ProcessedImage> {
        self.process_with_options(path, &ProcessOptions::default())
            .await
    }

    /// Process a single image with custom options.
    pub async fn process_with_options(
        &self,
        path: &Path,
        options: &ProcessOptions,
    ) -> Result<ProcessedImage> {
        let start = std::time::Instant::now();
        tracing::debug!("Processing: {:?}", path);

        // Validate
        self.validator.validate(path)?;
        let validate_time = start.elapsed();
        tracing::trace!("  Validate: {:?}", validate_time);

        // Decode
        let decode_start = std::time::Instant::now();
        let decoded = self.decoder.decode(path).await?;
        let decode_time = decode_start.elapsed();
        tracing::trace!("  Decode: {:?}", decode_time);

        // Extract metadata (non-blocking, sync operation)
        let metadata_start = std::time::Instant::now();
        let exif = MetadataExtractor::extract(path);
        let metadata_time = metadata_start.elapsed();
        tracing::trace!("  Metadata: {:?}", metadata_time);

        // Generate content hash
        let hash_start = std::time::Instant::now();
        let content_hash = Hasher::content_hash(path).map_err(|e| PipelineError::Decode {
            path: path.to_path_buf(),
            message: format!("Hash error: {}", e),
        })?;
        let hash_time = hash_start.elapsed();
        tracing::trace!("  Content hash: {:?}", hash_time);

        // Generate perceptual hash
        let phash_start = std::time::Instant::now();
        let perceptual_hash = if options.skip_perceptual_hash {
            None
        } else {
            Some(Hasher::perceptual_hash(&decoded.image))
        };
        let phash_time = phash_start.elapsed();
        tracing::trace!("  Perceptual hash: {:?}", phash_time);

        // Generate thumbnail
        let thumb_start = std::time::Instant::now();
        let thumbnail = if options.skip_thumbnail {
            None
        } else {
            self.thumbnail_gen.generate(&decoded.image)
        };
        let thumb_time = thumb_start.elapsed();
        tracing::trace!("  Thumbnail: {:?}", thumb_time);

        // Generate embedding (Phase 3)
        let embed_start = std::time::Instant::now();
        let embedding = if options.skip_embedding {
            vec![]
        } else if let Some(engine) = &self.embedding_engine {
            let engine = Arc::clone(engine);
            let image_clone = decoded.image.clone();
            let timeout_duration = Duration::from_millis(self.embed_timeout_ms);
            let embed_path = path.to_path_buf();

            let result = tokio::time::timeout(timeout_duration, async {
                tokio::task::spawn_blocking(move || engine.embed(&image_clone)).await
            })
            .await;

            match result {
                Ok(Ok(Ok(emb))) => emb,
                Ok(Ok(Err(e))) => return Err(e.into()),
                Ok(Err(e)) => {
                    return Err(PipelineError::Embedding {
                        path: embed_path,
                        message: format!("Embedding task panicked: {e}"),
                    }
                    .into())
                }
                Err(_) => {
                    return Err(PipelineError::Timeout {
                        path: embed_path,
                        stage: "embed".to_string(),
                        timeout_ms: self.embed_timeout_ms,
                    }
                    .into())
                }
            }
        } else {
            vec![]
        };
        let embed_time = embed_start.elapsed();
        tracing::trace!("  Embed: {:?}", embed_time);

        // Generate tags using embedding (Phase 4)
        let tag_start = std::time::Instant::now();
        let tags = if !options.skip_tagging {
            match (&self.tag_scorer, &self.relevance_tracker, &embedding) {
                // Pool-aware scoring (relevance pruning enabled)
                (Some(scorer_lock), Some(tracker_lock), emb) if !emb.is_empty() => {
                    // Phase 1: Score under READ lock (concurrent, ~2ms)
                    let (tags, raw_hits) = {
                        let scorer = scorer_lock
                            .read()
                            .expect("TagScorer lock poisoned during scoring");
                        let tracker = tracker_lock
                            .read()
                            .expect("RelevanceTracker lock poisoned during scoring");
                        scorer.score_with_pools(emb, &tracker)
                    };
                    // Read locks dropped here

                    // Phase 2: Record hits + periodic sweep under WRITE lock (brief, ~μs)
                    {
                        let mut tracker = tracker_lock
                            .write()
                            .expect("RelevanceTracker lock poisoned during hit recording");
                        tracker.record_hits(&raw_hits);

                        // Periodic sweep + neighbor expansion
                        if tracker.images_processed() % self.sweep_interval == 0
                            && tracker.images_processed() > 0
                        {
                            let promoted = tracker.sweep();
                            if !promoted.is_empty() && self.neighbor_expansion {
                                let scorer = scorer_lock
                                    .read()
                                    .expect("TagScorer lock poisoned during neighbor expansion");
                                let siblings =
                                    NeighborExpander::expand_all(scorer.vocabulary(), &promoted);
                                let cold_siblings: Vec<usize> = siblings
                                    .iter()
                                    .filter(|&&i| tracker.pool(i) == Pool::Cold)
                                    .copied()
                                    .collect();
                                if !cold_siblings.is_empty() {
                                    tracker.promote_to_warm(&cold_siblings);
                                    tracing::debug!(
                                        "Neighbor expansion: {} promoted, {} siblings queued",
                                        promoted.len(),
                                        cold_siblings.len()
                                    );
                                }
                            }
                            let (active, warm, cold) = tracker.pool_counts();
                            tracing::debug!(
                                "Pool sweep: {} active, {} warm, {} cold",
                                active,
                                warm,
                                cold
                            );
                        }
                    }
                    // Write lock dropped here

                    tags
                }
                // No relevance tracking — score all terms (4a behavior)
                (Some(scorer_lock), None, emb) if !emb.is_empty() => {
                    let scorer = scorer_lock
                        .read()
                        .expect("TagScorer lock poisoned during scoring");
                    scorer.score(emb)
                }
                _ => vec![],
            }
        } else {
            vec![]
        };
        let tag_time = tag_start.elapsed();
        tracing::trace!("  Tags: {:?} ({} tags)", tag_time, tags.len());

        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        let total_time = start.elapsed();
        tracing::debug!(
            "Processed {:?} in {:?} ({}x{}, embedding: {})",
            file_name,
            total_time,
            decoded.width,
            decoded.height,
            if embedding.is_empty() {
                "skipped".to_string()
            } else {
                format!("{}d", embedding.len())
            }
        );

        Ok(ProcessedImage {
            file_path: path.to_path_buf(),
            file_name,
            content_hash,
            width: decoded.width,
            height: decoded.height,
            format: format_to_string(decoded.format),
            file_size: decoded.file_size,
            embedding,
            exif,
            tags,
            description: None, // Placeholder - Phase 5
            thumbnail,
            perceptual_hash,
        })
    }

    /// Discover all image files at a path.
    pub fn discover(&self, path: &Path) -> Vec<DiscoveredFile> {
        self.discovery.discover(path)
    }

    /// Check if thumbnail generation is enabled.
    pub fn thumbnails_enabled(&self) -> bool {
        self.thumbnail_gen.is_enabled()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_options_default() {
        let options = ProcessOptions::default();
        assert!(!options.skip_thumbnail);
        assert!(!options.skip_perceptual_hash);
        assert!(!options.skip_embedding);
    }
}
