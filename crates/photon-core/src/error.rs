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

    /// LLM description generation failed
    #[error("LLM error for {path}: {message}")]
    Llm { path: PathBuf, message: String },

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

/// Convenience type alias for Photon results.
pub type Result<T> = std::result::Result<T, PhotonError>;

/// Convenience type alias for pipeline-specific results.
pub type PipelineResult<T> = std::result::Result<T, PipelineError>;
