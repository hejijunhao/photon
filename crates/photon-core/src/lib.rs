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
//! use photon_core::{Config, ImageProcessor, ProcessOptions};
//!
//! #[tokio::main]
//! async fn main() -> photon_core::Result<()> {
//!     let config = Config::load()?;
//!     let mut processor = ImageProcessor::new(&config);
//!     processor.load_embedding(&config)?;
//!     processor.load_tagging(&config)?;
//!
//!     let result = processor.process(std::path::Path::new("./image.jpg")).await?;
//!     println!("Tags: {:?}", result.tags);
//!     Ok(())
//! }
//! ```

// Force-link BLAS (Accelerate on macOS) for ndarray's optimized dot products.
// Without this, the linker won't pull in the blas-src symbols.
#[cfg(target_os = "macos")]
extern crate blas_src;

// Module declarations — public modules have re-exported consumer types
pub mod config;
pub(crate) mod embedding;
pub mod error;
pub(crate) mod llm;
pub(crate) mod math;
pub(crate) mod output;
pub(crate) mod pipeline;
pub(crate) mod tagging;
pub mod types;

// Re-exports for convenient access
pub use config::Config;
pub use embedding::preprocess::preprocess as preprocess_image;
pub use embedding::EmbeddingEngine;
pub use error::{ConfigError, PhotonError, PipelineError, PipelineResult, Result};
pub use llm::{EnrichOptions, EnrichResult, Enricher, LlmProviderFactory};
pub use output::{OutputFormat, OutputWriter};
pub use pipeline::{
    DiscoveredFile, FileDiscovery, Hasher, ImageDecoder, ImageProcessor, MetadataExtractor,
    ProcessOptions, ThumbnailGenerator,
};
pub use types::{EnrichmentPatch, ExifData, OutputRecord, ProcessedImage, ProcessingStats, Tag};

/// Library version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
    }
}
