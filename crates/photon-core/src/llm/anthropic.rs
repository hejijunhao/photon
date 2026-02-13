//! Anthropic LLM provider using the Messages API.
//!
//! Sends image + prompt via the Anthropic Messages API with base64 image content blocks.

use super::provider::{LlmProvider, LlmRequest, LlmResponse};
use crate::error::PipelineError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// Anthropic provider using the Messages API.
pub struct AnthropicProvider {
    api_key: String,
    model: String,
    client: reqwest::Client,
}

impl AnthropicProvider {
    pub fn new(api_key: &str, model: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            model: model.to_string(),
            client: reqwest::Client::new(),
        }
    }
}

// --- Request types ---

#[derive(Serialize)]
struct MessagesRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Serialize)]
struct Message {
    role: String,
    content: Vec<ContentBlock>,
}

#[derive(Serialize)]
#[serde(tag = "type")]
enum ContentBlock {
    #[serde(rename = "image")]
    Image { source: ImageSource },
    #[serde(rename = "text")]
    Text { text: String },
}

#[derive(Serialize)]
struct ImageSource {
    #[serde(rename = "type")]
    source_type: String,
    media_type: String,
    data: String,
}

// --- Response types ---

#[derive(Deserialize)]
struct MessagesResponse {
    content: Vec<ResponseContent>,
    model: String,
    usage: Usage,
}

#[derive(Deserialize)]
struct ResponseContent {
    text: Option<String>,
}

#[derive(Deserialize)]
struct Usage {
    input_tokens: u32,
    output_tokens: u32,
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    async fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }

    async fn generate(&self, request: &LlmRequest) -> Result<LlmResponse, PipelineError> {
        let start = Instant::now();

        let body = MessagesRequest {
            model: self.model.clone(),
            max_tokens: request.max_tokens,
            temperature: Some(request.temperature),
            messages: vec![Message {
                role: "user".to_string(),
                content: vec![
                    ContentBlock::Image {
                        source: ImageSource {
                            source_type: "base64".to_string(),
                            media_type: request.image.media_type.clone(),
                            data: request.image.data.clone(),
                        },
                    },
                    ContentBlock::Text {
                        text: request.prompt.clone(),
                    },
                ],
            }],
        };

        let resp = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| PipelineError::Llm {
                message: format!("Anthropic request failed: {e}"),
                status_code: None,
            })?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(PipelineError::Llm {
                message: format!("Anthropic HTTP {status}: {text}"),
                status_code: Some(status.as_u16()),
            });
        }

        let messages_resp: MessagesResponse =
            resp.json().await.map_err(|e| PipelineError::Llm {
                message: format!("Failed to parse Anthropic response: {e}"),
                status_code: None,
            })?;

        let text = messages_resp
            .content
            .into_iter()
            .filter_map(|c| c.text)
            .collect::<Vec<_>>()
            .join("");

        let text = text.trim().to_string();
        if text.is_empty() {
            return Err(PipelineError::Llm {
                message: "Anthropic returned empty response — no text content generated"
                    .to_string(),
                status_code: None,
            });
        }

        Ok(LlmResponse {
            text,
            model: messages_resp.model,
            tokens_used: Some(messages_resp.usage.input_tokens + messages_resp.usage.output_tokens),
            latency_ms: start.elapsed().as_millis() as u64,
        })
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(60)
    }
}
