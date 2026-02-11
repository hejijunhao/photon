//! Progressive vocabulary encoding for fast first-run startup.
//!
//! Encodes a seed set of high-value terms synchronously (~30s), starts image
//! processing immediately, then encodes remaining terms in background chunks —
//! swapping in progressively larger scorers as they become available.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use crate::config::TaggingConfig;
use crate::error::PipelineError;

use super::label_bank::LabelBank;
use super::scorer::TagScorer;
use super::text_encoder::SigLipTextEncoder;
use super::vocabulary::Vocabulary;

/// All state needed for progressive encoding, bundled to avoid too-many-arguments.
struct ProgressiveContext {
    full_vocabulary: Vocabulary,
    text_encoder: Arc<SigLipTextEncoder>,
    config: TaggingConfig,
    scorer_slot: Arc<RwLock<TagScorer>>,
    seed_indices: Vec<usize>,
    cache_path: PathBuf,
    vocab_hash: String,
    chunk_size: usize,
}

/// Orchestrates background vocabulary encoding with progressive scorer updates.
pub struct ProgressiveEncoder;

impl ProgressiveEncoder {
    /// Encode seed terms synchronously and return an initial TagScorer.
    ///
    /// Also spawns a background tokio task that encodes remaining terms
    /// and progressively swaps in larger scorers via the provided `RwLock`.
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        full_vocabulary: Vocabulary,
        text_encoder: Arc<SigLipTextEncoder>,
        config: TaggingConfig,
        scorer_slot: Arc<RwLock<TagScorer>>,
        seed_indices: Vec<usize>,
        cache_path: PathBuf,
        vocab_hash: String,
        chunk_size: usize,
    ) -> Result<TagScorer, PipelineError> {
        // 1. Create seed vocabulary + label bank (SYNCHRONOUS)
        let seed_vocab = full_vocabulary.subset(&seed_indices);
        let seed_bank = LabelBank::encode_all(&seed_vocab, &text_encoder, 64)?;

        tracing::info!(
            "Seed vocabulary ready: {} terms encoded",
            seed_indices.len(),
        );

        let seed_scorer = TagScorer::new(seed_vocab, seed_bank, config.clone());

        // 2. Determine remaining terms to encode
        let seed_set: HashSet<usize> = seed_indices.iter().copied().collect();
        let mut remaining: Vec<usize> = (0..full_vocabulary.len())
            .filter(|i| !seed_set.contains(i))
            .collect();
        remaining.sort();

        if remaining.is_empty() {
            // All terms were in the seed — save cache and return
            if let Err(e) = seed_scorer.label_bank().save(&cache_path, &vocab_hash) {
                tracing::error!("Failed to save label bank cache: {e}");
            }
            return Ok(seed_scorer);
        }

        // 3. Spawn background encoding task
        let total_terms = full_vocabulary.len();
        let ctx = ProgressiveContext {
            full_vocabulary,
            text_encoder,
            config,
            scorer_slot: Arc::clone(&scorer_slot),
            seed_indices,
            cache_path,
            vocab_hash,
            chunk_size,
        };

        tokio::spawn(async move {
            Self::background_encode(ctx, remaining, total_terms).await;
        });

        Ok(seed_scorer)
    }

    async fn background_encode(
        ctx: ProgressiveContext,
        remaining_indices: Vec<usize>,
        total_terms: usize,
    ) {
        // Accumulate all encoded indices + the running label bank matrix.
        // We APPEND new embeddings rather than re-encoding everything —
        // this keeps total encoding work at O(N) not O(N * num_swaps).
        let mut encoded_indices = ctx.seed_indices;
        let mut running_bank = {
            let scorer = ctx.scorer_slot.read().unwrap();
            scorer.label_bank().clone()
        };

        for chunk in remaining_indices.chunks(ctx.chunk_size) {
            // Encode ONLY this chunk's terms in a blocking task
            let chunk_indices: Vec<usize> = chunk.to_vec();
            let chunk_vocab = ctx.full_vocabulary.subset(&chunk_indices);
            let encoder = Arc::clone(&ctx.text_encoder);

            let chunk_bank = tokio::task::spawn_blocking(move || {
                LabelBank::encode_all(&chunk_vocab, &encoder, 64)
            })
            .await;

            let chunk_bank = match chunk_bank {
                Ok(Ok(bank)) => bank,
                Ok(Err(e)) => {
                    tracing::error!("Background encoding chunk failed: {e}");
                    continue; // Skip this chunk, try next
                }
                Err(e) => {
                    tracing::error!("Background encoding task panicked: {e}");
                    continue;
                }
            };

            // Append the new chunk's embeddings to the running bank
            running_bank.append(&chunk_bank);
            encoded_indices.extend_from_slice(&chunk_indices);

            // Build a new scorer from the accumulated data.
            // Note: subset() preserves the order given in encoded_indices,
            // which matches the running_bank's row order.
            let combined_vocab = ctx.full_vocabulary.subset(&encoded_indices);
            let new_scorer = TagScorer::new(
                combined_vocab,
                running_bank.clone(),
                ctx.config.clone(),
            );

            // Atomic swap — write lock held only for the duration of a field swap
            {
                let mut lock = ctx.scorer_slot.write().unwrap();
                *lock = new_scorer;
            }

            tracing::info!(
                "Progressive encoding: {}/{} terms encoded",
                encoded_indices.len(),
                total_terms,
            );
        }

        // All terms encoded — save complete cache to disk.
        // First ensure the parent directory exists.
        if let Some(parent) = ctx.cache_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                tracing::error!("Failed to create taxonomy dir {:?}: {e}", parent);
                return;
            }
        }

        if let Err(e) = running_bank.save(&ctx.cache_path, &ctx.vocab_hash) {
            tracing::error!("Failed to save complete label bank cache: {e}");
        } else {
            tracing::info!(
                "Progressive encoding complete. Full vocabulary ({} terms) cached to {:?}",
                total_terms,
                ctx.cache_path,
            );
        }
    }
}
