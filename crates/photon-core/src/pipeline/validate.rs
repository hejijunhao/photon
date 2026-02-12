//! Input validation before processing.

use std::io::Read;
use std::path::Path;

use crate::config::LimitsConfig;
use crate::error::PipelineError;

/// Validates files before processing.
pub struct Validator {
    limits: LimitsConfig,
}

impl Validator {
    /// Create a new validator with the given limits.
    pub fn new(limits: LimitsConfig) -> Self {
        Self { limits }
    }

    /// Perform quick validation before full decode.
    ///
    /// Checks:
    /// - File exists and is readable
    /// - File size is within limits
    /// - File has valid image magic bytes
    pub fn validate(&self, path: &Path) -> Result<(), PipelineError> {
        // Check file exists
        if !path.exists() {
            return Err(PipelineError::FileNotFound(path.to_path_buf()));
        }

        // Check file size
        let metadata = std::fs::metadata(path).map_err(|e| PipelineError::Decode {
            path: path.to_path_buf(),
            message: format!("Cannot read metadata: {}", e),
        })?;

        let max_bytes = self.limits.max_file_size_mb * 1024 * 1024;
        if metadata.len() > max_bytes {
            return Err(PipelineError::FileTooLarge {
                path: path.to_path_buf(),
                size_mb: metadata.len() / (1024 * 1024),
                max_mb: self.limits.max_file_size_mb,
            });
        }

        // Check magic bytes
        self.check_magic_bytes(path)?;

        Ok(())
    }

    /// Check file magic bytes to verify it's a valid image format.
    fn check_magic_bytes(&self, path: &Path) -> Result<(), PipelineError> {
        let mut file = std::fs::File::open(path).map_err(|e| PipelineError::Decode {
            path: path.to_path_buf(),
            message: format!("Cannot open file: {}", e),
        })?;

        let mut header = [0u8; 12];
        let bytes_read = file.read(&mut header).unwrap_or(0);

        if bytes_read < 4 {
            return Err(PipelineError::Decode {
                path: path.to_path_buf(),
                message: "File too small to be a valid image".to_string(),
            });
        }

        // Check common format signatures
        let is_valid = Self::is_valid_image_header(&header, bytes_read);

        if !is_valid {
            return Err(PipelineError::Decode {
                path: path.to_path_buf(),
                message: "Unrecognized image format (invalid magic bytes)".to_string(),
            });
        }

        Ok(())
    }

    /// Check if the header bytes match known image formats.
    fn is_valid_image_header(header: &[u8; 12], bytes_read: usize) -> bool {
        if bytes_read < 4 {
            return false;
        }

        // JPEG: FF D8 FF
        if header[0] == 0xFF && header[1] == 0xD8 && header[2] == 0xFF {
            return true;
        }

        // PNG: 89 50 4E 47
        if header[0] == 0x89 && header[1] == b'P' && header[2] == b'N' && header[3] == b'G' {
            return true;
        }

        // GIF: GIF8
        if header[0] == b'G' && header[1] == b'I' && header[2] == b'F' && header[3] == b'8' {
            return true;
        }

        // WebP: RIFF....WEBP
        if header[0] == b'R' && header[1] == b'I' && header[2] == b'F' && header[3] == b'F' {
            if bytes_read >= 12 {
                return header[8] == b'W'
                    && header[9] == b'E'
                    && header[10] == b'B'
                    && header[11] == b'P';
            }
            // Could be WebP, allow it to proceed
            return true;
        }

        // BMP: BM
        if header[0] == b'B' && header[1] == b'M' {
            return true;
        }

        // TIFF: II (little-endian) or MM (big-endian) followed by version 42
        if bytes_read >= 4 {
            let is_tiff_le =
                header[0] == b'I' && header[1] == b'I' && header[2] == 0x2A && header[3] == 0x00;
            let is_tiff_be =
                header[0] == b'M' && header[1] == b'M' && header[2] == 0x00 && header[3] == 0x2A;
            if is_tiff_le || is_tiff_be {
                return true;
            }
        }

        // HEIC/HEIF/AVIF: ftyp box at offset 4
        if bytes_read >= 12
            && header[4] == b'f'
            && header[5] == b't'
            && header[6] == b'y'
            && header[7] == b'p'
        {
            return true;
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_magic_bytes_jpeg() {
        let header = [0xFF, 0xD8, 0xFF, 0xE0, 0, 0, 0, 0, 0, 0, 0, 0];
        assert!(Validator::is_valid_image_header(&header, 12));
    }

    #[test]
    fn test_magic_bytes_png() {
        let header = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0];
        assert!(Validator::is_valid_image_header(&header, 12));
    }

    #[test]
    fn test_magic_bytes_webp() {
        let header = [b'R', b'I', b'F', b'F', 0, 0, 0, 0, b'W', b'E', b'B', b'P'];
        assert!(Validator::is_valid_image_header(&header, 12));
    }

    #[test]
    fn test_magic_bytes_invalid() {
        let header = [0x00, 0x00, 0x00, 0x00, 0, 0, 0, 0, 0, 0, 0, 0];
        assert!(!Validator::is_valid_image_header(&header, 12));
    }

    #[test]
    fn test_magic_bytes_tiff_le() {
        // Little-endian TIFF: II + version 42
        let header = [b'I', b'I', 0x2A, 0x00, 0, 0, 0, 0, 0, 0, 0, 0];
        assert!(Validator::is_valid_image_header(&header, 12));
    }

    #[test]
    fn test_magic_bytes_tiff_be() {
        // Big-endian TIFF: MM + version 42
        let header = [b'M', b'M', 0x00, 0x2A, 0, 0, 0, 0, 0, 0, 0, 0];
        assert!(Validator::is_valid_image_header(&header, 12));
    }

    #[test]
    fn test_magic_bytes_bare_ii_rejected() {
        // Bare "II" without TIFF version bytes should not match
        let header = [b'I', b'I', 0x00, 0x00, 0, 0, 0, 0, 0, 0, 0, 0];
        assert!(!Validator::is_valid_image_header(&header, 12));
    }

    #[test]
    fn test_magic_bytes_bare_mm_rejected() {
        // Bare "MM" without TIFF version bytes should not match
        let header = [b'M', b'M', 0x00, 0x00, 0, 0, 0, 0, 0, 0, 0, 0];
        assert!(!Validator::is_valid_image_header(&header, 12));
    }
}
