//! Image processing pipeline components.
//!
//! This module contains all the stages of the image processing pipeline:
//! - **decode**: Load and decode images from various formats
//! - **metadata**: Extract EXIF metadata from images
//! - **hash**: Generate content and perceptual hashes
//! - **thumbnail**: Generate WebP thumbnails
//! - **discovery**: Find image files in directories
//! - **validate**: Pre-processing validation
//! - **processor**: Orchestrates the full pipeline

pub(crate) mod decode;
pub(crate) mod discovery;
pub(crate) mod hash;
pub(crate) mod metadata;
pub(crate) mod processor;
pub(crate) mod thumbnail;
pub(crate) mod validate;

// Re-exports for convenient access
pub use decode::ImageDecoder;
pub use discovery::{DiscoveredFile, FileDiscovery};
pub use hash::Hasher;
pub use metadata::MetadataExtractor;
pub use processor::{ImageProcessor, ProcessOptions};
pub use thumbnail::ThumbnailGenerator;
