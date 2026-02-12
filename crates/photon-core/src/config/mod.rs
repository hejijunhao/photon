//! Configuration management for Photon.
//!
//! Configuration is loaded from `~/.photon/config.toml` with sensible defaults.
//! All config structs implement `Default` with values from the blueprint.

mod types;
mod validate;

pub use types::*;

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
        let mut config: Config = toml::from_str(&content)?;
        config.validate()?;
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

    #[test]
    fn test_tagging_config_hierarchy_defaults() {
        let config = TaggingConfig::default();
        assert!(!config.deduplicate_ancestors);
        assert!(!config.show_paths);
        assert_eq!(config.path_max_depth, 2);
    }

    #[test]
    fn test_tagging_config_includes_relevance() {
        let config = Config::default();
        assert!(!config.tagging.relevance.enabled); // Off by default
        assert_eq!(config.tagging.relevance.warm_check_interval, 100);
        assert!(config.tagging.relevance.neighbor_expansion);
    }
}
