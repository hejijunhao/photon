# Phase 5: LLM Enrichment (BYOK)

> **Duration:** 2 weeks
> **Milestone:** `photon enrich results.jsonl --llm anthropic` adds AI-generated descriptions to processed images

---

## Overview

This phase implements LLM integration for generating rich, natural language **descriptions** of images. The LLM is a **completely separate enrichment step** — it runs after `photon process`, not during it. The fast pipeline (decode, embed, tag) never blocks on the network.

Following the "Bring Your Own Key" (BYOK) philosophy, users can use their preferred provider: local models via Ollama, self-hosted cloud via Hyperbolic, or commercial APIs (Anthropic, OpenAI).

**Two-command architecture:**

```
STEP 1: Fast pipeline (Phase 1-4) — no network, no LLM
═══════════════════════════════════════════════════════
photon process ./photos/ --output results.jsonl

  → 10,000 images in ~3 hours
  → Each image: decode, hash, embed, tag (~50ms + ~200ms SigLIP)
  → Output: JSONL with tags, embedding, metadata, description: null


STEP 2: LLM enrichment (Phase 5) — separate, optional, retryable
═════════════════════════════════════════════════════════════════
photon enrich results.jsonl --llm anthropic --output enriched.jsonl

  → Reads JSONL, loads each image from file_path
  → Sends image + tags (as context) to LLM
  → Emits enrichment patches keyed by content_hash
  → Consuming app does: UPDATE images SET description = '...' WHERE content_hash = 'abc'
```

**Why two commands instead of inline?**
- The fast pipeline never blocks on network I/O (2-5s per LLM call)
- Enrichment is retryable — if it fails halfway, re-run and skip already-enriched
- Different LLM providers for different runs (cheap local pass, then re-enrich low-confidence with Claude)
- Tags feed the LLM as context — by the time enrichment runs, we already know what's in the image
- Users who don't want LLM descriptions never even think about it

**Relationship to Phase 4 (Tagging):**
- **Tags** (Phase 4): Fast (~1-5ms), local, structured, individual terms — powered by SigLIP + WordNet vocabulary
- **Descriptions** (Phase 5): Slow (1-5s), requires LLM, free-form sentences — optional enrichment
- Descriptions use tags as context to produce more focused, accurate output

---

## Prerequisites

- Phase 4a completed (tagging working — tags available for LLM context)
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
   pub mod retry;
   pub mod enricher;

   pub use provider::{LlmProvider, LlmRequest, LlmResponse, ImageInput};
   pub use enricher::{Enricher, EnrichmentPatch};
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
       /// Create a description request that includes tags as context.
       /// Tags from Phase 4's SigLIP pipeline are fed to the LLM to
       /// help it focus on what's actually in the image.
       pub fn describe_image(image: ImageInput, tags: &[String]) -> Self {
           let tag_context = if tags.is_empty() {
               String::new()
           } else {
               format!("\n\nDetected subjects: {}.", tags.join(", "))
           };

           Self {
               image,
               prompt: format!(
                   "Describe this image in detail. Focus on the main subjects, \
                    setting, mood, and any notable visual elements. \
                    Keep the description concise but comprehensive (2-3 sentences).{}",
                   tag_context
               ),
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
- [ ] Request always includes tags as context
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

### 5.6 Retry Logic and Timeout

**Goal:** Implement retry with backoff and timeout for network/API errors.

**Steps:**

1. Create retry wrapper:
   ```rust
   // crates/photon-core/src/llm/retry.rs

   use std::time::Duration;
   use tokio::time::sleep;

   use crate::error::PipelineError;

   use super::provider::{LlmProvider, LlmRequest, LlmResponse};

   pub struct RetryingProvider<P: LlmProvider> {
       inner: P,
       max_attempts: u32,
       delay_ms: u64,
       timeout_ms: u64,
   }

   impl<P: LlmProvider> RetryingProvider<P> {
       pub fn new(inner: P, max_attempts: u32, delay_ms: u64, timeout_ms: u64) -> Self {
           Self { inner, max_attempts, delay_ms, timeout_ms }
       }

       pub async fn generate(&self, request: LlmRequest) -> Result<LlmResponse, PipelineError> {
           let mut last_error = None;

           for attempt in 1..=self.max_attempts {
               let timeout_duration = Duration::from_millis(self.timeout_ms);

               match tokio::time::timeout(timeout_duration, self.inner.generate(request.clone())).await {
                   Ok(Ok(response)) => return Ok(response),
                   Ok(Err(e)) => {
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
                   Err(_) => {
                       let e = PipelineError::Timeout {
                           path: std::path::PathBuf::new(),
                           stage: "llm".to_string(),
                           timeout_ms: self.timeout_ms,
                       };
                       if attempt < self.max_attempts {
                           tracing::warn!("LLM request timed out (attempt {}/{})", attempt, self.max_attempts);
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
                   message.contains("timeout") ||
                   message.contains("connection") ||
                   message.contains("429") ||
                   message.contains("500") ||
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
- [ ] Retries on network errors, rate limits (429), server errors (5xx)
- [ ] Exponential backoff between retries
- [ ] Does not retry on auth errors (401, 403)
- [ ] Timeout triggers after configured duration
- [ ] Timeout errors are retryable

---

### 5.7 Enricher: The Core Enrichment Engine

**Goal:** Implement the enrichment engine that reads processed JSONL, sends images to LLM, and emits description patches.

This is the **key architectural piece** — it replaces the old inline-LLM-in-pipeline approach with a separate, retryable enrichment pass.

**Steps:**

1. Define the enrichment patch:
   ```rust
   // crates/photon-core/src/llm/enricher.rs

   use std::path::{Path, PathBuf};
   use std::io::{BufRead, BufReader, Write, BufWriter};
   use std::sync::Arc;

   use serde::{Deserialize, Serialize};

   use crate::error::PipelineError;
   use crate::types::ProcessedImage;

   use super::provider::{ImageInput, LlmProvider, LlmRequest};
   use super::retry::RetryingProvider;

   /// A lightweight patch emitted by the enrichment pass.
   /// Keyed by content_hash so the consuming application can
   /// UPDATE ... SET description = '...' WHERE content_hash = '...'
   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct EnrichmentPatch {
       pub content_hash: String,
       pub file_path: PathBuf,
       pub description: String,
       pub llm_model: String,
       pub llm_tokens: Option<u32>,
       pub llm_latency_ms: u64,
   }

   pub struct EnrichOptions {
       pub parallel: usize,
       pub skip_existing: bool,
       pub timeout_ms: u64,
       pub retry_attempts: u32,
       pub retry_delay_ms: u64,
   }

   impl Default for EnrichOptions {
       fn default() -> Self {
           Self {
               parallel: 4,
               skip_existing: true,
               timeout_ms: 60_000,
               retry_attempts: 3,
               retry_delay_ms: 1_000,
           }
       }
   }

   pub struct Enricher {
       provider: Arc<dyn LlmProvider>,
       options: EnrichOptions,
   }

   impl Enricher {
       pub fn new(provider: Box<dyn LlmProvider>, options: EnrichOptions) -> Self {
           Self {
               provider: Arc::from(provider),
               options,
           }
       }

       /// Run enrichment on a JSONL file of ProcessedImages.
       ///
       /// Reads each line, loads the image from file_path, sends it to the LLM
       /// with tags as context, and emits an EnrichmentPatch per image.
       ///
       /// Returns (succeeded, failed, skipped) counts.
       pub async fn enrich_file(
           &self,
           input_path: &Path,
           output: &mut dyn Write,
           existing_hashes: &std::collections::HashSet<String>,
       ) -> Result<(u64, u64, u64), PipelineError> {
           let file = std::fs::File::open(input_path)?;
           let reader = BufReader::new(file);

           let mut entries: Vec<ProcessedImage> = Vec::new();
           for line in reader.lines() {
               let line = line?;
               if line.trim().is_empty() {
                   continue;
               }
               match serde_json::from_str::<ProcessedImage>(&line) {
                   Ok(image) => {
                       // Skip if already enriched
                       if self.options.skip_existing && existing_hashes.contains(&image.content_hash) {
                           continue;
                       }
                       // Skip if already has description
                       if self.options.skip_existing && image.description.is_some() {
                           continue;
                       }
                       entries.push(image);
                   }
                   Err(e) => {
                       tracing::warn!("Skipping malformed JSONL line: {}", e);
                   }
               }
           }

           let total = entries.len();
           tracing::info!("Enriching {} images", total);

           let mut succeeded: u64 = 0;
           let mut failed: u64 = 0;
           let skipped = 0u64; // already filtered above

           // Process with concurrency limit
           let semaphore = Arc::new(tokio::sync::Semaphore::new(self.options.parallel));

           // For sequential output writing, collect results via channel
           let (tx, mut rx) = tokio::sync::mpsc::channel::<EnrichmentPatch>(self.options.parallel * 2);

           let provider = self.provider.clone();
           let timeout_ms = self.options.timeout_ms;
           let retry_attempts = self.options.retry_attempts;
           let retry_delay_ms = self.options.retry_delay_ms;

           // Spawn enrichment tasks
           let mut handles = Vec::new();
           for entry in entries {
               let semaphore = semaphore.clone();
               let tx = tx.clone();
               let provider = provider.clone();

               handles.push(tokio::spawn(async move {
                   let _permit = semaphore.acquire().await.unwrap();

                   let result = Self::enrich_single(
                       &*provider,
                       &entry,
                       timeout_ms,
                       retry_attempts,
                       retry_delay_ms,
                   ).await;

                   match result {
                       Ok(patch) => { let _ = tx.send(patch).await; true }
                       Err(e) => {
                           tracing::warn!("Failed to enrich {:?}: {}", entry.file_path, e);
                           false
                       }
                   }
               }));
           }

           // Drop our copy of tx so rx closes when all tasks finish
           drop(tx);

           // Write patches as they arrive
           let mut writer = BufWriter::new(output);
           while let Some(patch) = rx.recv().await {
               let line = serde_json::to_string(&patch).unwrap();
               writeln!(writer, "{}", line)?;
               writer.flush()?;
               succeeded += 1;
           }

           // Wait for all tasks and count failures
           for handle in handles {
               match handle.await {
                   Ok(true) => {} // already counted via channel
                   Ok(false) => { failed += 1; }
                   Err(_) => { failed += 1; }
               }
           }

           Ok((succeeded, failed, skipped))
       }

       /// Enrich a single image.
       async fn enrich_single(
           provider: &dyn LlmProvider,
           entry: &ProcessedImage,
           timeout_ms: u64,
           retry_attempts: u32,
           retry_delay_ms: u64,
       ) -> Result<EnrichmentPatch, PipelineError> {
           // Load image from disk
           let image_bytes = std::fs::read(&entry.file_path)?;
           let format = entry.format.as_str();
           let image_input = ImageInput::from_bytes(&image_bytes, format);

           // Build tag context from Phase 4 tags
           let tag_names: Vec<String> = entry.tags.iter()
               .map(|t| t.name.clone())
               .collect();

           let request = LlmRequest::describe_image(image_input, &tag_names);

           // Send to LLM with retry
           let timeout_duration = std::time::Duration::from_millis(timeout_ms);
           let mut last_error = None;

           for attempt in 1..=retry_attempts {
               match tokio::time::timeout(timeout_duration, provider.generate(request.clone())).await {
                   Ok(Ok(response)) => {
                       return Ok(EnrichmentPatch {
                           content_hash: entry.content_hash.clone(),
                           file_path: entry.file_path.clone(),
                           description: response.text,
                           llm_model: response.model,
                           llm_tokens: response.tokens_used,
                           llm_latency_ms: response.latency_ms,
                       });
                   }
                   Ok(Err(e)) => {
                       tracing::warn!("LLM failed for {:?} (attempt {}): {}", entry.file_path, attempt, e);
                       last_error = Some(e);
                   }
                   Err(_) => {
                       tracing::warn!("LLM timed out for {:?} (attempt {})", entry.file_path, attempt);
                       last_error = Some(PipelineError::Timeout {
                           path: entry.file_path.clone(),
                           stage: "llm".to_string(),
                           timeout_ms,
                       });
                   }
               }

               if attempt < retry_attempts {
                   tokio::time::sleep(std::time::Duration::from_millis(retry_delay_ms * attempt as u64)).await;
               }
           }

           Err(last_error.unwrap())
       }
   }
   ```

**Acceptance Criteria:**
- [ ] Reads JSONL input file correctly
- [ ] Loads images from `file_path` on disk
- [ ] Tags from Phase 4 passed as context to LLM
- [ ] Emits `EnrichmentPatch` per image (content_hash + description)
- [ ] Concurrent enrichment with configurable parallelism
- [ ] Skips images that already have descriptions
- [ ] Retry with backoff per image
- [ ] Failed images don't stop the batch

---

### 5.8 CLI: `photon enrich` Command

**Goal:** Add the `enrich` subcommand to the CLI.

**Steps:**

1. Create `crates/photon/src/cli/enrich.rs`:
   ```rust
   use std::path::PathBuf;
   use clap::Args;

   use photon_core::config::Config;
   use photon_core::llm::{Enricher, LlmProviderFactory, EnrichmentPatch};
   use photon_core::llm::enricher::EnrichOptions;

   #[derive(Args)]
   pub struct EnrichArgs {
       /// Input JSONL file from `photon process`
       pub input: PathBuf,

       /// LLM provider to use
       #[arg(long)]
       pub llm: String,

       /// Output file for enrichment patches (default: stdout)
       #[arg(short, long)]
       pub output: Option<PathBuf>,

       /// Number of concurrent LLM requests
       #[arg(long, default_value = "4")]
       pub parallel: usize,

       /// Skip images that already have descriptions
       #[arg(long, default_value = "true")]
       pub skip_existing: bool,

       /// LLM timeout in milliseconds
       #[arg(long, default_value = "60000")]
       pub timeout: u64,
   }

   pub async fn execute(args: EnrichArgs) -> anyhow::Result<()> {
       let config = Config::load()?;

       // Create LLM provider
       let provider = LlmProviderFactory::create(&args.llm, &config.llm)
           .ok_or_else(|| anyhow::anyhow!(
               "Unknown or unconfigured LLM provider: '{}'\n\
                Available: ollama, anthropic, openai, hyperbolic\n\
                Configure in ~/.photon/config.toml under [llm.{}]",
               args.llm, args.llm
           ))?;

       // Check availability
       if !provider.is_available().await {
           anyhow::bail!(
               "LLM provider '{}' is not available. Check your configuration and API keys.",
               args.llm
           );
       }

       tracing::info!("Using LLM provider: {}", args.llm);

       // Load existing enrichment hashes if output file exists
       let existing_hashes = if args.skip_existing {
           if let Some(ref path) = args.output {
               load_enriched_hashes(path)?
           } else {
               std::collections::HashSet::new()
           }
       } else {
           std::collections::HashSet::new()
       };

       let options = EnrichOptions {
           parallel: args.parallel,
           skip_existing: args.skip_existing,
           timeout_ms: args.timeout,
           retry_attempts: 3,
           retry_delay_ms: 1_000,
       };

       let enricher = Enricher::new(provider, options);

       // Output destination
       let mut output: Box<dyn std::io::Write> = match &args.output {
           Some(path) => Box::new(
               std::fs::OpenOptions::new()
                   .create(true)
                   .append(true)  // Append mode — safe to re-run
                   .open(path)?
           ),
           None => Box::new(std::io::stdout()),
       };

       let (succeeded, failed, skipped) = enricher
           .enrich_file(&args.input, &mut *output, &existing_hashes)
           .await?;

       tracing::info!(
           "Enrichment complete: {} succeeded, {} failed, {} skipped",
           succeeded, failed, skipped
       );

       Ok(())
   }

   fn load_enriched_hashes(path: &PathBuf) -> anyhow::Result<std::collections::HashSet<String>> {
       let mut hashes = std::collections::HashSet::new();
       if path.exists() {
           let content = std::fs::read_to_string(path)?;
           for line in content.lines() {
               if let Ok(patch) = serde_json::from_str::<EnrichmentPatch>(line) {
                   hashes.insert(patch.content_hash);
               }
           }
           tracing::info!("Found {} already-enriched images", hashes.len());
       }
       Ok(hashes)
   }
   ```

2. Register in CLI:
   ```rust
   // In cli/mod.rs

   #[derive(Subcommand)]
   pub enum Commands {
       /// Process images (fast pipeline: decode, hash, embed, tag)
       Process(process::ProcessArgs),

       /// Enrich processed images with LLM descriptions
       Enrich(enrich::EnrichArgs),

       /// Manage configuration
       Config(config::ConfigArgs),
   }
   ```

**Usage examples:**

```bash
# Step 1: Fast pipeline — processes all images, no network dependency
photon process ./photos/ --output results.jsonl

# Step 2: Enrich with LLM — separate, optional, retryable
photon enrich results.jsonl --llm anthropic --output enriched.jsonl

# Re-run enrichment (skips already-enriched images)
photon enrich results.jsonl --llm anthropic --output enriched.jsonl

# Use different provider for a second pass
photon enrich results.jsonl --llm ollama --output enriched.jsonl

# Higher concurrency for cloud APIs
photon enrich results.jsonl --llm openai --output enriched.jsonl --parallel 8
```

**Output format:**

The enrichment file is a JSONL of patches:
```jsonl
{"content_hash":"a7f3b2c1...","file_path":"/photos/beach.jpg","description":"A sandy tropical beach with turquoise waters and palm trees swaying in the breeze.","llm_model":"claude-sonnet-4-20250514","llm_tokens":47,"llm_latency_ms":1823}
{"content_hash":"b8e4c3d2...","file_path":"/photos/dog.jpg","description":"A golden labrador retriever lying on a Persian rug in a warmly lit living room.","llm_model":"claude-sonnet-4-20250514","llm_tokens":52,"llm_latency_ms":2104}
```

**Consuming application pattern:**
```
1. Read results.jsonl    → INSERT into DB (tags, embedding, metadata)
2. Read enriched.jsonl   → UPDATE in DB  (SET description WHERE content_hash = ...)
```

No conflicts, no overwrites. The LLM only fills in nullable `description` fields.

**Acceptance Criteria:**
- [ ] `photon enrich input.jsonl --llm anthropic` works
- [ ] Output is JSONL of `EnrichmentPatch` objects
- [ ] `--output` writes to file in append mode
- [ ] Re-running skips already-enriched images
- [ ] `--parallel` controls concurrency
- [ ] Progress feedback during enrichment
- [ ] Clear error message for unconfigured provider

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
        ImageInput::from_bytes(&image, "jpeg"),
        &["dog".to_string(), "carpet".to_string()],
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
        ImageInput::from_bytes(&image, "jpeg"),
        &[],
    );

    let response = provider.generate(request).await.unwrap();

    assert!(!response.text.is_empty());
    assert!(response.tokens_used.is_some());
}

#[tokio::test]
async fn test_enrichment_patch_serialization() {
    let patch = EnrichmentPatch {
        content_hash: "abc123".to_string(),
        file_path: PathBuf::from("/photos/test.jpg"),
        description: "A test image.".to_string(),
        llm_model: "test-model".to_string(),
        llm_tokens: Some(10),
        llm_latency_ms: 500,
    };

    let json = serde_json::to_string(&patch).unwrap();
    let deserialized: EnrichmentPatch = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.content_hash, "abc123");
    assert_eq!(deserialized.description, "A test image.");
}

#[tokio::test]
async fn test_enricher_skips_existing() {
    // Create a JSONL with one image that already has a description
    let image = ProcessedImage {
        content_hash: "abc123".to_string(),
        description: Some("Already described.".to_string()),
        // ... other fields ...
    };

    // Write to temp file
    let input_path = temp_dir().join("test_enrich_input.jsonl");
    std::fs::write(&input_path, serde_json::to_string(&image).unwrap()).unwrap();

    let existing = std::collections::HashSet::new();
    let provider = MockProvider::new(); // test helper
    let enricher = Enricher::new(Box::new(provider), EnrichOptions::default());

    let mut output = Vec::new();
    let (succeeded, failed, skipped) = enricher
        .enrich_file(&input_path, &mut output, &existing)
        .await
        .unwrap();

    // Should skip the image since it already has a description
    assert_eq!(succeeded, 0);
}
```

---

## Verification Checklist

Before moving to Phase 6:

- [ ] `photon enrich results.jsonl --llm ollama` works with local Ollama
- [ ] `photon enrich results.jsonl --llm anthropic` works with Claude
- [ ] `photon enrich results.jsonl --llm openai` works with GPT-4V
- [ ] `photon enrich results.jsonl --llm hyperbolic` works with Hyperbolic
- [ ] Output is JSONL of `EnrichmentPatch` objects
- [ ] Re-running skips already-enriched images (append mode)
- [ ] LLM failures don't crash the enrichment batch
- [ ] Timeout works correctly per image
- [ ] Retry logic handles transient failures
- [ ] Environment variable API keys work
- [ ] `--parallel` controls concurrent LLM requests
- [ ] Tags from Phase 4 appear in LLM prompts
- [ ] `photon process` does NOT touch any LLM — pipeline stays fast
- [ ] All integration tests pass

---

## Files Created/Modified

```
crates/photon-core/src/
├── llm/
│   ├── mod.rs           # Module exports
│   ├── provider.rs      # Provider trait + factory
│   ├── ollama.rs        # Ollama implementation
│   ├── hyperbolic.rs    # Hyperbolic implementation
│   ├── anthropic.rs     # Anthropic implementation
│   ├── openai.rs        # OpenAI implementation
│   ├── retry.rs         # Retry + timeout logic
│   └── enricher.rs      # Enrichment engine (reads JSONL, emits patches)
├── config.rs            # Updated with LLM configs

crates/photon/src/cli/
├── mod.rs               # Updated with Enrich subcommand
└── enrich.rs            # `photon enrich` command
```

**Note:** `processor.rs` is NOT modified. The fast pipeline has zero LLM dependency.

---

## Notes

- LLM enrichment is fully decoupled from the fast pipeline — `photon process` never blocks on network
- The enrichment output file is append-mode — safe to re-run after failures
- Tags from Phase 4 are passed as context, producing better descriptions than image-only prompts
- For Ollama, recommend llama3.2-vision or similar vision-capable models
- Concurrency should be tuned per provider: local Ollama (1-2), cloud APIs (4-8)
- Consider adding a `--dry-run` flag to preview which images would be enriched
- Future: could support enrichment directly to a database instead of JSONL patches
