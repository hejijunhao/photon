# Phase 5: LLM Enrichment — Single-Command Dual-Stream

> **Milestone:** `photon process ./photos/ --llm anthropic` outputs core results immediately, then LLM enrichments
> **Architecture:** Single command, dual-stream JSONL — core section first (fast), enrichment section second (async)

---

## Design

### Problem

LLM description calls take 1–5s each. A naive inline approach would slow the pipeline from ~50 img/sec to <1 img/sec.

### Solution

Decouple LLM from the core pipeline within a **single command**. Output two record types to the same stream, joined by `content_hash`:

```
photon process ./photos/ --llm anthropic --format jsonl
```

```jsonl
# === Section 1: Core records (emitted at full pipeline speed) ===
{"type":"core","content_hash":"abc123","file_name":"beach.jpg","tags":[...],"embedding":[...],...}
{"type":"core","content_hash":"def456","file_name":"dog.jpg","tags":[...],...}

# === Section 2: Enrichments (emitted as LLM calls complete) ===
{"type":"enrichment","content_hash":"abc123","description":"A sandy tropical beach...","llm_model":"claude-sonnet-4-5-20250929","llm_latency_ms":2100}
{"type":"enrichment","content_hash":"def456","description":"A golden retriever...","llm_model":"claude-sonnet-4-5-20250929","llm_latency_ms":1800}
```

**Without `--llm`**: Output is plain `ProcessedImage` as today — no `type` field, no breaking change.

### Consumer integration

```python
# Your app reads the JSONL stream and routes by "type":
for line in stream:
    record = json.loads(line)
    if record["type"] == "core":
        db.insert_image(record)           # INSERT with tags, embedding, metadata
    elif record["type"] == "enrichment":
        db.update_description(             # UPDATE ... WHERE content_hash = ...
            record["content_hash"],
            record["description"]
        )
```

---

## Existing Infrastructure (already in codebase)

These are already implemented and need **no changes**:

| What | Where | Status |
|------|-------|--------|
| `LlmConfig`, `OllamaConfig`, `AnthropicConfig`, `OpenAiConfig`, `HyperbolicConfig` | `config.rs:367–478` | Done |
| `PipelineError::Llm { path, message }` | `error.rs:68–70` | Done |
| `PipelineError::Timeout { path, stage, timeout_ms }` | `error.rs:72–78` | Done |
| `LimitsConfig::llm_timeout_ms` (60s default) | `config.rs:207` | Done |
| `PipelineConfig::retry_attempts` / `retry_delay_ms` | `config.rs:164–175` | Done |
| `ProcessedImage::description: Option<String>` | `types.rs:51` | Done |
| CLI `--llm`, `--llm-model`, `--no-description` flags | `cli/process.rs:41–59` | Done |
| CLI `LlmProvider` enum (Ollama/Hyperbolic/Anthropic/Openai) | `cli/process.rs:91–101` | Done |
| `reqwest` with json+stream features | workspace `Cargo.toml:30` | Done |
| `base64 = "0.22"` | `photon-core/Cargo.toml:33` | Done |

---

## Implementation Tasks

### Task 1: Add dependencies

**Files:**
- `Cargo.toml` (workspace root) — add `async-trait = "0.1"` to `[workspace.dependencies]`
- `crates/photon-core/Cargo.toml` — add `async-trait = "0.1"` and `reqwest.workspace = true`

**Why `async-trait`:** The `LlmProvider` trait uses `async fn` but must be object-safe for `Box<dyn LlmProvider>`. Native `async fn in trait` is not object-safe even on Rust 1.91.

**Verify:** `cargo check` compiles.

---

### Task 2: Add output types

**File:** `crates/photon-core/src/types.rs`

Add after `ProcessingStats`:

```rust
/// Lightweight patch emitted by the LLM enrichment pass.
/// Keyed by content_hash so the consumer can join with the core record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichmentPatch {
    pub content_hash: String,
    pub description: String,
    pub llm_model: String,
    pub llm_latency_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm_tokens: Option<u32>,
}

/// Tagged union for dual-stream output when --llm is enabled.
/// Internally tagged: {"type":"core",...} or {"type":"enrichment",...}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum OutputRecord {
    Core(ProcessedImage),
    Enrichment(EnrichmentPatch),
}
```

**Tests:** Serde roundtrip for both variants. Verify `"type"` field appears in JSON.

**Verify:** `cargo test`

---

### Task 3: Create LLM provider trait and types

**File (new):** `crates/photon-core/src/llm/provider.rs`

Contents:
- `ImageInput` — base64-encoded image data + MIME type, constructed via `from_bytes(bytes, format)`
- `LlmRequest` — image + prompt + max_tokens + temperature. Has `describe_image(image, tags)` constructor that feeds Phase 4 tags as context
- `LlmResponse` — text + model + tokens_used + latency_ms
- `LlmProvider` trait (`#[async_trait]`) — `name()`, `is_available()`, `generate(request)`, `timeout()`
- `LlmProviderFactory` — `create(provider_name, &LlmConfig, model_override)` → `Box<dyn LlmProvider>`

---

### Task 4: Create retry utilities

**File (new):** `crates/photon-core/src/llm/retry.rs`

Contents:
- `is_retryable(&PipelineError) -> bool` — returns true for timeouts, 429, 5xx errors
- `backoff_duration(attempt, base_delay_ms) -> Duration` — exponential backoff

---

### Task 5: Implement Ollama provider

**File (new):** `crates/photon-core/src/llm/ollama.rs`

- POST to `{endpoint}/api/generate` with `model`, `prompt`, `images` (base64 array), `stream: false`
- Health check: GET `{endpoint}/api/tags`
- Timeout: 120s (vision models are slow locally)
- No auth required

---

### Task 6: Implement Anthropic provider

**File (new):** `crates/photon-core/src/llm/anthropic.rs`

- POST to `https://api.anthropic.com/v1/messages`
- Headers: `x-api-key`, `anthropic-version: 2023-06-01`, `content-type: application/json`
- Body: Messages API with image content block (base64 source, media_type)
- Response: extract `content[].text`, capture `usage.input_tokens + usage.output_tokens`
- API key resolution: expand `${ANTHROPIC_API_KEY}` from environment

---

### Task 7: Implement OpenAI provider

**File (new):** `crates/photon-core/src/llm/openai.rs`

- POST to `https://api.openai.com/v1/chat/completions`
- Header: `Authorization: Bearer {key}`
- Body: Chat completions with `image_url` content (data URL: `data:{mime};base64,{data}`)
- Response: extract `choices[0].message.content`, capture `usage.total_tokens`

---

### Task 8: Implement Hyperbolic provider

**File (new):** `crates/photon-core/src/llm/hyperbolic.rs`

- OpenAI-compatible API format (same request/response shape as OpenAI)
- Endpoint: `{config.endpoint}/chat/completions`
- Header: `Authorization: Bearer {key}`

---

### Task 9: Create LLM module structure

**File (new):** `crates/photon-core/src/llm/mod.rs`

```rust
pub mod provider;
pub mod retry;
pub mod ollama;
pub mod anthropic;
pub mod openai;
pub mod hyperbolic;
pub mod enricher;

pub use provider::{LlmProvider, LlmProviderFactory, LlmRequest, LlmResponse, ImageInput};
pub use enricher::{Enricher, EnrichOptions, EnrichResult};
```

**File (modify):** `crates/photon-core/src/lib.rs`

- Add `pub mod llm;`
- Add re-exports: `OutputRecord`, `EnrichmentPatch`, `Enricher`

**Verify:** `cargo check`

---

### Task 10: Create enrichment engine

**File (new):** `crates/photon-core/src/llm/enricher.rs`

This is the core orchestration piece:

```rust
pub struct Enricher {
    provider: Arc<dyn LlmProvider>,
    options: EnrichOptions,
}

pub struct EnrichOptions {
    pub parallel: usize,        // max concurrent LLM calls
    pub timeout_ms: u64,        // per-request timeout
    pub retry_attempts: u32,    // max retries per image
    pub retry_delay_ms: u64,    // base backoff delay
}

pub enum EnrichResult {
    Success(EnrichmentPatch),
    Failure(PathBuf, String),
}
```

**`enrich_batch(&self, images: &[ProcessedImage], on_result: F) -> (usize, usize)`:**
1. Spawn one tokio task per image, bounded by `Semaphore(parallel)`
2. Each task: read image from disk → base64 → build `LlmRequest` with tags → call provider with retry + timeout
3. On completion, call `on_result(EnrichResult)` — lets the CLI emit JSONL lines in real time
4. Return `(succeeded, failed)` counts

---

### Task 11: Wire into CLI

**File (modify):** `crates/photon/src/cli/process.rs`

The integration point. Modified `execute()` flow:

```
let llm_enabled = args.llm.is_some() && !args.no_description;

// Phase 1: Core pipeline (fast, as today)
for each image:
    result = processor.process_with_options(path, options)
    if llm_enabled:
        emit OutputRecord::Core(result)     // {"type":"core",...}
    else:
        emit ProcessedImage directly         // backward compatible

    collect results into Vec<ProcessedImage>

// Phase 2: LLM enrichment (only if --llm)
if llm_enabled:
    provider = LlmProviderFactory::create(args.llm, config.llm, args.llm_model)
    if !provider.is_available():
        warn and skip
    enricher = Enricher::new(provider, options_from_config)
    enricher.enrich_batch(&results, |result| {
        emit OutputRecord::Enrichment(patch)  // {"type":"enrichment",...}
    })
    log enrichment stats
```

**Verify:** `cargo check && cargo test`

---

### Task 12: Tests and verification

1. `cargo test` — all 32 existing tests still pass
2. New unit tests:
   - `OutputRecord` serde roundtrip (both variants)
   - `ImageInput::from_bytes` (MIME detection)
   - `LlmRequest::describe_image` with and without tags
   - `is_retryable` for various error types
   - `EnrichmentPatch` serialization
3. Manual smoke tests:
   - `cargo run -- process tests/fixtures/images/dog.jpg` → plain JSON, no `type` field (backward compat)
   - `cargo run -- process tests/fixtures/images/dog.jpg --llm anthropic` → `{"type":"core",...}` then `{"type":"enrichment",...}`

---

## File Summary

### New files (8)

| File | Purpose |
|------|---------|
| `crates/photon-core/src/llm/mod.rs` | Module declarations + re-exports |
| `crates/photon-core/src/llm/provider.rs` | `LlmProvider` trait, `ImageInput`, `LlmRequest`, `LlmResponse`, factory |
| `crates/photon-core/src/llm/retry.rs` | Retry utilities (`is_retryable`, `backoff_duration`) |
| `crates/photon-core/src/llm/ollama.rs` | Ollama provider (local) |
| `crates/photon-core/src/llm/anthropic.rs` | Anthropic provider (Messages API) |
| `crates/photon-core/src/llm/openai.rs` | OpenAI provider (Chat Completions) |
| `crates/photon-core/src/llm/hyperbolic.rs` | Hyperbolic provider (OpenAI-compat) |
| `crates/photon-core/src/llm/enricher.rs` | Enrichment engine (concurrent LLM orchestration) |

### Modified files (5)

| File | Change |
|------|--------|
| `Cargo.toml` | Add `async-trait` to workspace deps |
| `crates/photon-core/Cargo.toml` | Add `async-trait`, `reqwest` to deps |
| `crates/photon-core/src/types.rs` | Add `EnrichmentPatch`, `OutputRecord` |
| `crates/photon-core/src/lib.rs` | Add `pub mod llm;` + re-exports |
| `crates/photon/src/cli/process.rs` | Wire `--llm` to dual-stream output |

### Untouched (critical)

| File | Why |
|------|-----|
| `pipeline/processor.rs` | Core pipeline stays fast — no LLM in the hot path |
| `config.rs` | All LLM config structs already exist |
| `error.rs` | `PipelineError::Llm` already defined |
| `main.rs` | No new subcommands |
