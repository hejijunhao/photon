//! Ollama LLM provider for local vision model inference.
//!
//! Talks to a local Ollama instance via its HTTP API.
//! No authentication required — just needs Ollama running locally.

use super::provider::{LlmProvider, LlmRequest, LlmResponse};
use crate::error::PipelineError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// Ollama provider for local vision model inference.
pub struct OllamaProvider {
    endpoint: String,
    model: String,
    client: reqwest::Client,
}

impl OllamaProvider {
    pub fn new(endpoint: &str, model: &str) -> Self {
        Self {
            endpoint: endpoint.trim_end_matches('/').to_string(),
            model: model.to_string(),
            client: reqwest::Client::new(),
        }
    }
}

/// Ollama /api/generate request body.
#[derive(Serialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
    images: Vec<String>,
    stream: bool,
    options: OllamaOptions,
}

#[derive(Serialize)]
struct OllamaOptions {
    temperature: f32,
    num_predict: u32,
}

/// Ollama /api/generate response.
#[derive(Deserialize)]
struct OllamaResponse {
    response: String,
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    fn name(&self) -> &str {
        "ollama"
    }

    async fn is_available(&self) -> bool {
        let url = format!("{}/api/tags", self.endpoint);
        match self.client.get(&url).timeout(Duration::from_secs(5)).send().await {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }

    async fn generate(&self, request: &LlmRequest) -> Result<LlmResponse, PipelineError> {
        let url = format!("{}/api/generate", self.endpoint);
        let start = Instant::now();

        let body = OllamaRequest {
            model: self.model.clone(),
            prompt: request.prompt.clone(),
            images: vec![request.image.data.clone()],
            stream: false,
            options: OllamaOptions {
                temperature: request.temperature,
                num_predict: request.max_tokens,
            },
        };

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .timeout(self.timeout())
            .send()
            .await
            .map_err(|e| PipelineError::Llm {
                message: format!("Ollama request failed: {e}"),
                status_code: None,
            })?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(PipelineError::Llm {
                message: format!("Ollama HTTP {status}: {text}"),
                status_code: Some(status.as_u16()),
            });
        }

        let ollama_resp: OllamaResponse =
            resp.json().await.map_err(|e| PipelineError::Llm {
                message: format!("Failed to parse Ollama response: {e}"),
                status_code: None,
            })?;

        let text = ollama_resp.response.trim().to_string();
        if text.is_empty() {
            return Err(PipelineError::Llm {
                message: "Ollama returned empty response — no content generated".to_string(),
                status_code: None,
            });
        }

        Ok(LlmResponse {
            text,
            model: self.model.clone(),
            tokens_used: None, // Ollama doesn't report token counts in generate endpoint
            latency_ms: start.elapsed().as_millis() as u64,
        })
    }

    fn timeout(&self) -> Duration {
        // Vision models running locally can be slow
        Duration::from_secs(120)
    }
}
