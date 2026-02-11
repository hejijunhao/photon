//! LLM provider trait and request/response types.
//!
//! Defines the interface that all LLM providers implement, plus the
//! factory that creates the right provider from CLI flags and config.

use crate::config::LlmConfig;
use crate::error::PipelineError;
use crate::types::Tag;
use async_trait::async_trait;
use base64::Engine;
use std::time::Duration;

/// Base64-encoded image ready to send to an LLM API.
#[derive(Debug, Clone)]
pub struct ImageInput {
    /// Base64-encoded image bytes
    pub data: String,
    /// MIME type (e.g., "image/jpeg", "image/png")
    pub media_type: String,
}

impl ImageInput {
    /// Create an `ImageInput` from raw bytes and format string.
    ///
    /// The format is the image format identifier (e.g., "jpeg", "png", "webp").
    pub fn from_bytes(bytes: &[u8], format: &str) -> Self {
        let media_type = match format {
            "jpeg" | "jpg" => "image/jpeg",
            "png" => "image/png",
            "webp" => "image/webp",
            "gif" => "image/gif",
            other => {
                tracing::warn!("Unknown image format '{other}', defaulting to image/jpeg");
                "image/jpeg"
            }
        };

        Self {
            data: base64::engine::general_purpose::STANDARD.encode(bytes),
            media_type: media_type.to_string(),
        }
    }

    /// Return a data URL suitable for OpenAI-style APIs.
    pub fn data_url(&self) -> String {
        format!("data:{};base64,{}", self.media_type, self.data)
    }
}

/// A request to generate an image description.
#[derive(Debug, Clone)]
pub struct LlmRequest {
    /// The image to describe
    pub image: ImageInput,
    /// Text prompt for the model
    pub prompt: String,
    /// Maximum tokens to generate
    pub max_tokens: u32,
    /// Sampling temperature
    pub temperature: f32,
}

impl LlmRequest {
    /// Build a description request from an image and optional tags.
    ///
    /// If tags are provided, they are included in the prompt to give the
    /// model context about what has already been detected in the image.
    pub fn describe_image(image: ImageInput, tags: &[Tag]) -> Self {
        let prompt = if tags.is_empty() {
            "Describe this image concisely in 1-3 sentences. \
             Focus on the main subject, setting, and mood."
                .to_string()
        } else {
            let tag_list: Vec<&str> = tags.iter().map(|t| t.name.as_str()).collect();
            format!(
                "Describe this image concisely in 1-3 sentences. \
                 Focus on the main subject, setting, and mood. \
                 Detected tags: {}.",
                tag_list.join(", ")
            )
        };

        Self {
            image,
            prompt,
            max_tokens: 300,
            temperature: 0.3,
        }
    }
}

/// The response from an LLM description call.
#[derive(Debug, Clone)]
pub struct LlmResponse {
    /// Generated text description
    pub text: String,
    /// Model identifier used
    pub model: String,
    /// Number of tokens used (input + output), if reported
    pub tokens_used: Option<u32>,
    /// Round-trip latency in milliseconds
    pub latency_ms: u64,
}

/// Trait that all LLM providers implement.
///
/// Uses `async_trait` because native async fn in trait is not object-safe
/// (we need `Box<dyn LlmProvider>` for dynamic dispatch).
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Provider name for logging (e.g., "anthropic", "ollama").
    fn name(&self) -> &str;

    /// Check whether the provider is configured and reachable.
    async fn is_available(&self) -> bool;

    /// Generate a description for the given request.
    async fn generate(&self, request: &LlmRequest) -> Result<LlmResponse, PipelineError>;

    /// Per-request timeout for this provider.
    fn timeout(&self) -> Duration;
}

/// Resolve `${ENV_VAR}` references in config strings.
pub fn resolve_env_var(value: &str) -> Option<String> {
    if value.starts_with("${") && value.ends_with('}') {
        let var_name = &value[2..value.len() - 1];
        std::env::var(var_name).ok()
    } else if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

/// Factory that creates the appropriate provider from CLI flags and config.
pub struct LlmProviderFactory;

impl LlmProviderFactory {
    /// Create an LLM provider based on provider name, config, and optional model override.
    ///
    /// # Arguments
    /// * `provider` - Provider identifier ("ollama", "anthropic", "openai", "hyperbolic")
    /// * `config` - The full LLM config section
    /// * `model_override` - Optional model name that overrides the config default
    pub fn create(
        provider: &str,
        config: &LlmConfig,
        model_override: Option<&str>,
    ) -> Result<Box<dyn LlmProvider>, PipelineError> {
        match provider {
            "ollama" => {
                let cfg = config.ollama.clone().unwrap_or_default();
                let model = model_override
                    .map(String::from)
                    .unwrap_or(cfg.model.clone());
                Ok(Box::new(super::ollama::OllamaProvider::new(
                    &cfg.endpoint,
                    &model,
                )))
            }
            "anthropic" => {
                let cfg = config.anthropic.clone().unwrap_or_default();
                let api_key = resolve_env_var(&cfg.api_key).ok_or_else(|| PipelineError::Llm {
                    message: "Anthropic API key not set. Set ANTHROPIC_API_KEY env var."
                        .to_string(),
                    status_code: None,
                })?;
                let model = model_override
                    .map(String::from)
                    .unwrap_or(cfg.model.clone());
                Ok(Box::new(super::anthropic::AnthropicProvider::new(
                    &api_key, &model,
                )))
            }
            "openai" => {
                let cfg = config.openai.clone().unwrap_or_default();
                let api_key = resolve_env_var(&cfg.api_key).ok_or_else(|| PipelineError::Llm {
                    message: "OpenAI API key not set. Set OPENAI_API_KEY env var.".to_string(),
                    status_code: None,
                })?;
                let model = model_override
                    .map(String::from)
                    .unwrap_or(cfg.model.clone());
                Ok(Box::new(super::openai::OpenAiProvider::new(
                    &api_key, &model,
                )))
            }
            "hyperbolic" => {
                let cfg = config.hyperbolic.clone().unwrap_or_default();
                let api_key = resolve_env_var(&cfg.api_key).ok_or_else(|| PipelineError::Llm {
                    message: "Hyperbolic API key not set. Set HYPERBOLIC_API_KEY env var."
                        .to_string(),
                    status_code: None,
                })?;
                let model = model_override
                    .map(String::from)
                    .unwrap_or(cfg.model.clone());
                Ok(Box::new(super::hyperbolic::HyperbolicProvider::new(
                    &cfg.endpoint,
                    &api_key,
                    &model,
                )))
            }
            other => Err(PipelineError::Llm {
                message: format!("Unknown LLM provider: {other}"),
                status_code: None,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_image_input_from_bytes_jpeg() {
        let input = ImageInput::from_bytes(&[0xFF, 0xD8, 0xFF], "jpeg");
        assert_eq!(input.media_type, "image/jpeg");
        assert!(!input.data.is_empty());
    }

    #[test]
    fn test_image_input_from_bytes_png() {
        let input = ImageInput::from_bytes(&[0x89, 0x50, 0x4E, 0x47], "png");
        assert_eq!(input.media_type, "image/png");
    }

    #[test]
    fn test_image_input_data_url() {
        let input = ImageInput::from_bytes(&[1, 2, 3], "jpeg");
        let url = input.data_url();
        assert!(url.starts_with("data:image/jpeg;base64,"));
    }

    #[test]
    fn test_describe_image_without_tags() {
        let image = ImageInput::from_bytes(&[1, 2, 3], "jpeg");
        let request = LlmRequest::describe_image(image, &[]);
        assert!(request.prompt.contains("Describe this image"));
        assert!(!request.prompt.contains("Detected tags"));
    }

    #[test]
    fn test_describe_image_with_tags() {
        let image = ImageInput::from_bytes(&[1, 2, 3], "jpeg");
        let tags = vec![Tag::new("beach", 0.95), Tag::new("sunset", 0.80)];
        let request = LlmRequest::describe_image(image, &tags);
        assert!(request.prompt.contains("beach, sunset"));
    }

    #[test]
    fn test_resolve_env_var() {
        // Non-env-var strings pass through
        assert_eq!(resolve_env_var("plain-key"), Some("plain-key".to_string()));
        // Empty returns None
        assert_eq!(resolve_env_var(""), None);
        // Unset env var returns None
        assert_eq!(resolve_env_var("${DEFINITELY_NOT_SET_XYZ_123}"), None);
    }
}
