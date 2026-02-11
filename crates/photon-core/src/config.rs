//! Configuration management for Photon.
//!
//! Configuration is loaded from `~/.photon/config.toml` with sensible defaults.
//! All config structs implement `Default` with values from the blueprint.

use crate::error::ConfigError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Root configuration structure for Photon.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// General settings
    pub general: GeneralConfig,

    /// Processing settings
    pub processing: ProcessingConfig,

    /// Pipeline settings
    pub pipeline: PipelineConfig,

    /// Resource limits
    pub limits: LimitsConfig,

    /// Embedding model settings
    pub embedding: EmbeddingConfig,

    /// Thumbnail generation settings
    pub thumbnail: ThumbnailConfig,

    /// Tagging settings
    pub tagging: TaggingConfig,

    /// Output settings
    pub output: OutputConfig,

    /// Logging settings
    pub logging: LoggingConfig,

    /// LLM provider settings
    pub llm: LlmConfig,
}

impl Config {
    /// Load configuration from the default location (~/.photon/config.toml).
    ///
    /// Returns default configuration if the file doesn't exist.
    pub fn load() -> Result<Self, ConfigError> {
        let path = Self::default_path();
        if path.exists() {
            Self::load_from(&path)
        } else {
            Ok(Self::default())
        }
    }

    /// Load configuration from a specific file path.
    pub fn load_from(path: &Path) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    /// Get the default config file path.
    ///
    /// Uses platform-appropriate directories:
    /// - macOS: ~/Library/Application Support/com.photon.photon/config.toml
    /// - Linux: ~/.config/photon/config.toml
    /// - Windows: C:\Users\<User>\AppData\Roaming\photon\config\config.toml
    ///
    /// Falls back to ~/.photon/config.toml if directory detection fails.
    pub fn default_path() -> PathBuf {
        directories::ProjectDirs::from("com", "photon", "photon")
            .map(|dirs| dirs.config_dir().to_path_buf().join("config.toml"))
            .unwrap_or_else(|| {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
                PathBuf::from(home).join(".photon").join("config.toml")
            })
    }

    /// Get the resolved model directory path (with ~ expansion).
    pub fn model_dir(&self) -> PathBuf {
        let path_str = self.general.model_dir.to_string_lossy();
        let expanded = shellexpand::tilde(&path_str);
        PathBuf::from(expanded.into_owned())
    }

    /// Get the resolved vocabulary directory path (with ~ expansion).
    pub fn vocabulary_dir(&self) -> PathBuf {
        let expanded = shellexpand::tilde(&self.tagging.vocabulary.dir);
        PathBuf::from(expanded.into_owned())
    }

    /// Get the taxonomy directory path (for cached label bank).
    ///
    /// Co-located with the models directory: if `model_dir` is `~/.photon/models`,
    /// taxonomy lands at `~/.photon/taxonomy`.
    pub fn taxonomy_dir(&self) -> PathBuf {
        let model_dir = self.model_dir();
        model_dir.parent().unwrap_or(&model_dir).join("taxonomy")
    }

    /// Serialize the config to a pretty TOML string.
    pub fn to_toml(&self) -> Result<String, ConfigError> {
        toml::to_string_pretty(self).map_err(|e| ConfigError::ValidationError(e.to_string()))
    }
}

/// General settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralConfig {
    /// Directory where models are stored
    pub model_dir: PathBuf,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            model_dir: PathBuf::from("~/.photon/models"),
        }
    }
}

/// Processing settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProcessingConfig {
    /// Number of parallel workers
    pub parallel_workers: usize,

    /// Supported input formats
    pub supported_formats: Vec<String>,
}

impl Default for ProcessingConfig {
    fn default() -> Self {
        Self {
            parallel_workers: 4,
            supported_formats: vec![
                "jpg".to_string(),
                "jpeg".to_string(),
                "png".to_string(),
                "webp".to_string(),
                "heic".to_string(),
                "raw".to_string(),
                "cr2".to_string(),
                "nef".to_string(),
                "arw".to_string(),
            ],
        }
    }
}

/// Pipeline settings for backpressure and retries.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PipelineConfig {
    /// Max images buffered between pipeline stages
    pub buffer_size: usize,

    /// Max retry attempts for transient failures
    pub retry_attempts: u32,

    /// Delay between retries in milliseconds
    pub retry_delay_ms: u64,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            buffer_size: 100,
            retry_attempts: 3,
            retry_delay_ms: 1000,
        }
    }
}

/// Resource limits to protect against problematic inputs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LimitsConfig {
    /// Maximum file size in megabytes
    pub max_file_size_mb: u64,

    /// Maximum image dimension (width or height)
    pub max_image_dimension: u32,

    /// Decode timeout in milliseconds
    pub decode_timeout_ms: u64,

    /// Embedding timeout in milliseconds
    pub embed_timeout_ms: u64,

    /// LLM call timeout in milliseconds
    pub llm_timeout_ms: u64,
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            max_file_size_mb: 100,
            max_image_dimension: 10000,
            decode_timeout_ms: 5000,
            embed_timeout_ms: 30000,
            llm_timeout_ms: 60000,
        }
    }
}

/// Embedding model settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EmbeddingConfig {
    /// Model name/variant ("siglip-base-patch16" or "siglip-base-patch16-384")
    pub model: String,

    /// Image input size — derived from model variant, not set directly.
    /// 224 for base, 384 for 384 variant.
    pub image_size: u32,

    /// Inference device ("cpu", "metal", "cuda")
    pub device: String,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            model: "siglip-base-patch16".to_string(),
            image_size: 224,
            device: "cpu".to_string(),
        }
    }
}

impl EmbeddingConfig {
    /// Resolve image size from model name.
    pub fn image_size_for_model(model: &str) -> u32 {
        if model.contains("384") {
            384
        } else {
            224
        }
    }
}

/// Thumbnail generation settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ThumbnailConfig {
    /// Whether to generate thumbnails
    pub enabled: bool,

    /// Thumbnail size in pixels (longest edge)
    pub size: u32,

    /// Output format
    pub format: String,

    /// Quality (0-100)
    pub quality: u8,
}

impl Default for ThumbnailConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            size: 256,
            format: "webp".to_string(),
            quality: 80,
        }
    }
}

/// Tagging settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TaggingConfig {
    /// Whether zero-shot tagging is enabled
    pub enabled: bool,

    /// Minimum confidence threshold for tags (after sigmoid).
    /// Default 0.0 disables filtering — use max_tags to limit output.
    /// SigLIP base model produces very low absolute sigmoid values;
    /// relative ordering is meaningful, not absolute confidence.
    pub min_confidence: f32,

    /// Maximum number of tags per image
    pub max_tags: usize,

    /// Vocabulary configuration
    pub vocabulary: VocabularyConfig,

    /// Progressive encoding settings for first-run optimization
    pub progressive: ProgressiveConfig,
}

impl Default for TaggingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_confidence: 0.0,
            max_tags: 15,
            vocabulary: VocabularyConfig::default(),
            progressive: ProgressiveConfig::default(),
        }
    }
}

/// Progressive encoding settings for first-run optimization.
///
/// On first run, encodes a seed set of high-value terms synchronously (~30s),
/// then background-encodes remaining terms in chunks while images are already
/// being processed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProgressiveConfig {
    /// Enable progressive encoding on first run.
    /// When false, falls back to blocking encode-all (legacy behavior).
    pub enabled: bool,

    /// Number of seed terms to encode synchronously.
    pub seed_size: usize,

    /// Number of terms per background encoding chunk.
    pub chunk_size: usize,
}

impl Default for ProgressiveConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            seed_size: 2000,
            chunk_size: 5000,
        }
    }
}

/// Vocabulary file settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VocabularyConfig {
    /// Directory containing vocabulary files
    pub dir: String,
}

impl Default for VocabularyConfig {
    fn default() -> Self {
        Self {
            dir: "~/.photon/vocabulary".to_string(),
        }
    }
}

/// Output settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OutputConfig {
    /// Default output format ("json" or "jsonl")
    pub format: String,

    /// Pretty-print JSON output
    pub pretty: bool,

    /// Include embedding vectors in output
    pub include_embedding: bool,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            format: "json".to_string(),
            pretty: false,
            include_embedding: true,
        }
    }
}

/// Logging settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LoggingConfig {
    /// Log level: error, warn, info, debug, trace
    pub level: String,

    /// Log format: "pretty" or "json"
    pub format: String,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            format: "pretty".to_string(),
        }
    }
}

/// LLM provider configurations.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct LlmConfig {
    /// Ollama (local) configuration
    pub ollama: Option<OllamaConfig>,

    /// Hyperbolic (self-hosted cloud) configuration
    pub hyperbolic: Option<HyperbolicConfig>,

    /// Anthropic configuration
    pub anthropic: Option<AnthropicConfig>,

    /// OpenAI configuration
    pub openai: Option<OpenAiConfig>,
}

/// Ollama configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
    /// Whether this provider is enabled
    pub enabled: bool,

    /// Ollama API endpoint
    pub endpoint: String,

    /// Model name
    pub model: String,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: "http://localhost:11434".to_string(),
            model: "llama3.2-vision".to_string(),
        }
    }
}

/// Hyperbolic configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HyperbolicConfig {
    /// Whether this provider is enabled
    pub enabled: bool,

    /// API endpoint
    pub endpoint: String,

    /// API key (supports ${ENV_VAR} syntax)
    pub api_key: String,

    /// Model name
    pub model: String,
}

impl Default for HyperbolicConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: "https://api.hyperbolic.xyz/v1".to_string(),
            api_key: "${HYPERBOLIC_API_KEY}".to_string(),
            model: "meta-llama/Llama-3.2-11B-Vision-Instruct".to_string(),
        }
    }
}

/// Anthropic configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicConfig {
    /// Whether this provider is enabled
    pub enabled: bool,

    /// API key (supports ${ENV_VAR} syntax)
    pub api_key: String,

    /// Model name
    pub model: String,
}

impl Default for AnthropicConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            api_key: "${ANTHROPIC_API_KEY}".to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
        }
    }
}

/// OpenAI configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiConfig {
    /// Whether this provider is enabled
    pub enabled: bool,

    /// API key (supports ${ENV_VAR} syntax)
    pub api_key: String,

    /// Model name
    pub model: String,
}

impl Default for OpenAiConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            api_key: "${OPENAI_API_KEY}".to_string(),
            model: "gpt-4o-mini".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.processing.parallel_workers, 4);
        assert_eq!(config.pipeline.buffer_size, 100);
        assert_eq!(config.limits.max_file_size_mb, 100);
    }

    #[test]
    fn test_config_to_toml() {
        let config = Config::default();
        let toml = config.to_toml().unwrap();
        assert!(toml.contains("[general]"));
        assert!(toml.contains("[processing]"));
    }

    #[test]
    fn test_progressive_config_defaults() {
        let config = ProgressiveConfig::default();
        assert!(config.enabled);
        assert_eq!(config.seed_size, 2000);
        assert_eq!(config.chunk_size, 5000);
    }

    #[test]
    fn test_tagging_config_includes_progressive() {
        let config = Config::default();
        assert!(config.tagging.progressive.enabled);
        assert_eq!(config.tagging.progressive.seed_size, 2000);
    }
}
