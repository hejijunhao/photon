//! Sub-configuration structs with defaults matching the blueprint.

use crate::tagging::relevance::RelevanceConfig;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            model: "siglip-base-patch16".to_string(),
            image_size: 224,
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
}

impl Default for ThumbnailConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            size: 256,
            format: "webp".to_string(),
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

    /// Relevance pruning settings (three-pool system)
    pub relevance: RelevanceConfig,

    /// Remove ancestor tags when a more specific descendant matches.
    /// E.g., suppress "dog" when "labrador retriever" is present.
    pub deduplicate_ancestors: bool,

    /// Include hierarchy paths in tag output.
    /// E.g., "animal > dog > labrador retriever"
    pub show_paths: bool,

    /// Maximum ancestor levels to show in hierarchy paths.
    pub path_max_depth: usize,
}

impl Default for TaggingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_confidence: 0.0,
            max_tags: 15,
            vocabulary: VocabularyConfig::default(),
            progressive: ProgressiveConfig::default(),
            relevance: RelevanceConfig::default(),
            deduplicate_ancestors: false,
            show_paths: false,
            path_max_depth: 2,
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
    /// Ollama API endpoint
    pub endpoint: String,

    /// Model name
    pub model: String,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:11434".to_string(),
            model: "llama3.2-vision".to_string(),
        }
    }
}

/// Hyperbolic configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HyperbolicConfig {
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
            endpoint: "https://api.hyperbolic.xyz/v1".to_string(),
            api_key: "${HYPERBOLIC_API_KEY}".to_string(),
            model: "meta-llama/Llama-3.2-11B-Vision-Instruct".to_string(),
        }
    }
}

/// Anthropic configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicConfig {
    /// API key (supports ${ENV_VAR} syntax)
    pub api_key: String,

    /// Model name
    pub model: String,
}

impl Default for AnthropicConfig {
    fn default() -> Self {
        Self {
            api_key: "${ANTHROPIC_API_KEY}".to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
        }
    }
}

/// OpenAI configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiConfig {
    /// API key (supports ${ENV_VAR} syntax)
    pub api_key: String,

    /// Model name
    pub model: String,
}

impl Default for OpenAiConfig {
    fn default() -> Self {
        Self {
            api_key: "${OPENAI_API_KEY}".to_string(),
            model: "gpt-4o-mini".to_string(),
        }
    }
}
