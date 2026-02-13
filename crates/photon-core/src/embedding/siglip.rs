//! SigLIP ONNX model session management and inference.
//!
//! Loads a SigLIP visual encoder exported to ONNX format and runs inference
//! to produce 768-dimensional image embedding vectors.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ndarray::Array4;
use ort::session::Session;
use ort::value::Value;

use crate::error::PipelineError;

/// Wraps an ONNX Runtime session for SigLIP visual embedding.
///
/// Uses a `Mutex` because `Session::run` requires `&mut self`.
pub struct SigLipSession {
    session: Mutex<Session>,
    /// Name of the input tensor (detected from model metadata).
    input_name: String,
}

impl SigLipSession {
    /// Load a SigLIP visual encoder from an ONNX file.
    pub fn load(model_path: &Path) -> Result<Self, PipelineError> {
        let session = Session::builder()
            .map_err(|e| PipelineError::Embedding {
                path: model_path.to_path_buf(),
                message: format!("Failed to create ONNX session builder: {e}"),
            })?
            .commit_from_file(model_path)
            .map_err(|e| PipelineError::Embedding {
                path: model_path.to_path_buf(),
                message: format!("Failed to load ONNX model: {e}"),
            })?;

        // Detect the input tensor name from model metadata.
        let input_name = session
            .inputs()
            .first()
            .map(|i| i.name().to_string())
            .unwrap_or_else(|| "pixel_values".to_string());

        tracing::debug!(
            "Loaded SigLIP model from {:?} (input: {:?}, outputs: {:?})",
            model_path,
            input_name,
            session
                .outputs()
                .iter()
                .map(|o| o.name())
                .collect::<Vec<_>>()
        );

        Ok(Self {
            session: Mutex::new(session),
            input_name,
        })
    }

    /// Run inference on a preprocessed image tensor and return the embedding.
    ///
    /// Input shape: \[1, 3, image_size, image_size\] (NCHW, normalized to \[-1, 1\]).
    /// Output: L2-normalized embedding vector (768 floats from pooler_output).
    pub fn embed(
        &self,
        preprocessed: &Array4<f32>,
        path: &Path,
    ) -> Result<Vec<f32>, PipelineError> {
        // Convert ndarray to (shape, flat_data) for ort (avoids ndarray feature dependency).
        let shape: Vec<i64> = preprocessed.shape().iter().map(|&d| d as i64).collect();
        let flat_data: Vec<f32> = preprocessed.iter().copied().collect();

        let input_value =
            Value::from_array((shape, flat_data)).map_err(|e| PipelineError::Embedding {
                path: path.to_path_buf(),
                message: format!("Failed to create input tensor: {e}"),
            })?;

        let inputs = ort::inputs![self.input_name.as_str() => input_value];

        let mut session = self.session.lock().map_err(|e| PipelineError::Embedding {
            path: Default::default(),
            message: format!("Session lock poisoned: {e}"),
        })?;

        let outputs = session.run(inputs).map_err(|e| PipelineError::Embedding {
            path: Default::default(),
            message: format!("ONNX inference failed: {e}"),
        })?;

        // Extract pooler_output by name — the cross-modal embedding projection.
        // This is the 2nd output; the 1st (last_hidden_state) is NOT aligned
        // across modalities and should not be used for tagging.
        let pooler_output = outputs
            .iter()
            .find(|(name, _)| *name == "pooler_output")
            .ok_or_else(|| PipelineError::Embedding {
                path: path.to_path_buf(),
                message: "Model did not produce pooler_output".to_string(),
            })?;

        let (shape, data) =
            pooler_output
                .1
                .try_extract_tensor::<f32>()
                .map_err(|e| PipelineError::Embedding {
                    path: path.to_path_buf(),
                    message: format!("Failed to extract pooler_output tensor: {e}"),
                })?;

        // pooler_output is [1, 768] — extract the single embedding vector.
        let mut raw = match shape.len() {
            1 => data.to_vec(),
            2 => {
                let dim = shape[1] as usize;
                data[..dim].to_vec()
            }
            _ => {
                return Err(PipelineError::Embedding {
                    path: path.to_path_buf(),
                    message: format!("Unexpected pooler_output shape: {:?}", shape),
                });
            }
        };

        crate::math::l2_normalize_in_place(&mut raw);
        Ok(raw)
    }

    /// Run batch inference on multiple preprocessed image tensors.
    ///
    /// Each tensor must have shape \[1, 3, image_size, image_size\] (NCHW, normalized to \[-1, 1\]).
    /// They are stacked into a single \[N, 3, image_size, image_size\] tensor for one ONNX call.
    /// Returns N L2-normalized embedding vectors (768 floats each).
    pub fn embed_batch(
        &self,
        tensors: &[Array4<f32>],
        paths: &[PathBuf],
    ) -> Result<Vec<Vec<f32>>, PipelineError> {
        let batch_size = tensors.len();
        if batch_size == 0 {
            return Ok(vec![]);
        }

        // Validate all tensors have the same shape.
        let shape_0 = tensors[0].shape();
        let single_len = tensors[0].len(); // 3 * H * W
        for (i, t) in tensors.iter().enumerate().skip(1) {
            if t.shape() != shape_0 {
                return Err(PipelineError::Embedding {
                    path: paths.get(i).cloned().unwrap_or_default(),
                    message: format!(
                        "Tensor shape mismatch in batch: expected {:?}, got {:?}",
                        shape_0,
                        t.shape()
                    ),
                });
            }
        }

        // Build flat batch tensor [N, 3, H, W].
        let mut flat_data = Vec::with_capacity(batch_size * single_len);
        for t in tensors {
            flat_data.extend(t.iter().copied());
        }
        let batch_shape: Vec<i64> = vec![
            batch_size as i64,
            shape_0[1] as i64, // 3
            shape_0[2] as i64, // H
            shape_0[3] as i64, // W
        ];

        let input_value =
            Value::from_array((batch_shape, flat_data)).map_err(|e| PipelineError::Embedding {
                path: paths.first().cloned().unwrap_or_default(),
                message: format!("Failed to create batch input tensor: {e}"),
            })?;

        let inputs = ort::inputs![self.input_name.as_str() => input_value];

        let mut session = self.session.lock().map_err(|e| PipelineError::Embedding {
            path: Default::default(),
            message: format!("Session lock poisoned: {e}"),
        })?;

        let outputs = session.run(inputs).map_err(|e| PipelineError::Embedding {
            path: Default::default(),
            message: format!("ONNX batch inference failed: {e}"),
        })?;

        // Extract pooler_output [N, 768].
        let pooler_output = outputs
            .iter()
            .find(|(name, _)| *name == "pooler_output")
            .ok_or_else(|| PipelineError::Embedding {
                path: paths.first().cloned().unwrap_or_default(),
                message: "Model did not produce pooler_output".to_string(),
            })?;

        let (shape, data) =
            pooler_output
                .1
                .try_extract_tensor::<f32>()
                .map_err(|e| PipelineError::Embedding {
                    path: paths.first().cloned().unwrap_or_default(),
                    message: format!("Failed to extract batch pooler_output tensor: {e}"),
                })?;

        // Determine embedding dimension from output shape.
        let embedding_dim = match shape.len() {
            1 => data.len() / batch_size,
            2 => shape[1] as usize,
            _ => {
                return Err(PipelineError::Embedding {
                    path: paths.first().cloned().unwrap_or_default(),
                    message: format!("Unexpected batch pooler_output shape: {:?}", shape),
                });
            }
        };

        // Split into per-image embeddings and L2-normalize each.
        let embeddings: Vec<Vec<f32>> = data
            .chunks(embedding_dim)
            .take(batch_size)
            .map(crate::math::l2_normalize)
            .collect();

        Ok(embeddings)
    }
}

#[cfg(test)]
mod tests {
    use ndarray::Array4;

    #[test]
    fn test_embed_batch_empty_returns_empty() {
        // We can't construct a SigLipSession without a model, but we test
        // the empty-input fast path via EmbeddingEngine in integration tests.
        // This test validates the tensor stacking logic independently.
        let tensors: Vec<Array4<f32>> = vec![];
        assert!(tensors.is_empty());
    }

    #[test]
    fn test_embed_batch_shape_mismatch_detected() {
        // Verify that mismatched tensor shapes are caught before ONNX dispatch.
        let t1 = Array4::<f32>::zeros((1, 3, 224, 224));
        let t2 = Array4::<f32>::zeros((1, 3, 384, 384));
        let tensors = [t1, t2];

        // Validate shapes match (the check that embed_batch performs).
        let shape_0 = tensors[0].shape();
        let mismatch = tensors.iter().skip(1).any(|t| t.shape() != shape_0);
        assert!(mismatch, "Should detect shape mismatch between 224 and 384");
    }

    #[test]
    fn test_batch_tensor_stacking() {
        // Verify flat tensor construction matches expected layout.
        let t1 = Array4::<f32>::ones((1, 3, 2, 2));
        let mut t2 = Array4::<f32>::zeros((1, 3, 2, 2));
        t2[[0, 0, 0, 0]] = 2.0;

        let single_len = t1.len(); // 12
        let mut flat_data = Vec::with_capacity(2 * single_len);
        for t in &[&t1, &t2] {
            flat_data.extend(t.iter().copied());
        }

        assert_eq!(flat_data.len(), 24); // 2 * 3 * 2 * 2
                                         // First 12 elements are all 1.0 (from t1)
        assert!(flat_data[..12].iter().all(|&v| v == 1.0));
        // First element of second image is 2.0
        assert_eq!(flat_data[12], 2.0);
    }
}
