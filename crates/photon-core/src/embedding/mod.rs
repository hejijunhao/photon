//! SigLIP embedding generation.
//!
//! This module handles converting images into 768-dimensional vector embeddings
//! using a SigLIP visual encoder running locally via ONNX Runtime.
//!
//! # Usage
//!
//! ```rust,ignore
//! use photon_core::embedding::EmbeddingEngine;
//! use photon_core::config::{EmbeddingConfig, Config};
//!
//! let config = Config::default();
//! let engine = EmbeddingEngine::load(&config.embedding, &config.model_dir())?;
//! let embedding = engine.embed(&decoded_image)?;
//! // embedding is a Vec<f32> with 768 elements
//! ```

pub(crate) mod preprocess;
pub(crate) mod siglip;

use std::path::{Path, PathBuf};

use image::DynamicImage;

use crate::config::EmbeddingConfig;
use crate::error::PipelineError;

use self::preprocess::preprocess;
use self::siglip::SigLipSession;

/// The visual encoder ONNX model filename.
const VISUAL_MODEL_FILENAME: &str = "visual.onnx";

/// Engine for generating image embeddings via SigLIP.
pub struct EmbeddingEngine {
    session: SigLipSession,
    image_size: u32,
}

impl EmbeddingEngine {
    /// Load the SigLIP visual encoder from the model directory.
    ///
    /// Expects the ONNX model at `{model_dir}/{model_name}/visual.onnx`.
    pub fn load(config: &EmbeddingConfig, model_dir: &Path) -> Result<Self, PipelineError> {
        let model_path = model_dir.join(&config.model).join(VISUAL_MODEL_FILENAME);

        if !model_path.exists() {
            return Err(PipelineError::Embedding {
                path: model_path,
                message: "Model not found. Run `photon models download` first.".to_string(),
            });
        }

        tracing::info!("Loading SigLIP model from {:?}", model_path);
        let session = SigLipSession::load(&model_path)?;
        tracing::info!("SigLIP model loaded successfully");

        let image_size = config.image_size;

        Ok(Self {
            session,
            image_size,
        })
    }

    /// Get the image input size for this model (224 or 384).
    pub fn image_size(&self) -> u32 {
        self.image_size
    }

    /// Generate an embedding vector for an image.
    ///
    /// Returns an L2-normalized Vec<f32> (typically 768 dimensions).
    pub fn embed(&self, image: &DynamicImage, path: &Path) -> Result<Vec<f32>, PipelineError> {
        let tensor = preprocess(image, self.image_size);
        self.session.embed(&tensor, path)
    }

    /// Generate an embedding from an already-preprocessed tensor.
    ///
    /// Use this when preprocessing has been done outside of `spawn_blocking`
    /// to avoid cloning the full `DynamicImage` across thread boundaries.
    pub fn embed_preprocessed(
        &self,
        tensor: &ndarray::Array4<f32>,
        path: &Path,
    ) -> Result<Vec<f32>, PipelineError> {
        self.session.embed(tensor, path)
    }

    /// Check whether the model files exist on disk.
    pub fn model_exists(config: &EmbeddingConfig, model_dir: &Path) -> bool {
        let model_path = model_dir.join(&config.model).join(VISUAL_MODEL_FILENAME);
        model_path.exists()
    }

    /// Get the expected model file path.
    pub fn model_path(config: &EmbeddingConfig, model_dir: &Path) -> PathBuf {
        model_dir.join(&config.model).join(VISUAL_MODEL_FILENAME)
    }
}
