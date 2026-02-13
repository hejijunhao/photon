//! Content and perceptual hashing for image deduplication.

use blake3::Hasher as Blake3Hasher;
use image::DynamicImage;
use image_hasher::{HashAlg, HasherConfig, ImageHash};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

/// Provides content hashing and perceptual hashing for images.
///
/// The perceptual hasher is pre-configured and cached to avoid
/// re-allocating the same `HasherConfig` for every image.
pub struct Hasher {
    phash_hasher: image_hasher::Hasher,
}

impl Default for Hasher {
    fn default() -> Self {
        Self::new()
    }
}

impl Hasher {
    /// Create a new hasher with a pre-configured perceptual hash algorithm.
    pub fn new() -> Self {
        let phash_hasher = HasherConfig::new()
            .hash_alg(HashAlg::DoubleGradient)
            .hash_size(16, 16)
            .to_hasher();
        Self { phash_hasher }
    }

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

    /// Generate a BLAKE3 hash from an in-memory byte buffer.
    ///
    /// Used when the file has already been read into memory (e.g., to avoid
    /// reading the file twice for both hashing and decoding).
    pub fn content_hash_from_bytes(data: &[u8]) -> String {
        let mut hasher = Blake3Hasher::new();
        hasher.update(data);
        hasher.finalize().to_hex().to_string()
    }

    /// Generate a perceptual hash for near-duplicate detection.
    ///
    /// Uses the pre-configured hasher to avoid per-image allocation overhead.
    /// Similar images will have similar hashes, allowing detection of
    /// resized, cropped, or slightly modified versions.
    pub fn perceptual_hash(&self, image: &DynamicImage) -> String {
        let hash = self.phash_hasher.hash_image(image);
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
        let hasher = Hasher::new();
        let img = DynamicImage::new_rgb8(100, 100);
        let hash1 = hasher.perceptual_hash(&img);
        let hash2 = hasher.perceptual_hash(&img);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_perceptual_distance_identical() {
        let hasher = Hasher::new();
        let img = DynamicImage::new_rgb8(100, 100);
        let hash = hasher.perceptual_hash(&img);
        let distance = Hasher::perceptual_distance(&hash, &hash);
        assert_eq!(distance, Some(0));
    }

    #[test]
    fn test_perceptual_distance_invalid_hash() {
        let distance = Hasher::perceptual_distance("invalid", "also_invalid");
        assert!(distance.is_none());
    }
}
