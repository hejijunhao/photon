//! Content and perceptual hashing for image deduplication.

use blake3::Hasher as Blake3Hasher;
use image::DynamicImage;
use image_hasher::{HashAlg, HasherConfig, ImageHash};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

/// Provides content and perceptual hashing for images.
pub struct Hasher;

impl Hasher {
    /// Generate a BLAKE3 hash of file contents for exact deduplication.
    ///
    /// Uses streaming to handle large files efficiently without loading
    /// the entire file into memory.
    pub fn content_hash(path: &Path) -> std::io::Result<String> {
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);
        let mut hasher = Blake3Hasher::new();

        // Use 64KB buffer for efficient reading
        let mut buffer = [0u8; 65536];
        loop {
            let bytes_read = reader.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }

        Ok(hasher.finalize().to_hex().to_string())
    }

    /// Generate a perceptual hash for near-duplicate detection.
    ///
    /// Similar images will have similar hashes, allowing detection of
    /// resized, cropped, or slightly modified versions.
    pub fn perceptual_hash(image: &DynamicImage) -> String {
        let hasher = HasherConfig::new()
            .hash_alg(HashAlg::DoubleGradient)
            .hash_size(16, 16)
            .to_hasher();

        let hash = hasher.hash_image(image);
        hash.to_base64()
    }

    /// Compare two perceptual hashes and return their Hamming distance.
    ///
    /// Returns `None` if either hash is invalid.
    /// A distance of 0 means identical images.
    /// Distances < 10 typically indicate very similar images.
    pub fn perceptual_distance(hash1: &str, hash2: &str) -> Option<u32> {
        let h1 = ImageHash::<Vec<u8>>::from_base64(hash1).ok()?;
        let h2 = ImageHash::<Vec<u8>>::from_base64(hash2).ok()?;
        Some(h1.dist(&h2))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_perceptual_hash_consistency() {
        // Same image should produce same hash
        let img = DynamicImage::new_rgb8(100, 100);
        let hash1 = Hasher::perceptual_hash(&img);
        let hash2 = Hasher::perceptual_hash(&img);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_perceptual_distance_identical() {
        let img = DynamicImage::new_rgb8(100, 100);
        let hash = Hasher::perceptual_hash(&img);
        let distance = Hasher::perceptual_distance(&hash, &hash);
        assert_eq!(distance, Some(0));
    }

    #[test]
    fn test_perceptual_distance_invalid_hash() {
        let distance = Hasher::perceptual_distance("invalid", "also_invalid");
        assert!(distance.is_none());
    }
}
