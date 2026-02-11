//! Image decoding with format detection, validation, and timeout support.

use image::{DynamicImage, GenericImageView, ImageFormat};
use std::path::Path;
use std::time::Duration;
use tokio::time::timeout;

use crate::config::LimitsConfig;
use crate::error::PipelineError;

/// Image decoder with configurable limits and timeout.
pub struct ImageDecoder {
    limits: LimitsConfig,
}

/// Result of decoding an image.
pub struct DecodedImage {
    /// The decoded image data
    pub image: DynamicImage,
    /// Detected image format
    pub format: ImageFormat,
    /// Image width in pixels
    pub width: u32,
    /// Image height in pixels
    pub height: u32,
    /// Original file size in bytes
    pub file_size: u64,
}

impl ImageDecoder {
    /// Create a new decoder with the given limits.
    pub fn new(limits: LimitsConfig) -> Self {
        Self { limits }
    }

    /// Decode an image from a file path with validation and timeout.
    ///
    /// Note: file-size validation is handled by `Validator::validate()` which runs
    /// before decode in the pipeline. We still read file_size for the output metadata.
    pub async fn decode(&self, path: &Path) -> Result<DecodedImage, PipelineError> {
        let file_size =
            std::fs::metadata(path)
                .map(|m| m.len())
                .map_err(|e| PipelineError::Decode {
                    path: path.to_path_buf(),
                    message: format!("Cannot read file: {}", e),
                })?;

        // Decode with timeout using spawn_blocking to avoid blocking async runtime
        let path_owned = path.to_path_buf();
        let timeout_duration = Duration::from_millis(self.limits.decode_timeout_ms);

        let decode_result = timeout(timeout_duration, async {
            tokio::task::spawn_blocking(move || Self::decode_sync(&path_owned)).await
        })
        .await;

        match decode_result {
            Ok(Ok(Ok(mut decoded))) => {
                // Validate dimensions
                if decoded.width > self.limits.max_image_dimension
                    || decoded.height > self.limits.max_image_dimension
                {
                    return Err(PipelineError::ImageTooLarge {
                        path: path.to_path_buf(),
                        width: decoded.width,
                        height: decoded.height,
                        max_dim: self.limits.max_image_dimension,
                    });
                }
                // Set file size from metadata (more accurate)
                decoded.file_size = file_size;
                Ok(decoded)
            }
            Ok(Ok(Err(e))) => Err(e),
            Ok(Err(e)) => Err(PipelineError::Decode {
                path: path.to_path_buf(),
                message: format!("Task join error: {}", e),
            }),
            Err(_) => Err(PipelineError::Timeout {
                path: path.to_path_buf(),
                stage: "decode".to_string(),
                timeout_ms: self.limits.decode_timeout_ms,
            }),
        }
    }

    /// Synchronous decode implementation (runs in spawn_blocking).
    fn decode_sync(path: &Path) -> Result<DecodedImage, PipelineError> {
        // Detect format from path extension and file contents
        let format = ImageFormat::from_path(path).map_err(|e| PipelineError::Decode {
            path: path.to_path_buf(),
            message: format!("Unknown format: {}", e),
        })?;

        // Open and decode the image
        let image = image::open(path).map_err(|e| PipelineError::Decode {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;

        let (width, height) = image.dimensions();
        let file_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);

        Ok(DecodedImage {
            image,
            format,
            width,
            height,
            file_size,
        })
    }
}

/// Convert an ImageFormat to a string representation.
pub fn format_to_string(format: ImageFormat) -> String {
    match format {
        ImageFormat::Jpeg => "jpeg".to_string(),
        ImageFormat::Png => "png".to_string(),
        ImageFormat::WebP => "webp".to_string(),
        ImageFormat::Gif => "gif".to_string(),
        ImageFormat::Tiff => "tiff".to_string(),
        ImageFormat::Bmp => "bmp".to_string(),
        ImageFormat::Ico => "ico".to_string(),
        ImageFormat::Pnm => "pnm".to_string(),
        ImageFormat::Avif => "avif".to_string(),
        _ => "unknown".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_to_string() {
        assert_eq!(format_to_string(ImageFormat::Jpeg), "jpeg");
        assert_eq!(format_to_string(ImageFormat::Png), "png");
        assert_eq!(format_to_string(ImageFormat::WebP), "webp");
    }
}
