# Phase 5: LLM Integration (BYOK)

> **Duration:** 2 weeks
> **Milestone:** `photon process image.jpg --llm anthropic` outputs AI-generated description

---

## Overview

This phase implements LLM integration for generating rich, natural language descriptions of images. Following the "Bring Your Own Key" (BYOK) philosophy, users can use their preferred provider: local models via Ollama, self-hosted cloud via Hyperbolic, or commercial APIs (Anthropic, OpenAI).

---

## Prerequisites

- Phase 4 completed (tagging working)
- Understanding of vision-language models
- API keys for testing (Anthropic, OpenAI) or Ollama installed locally

---

## Background: Vision-Language Models

**What they do:**
- Accept images + text prompts as input
- Generate natural language descriptions, answers, analysis
- Can follow specific formatting instructions

**Provider comparison:**

| Provider | Latency | Cost | Privacy | Quality |
|----------|---------|------|---------|---------|
| Ollama (local) | Low | Free | Full | Good |
| Hyperbolic | Medium | Low | Good | Good |
| Anthropic | Medium | Medium | Standard | Excellent |
| OpenAI | Medium | Medium | Standard | Excellent |

---

## Implementation Tasks

### 5.1 LLM Provider Trait Abstraction

**Goal:** Define a common interface for all LLM providers.

**Steps:**

1. Create `crates/photon-core/src/llm/mod.rs`:
   ```rust
   pub mod provider;
   pub mod ollama;
   pub mod hyperbolic;
   pub mod anthropic;
   pub mod openai;

   pub use provider::{LlmProvider, LlmRequest, LlmResponse, ImageInput};
   ```

2. Define the provider trait:
   ```rust
   // crates/photon-core/src/llm/provider.rs

   use async_trait::async_trait;
   use serde::{Deserialize, Serialize};
   use std::time::Duration;

   use crate::error::PipelineError;

   /// Image input for vision models
   #[derive(Debug, Clone)]
   pub struct ImageInput {
       /// Base64-encoded image data
       pub data: String,
       /// MIME type (e.g., "image/jpeg", "image/png")
       pub media_type: String,
   }

   impl ImageInput {
       pub fn from_bytes(bytes: &[u8], format: &str) -> Self {
           use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

           let media_type = match format.to_lowercase().as_str() {
               "jpeg" | "jpg" => "image/jpeg",
               "png" => "image/png",
               "webp" => "image/webp",
               "gif" => "image/gif",
               _ => "image/jpeg",
           };

           Self {
               data: BASE64.encode(bytes),
               media_type: media_type.to_string(),
           }
       }
   }

   /// Request to the LLM
   #[derive(Debug, Clone)]
   pub struct LlmRequest {
       pub image: ImageInput,
       pub prompt: String,
       pub max_tokens: Option<u32>,
       pub temperature: Option<f32>,
   }

   impl LlmRequest {
       pub fn describe_image(image: ImageInput) -> Self {
           Self {
               image,
               prompt: "Describe this image in detail. Focus on the main subjects, \
                        setting, mood, and any notable visual elements. \
                        Keep the description concise but comprehensive (2-3 sentences).".to_string(),
               max_tokens: Some(256),
               temperature: Some(0.3),
           }
       }
   }

   /// Response from the LLM
   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct LlmResponse {
       pub text: String,
       pub model: String,
       pub tokens_used: Option<u32>,
       pub latency_ms: u64,
   }

   /// LLM provider trait
   #[async_trait]
   pub trait LlmProvider: Send + Sync {
       /// Provider name for logging
       fn name(&self) -> &str;

       /// Check if the provider is available/configured
       async fn is_available(&self) -> bool;

       /// Generate a response for the given request
       async fn generate(&self, request: LlmRequest) -> Result<LlmResponse, PipelineError>;

       /// Default timeout for requests
       fn timeout(&self) -> Duration {
           Duration::from_secs(60)
       }
   }

   /// Factory for creating providers from config
   pub struct LlmProviderFactory;

   impl LlmProviderFactory {
       pub fn create(provider: &str, config: &crate::config::LlmConfig) -> Option<Box<dyn LlmProvider>> {
           match provider.to_lowercase().as_str() {
               "ollama" => config.ollama.as_ref().map(|c| {
                   Box::new(super::ollama::OllamaProvider::new(c.clone())) as Box<dyn LlmProvider>
               }),
               "hyperbolic" => config.hyperbolic.as_ref().map(|c| {
                   Box::new(super::hyperbolic::HyperbolicProvider::new(c.clone())) as Box<dyn LlmProvider>
               }),
               "anthropic" => config.anthropic.as_ref().map(|c| {
                   Box::new(super::anthropic::AnthropicProvider::new(c.clone())) as Box<dyn LlmProvider>
               }),
               "openai" => config.openai.as_ref().map(|c| {
                   Box::new(super::openai::OpenAiProvider::new(c.clone())) as Box<dyn LlmProvider>
               }),
               _ => None,
           }
       }
   }
   ```

**Acceptance Criteria:**
- [ ] Trait defines common interface
- [ ] ImageInput handles base64 encoding
- [ ] Request includes prompt customization
- [ ] Response includes metadata (tokens, latency)
- [ ] Factory creates providers from config

---

### 5.2 Ollama Integration (Local Models)

**Goal:** Implement local LLM inference via Ollama.

**Steps:**

1. Create Ollama provider:
   ```rust
   // crates/photon-core/src/llm/ollama.rs

   use async_trait::async_trait;
   use reqwest::Client;
   use serde::{Deserialize, Serialize};
   use std::time::{Duration, Instant};

   use crate::config::OllamaConfig;
   use crate::error::PipelineError;

   use super::provider::{ImageInput, LlmProvider, LlmRequest, LlmResponse};

   pub struct OllamaProvider {
       client: Client,
       config: OllamaConfig,
   }

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
       num_predict: i32,
   }

   #[derive(Deserialize)]
   struct OllamaResponse {
       response: String,
       #[serde(default)]
       eval_count: Option<u32>,
   }

   impl OllamaProvider {
       pub fn new(config: OllamaConfig) -> Self {
           let client = Client::builder()
               .timeout(Duration::from_secs(120))
               .build()
               .expect("Failed to create HTTP client");

           Self { client, config }
       }
   }

   #[async_trait]
   impl LlmProvider for OllamaProvider {
       fn name(&self) -> &str {
           "ollama"
       }

       async fn is_available(&self) -> bool {
           let url = format!("{}/api/tags", self.config.endpoint);
           self.client.get(&url).send().await.is_ok()
       }

       async fn generate(&self, request: LlmRequest) -> Result<LlmResponse, PipelineError> {
           let start = Instant::now();

           let url = format!("{}/api/generate", self.config.endpoint);

           let ollama_request = OllamaRequest {
               model: self.config.model.clone(),
               prompt: request.prompt,
               images: vec![request.image.data],
               stream: false,
               options: OllamaOptions {
                   temperature: request.temperature.unwrap_or(0.3),
                   num_predict: request.max_tokens.unwrap_or(256) as i32,
               },
           };

           let response = self.client
               .post(&url)
               .json(&ollama_request)
               .send()
               .await
               .map_err(|e| PipelineError::Llm {
                   path: std::path::PathBuf::new(),
                   message: format!("Ollama request failed: {}", e),
               })?;

           if !response.status().is_success() {
               let status = response.status();
               let body = response.text().await.unwrap_or_default();
               return Err(PipelineError::Llm {
                   path: std::path::PathBuf::new(),
                   message: format!("Ollama error {}: {}", status, body),
               });
           }

           let ollama_response: OllamaResponse = response.json().await.map_err(|e| {
               PipelineError::Llm {
                   path: std::path::PathBuf::new(),
                   message: format!("Failed to parse Ollama response: {}", e),
               }
           })?;

           Ok(LlmResponse {
               text: ollama_response.response.trim().to_string(),
               model: self.config.model.clone(),
               tokens_used: ollama_response.eval_count,
               latency_ms: start.elapsed().as_millis() as u64,
           })
       }

       fn timeout(&self) -> Duration {
           Duration::from_secs(120) // Vision models can be slow locally
       }
   }
   ```

2. Add Ollama config:
   ```rust
   // In config.rs

   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct OllamaConfig {
       pub enabled: bool,
       pub endpoint: String,
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
   ```

**Acceptance Criteria:**
- [ ] Connects to local Ollama server
- [ ] Sends image + prompt correctly
- [ ] Receives and parses response
- [ ] Handles connection errors gracefully
- [ ] `is_available()` checks server health

---

### 5.3 Hyperbolic Integration (Self-Hosted Cloud)

**Goal:** Implement Hyperbolic API for self-hosted cloud inference.

**Steps:**

1. Create Hyperbolic provider:
   ```rust
   // crates/photon-core/src/llm/hyperbolic.rs

   use async_trait::async_trait;
   use reqwest::Client;
   use serde::{Deserialize, Serialize};
   use std::time::{Duration, Instant};

   use crate::config::HyperbolicConfig;
   use crate::error::PipelineError;

   use super::provider::{ImageInput, LlmProvider, LlmRequest, LlmResponse};

   pub struct HyperbolicProvider {
       client: Client,
       config: HyperbolicConfig,
   }

   #[derive(Serialize)]
   struct HyperbolicRequest {
       model: String,
       messages: Vec<HyperbolicMessage>,
       max_tokens: u32,
       temperature: f32,
   }

   #[derive(Serialize)]
   struct HyperbolicMessage {
       role: String,
       content: Vec<HyperbolicContent>,
   }

   #[derive(Serialize)]
   #[serde(tag = "type")]
   enum HyperbolicContent {
       #[serde(rename = "text")]
       Text { text: String },
       #[serde(rename = "image_url")]
       Image { image_url: HyperbolicImageUrl },
   }

   #[derive(Serialize)]
   struct HyperbolicImageUrl {
       url: String,
   }

   #[derive(Deserialize)]
   struct HyperbolicResponse {
       choices: Vec<HyperbolicChoice>,
       usage: Option<HyperbolicUsage>,
   }

   #[derive(Deserialize)]
   struct HyperbolicChoice {
       message: HyperbolicMessageResponse,
   }

   #[derive(Deserialize)]
   struct HyperbolicMessageResponse {
       content: String,
   }

   #[derive(Deserialize)]
   struct HyperbolicUsage {
       total_tokens: u32,
   }

   impl HyperbolicProvider {
       pub fn new(config: HyperbolicConfig) -> Self {
           let client = Client::builder()
               .timeout(Duration::from_secs(90))
               .build()
               .expect("Failed to create HTTP client");

           Self { client, config }
       }

       fn resolve_api_key(&self) -> String {
           // Support environment variable references like ${HYPERBOLIC_API_KEY}
           if self.config.api_key.starts_with("${") && self.config.api_key.ends_with("}") {
               let var_name = &self.config.api_key[2..self.config.api_key.len()-1];
               std::env::var(var_name).unwrap_or_default()
           } else {
               self.config.api_key.clone()
           }
       }
   }

   #[async_trait]
   impl LlmProvider for HyperbolicProvider {
       fn name(&self) -> &str {
           "hyperbolic"
       }

       async fn is_available(&self) -> bool {
           !self.resolve_api_key().is_empty()
       }

       async fn generate(&self, request: LlmRequest) -> Result<LlmResponse, PipelineError> {
           let start = Instant::now();

           let api_key = self.resolve_api_key();
           if api_key.is_empty() {
               return Err(PipelineError::Llm {
                   path: std::path::PathBuf::new(),
                   message: "Hyperbolic API key not configured".to_string(),
               });
           }

           let url = format!("{}/chat/completions", self.config.endpoint);

           // Format image as data URL
           let image_url = format!(
               "data:{};base64,{}",
               request.image.media_type,
               request.image.data
           );

           let hyperbolic_request = HyperbolicRequest {
               model: self.config.model.clone(),
               messages: vec![HyperbolicMessage {
                   role: "user".to_string(),
                   content: vec![
                       HyperbolicContent::Image {
                           image_url: HyperbolicImageUrl { url: image_url },
                       },
                       HyperbolicContent::Text {
                           text: request.prompt,
                       },
                   ],
               }],
               max_tokens: request.max_tokens.unwrap_or(256),
               temperature: request.temperature.unwrap_or(0.3),
           };

           let response = self.client
               .post(&url)
               .header("Authorization", format!("Bearer {}", api_key))
               .header("Content-Type", "application/json")
               .json(&hyperbolic_request)
               .send()
               .await
               .map_err(|e| PipelineError::Llm {
                   path: std::path::PathBuf::new(),
                   message: format!("Hyperbolic request failed: {}", e),
               })?;

           if !response.status().is_success() {
               let status = response.status();
               let body = response.text().await.unwrap_or_default();
               return Err(PipelineError::Llm {
                   path: std::path::PathBuf::new(),
                   message: format!("Hyperbolic error {}: {}", status, body),
               });
           }

           let hyperbolic_response: HyperbolicResponse = response.json().await.map_err(|e| {
               PipelineError::Llm {
                   path: std::path::PathBuf::new(),
                   message: format!("Failed to parse Hyperbolic response: {}", e),
               }
           })?;

           let text = hyperbolic_response.choices
               .first()
               .map(|c| c.message.content.clone())
               .unwrap_or_default();

           Ok(LlmResponse {
               text: text.trim().to_string(),
               model: self.config.model.clone(),
               tokens_used: hyperbolic_response.usage.map(|u| u.total_tokens),
               latency_ms: start.elapsed().as_millis() as u64,
           })
       }
   }
   ```

**Acceptance Criteria:**
- [ ] Authenticates with API key
- [ ] Sends vision request correctly
- [ ] Parses OpenAI-compatible response
- [ ] Resolves environment variables in config
- [ ] Handles API errors

---

### 5.4 Anthropic Integration (Claude)

**Goal:** Implement Anthropic API for Claude vision models.

**Steps:**

1. Create Anthropic provider:
   ```rust
   // crates/photon-core/src/llm/anthropic.rs

   use async_trait::async_trait;
   use reqwest::Client;
   use serde::{Deserialize, Serialize};
   use std::time::{Duration, Instant};

   use crate::config::AnthropicConfig;
   use crate::error::PipelineError;

   use super::provider::{ImageInput, LlmProvider, LlmRequest, LlmResponse};

   const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
   const ANTHROPIC_VERSION: &str = "2023-06-01";

   pub struct AnthropicProvider {
       client: Client,
       config: AnthropicConfig,
   }

   #[derive(Serialize)]
   struct AnthropicRequest {
       model: String,
       max_tokens: u32,
       messages: Vec<AnthropicMessage>,
   }

   #[derive(Serialize)]
   struct AnthropicMessage {
       role: String,
       content: Vec<AnthropicContent>,
   }

   #[derive(Serialize)]
   #[serde(tag = "type")]
   enum AnthropicContent {
       #[serde(rename = "text")]
       Text { text: String },
       #[serde(rename = "image")]
       Image { source: AnthropicImageSource },
   }

   #[derive(Serialize)]
   struct AnthropicImageSource {
       #[serde(rename = "type")]
       source_type: String,
       media_type: String,
       data: String,
   }

   #[derive(Deserialize)]
   struct AnthropicResponse {
       content: Vec<AnthropicContentResponse>,
       model: String,
       usage: AnthropicUsage,
   }

   #[derive(Deserialize)]
   struct AnthropicContentResponse {
       text: Option<String>,
   }

   #[derive(Deserialize)]
   struct AnthropicUsage {
       input_tokens: u32,
       output_tokens: u32,
   }

   impl AnthropicProvider {
       pub fn new(config: AnthropicConfig) -> Self {
           let client = Client::builder()
               .timeout(Duration::from_secs(90))
               .build()
               .expect("Failed to create HTTP client");

           Self { client, config }
       }

       fn resolve_api_key(&self) -> String {
           if self.config.api_key.starts_with("${") && self.config.api_key.ends_with("}") {
               let var_name = &self.config.api_key[2..self.config.api_key.len()-1];
               std::env::var(var_name).unwrap_or_default()
           } else {
               self.config.api_key.clone()
           }
       }
   }

   #[async_trait]
   impl LlmProvider for AnthropicProvider {
       fn name(&self) -> &str {
           "anthropic"
       }

       async fn is_available(&self) -> bool {
           !self.resolve_api_key().is_empty()
       }

       async fn generate(&self, request: LlmRequest) -> Result<LlmResponse, PipelineError> {
           let start = Instant::now();

           let api_key = self.resolve_api_key();
           if api_key.is_empty() {
               return Err(PipelineError::Llm {
                   path: std::path::PathBuf::new(),
                   message: "Anthropic API key not configured".to_string(),
               });
           }

           let anthropic_request = AnthropicRequest {
               model: self.config.model.clone(),
               max_tokens: request.max_tokens.unwrap_or(256),
               messages: vec![AnthropicMessage {
                   role: "user".to_string(),
                   content: vec![
                       AnthropicContent::Image {
                           source: AnthropicImageSource {
                               source_type: "base64".to_string(),
                               media_type: request.image.media_type,
                               data: request.image.data,
                           },
                       },
                       AnthropicContent::Text {
                           text: request.prompt,
                       },
                   ],
               }],
           };

           let response = self.client
               .post(ANTHROPIC_API_URL)
               .header("x-api-key", &api_key)
               .header("anthropic-version", ANTHROPIC_VERSION)
               .header("Content-Type", "application/json")
               .json(&anthropic_request)
               .send()
               .await
               .map_err(|e| PipelineError::Llm {
                   path: std::path::PathBuf::new(),
                   message: format!("Anthropic request failed: {}", e),
               })?;

           if !response.status().is_success() {
               let status = response.status();
               let body = response.text().await.unwrap_or_default();
               return Err(PipelineError::Llm {
                   path: std::path::PathBuf::new(),
                   message: format!("Anthropic error {}: {}", status, body),
               });
           }

           let anthropic_response: AnthropicResponse = response.json().await.map_err(|e| {
               PipelineError::Llm {
                   path: std::path::PathBuf::new(),
                   message: format!("Failed to parse Anthropic response: {}", e),
               }
           })?;

           let text = anthropic_response.content
               .iter()
               .filter_map(|c| c.text.clone())
               .collect::<Vec<_>>()
               .join("");

           Ok(LlmResponse {
               text: text.trim().to_string(),
               model: anthropic_response.model,
               tokens_used: Some(anthropic_response.usage.input_tokens + anthropic_response.usage.output_tokens),
               latency_ms: start.elapsed().as_millis() as u64,
           })
       }
   }
   ```

**Acceptance Criteria:**
- [ ] Uses correct Anthropic API format
- [ ] Sends image as base64 with media type
- [ ] Parses Claude response correctly
- [ ] Includes token usage in response
- [ ] Handles rate limits gracefully

---

### 5.5 OpenAI Integration (GPT-4V)

**Goal:** Implement OpenAI API for GPT-4 Vision.

**Steps:**

1. Create OpenAI provider:
   ```rust
   // crates/photon-core/src/llm/openai.rs

   use async_trait::async_trait;
   use reqwest::Client;
   use serde::{Deserialize, Serialize};
   use std::time::{Duration, Instant};

   use crate::config::OpenAiConfig;
   use crate::error::PipelineError;

   use super::provider::{ImageInput, LlmProvider, LlmRequest, LlmResponse};

   const OPENAI_API_URL: &str = "https://api.openai.com/v1/chat/completions";

   pub struct OpenAiProvider {
       client: Client,
       config: OpenAiConfig,
   }

   #[derive(Serialize)]
   struct OpenAiRequest {
       model: String,
       messages: Vec<OpenAiMessage>,
       max_tokens: u32,
       temperature: f32,
   }

   #[derive(Serialize)]
   struct OpenAiMessage {
       role: String,
       content: Vec<OpenAiContent>,
   }

   #[derive(Serialize)]
   #[serde(tag = "type")]
   enum OpenAiContent {
       #[serde(rename = "text")]
       Text { text: String },
       #[serde(rename = "image_url")]
       Image { image_url: OpenAiImageUrl },
   }

   #[derive(Serialize)]
   struct OpenAiImageUrl {
       url: String,
       detail: String,
   }

   #[derive(Deserialize)]
   struct OpenAiResponse {
       choices: Vec<OpenAiChoice>,
       model: String,
       usage: Option<OpenAiUsage>,
   }

   #[derive(Deserialize)]
   struct OpenAiChoice {
       message: OpenAiMessageResponse,
   }

   #[derive(Deserialize)]
   struct OpenAiMessageResponse {
       content: String,
   }

   #[derive(Deserialize)]
   struct OpenAiUsage {
       total_tokens: u32,
   }

   impl OpenAiProvider {
       pub fn new(config: OpenAiConfig) -> Self {
           let client = Client::builder()
               .timeout(Duration::from_secs(90))
               .build()
               .expect("Failed to create HTTP client");

           Self { client, config }
       }

       fn resolve_api_key(&self) -> String {
           if self.config.api_key.starts_with("${") && self.config.api_key.ends_with("}") {
               let var_name = &self.config.api_key[2..self.config.api_key.len()-1];
               std::env::var(var_name).unwrap_or_default()
           } else {
               self.config.api_key.clone()
           }
       }
   }

   #[async_trait]
   impl LlmProvider for OpenAiProvider {
       fn name(&self) -> &str {
           "openai"
       }

       async fn is_available(&self) -> bool {
           !self.resolve_api_key().is_empty()
       }

       async fn generate(&self, request: LlmRequest) -> Result<LlmResponse, PipelineError> {
           let start = Instant::now();

           let api_key = self.resolve_api_key();
           if api_key.is_empty() {
               return Err(PipelineError::Llm {
                   path: std::path::PathBuf::new(),
                   message: "OpenAI API key not configured".to_string(),
               });
           }

           // Format image as data URL
           let image_url = format!(
               "data:{};base64,{}",
               request.image.media_type,
               request.image.data
           );

           let openai_request = OpenAiRequest {
               model: self.config.model.clone(),
               messages: vec![OpenAiMessage {
                   role: "user".to_string(),
                   content: vec![
                       OpenAiContent::Image {
                           image_url: OpenAiImageUrl {
                               url: image_url,
                               detail: "auto".to_string(),
                           },
                       },
                       OpenAiContent::Text {
                           text: request.prompt,
                       },
                   ],
               }],
               max_tokens: request.max_tokens.unwrap_or(256),
               temperature: request.temperature.unwrap_or(0.3),
           };

           let response = self.client
               .post(OPENAI_API_URL)
               .header("Authorization", format!("Bearer {}", api_key))
               .header("Content-Type", "application/json")
               .json(&openai_request)
               .send()
               .await
               .map_err(|e| PipelineError::Llm {
                   path: std::path::PathBuf::new(),
                   message: format!("OpenAI request failed: {}", e),
               })?;

           if !response.status().is_success() {
               let status = response.status();
               let body = response.text().await.unwrap_or_default();
               return Err(PipelineError::Llm {
                   path: std::path::PathBuf::new(),
                   message: format!("OpenAI error {}: {}", status, body),
               });
           }

           let openai_response: OpenAiResponse = response.json().await.map_err(|e| {
               PipelineError::Llm {
                   path: std::path::PathBuf::new(),
                   message: format!("Failed to parse OpenAI response: {}", e),
               }
           })?;

           let text = openai_response.choices
               .first()
               .map(|c| c.message.content.clone())
               .unwrap_or_default();

           Ok(LlmResponse {
               text: text.trim().to_string(),
               model: openai_response.model,
               tokens_used: openai_response.usage.map(|u| u.total_tokens),
               latency_ms: start.elapsed().as_millis() as u64,
           })
       }
   }
   ```

**Acceptance Criteria:**
- [ ] Uses OpenAI chat completions format
- [ ] Sends image as data URL
- [ ] Parses response correctly
- [ ] Includes model and usage info
- [ ] Handles API errors

---

### 5.6 Retry Logic for Transient Failures

**Goal:** Implement retry with backoff for network/API errors.

**Steps:**

1. Create retry wrapper:
   ```rust
   // crates/photon-core/src/llm/retry.rs

   use std::time::Duration;
   use tokio::time::sleep;

   use crate::config::PipelineConfig;
   use crate::error::PipelineError;

   use super::provider::{LlmProvider, LlmRequest, LlmResponse};

   pub struct RetryingProvider<P: LlmProvider> {
       inner: P,
       max_attempts: u32,
       delay_ms: u64,
   }

   impl<P: LlmProvider> RetryingProvider<P> {
       pub fn new(inner: P, config: &PipelineConfig) -> Self {
           Self {
               inner,
               max_attempts: config.retry_attempts,
               delay_ms: config.retry_delay_ms,
           }
       }

       pub async fn generate(&self, request: LlmRequest) -> Result<LlmResponse, PipelineError> {
           let mut last_error = None;

           for attempt in 1..=self.max_attempts {
               match self.inner.generate(request.clone()).await {
                   Ok(response) => return Ok(response),
                   Err(e) => {
                       // Check if error is retryable
                       if Self::is_retryable(&e) && attempt < self.max_attempts {
                           tracing::warn!(
                               "LLM request failed (attempt {}/{}): {}. Retrying...",
                               attempt, self.max_attempts, e
                           );
                           sleep(Duration::from_millis(self.delay_ms * attempt as u64)).await;
                           last_error = Some(e);
                       } else {
                           return Err(e);
                       }
                   }
               }
           }

           Err(last_error.unwrap_or_else(|| PipelineError::Llm {
               path: std::path::PathBuf::new(),
               message: "All retry attempts failed".to_string(),
           }))
       }

       fn is_retryable(error: &PipelineError) -> bool {
           match error {
               PipelineError::Llm { message, .. } => {
                   // Retry on network errors, rate limits, server errors
                   message.contains("timeout") ||
                   message.contains("connection") ||
                   message.contains("429") ||  // Rate limit
                   message.contains("500") ||  // Server error
                   message.contains("502") ||
                   message.contains("503") ||
                   message.contains("504")
               }
               _ => false,
           }
       }
   }
   ```

**Acceptance Criteria:**
- [ ] Retries on network errors
- [ ] Retries on rate limits (429)
- [ ] Retries on server errors (5xx)
- [ ] Exponential backoff between retries
- [ ] Does not retry on auth errors (401, 403)

---

### 5.7 LLM Timeout

**Goal:** Implement timeout for LLM requests.

**Steps:**

1. Add timeout wrapper:
   ```rust
   use std::time::Duration;
   use tokio::time::timeout;

   impl<P: LlmProvider> RetryingProvider<P> {
       pub async fn generate_with_timeout(
           &self,
           request: LlmRequest,
           timeout_ms: u64,
       ) -> Result<LlmResponse, PipelineError> {
           let timeout_duration = Duration::from_millis(timeout_ms);

           match timeout(timeout_duration, self.generate(request)).await {
               Ok(result) => result,
               Err(_) => Err(PipelineError::Timeout {
                   path: std::path::PathBuf::new(),
                   stage: "llm".to_string(),
                   timeout_ms,
               }),
           }
       }
   }
   ```

**Acceptance Criteria:**
- [ ] Timeout triggers after configured duration
- [ ] Timeout produces clear error
- [ ] Partial responses are discarded

---

### 5.8 Integrate LLM into Pipeline

**Goal:** Wire LLM descriptions into the image processing pipeline.

**Steps:**

1. Update `ImageProcessor`:
   ```rust
   // In crates/photon-core/src/pipeline/processor.rs

   use crate::llm::{LlmProvider, LlmProviderFactory, LlmRequest, ImageInput};
   use crate::llm::retry::RetryingProvider;

   pub struct ImageProcessor {
       decoder: ImageDecoder,
       thumbnail_gen: ThumbnailGenerator,
       validator: Validator,
       embedder: Arc<SigLipEmbedder>,
       tagger: Option<Arc<ZeroShotTagger>>,
       llm_provider: Option<Box<dyn LlmProvider>>,
       llm_timeout_ms: u64,
   }

   pub struct ProcessOptions {
       pub generate_thumbnail: bool,
       pub use_llm: Option<String>,  // Provider name
       pub llm_model: Option<String>,
   }

   impl ImageProcessor {
       pub async fn new(config: &Config) -> Result<Self> {
           // ... existing initialization ...

           // Initialize LLM provider if configured
           let llm_provider = None; // Set via process options

           Ok(Self {
               // ...
               llm_provider,
               llm_timeout_ms: config.limits.llm_timeout_ms,
           })
       }

       /// Set LLM provider for this processor
       pub fn with_llm(mut self, provider: &str, config: &crate::config::LlmConfig) -> Self {
           self.llm_provider = LlmProviderFactory::create(provider, config);
           self
       }

       pub async fn process(&self, path: &Path) -> Result<ProcessedImage> {
           // ... existing pipeline stages ...

           // Generate LLM description if configured
           let description = if let Some(provider) = &self.llm_provider {
               self.generate_description(provider.as_ref(), &decoded.image, path).await?
           } else {
               None
           };

           // ...

           Ok(ProcessedImage {
               // ...
               description,
               // ...
           })
       }

       async fn generate_description(
           &self,
           provider: &dyn LlmProvider,
           image: &DynamicImage,
           path: &Path,
       ) -> Result<Option<String>> {
           // Encode image for LLM
           let mut buffer = std::io::Cursor::new(Vec::new());
           image.write_to(&mut buffer, image::ImageFormat::Jpeg)
               .map_err(|e| PipelineError::Llm {
                   path: path.to_path_buf(),
                   message: format!("Failed to encode image: {}", e),
               })?;

           let image_input = ImageInput::from_bytes(buffer.get_ref(), "jpeg");
           let request = LlmRequest::describe_image(image_input);

           match tokio::time::timeout(
               Duration::from_millis(self.llm_timeout_ms),
               provider.generate(request),
           ).await {
               Ok(Ok(response)) => {
                   tracing::debug!(
                       "LLM description generated in {}ms ({} tokens)",
                       response.latency_ms,
                       response.tokens_used.unwrap_or(0)
                   );
                   Ok(Some(response.text))
               }
               Ok(Err(e)) => {
                   tracing::warn!("LLM description failed: {}", e);
                   Ok(None) // Don't fail the whole image
               }
               Err(_) => {
                   tracing::warn!("LLM request timed out after {}ms", self.llm_timeout_ms);
                   Ok(None)
               }
           }
       }
   }
   ```

2. Update CLI to support LLM flags:
   ```rust
   // In cli/process.rs

   pub async fn execute(args: ProcessArgs) -> anyhow::Result<()> {
       let config = Config::load()?;
       let mut processor = ImageProcessor::new(&config).await?;

       // Configure LLM if requested
       if let Some(provider) = &args.llm {
           processor = processor.with_llm(provider, &config.llm);
           tracing::info!("Using LLM provider: {}", provider);
       }

       // ... rest of execution
   }
   ```

**Acceptance Criteria:**
- [ ] `--llm anthropic` enables Claude descriptions
- [ ] `--llm ollama` uses local Ollama
- [ ] Description appears in JSON output
- [ ] LLM failures don't fail the entire image
- [ ] Timeout is respected

---

## Integration Tests

```rust
#[tokio::test]
async fn test_ollama_provider() {
    let config = OllamaConfig {
        enabled: true,
        endpoint: "http://localhost:11434".to_string(),
        model: "llama3.2-vision".to_string(),
    };

    let provider = OllamaProvider::new(config);

    // Skip if Ollama not running
    if !provider.is_available().await {
        println!("Skipping: Ollama not available");
        return;
    }

    let image = std::fs::read("tests/fixtures/images/test.jpg").unwrap();
    let request = LlmRequest::describe_image(
        ImageInput::from_bytes(&image, "jpeg")
    );

    let response = provider.generate(request).await.unwrap();

    assert!(!response.text.is_empty());
    assert!(response.latency_ms > 0);
}

#[tokio::test]
async fn test_anthropic_provider() {
    let api_key = std::env::var("ANTHROPIC_API_KEY").ok();
    if api_key.is_none() {
        println!("Skipping: ANTHROPIC_API_KEY not set");
        return;
    }

    let config = AnthropicConfig {
        enabled: true,
        api_key: api_key.unwrap(),
        model: "claude-sonnet-4-20250514".to_string(),
    };

    let provider = AnthropicProvider::new(config);
    let image = std::fs::read("tests/fixtures/images/test.jpg").unwrap();
    let request = LlmRequest::describe_image(
        ImageInput::from_bytes(&image, "jpeg")
    );

    let response = provider.generate(request).await.unwrap();

    assert!(!response.text.is_empty());
    assert!(response.tokens_used.is_some());
}

#[tokio::test]
async fn test_llm_in_pipeline() {
    let mut config = Config::default();
    config.llm.anthropic = Some(AnthropicConfig {
        enabled: true,
        api_key: "${ANTHROPIC_API_KEY}".to_string(),
        model: "claude-sonnet-4-20250514".to_string(),
    });

    // Skip if no API key
    if std::env::var("ANTHROPIC_API_KEY").is_err() {
        return;
    }

    let processor = ImageProcessor::new(&config).await.unwrap()
        .with_llm("anthropic", &config.llm);

    let result = processor.process(Path::new("tests/fixtures/images/test.jpg")).await;
    let image = result.unwrap();

    assert!(image.description.is_some());
    let desc = image.description.unwrap();
    assert!(!desc.is_empty());
}
```

---

## Verification Checklist

Before moving to Phase 6:

- [ ] `photon process image.jpg --llm ollama` works with local Ollama
- [ ] `photon process image.jpg --llm anthropic` works with Claude
- [ ] `photon process image.jpg --llm openai` works with GPT-4V
- [ ] `photon process image.jpg --llm hyperbolic` works with Hyperbolic
- [ ] Description appears in JSON output
- [ ] LLM failures don't crash the pipeline
- [ ] Timeout works correctly
- [ ] Retry logic handles transient failures
- [ ] Environment variable API keys work
- [ ] `--no-description` skips LLM even if configured
- [ ] All integration tests pass

---

## Files Created/Modified

```
crates/photon-core/src/
├── llm/
│   ├── mod.rs           # Module exports
│   ├── provider.rs      # Provider trait
│   ├── ollama.rs        # Ollama implementation
│   ├── hyperbolic.rs    # Hyperbolic implementation
│   ├── anthropic.rs     # Anthropic implementation
│   ├── openai.rs        # OpenAI implementation
│   └── retry.rs         # Retry logic
├── config.rs            # Updated with LLM configs
└── pipeline/
    └── processor.rs     # Updated with LLM integration

crates/photon/src/cli/
└── process.rs           # Updated with --llm flag
```

---

## Notes

- LLM descriptions add significant latency (1-5 seconds per image)
- Consider making descriptions optional/async for batch processing
- Keep descriptions concise to manage token costs
- Store the model used for reproducibility
- Consider caching descriptions for the same content_hash
- For Ollama, recommend llama3.2-vision or similar vision-capable models
