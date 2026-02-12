# Task 5: Add Enricher Unit Tests with MockProvider

**Status:** Complete
**Date:** 2026-02-13

## Summary

Added 6 unit tests for the LLM enricher module (`llm/enricher.rs`) using a configurable `MockProvider` that implements the `LlmProvider` trait. Tests cover success, retry, auth failure, timeout, partial batch failure, and missing file scenarios. Test count: 185 → 191.

## MockProvider Design

Since `PipelineError` doesn't implement `Clone`, the mock uses a factory function pattern instead of storing pre-built `Result` values:

```rust
struct MockProvider {
    response_fn: Box<dyn Fn(u32) -> Result<LlmResponse, PipelineError> + Send + Sync>,
    call_count: AtomicU32,
    delay: Option<Duration>,
}
```

Constructors:
- `MockProvider::success(text)` — always returns `Ok(LlmResponse { text, ... })`
- `MockProvider::failing(status_code, message)` — always returns `Err(PipelineError::Llm { ... })`
- `MockProvider::fail_then_succeed(status_code, error_msg, success_text)` — first call fails, subsequent succeed
- `.with_delay(duration)` — adds artificial latency (for timeout testing)

All tests use `#[tokio::test(flavor = "multi_thread")]` because `enrich_batch()` uses `tokio::spawn`.

## Tests Added

| Test | What it verifies |
|------|-----------------|
| `test_enricher_basic_success` | Single image → correct description, content_hash, model in patch |
| `test_enricher_retry_on_transient_error` | 429 → retry → success on second attempt |
| `test_enricher_no_retry_on_auth_error` | 401 fails immediately, no retries despite `retry_attempts: 3` |
| `test_enricher_timeout` | Provider sleeps 5s, enricher timeout is 50ms → timeout failure |
| `test_enricher_batch_partial_failure` | 3 images (2 real, 1 nonexistent) → 2 succeed, 1 fails |
| `test_enricher_missing_image_file` | Nonexistent file path → "Failed to read image" failure |

## Key Implementation Notes

- Tests use real fixture images (`tests/fixtures/images/beach.jpg`, `car.jpg`) because `enrich_single()` reads the file from disk before calling the LLM provider
- Missing file tests use `/nonexistent/path/` to trigger the file read error path before the provider is ever called
- `fast_options()` helper: `retry_attempts: 0, retry_delay_ms: 10, timeout_ms: 5000` for fast test execution
- `run_enricher()` helper: wraps `enrich_batch()` with an `Arc<Mutex<Vec>>` callback collector

## Files Modified

- `crates/photon-core/src/llm/enricher.rs` — added `#[cfg(test)] mod tests` with MockProvider + 6 tests

## Verification

- 191 tests passing (37 CLI + 144 core + 10 integration)
- Zero clippy warnings (`-D warnings`)
- Zero formatting violations
