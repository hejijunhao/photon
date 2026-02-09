//! Thumbnail generation with WebP output.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use image::{DynamicImage, ImageFormat};
use std::io::Cursor;

use crate::config::ThumbnailConfig;

/// Generates thumbnails from images.
pub struct ThumbnailGenerator {
    config: ThumbnailConfig,
}

impl ThumbnailGenerator {
    /// Create a new thumbnail generator with the given configuration.
    pub fn new(config: ThumbnailConfig) -> Self {
        Self { config }
    }

    /// Generate a thumbnail and return it as a base64-encoded WebP string.
    ///
    /// Returns `None` if thumbnail generation is disabled or fails.
    pub fn generate(&self, image: &DynamicImage) -> Option<String> {
        if !self.config.enabled {
            return None;
        }

        // Resize maintaining aspect ratio (longest edge = config.size)
        let thumbnail = image.thumbnail(self.config.size, self.config.size);

        // Encode to WebP
        let mut buffer = Cursor::new(Vec::new());
        thumbnail.write_to(&mut buffer, ImageFormat::WebP).ok()?;

        // Return as base64
        Some(BASE64.encode(buffer.into_inner()))
    }

    /// Generate a thumbnail and return the raw bytes.
    ///
    /// Useful for writing directly to disk.
    pub fn generate_bytes(&self, image: &DynamicImage) -> Option<Vec<u8>> {
        if !self.config.enabled {
            return None;
        }

        let thumbnail = image.thumbnail(self.config.size, self.config.size);

        let mut buffer = Cursor::new(Vec::new());
        thumbnail.write_to(&mut buffer, ImageFormat::WebP).ok()?;

        Some(buffer.into_inner())
    }

    /// Check if thumbnail generation is enabled.
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ThumbnailConfig;

    #[test]
    fn test_thumbnail_generation() {
        let config = ThumbnailConfig {
            enabled: true,
            size: 128,
            format: "webp".to_string(),
            quality: 80,
        };
        let generator = ThumbnailGenerator::new(config);

        // Create a simple test image
        let img = DynamicImage::new_rgb8(1000, 500);
        let thumbnail = generator.generate(&img);

        assert!(thumbnail.is_some());
        // Base64 should be non-empty
        assert!(!thumbnail.unwrap().is_empty());
    }

    #[test]
    fn test_thumbnail_disabled() {
        let config = ThumbnailConfig {
            enabled: false,
            size: 128,
            format: "webp".to_string(),
            quality: 80,
        };
        let generator = ThumbnailGenerator::new(config);

        let img = DynamicImage::new_rgb8(1000, 500);
        let thumbnail = generator.generate(&img);

        assert!(thumbnail.is_none());
    }

    #[test]
    fn test_thumbnail_bytes() {
        let config = ThumbnailConfig {
            enabled: true,
            size: 64,
            format: "webp".to_string(),
            quality: 80,
        };
        let generator = ThumbnailGenerator::new(config);

        let img = DynamicImage::new_rgb8(200, 200);
        let bytes = generator.generate_bytes(&img);

        assert!(bytes.is_some());
        let bytes = bytes.unwrap();
        // WebP files start with "RIFF"
        assert_eq!(&bytes[0..4], b"RIFF");
    }
}
