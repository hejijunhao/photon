//! SigLIP text encoder for generating text embeddings.
//!
//! Loads the SigLIP text ONNX model and tokenizer, encodes text strings
//! to 768-dimensional vectors aligned with the vision encoder's space.

use std::path::Path;
use std::sync::Mutex;

use ort::session::Session;
use ort::value::Value;

use crate::error::PipelineError;

/// SigLIP text encoder wrapper.
///
/// Uses the same `Mutex<Session>` pattern as the vision encoder.
pub struct SigLipTextEncoder {
    session: Mutex<Session>,
    tokenizer: tokenizers::Tokenizer,
    embedding_dim: usize,
}

impl SigLipTextEncoder {
    /// Load the text encoder from the model directory.
    ///
    /// Expects `text_model.onnx` and `tokenizer.json` in `model_dir`.
    pub fn new(model_dir: &Path) -> Result<Self, PipelineError> {
        let text_model_path = model_dir.join("text_model.onnx");
        let tokenizer_path = model_dir.join("tokenizer.json");

        if !text_model_path.exists() {
            return Err(PipelineError::Model {
                message: format!(
                    "Text encoder not found at {:?}. Run `photon models download` first.",
                    text_model_path
                ),
            });
        }

        if !tokenizer_path.exists() {
            return Err(PipelineError::Model {
                message: format!(
                    "Tokenizer not found at {:?}. Run `photon models download` first.",
                    tokenizer_path
                ),
            });
        }

        let session = Session::builder()
            .map_err(|e| PipelineError::Model {
                message: format!("Failed to create ONNX session builder: {e}"),
            })?
            .commit_from_file(&text_model_path)
            .map_err(|e| PipelineError::Model {
                message: format!("Failed to load text encoder model: {e}"),
            })?;

        let tokenizer = tokenizers::Tokenizer::from_file(&tokenizer_path).map_err(|e| {
            PipelineError::Model {
                message: format!("Failed to load tokenizer: {e}"),
            }
        })?;

        tracing::debug!(
            "Loaded SigLIP text encoder (inputs: {:?}, outputs: {:?})",
            session
                .inputs()
                .iter()
                .map(|i| i.name())
                .collect::<Vec<_>>(),
            session
                .outputs()
                .iter()
                .map(|o| o.name())
                .collect::<Vec<_>>()
        );

        Ok(Self {
            session: Mutex::new(session),
            tokenizer,
            embedding_dim: 768,
        })
    }

    /// Encode a batch of text strings to normalized embeddings.
    ///
    /// Returns a Vec of 768-dim f32 vectors, one per input text.
    pub fn encode_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, PipelineError> {
        let max_length = 64; // SigLIP default sequence length
        let batch_size = texts.len();

        // Tokenize all texts
        let encodings = self
            .tokenizer
            .encode_batch(texts.to_vec(), true)
            .map_err(|e| PipelineError::Model {
                message: format!("Tokenization failed: {e}"),
            })?;

        // Build flat input_ids tensor — SigLIP text model takes input_ids only (no attention_mask)
        let mut input_ids = vec![0i64; batch_size * max_length];

        for (i, encoding) in encodings.iter().enumerate() {
            let ids = encoding.get_ids();
            for (j, &id) in ids.iter().take(max_length).enumerate() {
                input_ids[i * max_length + j] = id as i64;
            }
        }

        // Run inference
        let mut session = self.session.lock().map_err(|e| PipelineError::Model {
            message: format!("Text encoder lock poisoned: {e}"),
        })?;

        let input_ids_value =
            Value::from_array((vec![batch_size as i64, max_length as i64], input_ids)).map_err(
                |e| PipelineError::Model {
                    message: format!("Failed to create input tensor: {e}"),
                },
            )?;

        let outputs = session
            .run(ort::inputs!["input_ids" => input_ids_value])
            .map_err(|e| PipelineError::Model {
                message: format!("Text encoder inference failed: {e}"),
            })?;

        // Extract pooler_output by name — the cross-modal embedding
        let pooler_output = outputs
            .iter()
            .find(|(name, _)| *name == "pooler_output")
            .ok_or_else(|| PipelineError::Model {
                message: "Text encoder did not produce pooler_output".to_string(),
            })?;

        let (_shape, data) =
            pooler_output
                .1
                .try_extract_tensor::<f32>()
                .map_err(|e| PipelineError::Model {
                    message: format!("Failed to extract pooler_output: {e}"),
                })?;

        // Split flat output into per-text embeddings and L2-normalize
        let embeddings: Vec<Vec<f32>> = data
            .chunks(self.embedding_dim)
            .map(crate::math::l2_normalize)
            .collect();

        Ok(embeddings)
    }

    /// Encode a single text string to a normalized embedding.
    pub fn encode(&self, text: &str) -> Result<Vec<f32>, PipelineError> {
        let batch = self.encode_batch(&[text.to_string()])?;
        batch
            .into_iter()
            .next()
            .ok_or_else(|| PipelineError::Model {
                message: "Text encoder returned empty result for single input".to_string(),
            })
    }

    /// Check whether the text encoder model files exist.
    pub fn model_exists(model_dir: &Path) -> bool {
        model_dir.join("text_model.onnx").exists() && model_dir.join("tokenizer.json").exists()
    }
}
