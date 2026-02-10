//! Flat brute-force scoring of image embeddings against the vocabulary.
//!
//! Computes dot products between a single image embedding and all term embeddings
//! in the label bank, applies SigLIP's sigmoid scoring, and returns filtered tags.

use crate::config::TaggingConfig;
use crate::types::Tag;

use super::label_bank::LabelBank;
use super::vocabulary::Vocabulary;

/// SigLIP learned scaling parameters (derived from combined model logits).
///
/// These amplify tiny cosine differences into meaningful logits.
/// See `docs/completions/phase-4-text-encoder-spike.md` for derivation.
const LOGIT_SCALE: f32 = 117.33;
const LOGIT_BIAS: f32 = -12.93;

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

    /// Convert cosine similarity to confidence via SigLIP's sigmoid scoring.
    ///
    /// `logit = LOGIT_SCALE * cosine + LOGIT_BIAS`, then `sigmoid(logit)`.
    fn cosine_to_confidence(cosine: f32) -> f32 {
        let logit = LOGIT_SCALE * cosine + LOGIT_BIAS;
        1.0 / (1.0 + (-logit).exp())
    }

    /// Score an image embedding against the full vocabulary.
    ///
    /// Returns tags sorted by confidence, filtered by min_confidence, limited to max_tags.
    /// Both image and term embeddings are L2-normalized, so dot product = cosine similarity.
    pub fn score(&self, image_embedding: &[f32]) -> Vec<Tag> {
        let n = self.label_bank.term_count();
        let dim = self.label_bank.embedding_dim();
        let matrix = self.label_bank.matrix();

        let mut scores: Vec<(usize, f32)> = Vec::with_capacity(n);

        for i in 0..n {
            let offset = i * dim;
            let cosine: f32 = (0..dim)
                .map(|j| image_embedding[j] * matrix[offset + j])
                .sum();
            let confidence = Self::cosine_to_confidence(cosine);
            scores.push((i, confidence));
        }

        // Filter by min_confidence
        let terms = self.vocabulary.all_terms();
        let mut tags: Vec<Tag> = scores
            .into_iter()
            .filter(|(_, confidence)| *confidence >= self.config.min_confidence)
            .map(|(idx, confidence)| {
                let term = &terms[idx];
                Tag {
                    name: term.display_name.clone(),
                    confidence,
                    category: term.category.clone(),
                }
            })
            .collect();

        // Sort by confidence descending
        tags.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

        // Limit to max_tags
        tags.truncate(self.config.max_tags);

        tags
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
