//! Photon Core - Embeddable image processing library.
//!
//! Photon is a pure image processing pipeline that takes images as input and
//! outputs structured data: vector embeddings, semantic tags, metadata, and
//! descriptions.
//!
//! # Architecture
//!
//! Photon is designed as a pure pipeline with no database dependencies:
//!
//! ```text
//! Image → Decode → Extract Metadata → Embed (SigLIP) → Tags/Description → JSON
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! use photon_core::{Config, Photon, ProcessOptions};
//!
//! #[tokio::main]
//! async fn main() -> photon_core::Result<()> {
//!     let config = Config::load()?;
//!     let photon = Photon::new(config).await?;
//!
//!     let result = photon.process("./image.jpg", ProcessOptions::default()).await?;
//!     println!("Tags: {:?}", result.tags);
//!     Ok(())
//! }
//! ```

// Module declarations
pub mod config;
pub mod embedding;
pub mod error;
pub mod output;
pub mod pipeline;
pub mod tagging;
pub mod types;

// Re-exports for convenient access
pub use config::Config;
pub use embedding::EmbeddingEngine;
pub use error::{ConfigError, PhotonError, PipelineError, PipelineResult, Result};
pub use output::{OutputFormat, OutputWriter};
pub use pipeline::{ImageProcessor, ProcessOptions};
pub use types::{ExifData, ProcessedImage, ProcessingStats, Tag};

/// Library version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Photon processor - the main entry point for image processing.
///
/// This struct will be fully implemented in Phase 2+. For now, it provides
/// configuration loading and placeholder methods.
pub struct Photon {
    config: Config,
}

impl Photon {
    /// Create a new Photon instance with the given configuration.
    pub async fn new(config: Config) -> Result<Self> {
        tracing::debug!("Initializing Photon v{}", VERSION);
        Ok(Self { config })
    }

    /// Create a new Photon instance with default configuration.
    pub async fn with_defaults() -> Result<Self> {
        let config = Config::load()?;
        Self::new(config).await
    }

    /// Get a reference to the current configuration.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Get the model directory path.
    pub fn model_dir(&self) -> std::path::PathBuf {
        self.config.model_dir()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
    }

    #[tokio::test]
    async fn test_photon_new() {
        let config = Config::default();
        let photon = Photon::new(config).await.unwrap();
        assert_eq!(photon.config().processing.parallel_workers, 4);
    }
}
