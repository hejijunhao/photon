# Phase 5: LLM Integration

> Completed 2026-02-11

## Milestone

`photon process ./photos/ --llm anthropic` outputs core results immediately, then LLM-generated descriptions as enrichment patches. Without `--llm`, output is identical to pre-Phase-5 (backward compatible).

## What Was Built

A BYOK (Bring Your Own Key) LLM enrichment system supporting four providers: Ollama (local), Anthropic, OpenAI, and Hyperbolic. The architecture uses a **dual-stream output** model — core pipeline results emit first at full speed, then LLM descriptions follow as they complete. This keeps the core pipeline fast (~50 img/sec) while LLM calls run asynchronously with bounded concurrency, retry logic, and per-request timeouts.

## Architecture: Dual-Stream Output

```
                      ┌──────────────┐
                      │ Core Pipeline │  (fast, ~50 img/sec)
                      │ Phases 1–4   │
                      └──────┬───────┘
                             │
              ┌──────────────┼──────────────┐
              ▼                              ▼
   {"type":"core",...}              collect ProcessedImages
   (emitted immediately)                     │
                                             ▼
                                  ┌──────────────────┐
                                  │    Enricher       │
                                  │ (semaphore-bound) │
                                  └────────┬─────────┘
                                           │
                                           ▼
                                {"type":"enrichment",...}
                                (emitted as LLM calls finish)
```

When `--llm` is not specified, output is plain `ProcessedImage` JSON — no `type` field, no wrapping, no breaking change.

## Files Created (8)

| File | Purpose |
|------|---------|
| `crates/photon-core/src/llm/mod.rs` | Module declarations and re-exports for the LLM subsystem. |
| `crates/photon-core/src/llm/provider.rs` | `LlmProvider` trait (`#[async_trait]`), `ImageInput` (base64 + MIME), `LlmRequest` (with tag-aware prompt builder), `LlmResponse`, `LlmProviderFactory` (creates `Box<dyn LlmProvider>` from provider name + config), `resolve_env_var()` for `${ENV_VAR}` expansion. |
| `crates/photon-core/src/llm/retry.rs` | `is_retryable()` — classifies timeouts, 429, 5xx as retryable. `backoff_duration()` — exponential backoff capped at 30s. |
| `crates/photon-core/src/llm/ollama.rs` | `OllamaProvider` — POST to `/api/generate` with base64 images array, no auth, 120s timeout (vision models are slow locally). |
| `crates/photon-core/src/llm/anthropic.rs` | `AnthropicProvider` — POST to Messages API with `x-api-key` + `anthropic-version` headers, base64 image source content block. Captures `usage.input_tokens + output_tokens`. |
| `crates/photon-core/src/llm/openai.rs` | `OpenAiProvider` — POST to Chat Completions with `Authorization: Bearer` header, data URL image content. Supports custom endpoint (reused by Hyperbolic). |
| `crates/photon-core/src/llm/hyperbolic.rs` | `HyperbolicProvider` — thin wrapper over `OpenAiProvider` with custom endpoint (`{config.endpoint}/chat/completions`). |
| `crates/photon-core/src/llm/enricher.rs` | `Enricher` — concurrent LLM orchestration engine. Uses `tokio::Semaphore` for bounded parallelism, reads images from disk, builds tag-aware prompts, runs retry loop with timeout, delivers results via callback for real-time JSONL streaming. |

## Files Modified (5)

| File | Change |
|------|--------|
| `Cargo.toml` (workspace) | Added `async-trait = "0.1"` to workspace dependencies. |
| `crates/photon-core/Cargo.toml` | Added `async-trait`, `reqwest`, `futures-util` as workspace dependencies. |
| `crates/photon-core/src/types.rs` | Added `EnrichmentPatch` struct (content_hash, description, llm_model, llm_latency_ms, llm_tokens) and `OutputRecord` enum (internally tagged: `Core(Box<ProcessedImage>)` / `Enrichment(EnrichmentPatch)`). Added 3 serde roundtrip tests. |
| `crates/photon-core/src/lib.rs` | Added `pub mod llm`, re-exported `EnrichmentPatch` and `OutputRecord`. |
| `crates/photon/src/cli/process.rs` | Rewrote `execute()` for dual-stream output: when `--llm` is active, wraps results as `OutputRecord::Core`, then runs `Enricher::enrich_batch()` and emits `OutputRecord::Enrichment` patches. Added `create_enricher()` helper (caps LLM concurrency at 8, wires pipeline retry/timeout config). Backward compatible — without `--llm`, output is unchanged. |

## Files Untouched (critical)

| File | Why |
|------|-----|
| `pipeline/processor.rs` | Core pipeline stays fast — no LLM in the hot path. |
| `config.rs` | All `LlmConfig`, `OllamaConfig`, `AnthropicConfig`, `OpenAiConfig`, `HyperbolicConfig` structs were already defined. |
| `error.rs` | `PipelineError::Llm` and `PipelineError::Timeout` were already defined. |
| `main.rs` | No new subcommands needed. |

## Design Decisions

### Why `async_trait` instead of native async fn in trait

Rust 1.91's native `async fn` in traits is not object-safe — it can't be used with `Box<dyn LlmProvider>`. Since we need dynamic dispatch to select providers at runtime from CLI flags, `#[async_trait]` is required. This adds one heap allocation per call (the boxed future), which is negligible compared to network round-trip time.

### Why `Box<ProcessedImage>` in OutputRecord

Clippy flagged `large_enum_variant`: `ProcessedImage` is ~408 bytes (paths, vecs, strings) while `EnrichmentPatch` is ~88 bytes. Boxing the larger variant avoids inflating the enum size for the common enrichment path.

### Why dual-stream instead of inline enrichment

LLM calls take 1-5s each. Running them inline would drop throughput from ~50 img/sec to <1 img/sec. The dual-stream approach lets the core pipeline run at full speed while LLM enrichments trickle in asynchronously. Consumers join records by `content_hash`.

### Why semaphore-bounded concurrency

The `Enricher` uses a `tokio::Semaphore` to cap concurrent LLM calls (default: `min(parallel, 8)`). This prevents overwhelming API rate limits while still parallelizing across images. Each task acquires a permit before starting, releases it when done.

### Why tag-aware prompts

`LlmRequest::describe_image()` includes Phase 4 zero-shot tags in the prompt when available. This gives the LLM context about what SigLIP already detected, producing more focused and accurate descriptions rather than redundant observations.

### Why Hyperbolic delegates to OpenAI

Hyperbolic uses an OpenAI-compatible API format (same request/response shape). Rather than duplicating serialization code, `HyperbolicProvider` wraps `OpenAiProvider::with_endpoint()`. If the API drifts, it can be split into its own implementation.

## Dependencies Added

| Dependency | Why |
|-----------|-----|
| `async-trait = "0.1"` | Object-safe async trait for `Box<dyn LlmProvider>` |
| `reqwest` (workspace, already existed) | HTTP client for LLM API calls — now also in photon-core |
| `futures-util` (workspace, already existed) | Now also in photon-core |

## Tests

48 tests passing (+16 new over pre-Phase-5 baseline of 32):

| Test | File | What it verifies |
|------|------|------------------|
| `test_image_input_from_bytes_jpeg` | `provider.rs` | JPEG MIME detection and base64 encoding |
| `test_image_input_from_bytes_png` | `provider.rs` | PNG MIME detection |
| `test_image_input_data_url` | `provider.rs` | Data URL format for OpenAI-style APIs |
| `test_describe_image_without_tags` | `provider.rs` | Prompt generation without tag context |
| `test_describe_image_with_tags` | `provider.rs` | Tag names injected into prompt |
| `test_resolve_env_var` | `provider.rs` | `${VAR}` expansion, empty, plain strings |
| `test_timeout_is_retryable` | `retry.rs` | Timeout errors classified as retryable |
| `test_rate_limit_is_retryable` | `retry.rs` | HTTP 429 classified as retryable |
| `test_server_error_is_retryable` | `retry.rs` | HTTP 5xx classified as retryable |
| `test_auth_error_not_retryable` | `retry.rs` | HTTP 401 not retried |
| `test_decode_error_not_retryable` | `retry.rs` | Non-LLM errors not retried |
| `test_backoff_exponential` | `retry.rs` | 1s, 2s, 4s, 8s progression |
| `test_backoff_capped_at_30s` | `retry.rs` | Backoff never exceeds 30 seconds |
| `test_output_record_core_roundtrip` | `types.rs` | Core variant serializes with `"type":"core"`, roundtrips |
| `test_output_record_enrichment_roundtrip` | `types.rs` | Enrichment variant serializes with `"type":"enrichment"`, roundtrips |
| `test_enrichment_patch_skips_none_tokens` | `types.rs` | `llm_tokens: None` omitted from JSON |

## CLI Usage

```bash
# Without LLM — unchanged output (backward compatible)
photon process image.jpg

# With LLM — dual-stream JSONL output
photon process ./photos/ --llm anthropic --format jsonl

# Override model
photon process image.jpg --llm ollama --llm-model llava:13b

# Disable descriptions even when --llm is set
photon process ./photos/ --llm anthropic --no-description
```

## Consumer Integration

```python
for line in stream:
    record = json.loads(line)
    if record["type"] == "core":
        db.insert_image(record)
    elif record["type"] == "enrichment":
        db.update_description(record["content_hash"], record["description"])
```
