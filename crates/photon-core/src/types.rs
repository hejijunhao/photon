//! Core data types for the Photon image processing pipeline.
//!
//! These types represent the output of processing an image through the pipeline.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The complete output for a processed image.
///
/// This struct contains all the data extracted and generated from an image,
/// including file identification, vector embeddings, metadata, tags, and descriptions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessedImage {
    // === File Identification ===
    /// Absolute path to the source file
    pub file_path: PathBuf,

    /// Just the filename portion
    pub file_name: String,

    /// BLAKE3 hash for content-based deduplication
    pub content_hash: String,

    // === Image Properties ===
    /// Image width in pixels
    pub width: u32,

    /// Image height in pixels
    pub height: u32,

    /// Detected format ("jpeg", "png", "webp", etc.)
    pub format: String,

    /// File size in bytes
    pub file_size: u64,

    // === Vector Embedding ===
    /// 768-dimensional embedding vector from SigLIP
    pub embedding: Vec<f32>,

    // === Metadata ===
    /// EXIF data if available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exif: Option<ExifData>,

    // === AI-Generated Content ===
    /// Semantic tags with confidence scores
    pub tags: Vec<Tag>,

    /// LLM-generated description (if enabled)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    // === Optional Outputs ===
    /// Base64-encoded WebP thumbnail
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,

    /// Perceptual hash for similarity detection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub perceptual_hash: Option<String>,
}

/// EXIF metadata extracted from an image.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExifData {
    /// When the photo was captured (ISO 8601 format)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub captured_at: Option<String>,

    /// Camera manufacturer
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera_make: Option<String>,

    /// Camera model
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera_model: Option<String>,

    /// GPS latitude (decimal degrees)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gps_latitude: Option<f64>,

    /// GPS longitude (decimal degrees)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gps_longitude: Option<f64>,

    /// ISO sensitivity
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iso: Option<u32>,

    /// Aperture (e.g., "f/1.8")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aperture: Option<String>,

    /// Shutter speed (e.g., "1/1000")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shutter_speed: Option<String>,

    /// Focal length in mm
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focal_length: Option<f32>,

    /// Image orientation (1-8 per EXIF spec)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orientation: Option<u32>,
}

/// A semantic tag with confidence score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    /// The tag label (e.g., "beach", "sunset", "portrait")
    pub name: String,

    /// Confidence score from 0.0 to 1.0
    pub confidence: f32,

    /// Optional category ("object", "scene", "color", "style", etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,

    /// Optional hierarchy path (e.g., "animal > dog > labrador retriever")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl Tag {
    /// Create a new tag with the given name and confidence.
    pub fn new(name: impl Into<String>, confidence: f32) -> Self {
        Self {
            name: name.into(),
            confidence,
            category: None,
            path: None,
        }
    }

    /// Create a new tag with category.
    pub fn with_category(
        name: impl Into<String>,
        confidence: f32,
        category: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            confidence,
            category: Some(category.into()),
            path: None,
        }
    }
}

/// Lightweight patch emitted by the LLM enrichment pass.
/// Keyed by content_hash so the consumer can join with the core record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichmentPatch {
    pub content_hash: String,
    pub description: String,
    pub llm_model: String,
    pub llm_latency_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm_tokens: Option<u32>,
}

/// Tagged union for dual-stream output when --llm is enabled.
/// Internally tagged: `{"type":"core",...}` or `{"type":"enrichment",...}`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum OutputRecord {
    Core(Box<ProcessedImage>),
    Enrichment(EnrichmentPatch),
}

/// Processing statistics for a batch run.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProcessingStats {
    /// Total images processed successfully
    pub succeeded: usize,

    /// Total images that failed
    pub failed: usize,

    /// Total images skipped (already processed)
    pub skipped: usize,

    /// Processing rate in images per second
    pub images_per_second: f64,

    /// Total processing time in seconds
    pub total_seconds: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sample_processed_image() -> ProcessedImage {
        ProcessedImage {
            file_path: PathBuf::from("/photos/beach.jpg"),
            file_name: "beach.jpg".to_string(),
            content_hash: "abc123".to_string(),
            width: 1920,
            height: 1080,
            format: "jpeg".to_string(),
            file_size: 2048,
            embedding: vec![0.1, 0.2, 0.3],
            exif: None,
            tags: vec![Tag::new("beach", 0.95)],
            description: None,
            thumbnail: None,
            perceptual_hash: None,
        }
    }

    #[test]
    fn test_output_record_core_roundtrip() {
        let record = OutputRecord::Core(Box::new(sample_processed_image()));
        let json = serde_json::to_string(&record).unwrap();

        // Verify the "type" field is present
        assert!(json.contains("\"type\":\"core\""));
        assert!(json.contains("\"content_hash\":\"abc123\""));

        // Roundtrip
        let parsed: OutputRecord = serde_json::from_str(&json).unwrap();
        match parsed {
            OutputRecord::Core(img) => {
                assert_eq!(img.content_hash, "abc123");
                assert_eq!(img.file_name, "beach.jpg");
            }
            _ => panic!("Expected Core variant"),
        }
    }

    #[test]
    fn test_output_record_enrichment_roundtrip() {
        let patch = EnrichmentPatch {
            content_hash: "abc123".to_string(),
            description: "A sandy tropical beach".to_string(),
            llm_model: "claude-sonnet-4-5-20250929".to_string(),
            llm_latency_ms: 2100,
            llm_tokens: Some(150),
        };
        let record = OutputRecord::Enrichment(patch);
        let json = serde_json::to_string(&record).unwrap();

        assert!(json.contains("\"type\":\"enrichment\""));
        assert!(json.contains("\"description\":\"A sandy tropical beach\""));
        assert!(json.contains("\"llm_tokens\":150"));

        let parsed: OutputRecord = serde_json::from_str(&json).unwrap();
        match parsed {
            OutputRecord::Enrichment(p) => {
                assert_eq!(p.content_hash, "abc123");
                assert_eq!(p.llm_latency_ms, 2100);
            }
            _ => panic!("Expected Enrichment variant"),
        }
    }

    #[test]
    fn test_tag_serde_without_path() {
        let tag = Tag::new("beach", 0.95);
        let json = serde_json::to_string(&tag).unwrap();
        assert!(!json.contains("path"));
        let parsed: Tag = serde_json::from_str(&json).unwrap();
        assert!(parsed.path.is_none());
    }

    #[test]
    fn test_tag_serde_with_path() {
        let mut tag = Tag::new("labrador retriever", 0.87);
        tag.path = Some("animal > dog > labrador retriever".to_string());
        let json = serde_json::to_string(&tag).unwrap();
        assert!(json.contains("\"path\":\"animal > dog > labrador retriever\""));
        let parsed: Tag = serde_json::from_str(&json).unwrap();
        assert_eq!(
            parsed.path.as_deref(),
            Some("animal > dog > labrador retriever")
        );
    }

    #[test]
    fn test_enrichment_patch_skips_none_tokens() {
        let patch = EnrichmentPatch {
            content_hash: "def456".to_string(),
            description: "A dog".to_string(),
            llm_model: "gpt-4o-mini".to_string(),
            llm_latency_ms: 800,
            llm_tokens: None,
        };
        let json = serde_json::to_string(&patch).unwrap();
        assert!(!json.contains("llm_tokens"));
    }
}
