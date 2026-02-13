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

    /// Decode an image from an in-memory byte buffer with validation and timeout.
    ///
    /// Used when the file has already been read (e.g., for combined hash+decode
    /// to avoid reading the file twice).
    pub async fn decode_from_bytes(
        &self,
        bytes: Vec<u8>,
        path: &Path,
    ) -> Result<DecodedImage, PipelineError> {
        let file_size = bytes.len() as u64;
        let path_owned = path.to_path_buf();
        let timeout_duration = Duration::from_millis(self.limits.decode_timeout_ms);

        let decode_result = timeout(timeout_duration, async {
            tokio::task::spawn_blocking(move || Self::decode_bytes_sync(bytes, &path_owned)).await
        })
        .await;

        match decode_result {
            Ok(Ok(Ok(mut decoded))) => {
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

    /// Synchronous decode from bytes (runs in spawn_blocking).
    fn decode_bytes_sync(bytes: Vec<u8>, path: &Path) -> Result<DecodedImage, PipelineError> {
        use std::io::Cursor;

        let file_size = bytes.len() as u64;
        let cursor = Cursor::new(bytes);
        let reader = image::ImageReader::new(cursor)
            .with_guessed_format()
            .map_err(|e| PipelineError::Decode {
                path: path.to_path_buf(),
                message: format!("Cannot detect image format: {}", e),
            })?;
        let format = match reader.format() {
            Some(f) => f,
            None => ImageFormat::from_path(path).map_err(|_| PipelineError::UnsupportedFormat {
                path: path.to_path_buf(),
                format: path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("unknown")
                    .to_string(),
            })?,
        };
        let image = reader.decode().map_err(|e| PipelineError::Decode {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;

        let (width, height) = image.dimensions();
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

    #[test]
    fn test_format_detected_by_content() {
        // Copy a PNG file to a .jpg extension — format should be detected as PNG
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/images/test.png");
        let dir = tempfile::tempdir().unwrap();
        let misnamed = dir.path().join("test_misnamed.jpg");
        std::fs::copy(&fixture, &misnamed).unwrap();

        let bytes = std::fs::read(&misnamed).unwrap();
        let result = ImageDecoder::decode_bytes_sync(bytes, &misnamed).unwrap();
        assert_eq!(result.format, ImageFormat::Png);
    }
}
