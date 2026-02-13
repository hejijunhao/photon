//! Vectorized scoring of image embeddings against the vocabulary.
//!
//! Uses ndarray matrix-vector operations for efficient dot products between
//! image embeddings and term embeddings. With BLAS (Accelerate on macOS),
//! full-vocabulary scoring becomes a single optimized sgemv call.

use std::path::PathBuf;

use ndarray::{ArrayView1, ArrayView2};

use crate::config::TaggingConfig;
use crate::error::PipelineError;
use crate::types::Tag;

use super::hierarchy::HierarchyDedup;
use super::label_bank::LabelBank;
use super::relevance::RelevanceTracker;
use super::vocabulary::Vocabulary;

/// SigLIP learned scaling parameters (derived from combined model logits).
///
/// These amplify tiny cosine differences into meaningful logits.
/// See `docs/completions/phase-4-text-encoder-spike.md` for derivation.
const LOGIT_SCALE: f32 = 117.33;
const LOGIT_BIAS: f32 = -12.93;

/// Tags with their raw (term_index, confidence) hits for relevance tracking.
pub type ScoringResult = (Vec<Tag>, Vec<(usize, f32)>);

/// Scores images against the full vocabulary via matrix multiplication.
pub struct TagScorer {
    vocabulary: Vocabulary,
    label_bank: LabelBank,
    config: TaggingConfig,
}

impl TagScorer {
    /// Create a new scorer with the given vocabulary, label bank, and config.
    pub fn new(vocabulary: Vocabulary, label_bank: LabelBank, config: TaggingConfig) -> Self {
        Self {
            vocabulary,
            label_bank,
            config,
        }
    }

    /// Get a reference to the label bank.
    pub fn label_bank(&self) -> &LabelBank {
        &self.label_bank
    }

    /// Get a reference to the vocabulary.
    pub fn vocabulary(&self) -> &Vocabulary {
        &self.vocabulary
    }

    /// Convert cosine similarity to confidence via SigLIP's sigmoid scoring.
    ///
    /// `logit = LOGIT_SCALE * cosine + LOGIT_BIAS`, then `sigmoid(logit)`.
    fn cosine_to_confidence(cosine: f32) -> f32 {
        let logit = LOGIT_SCALE * cosine + LOGIT_BIAS;
        1.0 / (1.0 + (-logit).exp())
    }

    /// Convert raw (term_index, confidence) hits into filtered, sorted, truncated tags.
    ///
    /// Shared by `score()` and `score_with_pools()` to avoid logic divergence.
    fn hits_to_tags(&self, hits: &[(usize, f32)]) -> Vec<Tag> {
        let terms = self.vocabulary.all_terms();
        let mut tags: Vec<Tag> = hits
            .iter()
            .filter(|(_, conf)| *conf >= self.config.min_confidence)
            .map(|(idx, confidence)| {
                let term = &terms[*idx];
                Tag {
                    name: term.display_name.clone(),
                    confidence: *confidence,
                    category: term.category.clone(),
                    path: None,
                }
            })
            .collect();

        tags.sort_by(|a, b| b.confidence.total_cmp(&a.confidence));
        tags.truncate(self.config.max_tags);

        // Phase 4e: Hierarchy deduplication
        let mut tags = if self.config.deduplicate_ancestors {
            HierarchyDedup::deduplicate(&tags, &self.vocabulary)
        } else {
            tags
        };

        // Phase 4e: Path annotation
        if self.config.show_paths {
            HierarchyDedup::add_paths(&mut tags, &self.vocabulary, self.config.path_max_depth);
        }

        tags
    }

    /// Validate that an image embedding has the expected dimension.
    fn validate_embedding(&self, image_embedding: &[f32]) -> Result<(), PipelineError> {
        let dim = self.label_bank.embedding_dim();
        if image_embedding.len() != dim {
            return Err(PipelineError::Tagging {
                path: PathBuf::new(),
                message: format!(
                    "Embedding dimension mismatch: got {}, expected {}",
                    image_embedding.len(),
                    dim,
                ),
            });
        }
        Ok(())
    }

    /// Score an image embedding against the full vocabulary.
    ///
    /// Uses a single ndarray matrix-vector multiply (sgemv with BLAS) to compute
    /// all N cosine similarities at once, then applies SigLIP sigmoid scoring.
    /// Both image and term embeddings are L2-normalized, so dot product = cosine.
    pub fn score(&self, image_embedding: &[f32]) -> Result<Vec<Tag>, PipelineError> {
        self.validate_embedding(image_embedding)?;

        let n = self.label_bank.term_count();
        let dim = self.label_bank.embedding_dim();
        let matrix = self.label_bank.matrix();

        // Zero-copy views into existing data — single mat-vec multiply
        let mat = ArrayView2::from_shape((n, dim), matrix).expect("label bank shape mismatch");
        let img = ArrayView1::from(image_embedding);
        let cosines = mat.dot(&img);

        let scores: Vec<(usize, f32)> = cosines
            .iter()
            .enumerate()
            .map(|(i, &cosine)| (i, Self::cosine_to_confidence(cosine)))
            .collect();

        Ok(self.hits_to_tags(&scores))
    }

    /// Score only the terms at the given indices.
    ///
    /// Uses ndarray dot products per row for vectorized computation.
    /// Returns raw `(term_index, confidence)` pairs above `min_confidence`.
    /// Designed for use with `RelevanceTracker::active_indices()` /
    /// `warm_indices()` to avoid scanning all 68K terms.
    pub fn score_indices(&self, image_embedding: &[f32], indices: &[usize]) -> Vec<(usize, f32)> {
        let dim = self.label_bank.embedding_dim();
        let matrix = self.label_bank.matrix();
        let img = ArrayView1::from(image_embedding);

        indices
            .iter()
            .filter_map(|&i| {
                let offset = i * dim;
                let row = ArrayView1::from(&matrix[offset..offset + dim]);
                let cosine = row.dot(&img);
                let confidence = Self::cosine_to_confidence(cosine);
                (confidence >= self.config.min_confidence).then_some((i, confidence))
            })
            .collect()
    }

    /// Pool-aware scoring: active terms every image + warm check every Nth image.
    ///
    /// Uses precomputed pool index lists from `RelevanceTracker` to iterate only
    /// relevant terms (~2K active) instead of scanning all 68K.
    ///
    /// Returns both formatted tags (for output) and raw hits (for recording in
    /// the tracker). This method does NOT mutate the tracker — the caller is
    /// responsible for calling `record_hits()` separately, allowing scoring to
    /// run under a read lock while only the brief recording needs a write lock.
    pub fn score_with_pools(
        &self,
        image_embedding: &[f32],
        tracker: &RelevanceTracker,
    ) -> Result<ScoringResult, PipelineError> {
        self.validate_embedding(image_embedding)?;

        // 1. Score active pool (every image) — uses precomputed index list
        let mut all_hits = self.score_indices(image_embedding, tracker.active_indices());

        // 2. Optionally score warm pool (every Nth image)
        if tracker.should_check_warm() {
            let warm_hits = self.score_indices(image_embedding, tracker.warm_indices());
            all_hits.extend(warm_hits);
        }

        // 3. Convert to tags using shared helper
        let tags = self.hits_to_tags(&all_hits);

        Ok((tags, all_hits))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tagging::relevance::RelevanceConfig;

    #[test]
    fn test_cosine_to_confidence_range() {
        // Very negative cosine -> near 0 confidence
        let conf = TagScorer::cosine_to_confidence(-0.2);
        assert!(conf < 0.01);

        // SigLIP typical matching cosine (~-0.05) should give some confidence
        let conf = TagScorer::cosine_to_confidence(-0.05);
        assert!(conf > 0.0 && conf < 1.0);

        // Positive cosine (unusual for SigLIP) -> high confidence
        let conf = TagScorer::cosine_to_confidence(0.2);
        assert!(conf > 0.99);
    }

    #[test]
    fn test_sigmoid_monotonic() {
        let c1 = TagScorer::cosine_to_confidence(-0.10);
        let c2 = TagScorer::cosine_to_confidence(-0.07);
        let c3 = TagScorer::cosine_to_confidence(-0.05);
        assert!(c1 < c2);
        assert!(c2 < c3);
    }

    /// Helper: create a minimal scorer with synthetic embeddings for testing.
    ///
    /// Returns the TempDir alongside the scorer so it stays alive for the
    /// test's duration and is cleaned up when the test completes.
    fn test_scorer(n_terms: usize, dim: usize) -> (TagScorer, Vec<f32>, tempfile::TempDir) {
        use std::io::Write;

        // Create vocabulary
        let dir = tempfile::tempdir().unwrap();
        let nouns_path = dir.path().join("wordnet_nouns.txt");
        let mut f = std::fs::File::create(&nouns_path).unwrap();
        for i in 0..n_terms {
            writeln!(f, "term_{i}\t0000000{i}\tanimal").unwrap();
        }
        let supp_path = dir.path().join("supplemental.txt");
        std::fs::File::create(&supp_path).unwrap();
        let vocab = Vocabulary::load(dir.path()).unwrap();

        // Create label bank with synthetic embeddings
        // Term 0: strong positive cosine with image
        // Term 1: moderate cosine
        // Others: low cosine
        let mut matrix = vec![0.0f32; n_terms * dim];
        // Term 0: unit vector in dim 0
        matrix[0] = 1.0;
        // Term 1: 0.5 in dim 0
        if n_terms > 1 {
            matrix[dim] = 0.5;
        }

        let label_bank = LabelBank::from_raw(matrix, dim, n_terms);
        let config = TaggingConfig {
            min_confidence: 0.0,
            max_tags: 15,
            ..TaggingConfig::default()
        };

        let scorer = TagScorer::new(vocab, label_bank, config);

        // Image embedding: unit vector in dim 0
        let mut image_emb = vec![0.0f32; dim];
        image_emb[0] = 1.0;

        (scorer, image_emb, dir)
    }

    #[test]
    fn test_hits_to_tags_filters_sorts_truncates() {
        let (scorer, _, _dir) = test_scorer(5, 4);

        let hits = vec![(0, 0.9), (1, 0.3), (2, 0.7), (3, 0.1), (4, 0.5)];
        let tags = scorer.hits_to_tags(&hits);

        // Should be sorted descending by confidence
        assert!(tags[0].confidence >= tags[1].confidence);
        assert!(tags.len() <= 15);
        // First tag should be highest confidence
        assert!((tags[0].confidence - 0.9).abs() < 0.001);
    }

    #[test]
    fn test_score_indices_scores_only_requested() {
        let (scorer, image_emb, _dir) = test_scorer(5, 4);

        // Score only indices 0 and 2 (skip 1, 3, 4)
        let hits = scorer.score_indices(&image_emb, &[0, 2]);

        // Only term 0 and 2 should appear in results
        let hit_indices: Vec<usize> = hits.iter().map(|(i, _)| *i).collect();
        assert!(hit_indices.contains(&0));
        // Term 2 is all zeros, cosine = 0, confidence depends on LOGIT_BIAS
        // With min_confidence 0.0, it should still appear
        assert!(hit_indices.contains(&2));
        assert!(!hit_indices.contains(&1));
        assert!(!hit_indices.contains(&3));
    }

    #[test]
    fn test_score_indices_uses_tracker_active_list() {
        let (scorer, image_emb, _dir) = test_scorer(3, 4);

        let mask = vec![true, true, true]; // All start Active
        let tracker = RelevanceTracker::new(3, &mask, RelevanceConfig::default());

        // Active indices should contain all 3 terms
        assert_eq!(tracker.active_indices().len(), 3);
        assert!(tracker.warm_indices().is_empty());

        let hits = scorer.score_indices(&image_emb, tracker.active_indices());
        assert_eq!(hits.len(), 3); // All active, all scored
    }

    #[test]
    fn test_score_indices_empty_returns_empty() {
        let (scorer, image_emb, _dir) = test_scorer(3, 4);
        let hits = scorer.score_indices(&image_emb, &[]);
        assert!(hits.is_empty());
    }

    #[test]
    fn test_score_with_pools_returns_hits() {
        let (scorer, image_emb, _dir) = test_scorer(3, 4);

        let mask = vec![true, true, true];
        let tracker = RelevanceTracker::new(3, &mask, RelevanceConfig::default());

        let (tags, raw_hits) = scorer.score_with_pools(&image_emb, &tracker).unwrap();

        // Should have tags from the active pool
        assert!(!tags.is_empty());
        // raw_hits should contain all terms above min_confidence
        assert_eq!(raw_hits.len(), 3);
    }

    #[test]
    fn test_score_dimension_mismatch() {
        let (scorer, _, _dir) = test_scorer(3, 4);

        // Embedding with wrong dimension (2 instead of 4)
        let wrong_emb = vec![1.0, 0.0];
        let result = scorer.score(&wrong_emb);
        assert!(result.is_err());
        match result.unwrap_err() {
            PipelineError::Tagging { message, .. } => {
                assert!(message.contains("dimension mismatch"));
                assert!(message.contains("got 2"));
                assert!(message.contains("expected 4"));
            }
            other => panic!("Expected Tagging error, got: {other:?}"),
        }
    }

    #[test]
    fn test_score_with_pools_dimension_mismatch() {
        let (scorer, _, _dir) = test_scorer(3, 4);
        let mask = vec![true, true, true];
        let tracker = RelevanceTracker::new(3, &mask, RelevanceConfig::default());

        let wrong_emb = vec![1.0]; // 1 instead of 4
        let result = scorer.score_with_pools(&wrong_emb, &tracker);
        assert!(result.is_err());
        match result.unwrap_err() {
            PipelineError::Tagging { message, .. } => {
                assert!(message.contains("dimension mismatch"));
            }
            other => panic!("Expected Tagging error, got: {other:?}"),
        }
    }
}
