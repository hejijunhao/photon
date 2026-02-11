//! OpenAI LLM provider using the Chat Completions API.
//!
//! Sends image via data URL in the user message content array.

use super::provider::{LlmProvider, LlmRequest, LlmResponse};
use crate::error::PipelineError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// OpenAI provider using Chat Completions API.
pub struct OpenAiProvider {
    api_key: String,
    model: String,
    client: reqwest::Client,
    endpoint: String,
}

impl OpenAiProvider {
    pub fn new(api_key: &str, model: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            model: model.to_string(),
            client: reqwest::Client::new(),
            endpoint: "https://api.openai.com/v1/chat/completions".to_string(),
        }
    }

    /// Create with a custom endpoint (used by Hyperbolic provider).
    pub fn with_endpoint(api_key: &str, model: &str, endpoint: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            model: model.to_string(),
            client: reqwest::Client::new(),
            endpoint: endpoint.to_string(),
        }
    }
}

// --- Request types ---

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    max_tokens: u32,
    temperature: f32,
}

#[derive(Serialize)]
struct ChatMessage {
    role: String,
    content: Vec<ChatContent>,
}

#[derive(Serialize)]
#[serde(tag = "type")]
enum ChatContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: ImageUrl },
}

#[derive(Serialize)]
struct ImageUrl {
    url: String,
}

// --- Response types ---

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
    model: String,
    usage: Option<ChatUsage>,
}

#[derive(Deserialize)]
struct Choice {
    message: ChoiceMessage,
}

#[derive(Deserialize)]
struct ChoiceMessage {
    content: Option<String>,
}

#[derive(Deserialize)]
struct ChatUsage {
    total_tokens: u32,
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    fn name(&self) -> &str {
        "openai"
    }

    async fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }

    async fn generate(&self, request: &LlmRequest) -> Result<LlmResponse, PipelineError> {
        let start = Instant::now();

        let body = ChatRequest {
            model: self.model.clone(),
            max_tokens: request.max_tokens,
            temperature: request.temperature,
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: vec![
                    ChatContent::ImageUrl {
                        image_url: ImageUrl {
                            url: request.image.data_url(),
                        },
                    },
                    ChatContent::Text {
                        text: request.prompt.clone(),
                    },
                ],
            }],
        };

        let resp = self
            .client
            .post(&self.endpoint)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .timeout(self.timeout())
            .send()
            .await
            .map_err(|e| PipelineError::Llm {
                message: format!("OpenAI request failed: {e}"),
                status_code: None,
            })?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(PipelineError::Llm {
                message: format!("OpenAI HTTP {status}: {text}"),
                status_code: Some(status.as_u16()),
            });
        }

        let chat_resp: ChatResponse = resp.json().await.map_err(|e| PipelineError::Llm {
            message: format!("Failed to parse OpenAI response: {e}"),
            status_code: None,
        })?;

        let text = chat_resp
            .choices
            .first()
            .and_then(|c| c.message.content.clone())
            .ok_or_else(|| PipelineError::Llm {
                message: "OpenAI returned empty choices array — no content generated".to_string(),
                status_code: None,
            })?;

        Ok(LlmResponse {
            text: text.trim().to_string(),
            model: chat_resp.model,
            tokens_used: chat_resp.usage.map(|u| u.total_tokens),
            latency_ms: start.elapsed().as_millis() as u64,
        })
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(60)
    }
}
