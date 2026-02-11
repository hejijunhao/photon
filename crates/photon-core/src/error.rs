//! Error types for the Photon image processing pipeline.
//!
//! Errors are organized by stage to provide clear, actionable error messages
//! that include relevant context (file paths, stage names, specific issues).

use std::path::PathBuf;
use thiserror::Error;

/// Top-level error type for Photon operations.
#[derive(Error, Debug)]
pub enum PhotonError {
    /// Configuration-related errors
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    /// Pipeline processing errors
    #[error("Pipeline error: {0}")]
    Pipeline(#[from] PipelineError),

    /// General I/O errors
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization/deserialization errors
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Configuration-specific errors.
#[derive(Error, Debug)]
pub enum ConfigError {
    /// Failed to read the config file from disk
    #[error("Failed to read config file: {0}")]
    ReadError(#[from] std::io::Error),

    /// Failed to parse TOML configuration
    #[error("Failed to parse config: {0}")]
    ParseError(#[from] toml::de::Error),

    /// Configuration values are invalid
    #[error("Invalid configuration: {0}")]
    ValidationError(String),
}

/// Pipeline processing errors, organized by stage.
#[derive(Error, Debug)]
pub enum PipelineError {
    /// Image decoding failed
    #[error("Decode error for {path}: {message}")]
    Decode { path: PathBuf, message: String },

    /// Metadata extraction failed
    #[error("Metadata extraction failed for {path}: {message}")]
    Metadata { path: PathBuf, message: String },

    /// Embedding generation failed
    #[error("Embedding failed for {path}: {message}")]
    Embedding { path: PathBuf, message: String },

    /// Tag generation failed
    #[error("Tagging failed for {path}: {message}")]
    Tagging { path: PathBuf, message: String },

    /// Model loading or initialization failed (not per-image)
    #[error("Model error: {message}")]
    Model { message: String },

    /// LLM description generation failed.
    ///
    /// Path context is carried by `EnrichResult::Failure`, not this variant.
    #[error("LLM error: {message}")]
    Llm {
        message: String,
        /// HTTP status code from the provider response, if available.
        /// Used for structured retry classification (e.g., 429, 5xx).
        status_code: Option<u16>,
    },

    /// Operation timed out
    #[error("Timeout in {stage} stage for {path} after {timeout_ms}ms")]
    Timeout {
        path: PathBuf,
        stage: String,
        timeout_ms: u64,
    },

    /// File exceeds size limit
    #[error("File too large: {path} ({size_mb}MB > {max_mb}MB)")]
    FileTooLarge {
        path: PathBuf,
        size_mb: u64,
        max_mb: u64,
    },

    /// Image dimensions exceed limit
    #[error("Image too large: {path} ({width}x{height} > {max_dim})")]
    ImageTooLarge {
        path: PathBuf,
        width: u32,
        height: u32,
        max_dim: u32,
    },

    /// Unsupported image format
    #[error("Unsupported format for {path}: {format}")]
    UnsupportedFormat { path: PathBuf, format: String },

    /// File not found
    #[error("File not found: {0}")]
    FileNotFound(PathBuf),
}

impl PipelineError {
    /// Return a user-friendly hint for recovering from this error.
    pub fn hint(&self) -> Option<&'static str> {
        match self {
            PipelineError::Decode { .. } => {
                Some("The file may be corrupted or in an unsupported format.")
            }
            PipelineError::Model { .. } => {
                Some("Run `photon models download` to install required models.")
            }
            PipelineError::Llm { status_code, .. } => match status_code {
                Some(401) | Some(403) => {
                    Some("Check your API key. Set the appropriate environment variable (e.g. ANTHROPIC_API_KEY).")
                }
                Some(429) => Some("Rate limited by the provider. Try again later or reduce --parallel."),
                Some(500..=599) => Some("The LLM provider is experiencing issues. Try again later."),
                _ => Some("Check your LLM provider configuration with `photon config show`."),
            },
            PipelineError::Timeout { .. } => {
                Some("Try increasing the timeout in config.toml or use a simpler model.")
            }
            PipelineError::FileTooLarge { .. } => {
                Some("Increase `limits.max_file_size_mb` in config, or resize the image.")
            }
            PipelineError::ImageTooLarge { .. } => {
                Some("Increase `limits.max_image_dimension` in config, or resize the image.")
            }
            PipelineError::UnsupportedFormat { .. } => {
                Some("Supported formats: JPEG, PNG, WebP, GIF, TIFF, BMP, AVIF.")
            }
            PipelineError::FileNotFound(_) => Some("Check the file path and try again."),
            _ => None,
        }
    }
}

impl PhotonError {
    /// Return a user-friendly hint for recovering from this error.
    pub fn hint(&self) -> Option<&'static str> {
        match self {
            PhotonError::Config(_) => {
                Some("Run `photon config show` to see current configuration.")
            }
            PhotonError::Pipeline(e) => e.hint(),
            _ => None,
        }
    }
}

/// Convenience type alias for Photon results.
pub type Result<T> = std::result::Result<T, PhotonError>;

/// Convenience type alias for pipeline-specific results.
pub type PipelineResult<T> = std::result::Result<T, PipelineError>;
