# Phase 5 Bugs

> Discovered during post-Phase-5 code review, 2026-02-11

---

## Bug 1: Enricher success/failure counting is wrong

**Severity:** High
**File:** `crates/photon-core/src/llm/enricher.rs`

`enrich_batch` counts `Ok(())` from `handle.await` as "succeeded" and `Err(e)` (task panic) as "failed". But `enrich_single` always returns `Ok(())` regardless of whether the LLM call succeeded or failed — failures are communicated through the `on_result` callback as `EnrichResult::Failure`. The `(succeeded, failed)` tuple therefore means "tasks that didn't panic / tasks that panicked", **not** "descriptions generated / descriptions failed". Stats logged by `process.rs` will overcount successes and undercount failures.

**Fix:** Track actual LLM outcomes. Either count success/failure inside the callback, or have `enrich_single` return a `Result` that reflects the LLM outcome.

---

## Bug 2: Batch JSON stdout + `--llm` silently drops core records

**Severity:** High
**File:** `crates/photon/src/cli/process.rs`

In the batch processing path with `--format json` and `--llm` enabled, outputting to stdout:

1. Core records are collected into `results` (because `llm_enabled` is true)
2. The JSON stdout block checks `!llm_enabled` and skips — core records are never printed
3. The LLM enrichment block runs and prints only `EnrichmentPatch` records to stdout

Result: the user gets enrichment patches without the core data they reference. Silent data loss.

**Fix:** Ensure core records are emitted to stdout before enrichment patches, regardless of format.

---

## Bug 3: String-based retry classification is fragile

**Severity:** Medium
**File:** `crates/photon-core/src/llm/retry.rs`

`is_retryable` uses `message.contains("429")`, `message.contains("500")`, etc. This substring matching produces false positives:

- `"Processed 500 tokens"` matches as server error
- Error on port `5003` matches the `"500"` check
- `"connection"` matches unrelated messages

**Fix:** Use structured error data (e.g., an HTTP status code field in the error) rather than parsing free-form strings. Alternatively, prefix status codes with `"HTTP "` in error messages and match on the prefix.

---

## Bug 4: Empty OpenAI `choices` array produces blank description silently

**Severity:** Medium
**File:** `crates/photon-core/src/llm/openai.rs`

If the API returns `{"choices": []}`, the code produces an empty string via `unwrap_or_default()` and stores it as a successful `EnrichmentPatch` with a blank description. No error is raised.

**Fix:** Return a `PipelineError::Llm` when the response contains no content.

---

## Bug 5: Blocking `std::fs::read` in async context

**Severity:** Medium
**File:** `crates/photon-core/src/llm/enricher.rs`

`enrich_single` calls `std::fs::read(&image.file_path)` inside a `tokio::spawn` task. This is a blocking filesystem call on the tokio runtime's worker thread. For large images (up to 100MB per config limits), this can stall the executor and starve other concurrent tasks.

**Fix:** Replace with `tokio::fs::read()` or wrap in `tokio::task::spawn_blocking`.

---

## Bug 6: `HyperbolicProvider::timeout()` is dead code

**Severity:** Low
**File:** `crates/photon-core/src/llm/hyperbolic.rs`

`HyperbolicProvider::timeout()` returns 60s, but it is never called. When `generate()` delegates to `self.inner.generate()`, the inner `OpenAiProvider` uses its own `timeout()`. If someone changed Hyperbolic's timeout expecting it to take effect, it would silently have no effect.

**Fix:** Either pass the outer timeout into the inner provider, or remove the dead `timeout()` override and document that Hyperbolic inherits OpenAI's timeout.

---

## Bug 7: `PathBuf::new()` sentinel in all LLM errors

**Severity:** Low
**Files:** `crates/photon-core/src/llm/ollama.rs`, `anthropic.rs`, `openai.rs`, `provider.rs`

All `PipelineError::Llm` errors constructed in provider code use `path: PathBuf::new()` because the provider has no image path context. This produces error messages like `"LLM error for : Anthropic request failed: ..."` with an empty path.

In practice, `enrich_single` in `enricher.rs` catches these and re-wraps them with the correct path in `EnrichResult::Failure`, so the empty path is never surfaced to users. But it is a leaky abstraction — provider-level errors (factory failures, availability checks) that skip the enricher will show empty paths.

**Fix:** Either make `PipelineError::Llm { path }` accept `Option<PathBuf>`, or introduce a dedicated provider error type that doesn't require a path, and have the enricher wrap it with path context.

---

## Code Smells (non-bugs, but worth addressing)

### No jitter in exponential backoff
**File:** `crates/photon-core/src/llm/retry.rs`

Deterministic backoff (`base * 2^attempt`) causes thundering herd when multiple concurrent enrichments hit rate limits simultaneously. Standard practice is to add random jitter.

### `execute()` function is 318 lines
**File:** `crates/photon/src/cli/process.rs`

Deeply nested branching on 4 orthogonal concerns (single/batch, LLM/no-LLM, stdout/file, json/jsonl). Should be decomposed into smaller functions.

### Dual timeout layering
**Files:** `enricher.rs` + all providers

The enricher applies `tokio::time::timeout(options.timeout_ms)` around `provider.generate()`, but each provider also applies its own hardcoded reqwest timeout. The lower timeout fires first. If config says 180s but the provider hardcodes 60s, the reqwest timeout fires and the enricher sees a reqwest error, not its own timeout. Confusing and should be unified.

### `is_available()` never called
**Files:** All provider implementations

Defined on the `LlmProvider` trait but never invoked anywhere in the codebase. Dead interface.

### `std::sync::mpsc` in async code
**File:** `crates/photon/src/cli/process.rs`

Uses `std::sync::mpsc::channel` inside async code for enrichment result collection. Should prefer `tokio::sync::mpsc` for consistency with the async runtime.

### Provider `enabled` config fields unused
**File:** `crates/photon-core/src/config.rs`

Each provider config has an `enabled: bool` field that is serialized/deserialized but never consulted at runtime. CLI `--llm` flag is the sole activation mechanism.

---

## Test Gaps

| Area | Current tests | What's needed |
|------|---------------|---------------|
| `enricher.rs` | 0 | Mock provider tests for concurrency, retry, callback delivery |
| Provider HTTP calls | 0 | `wiremock`-based tests for each provider's request/response handling |
| `process.rs` orchestration | 0 | Integration tests for all format/output/LLM combinations |
| Batch JSON + LLM path | 0 | Regression test for Bug 2 |
| Error classification | 0 | Tests with realistic error messages to catch false positives (Bug 3) |
