//! Configuration validation with range checks.

use crate::error::ConfigError;

use super::Config;

impl Config {
    /// Validate configuration values are within acceptable ranges.
    pub(crate) fn validate(&self) -> Result<(), ConfigError> {
        if self.processing.parallel_workers == 0 {
            return Err(ConfigError::ValidationError(
                "processing.parallel_workers must be > 0".into(),
            ));
        }
        if self.pipeline.buffer_size == 0 {
            return Err(ConfigError::ValidationError(
                "pipeline.buffer_size must be > 0".into(),
            ));
        }
        if self.limits.max_file_size_mb == 0 {
            return Err(ConfigError::ValidationError(
                "limits.max_file_size_mb must be > 0".into(),
            ));
        }
        if self.limits.max_image_dimension == 0 {
            return Err(ConfigError::ValidationError(
                "limits.max_image_dimension must be > 0".into(),
            ));
        }
        if self.limits.decode_timeout_ms == 0 {
            return Err(ConfigError::ValidationError(
                "limits.decode_timeout_ms must be > 0".into(),
            ));
        }
        if self.limits.embed_timeout_ms == 0 {
            return Err(ConfigError::ValidationError(
                "limits.embed_timeout_ms must be > 0".into(),
            ));
        }
        if self.limits.llm_timeout_ms == 0 {
            return Err(ConfigError::ValidationError(
                "limits.llm_timeout_ms must be > 0".into(),
            ));
        }
        if self.thumbnail.size == 0 {
            return Err(ConfigError::ValidationError(
                "thumbnail.size must be > 0".into(),
            ));
        }
        if self.tagging.min_confidence < 0.0 || self.tagging.min_confidence > 1.0 {
            return Err(ConfigError::ValidationError(
                "tagging.min_confidence must be between 0.0 and 1.0".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_passes_validation() {
        let config = Config::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_rejects_zero_parallel_workers() {
        let mut config = Config::default();
        config.processing.parallel_workers = 0;
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("parallel_workers"));
    }

    #[test]
    fn test_validate_rejects_zero_thumbnail_size() {
        let mut config = Config::default();
        config.thumbnail.size = 0;
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("thumbnail.size"));
    }

    #[test]
    fn test_validate_rejects_zero_timeout() {
        let mut config = Config::default();
        config.limits.decode_timeout_ms = 0;
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("decode_timeout_ms"));
    }

    #[test]
    fn test_validate_rejects_invalid_min_confidence() {
        let mut config = Config::default();
        config.tagging.min_confidence = 1.5;
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("min_confidence"));

        config.tagging.min_confidence = -0.1;
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("min_confidence"));
    }
}
