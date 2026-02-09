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
}

impl Tag {
    /// Create a new tag with the given name and confidence.
    pub fn new(name: impl Into<String>, confidence: f32) -> Self {
        Self {
            name: name.into(),
            confidence,
            category: None,
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
        }
    }
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
