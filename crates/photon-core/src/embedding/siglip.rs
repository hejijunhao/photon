//! SigLIP ONNX model session management and inference.
//!
//! Loads a SigLIP visual encoder exported to ONNX format and runs inference
//! to produce 768-dimensional image embedding vectors.

use std::path::Path;
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
}
