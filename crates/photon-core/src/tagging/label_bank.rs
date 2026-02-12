//! Pre-computed term embeddings for fast scoring.
//!
//! The label bank stores a flat N×768 matrix of text embeddings (one per vocabulary term)
//! that can be dot-producted against image embeddings for instant scoring.

use std::path::Path;

use crate::error::PipelineError;

use super::text_encoder::SigLipTextEncoder;
use super::vocabulary::Vocabulary;

/// Pre-computed term embeddings for scoring.
///
/// Stores a single flat matrix (N × 768, row-major) for efficient dot product.
#[derive(Clone)]
pub struct LabelBank {
    /// Flat matrix: N × 768 stored row-major.
    matrix: Vec<f32>,
    embedding_dim: usize,
    term_count: usize,
}

impl LabelBank {
    /// Create an empty label bank (placeholder for RwLock initialization).
    pub fn empty() -> Self {
        Self {
            matrix: vec![],
            embedding_dim: 768,
            term_count: 0,
        }
    }

    /// Create a label bank from a pre-computed matrix (for testing).
    #[cfg(test)]
    pub fn from_raw(matrix: Vec<f32>, embedding_dim: usize, term_count: usize) -> Self {
        assert_eq!(
            matrix.len(),
            embedding_dim * term_count,
            "Matrix size ({}) does not match {} terms × {} dim",
            matrix.len(),
            term_count,
            embedding_dim,
        );
        Self {
            matrix,
            embedding_dim,
            term_count,
        }
    }

    /// Append another label bank's embeddings to this one.
    ///
    /// The caller must ensure vocabulary ordering matches (i.e., the appended
    /// bank's terms come after this bank's terms in the combined vocabulary).
    pub fn append(&mut self, other: &LabelBank) -> Result<(), PipelineError> {
        if self.embedding_dim != other.embedding_dim {
            return Err(PipelineError::Model {
                message: format!(
                    "Cannot append label banks: dimension mismatch ({} vs {})",
                    self.embedding_dim, other.embedding_dim
                ),
            });
        }
        self.matrix.extend_from_slice(&other.matrix);
        self.term_count += other.term_count;
        Ok(())
    }

    /// Encode all vocabulary terms and build the label bank.
    ///
    /// Uses the "a photo of a {term}" prompt template and batches many terms
    /// per ONNX inference call for efficiency.
    pub fn encode_all(
        vocabulary: &Vocabulary,
        text_encoder: &SigLipTextEncoder,
        batch_size: usize,
    ) -> Result<Self, PipelineError> {
        let terms = vocabulary.all_terms();
        let embedding_dim = 768;
        let mut matrix: Vec<f32> = Vec::with_capacity(terms.len() * embedding_dim);

        tracing::info!(
            "Encoding {} vocabulary terms (this may take a few minutes on first run)...",
            terms.len()
        );

        // Collect all prompts — use "a photo of a {term}" for each
        let prompts: Vec<String> = terms
            .iter()
            .map(|t| format!("a photo of a {}", t.display_name))
            .collect();

        // Encode in large batches for efficiency
        for (batch_idx, chunk) in prompts.chunks(batch_size).enumerate() {
            let embeddings = text_encoder
                .encode_batch(&chunk.iter().map(|s| s.to_string()).collect::<Vec<_>>())?;

            for emb in &embeddings {
                matrix.extend_from_slice(emb);
            }

            // Progress logging
            let encoded = (batch_idx + 1) * batch_size;
            if encoded % 5000 < batch_size || encoded >= terms.len() {
                tracing::info!(
                    "  Encoded {}/{} terms",
                    encoded.min(terms.len()),
                    terms.len()
                );
            }
        }

        let term_count = matrix.len() / embedding_dim;
        tracing::info!(
            "Label bank ready: {} terms x {} dims ({:.1} MB)",
            term_count,
            embedding_dim,
            (term_count * embedding_dim * 4) as f64 / 1_000_000.0
        );

        Ok(Self {
            matrix,
            embedding_dim,
            term_count,
        })
    }

    /// Save label bank to disk as raw f32 binary for fast reload.
    ///
    /// Also writes a `.meta` sidecar with vocabulary hash for cache invalidation.
    pub fn save(&self, path: &Path, vocab_hash: &str) -> Result<(), PipelineError> {
        let bytes: Vec<u8> = self.matrix.iter().flat_map(|f| f.to_le_bytes()).collect();
        std::fs::write(path, &bytes).map_err(|e| PipelineError::Model {
            message: format!("Failed to save label bank to {:?}: {}", path, e),
        })?;

        // Write metadata sidecar
        let meta_path = path.with_extension("meta");
        let meta = format!(
            "vocab_hash={}\nterm_count={}\nembedding_dim={}\n",
            vocab_hash, self.term_count, self.embedding_dim
        );
        std::fs::write(&meta_path, meta).map_err(|e| PipelineError::Model {
            message: format!(
                "Failed to save label bank metadata to {:?}: {}",
                meta_path, e
            ),
        })?;

        tracing::info!(
            "Saved label bank to {:?} ({:.1} MB)",
            path,
            bytes.len() as f64 / 1_000_000.0
        );
        Ok(())
    }

    /// Load label bank from a raw f32 binary file.
    pub fn load(path: &Path, term_count: usize) -> Result<Self, PipelineError> {
        let embedding_dim = 768;
        let expected_len = term_count * embedding_dim * 4; // 4 bytes per f32

        let bytes = std::fs::read(path).map_err(|e| PipelineError::Model {
            message: format!("Failed to read label bank from {:?}: {}", path, e),
        })?;

        if bytes.len() != expected_len {
            return Err(PipelineError::Model {
                message: format!(
                    "Label bank size mismatch: expected {} bytes ({} terms), got {} bytes",
                    expected_len,
                    term_count,
                    bytes.len()
                ),
            });
        }

        let matrix: Vec<f32> = bytes
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();

        tracing::info!("Loaded label bank: {} terms from {:?}", term_count, path);

        Ok(Self {
            matrix,
            embedding_dim,
            term_count,
        })
    }

    /// Check if a saved label bank exists at the given path.
    pub fn exists(path: &Path) -> bool {
        path.exists()
    }

    /// Check if a cached label bank's vocabulary hash matches the current vocabulary.
    ///
    /// Returns `true` if the cache is valid (hashes match), `false` otherwise.
    pub fn cache_valid(path: &Path, vocab_hash: &str) -> bool {
        let meta_path = path.with_extension("meta");
        let Ok(content) = std::fs::read_to_string(&meta_path) else {
            return false;
        };
        content
            .lines()
            .any(|line| line == format!("vocab_hash={}", vocab_hash))
    }

    /// Get the flat matrix for batch dot product.
    pub fn matrix(&self) -> &[f32] {
        &self.matrix
    }

    /// Get the embedding dimension (768).
    pub fn embedding_dim(&self) -> usize {
        self.embedding_dim
    }

    /// Get the number of terms in the bank.
    pub fn term_count(&self) -> usize {
        self.term_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_label_bank() {
        let bank = LabelBank::empty();
        assert_eq!(bank.term_count(), 0);
        assert_eq!(bank.embedding_dim(), 768);
        assert!(bank.matrix().is_empty());
    }

    #[test]
    fn test_append_grows_matrix() {
        let dim = 768;
        let mut bank_a = LabelBank {
            matrix: vec![1.0; 3 * dim],
            embedding_dim: dim,
            term_count: 3,
        };
        let bank_b = LabelBank {
            matrix: vec![2.0; 5 * dim],
            embedding_dim: dim,
            term_count: 5,
        };

        bank_a.append(&bank_b).unwrap();
        assert_eq!(bank_a.term_count(), 8);
        assert_eq!(bank_a.matrix().len(), 8 * dim);
    }

    #[test]
    fn test_append_preserves_existing() {
        let dim = 768;
        let original_data: Vec<f32> = (0..3 * dim).map(|i| i as f32).collect();
        let mut bank_a = LabelBank {
            matrix: original_data.clone(),
            embedding_dim: dim,
            term_count: 3,
        };
        let bank_b = LabelBank {
            matrix: vec![99.0; 2 * dim],
            embedding_dim: dim,
            term_count: 2,
        };

        bank_a.append(&bank_b).unwrap();

        // First 3*dim values should be unchanged
        assert_eq!(&bank_a.matrix()[..3 * dim], &original_data[..]);
        // Last 2*dim values should be 99.0
        assert!(bank_a.matrix()[3 * dim..].iter().all(|&v| v == 99.0));
    }

    #[test]
    fn test_append_empty_to_empty() {
        let mut bank_a = LabelBank::empty();
        let bank_b = LabelBank::empty();

        bank_a.append(&bank_b).unwrap();
        assert_eq!(bank_a.term_count(), 0);
        assert!(bank_a.matrix().is_empty());
    }

    #[test]
    fn test_append_to_empty() {
        let dim = 768;
        let mut bank_a = LabelBank::empty();
        let bank_b = LabelBank {
            matrix: vec![1.0; 3 * dim],
            embedding_dim: dim,
            term_count: 3,
        };

        bank_a.append(&bank_b).unwrap();
        assert_eq!(bank_a.term_count(), 3);
        assert_eq!(bank_a.matrix().len(), 3 * dim);
    }

    #[test]
    fn test_append_dimension_mismatch_returns_error() {
        let mut bank_a = LabelBank {
            matrix: vec![1.0; 768],
            embedding_dim: 768,
            term_count: 1,
        };
        let bank_b = LabelBank {
            matrix: vec![1.0; 512],
            embedding_dim: 512,
            term_count: 1,
        };

        let result = bank_a.append(&bank_b);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("dimension mismatch"));
    }
}
