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

// Module declarations
pub mod config;
pub mod embedding;
pub mod error;
pub mod math;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
    }
}
