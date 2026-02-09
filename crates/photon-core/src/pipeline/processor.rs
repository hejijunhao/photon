//! Pipeline orchestration - wires together all processing stages.

use std::path::Path;

use crate::config::Config;
use crate::error::{PipelineError, Result};
use crate::types::ProcessedImage;

use super::decode::{format_to_string, ImageDecoder};
use super::discovery::{DiscoveredFile, FileDiscovery};
use super::hash::Hasher;
use super::metadata::MetadataExtractor;
use super::thumbnail::ThumbnailGenerator;
use super::validate::Validator;

/// Options for controlling image processing behavior.
#[derive(Debug, Clone, Default)]
pub struct ProcessOptions {
    /// Skip thumbnail generation
    pub skip_thumbnail: bool,
    /// Skip perceptual hash generation
    pub skip_perceptual_hash: bool,
}

/// The main image processor that orchestrates the full pipeline.
pub struct ImageProcessor {
    decoder: ImageDecoder,
    thumbnail_gen: ThumbnailGenerator,
    validator: Validator,
    discovery: FileDiscovery,
}

impl ImageProcessor {
    /// Create a new image processor with the given configuration.
    pub fn new(config: &Config) -> Self {
        Self {
            decoder: ImageDecoder::new(config.limits.clone()),
            thumbnail_gen: ThumbnailGenerator::new(config.thumbnail.clone()),
            validator: Validator::new(config.limits.clone()),
            discovery: FileDiscovery::new(config.processing.clone()),
        }
    }

    /// Process a single image through the full pipeline.
    ///
    /// Returns a `ProcessedImage` with all available metadata.
    /// The `embedding`, `tags`, and `description` fields will be empty
    /// (those are populated in later phases).
    pub async fn process(&self, path: &Path) -> Result<ProcessedImage> {
        self.process_with_options(path, &ProcessOptions::default())
            .await
    }

    /// Process a single image with custom options.
    pub async fn process_with_options(
        &self,
        path: &Path,
        options: &ProcessOptions,
    ) -> Result<ProcessedImage> {
        let start = std::time::Instant::now();
        tracing::debug!("Processing: {:?}", path);

        // Validate
        self.validator.validate(path)?;
        let validate_time = start.elapsed();
        tracing::trace!("  Validate: {:?}", validate_time);

        // Decode
        let decode_start = std::time::Instant::now();
        let decoded = self.decoder.decode(path).await?;
        let decode_time = decode_start.elapsed();
        tracing::trace!("  Decode: {:?}", decode_time);

        // Extract metadata (non-blocking, sync operation)
        let metadata_start = std::time::Instant::now();
        let exif = MetadataExtractor::extract(path);
        let metadata_time = metadata_start.elapsed();
        tracing::trace!("  Metadata: {:?}", metadata_time);

        // Generate content hash
        let hash_start = std::time::Instant::now();
        let content_hash = Hasher::content_hash(path).map_err(|e| PipelineError::Decode {
            path: path.to_path_buf(),
            message: format!("Hash error: {}", e),
        })?;
        let hash_time = hash_start.elapsed();
        tracing::trace!("  Content hash: {:?}", hash_time);

        // Generate perceptual hash
        let phash_start = std::time::Instant::now();
        let perceptual_hash = if options.skip_perceptual_hash {
            None
        } else {
            Some(Hasher::perceptual_hash(&decoded.image))
        };
        let phash_time = phash_start.elapsed();
        tracing::trace!("  Perceptual hash: {:?}", phash_time);

        // Generate thumbnail
        let thumb_start = std::time::Instant::now();
        let thumbnail = if options.skip_thumbnail {
            None
        } else {
            self.thumbnail_gen.generate(&decoded.image)
        };
        let thumb_time = thumb_start.elapsed();
        tracing::trace!("  Thumbnail: {:?}", thumb_time);

        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        let total_time = start.elapsed();
        tracing::debug!(
            "Processed {:?} in {:?} ({}x{})",
            file_name,
            total_time,
            decoded.width,
            decoded.height
        );

        Ok(ProcessedImage {
            file_path: path.to_path_buf(),
            file_name,
            content_hash,
            width: decoded.width,
            height: decoded.height,
            format: format_to_string(decoded.format),
            file_size: decoded.file_size,
            embedding: vec![], // Placeholder - Phase 3
            exif,
            tags: vec![],      // Placeholder - Phase 4
            description: None, // Placeholder - Phase 5
            thumbnail,
            perceptual_hash,
        })
    }

    /// Discover all image files at a path.
    pub fn discover(&self, path: &Path) -> Vec<DiscoveredFile> {
        self.discovery.discover(path)
    }

    /// Check if thumbnail generation is enabled.
    pub fn thumbnails_enabled(&self) -> bool {
        self.thumbnail_gen.is_enabled()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_options_default() {
        let options = ProcessOptions::default();
        assert!(!options.skip_thumbnail);
        assert!(!options.skip_perceptual_hash);
    }
}
