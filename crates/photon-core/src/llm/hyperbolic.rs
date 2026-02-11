//! Hyperbolic LLM provider (OpenAI-compatible API).
//!
//! Hyperbolic uses the same Chat Completions format as OpenAI,
//! so this delegates to `OpenAiProvider` with a custom endpoint.

use super::openai::OpenAiProvider;
use super::provider::{LlmProvider, LlmRequest, LlmResponse};
use crate::error::PipelineError;
use async_trait::async_trait;
use std::time::Duration;

/// Hyperbolic provider wrapping an OpenAI-compatible endpoint.
pub struct HyperbolicProvider {
    inner: OpenAiProvider,
}

impl HyperbolicProvider {
    pub fn new(endpoint: &str, api_key: &str, model: &str) -> Self {
        let url = format!("{}/chat/completions", endpoint.trim_end_matches('/'));
        Self {
            inner: OpenAiProvider::with_endpoint(api_key, model, &url),
        }
    }
}

#[async_trait]
impl LlmProvider for HyperbolicProvider {
    fn name(&self) -> &str {
        "hyperbolic"
    }

    async fn is_available(&self) -> bool {
        self.inner.is_available().await
    }

    async fn generate(&self, request: &LlmRequest) -> Result<LlmResponse, PipelineError> {
        self.inner.generate(request).await
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(60)
    }
}
