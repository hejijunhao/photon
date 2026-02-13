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

    /// Generate embeddings for multiple already-preprocessed tensors in a single ONNX call.
    ///
    /// Amortizes session dispatch overhead across N images. Each tensor should
    /// have shape \[1, 3, image_size, image_size\]. Returns one L2-normalized
    /// `Vec<f32>` per input tensor.
    pub fn embed_batch_preprocessed(
        &self,
        tensors: &[ndarray::Array4<f32>],
        paths: &[PathBuf],
    ) -> Result<Vec<Vec<f32>>, PipelineError> {
        self.session.embed_batch(tensors, paths)
    }

    /// Generate embeddings for multiple images in a single ONNX call.
    ///
    /// Preprocesses each image, then batches them for inference.
    pub fn embed_batch(
        &self,
        images: &[(&DynamicImage, &Path)],
    ) -> Result<Vec<Vec<f32>>, PipelineError> {
        let tensors: Vec<ndarray::Array4<f32>> = images
            .iter()
            .map(|(img, _)| preprocess(img, self.image_size))
            .collect();
        let paths: Vec<PathBuf> = images.iter().map(|(_, p)| p.to_path_buf()).collect();
        self.session.embed_batch(&tensors, &paths)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    /// Try to load the engine for tests; returns None if model files are missing.
    fn try_load_engine() -> Option<EmbeddingEngine> {
        let config = Config::default();
        if !EmbeddingEngine::model_exists(&config.embedding, &config.model_dir()) {
            return None;
        }
        EmbeddingEngine::load(&config.embedding, &config.model_dir()).ok()
    }

    #[test]
    fn test_embed_batch_empty_input() {
        let Some(engine) = try_load_engine() else {
            eprintln!("Skipping: model not found");
            return;
        };
        let result = engine.embed_batch_preprocessed(&[], &[]).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_embed_batch_single_matches_single() {
        let Some(engine) = try_load_engine() else {
            eprintln!("Skipping: model not found");
            return;
        };

        let img = DynamicImage::ImageRgb8(image::RgbImage::new(640, 480));
        let path = PathBuf::from("test.png");
        let tensor = preprocess(&img, engine.image_size());

        // Single embed
        let single = engine.embed_preprocessed(&tensor, &path).unwrap();

        // Batch embed with 1 item
        let batch = engine.embed_batch_preprocessed(&[tensor], &[path]).unwrap();

        assert_eq!(batch.len(), 1);
        assert_eq!(single.len(), batch[0].len());
        // Embeddings should be identical (same input, same model).
        for (a, b) in single.iter().zip(batch[0].iter()) {
            assert!(
                (a - b).abs() < 1e-5,
                "Embedding mismatch: single={a}, batch={b}"
            );
        }
    }

    #[test]
    fn test_embed_batch_multiple_normalized() {
        let Some(engine) = try_load_engine() else {
            eprintln!("Skipping: model not found");
            return;
        };

        let img1 = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            224,
            224,
            image::Rgb([255, 0, 0]),
        ));
        let img2 = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            224,
            224,
            image::Rgb([0, 0, 255]),
        ));

        let t1 = preprocess(&img1, engine.image_size());
        let t2 = preprocess(&img2, engine.image_size());
        let paths = vec![PathBuf::from("red.png"), PathBuf::from("blue.png")];

        let results = engine.embed_batch_preprocessed(&[t1, t2], &paths).unwrap();

        assert_eq!(results.len(), 2);
        for (i, emb) in results.iter().enumerate() {
            assert_eq!(emb.len(), 768, "Embedding {i} should be 768-dim");
            let norm: f32 = emb.iter().map(|x| x * x).sum::<f32>().sqrt();
            assert!(
                (norm - 1.0).abs() < 1e-4,
                "Embedding {i} should be L2-normalized, got norm={norm}"
            );
        }
    }
}
