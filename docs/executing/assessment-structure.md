# Assessment Structural Improvements

> Source: `docs/plans/code-assessment.md` — Phase 3 (refactoring) + Phase 4 (test coverage)
> Scope: 1 file split + 4 test coverage phases
> Prerequisite: `assessment-correctness.md` should be completed first (some tests depend on the fixes)

---

## Overview

| Phase | Scope | Files Changed | New Tests |
|-------|-------|---------------|-----------|
| **1** | Split `processor.rs` into focused modules | 3-4 new files | 0 |
| **2** | Test `--skip-existing` + batch output paths | 1 file | ~6 tests |
| **3** | Test enricher retry/timeout/concurrency | 1 file | ~6 tests |
| **4** | Edge case + config validation tests | 2-3 files | ~10 tests |

---

## Phase 1 — Split `processor.rs` (567 → ~3 files)

**File:** `crates/photon-core/src/pipeline/processor.rs` (559 lines)

The processor has three distinct responsibilities mixed into one file:
1. **Tagging initialization** — loading label banks, setting up progressive encoding
2. **Processing pipeline** — the actual image processing stages
3. **Relevance-aware scoring** — pool management during tag scoring

### Target Structure

```
pipeline/
  processor.rs       → ~200 lines (struct def, new(), process(), process_with_options())
  tagging_loader.rs  → ~200 lines (load_tagging, load_tagging_blocking, load_relevance_tracker, save_relevance)
  scoring.rs         → ~120 lines (score_with_relevance helper, extracted from process_with_options)
  mod.rs             → add new module declarations
```

### Step 1 — Extract `tagging_loader.rs`

Move these functions out of `processor.rs`:

| Function | Current Lines | Description |
|----------|---------------|-------------|
| `load_tagging()` | 88-198 | Async tagging init with 3 paths (cached/progressive/blocking) |
| `load_tagging_blocking()` | 200-236 | Synchronous fallback encoding |
| `load_relevance_tracker()` | 238-284 | Loads or creates relevance tracker |
| `save_relevance()` | 291-315 | Persists relevance state to disk |

These are all `impl ImageProcessor` methods. They can stay as methods — just move the `impl` block to the new file:

```rust
// tagging_loader.rs
use super::processor::ImageProcessor;
// ... imports ...

impl ImageProcessor {
    pub async fn load_tagging(&self, ...) -> Result<()> { ... }
    fn load_tagging_blocking(&self, ...) -> Result<(TagScorer, LabelBank)> { ... }
    fn load_relevance_tracker(&self, ...) -> Result<RelevanceTracker> { ... }
    pub async fn save_relevance(&self) -> Result<()> { ... }
}
```

**Note:** Rust allows `impl` blocks for a type to be split across files within the same crate. The struct definition stays in `processor.rs`.

### Step 2 — Extract `scoring.rs`

Extract the pool-aware scoring block from `process_with_options()` (lines 422-496) into a helper:

```rust
// scoring.rs

/// Score image tags using the relevance tracker's three-pool system.
///
/// Reads tags under a read lock, then updates hit stats and runs sweep
/// under a write lock. Returns the final tag list.
pub(crate) async fn score_with_relevance(
    scorer_lock: &RwLock<TagScorer>,
    tracker_lock: &RwLock<RelevanceTracker>,
    embedding: &[f32],
    config: &TaggingConfig,
) -> Result<Vec<Tag>> {
    // Phase 1: score under read lock
    // Phase 2: record hits + sweep under write lock
    // Phase 3: neighbor expansion if newly promoted
}
```

Then `process_with_options()` calls:

```rust
let tags = if let (Some(scorer_lock), Some(tracker_lock)) = (&self.scorer, &self.tracker) {
    score_with_relevance(scorer_lock, tracker_lock, &embedding, &self.config.tagging).await?
} else if let Some(scorer_lock) = &self.scorer {
    let scorer = scorer_lock.read().await;
    scorer.score(embedding, &self.config.tagging)?
} else {
    Vec::new()
};
```

### Step 3 — Update `mod.rs`

Add to `pipeline/mod.rs`:

```rust
mod scoring;
mod tagging_loader;
```

No `pub use` needed — these are internal implementation details.

### Verification

- **Zero logic changes** — this is a pure structural refactoring
- `cargo test --workspace` — all 164+ tests pass
- `cargo clippy --workspace -- -D warnings`
- Verify `processor.rs` is under 250 lines

### Note on `relevance.rs` and `models.rs`

The assessment flagged these as >500 lines, but:
- `relevance.rs`: 305 impl + 365 test — production code is well under 500
- `models.rs`: 404 impl + 140 test — production code is under 500

**Recommendation:** Leave these as-is. Extracting test modules to separate files is optional and low-value. Only split if the files grow further.

---

## Phase 2 — Batch Output Tests

**File:** New test file or extend existing tests in `crates/photon/src/cli/process/batch.rs`

These tests verify the fixes from `assessment-correctness.md` Phase 1 (M2+M3) and guard against regressions.

### Tests to Add

| Test | What it verifies |
|------|-----------------|
| `test_load_hashes_from_json_array` | Parses `[{"content_hash":"abc",...}, ...]` correctly |
| `test_load_hashes_from_jsonl` | Parses line-by-line JSONL (regression) |
| `test_load_hashes_from_dual_stream_json` | Parses array containing both `Core` and `Enrichment` records |
| `test_load_hashes_from_empty_file` | Returns empty set |
| `test_load_hashes_from_missing_file` | Returns empty set (no error) |
| `test_load_hashes_ignores_enrichment_records` | Only extracts hashes from `Core` records, not `Enrichment` |

### Implementation Pattern

These are unit tests that write temp files and call `load_existing_hashes()`:

```rust
#[test]
fn test_load_hashes_from_json_array() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("output.json");
    std::fs::write(&path, r#"[{"content_hash":"abc123","file_path":"a.jpg",...}]"#).unwrap();

    let hashes = load_existing_hashes(&Some(path)).unwrap();
    assert!(hashes.contains("abc123"));
}
```

**Note:** `load_existing_hashes` is currently `fn` (private). Change to `pub(crate)` for testability, or test via the public batch processing interface.

### Verification

```bash
cargo test -p photon
```

---

## Phase 3 — Enricher Tests

**File:** `crates/photon-core/src/llm/enricher.rs` (178 lines, zero tests)

The enricher orchestrates concurrent LLM calls with semaphore-bounded concurrency, retry with exponential backoff, and per-request timeouts. Testing requires mocking the `LlmProvider` trait.

### Mock Provider

Create a test-only mock:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider {
        responses: Mutex<Vec<Result<String, LlmError>>>,
        call_count: AtomicU32,
        delay: Option<Duration>,
    }

    #[async_trait]
    impl LlmProvider for MockProvider {
        async fn describe(&self, request: &LlmRequest) -> Result<String, LlmError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            if let Some(delay) = self.delay {
                tokio::time::sleep(delay).await;
            }
            let mut responses = self.responses.lock().await;
            responses.pop().unwrap_or(Ok("description".into()))
        }

        fn name(&self) -> &str { "mock" }
    }
}
```

### Tests to Add

| Test | What it verifies |
|------|-----------------|
| `test_enricher_basic_success` | Single image enrichment returns description |
| `test_enricher_concurrent_limit` | Semaphore caps concurrent calls (check call timing) |
| `test_enricher_retry_on_transient_error` | Retries on 429/5xx, returns success if retry succeeds |
| `test_enricher_no_retry_on_auth_error` | 401/403 fails immediately without retry |
| `test_enricher_timeout` | Slow provider triggers timeout error |
| `test_enricher_batch_partial_failure` | Some images fail, others succeed — no panic |

### Verification

```bash
cargo test -p photon-core -- enricher
```

---

## Phase 4 — Edge Case & Validation Tests

### 4A — Config Validation Tests

**File:** `crates/photon-core/src/config/validate.rs`

The assessment noted 5 of 9 validation rules are untested:

| Rule | Field | Constraint |
|------|-------|-----------|
| `buffer_size` | `pipeline.buffer_size` | Must be > 0 |
| `max_file_size_mb` | `limits.max_file_size_mb` | Must be > 0 |
| `max_image_dimension` | `limits.max_image_dimension` | Must be > 0 |
| `embed_timeout_ms` | `limits.embed_timeout_ms` | Must be > 0 |
| `llm_timeout_ms` | `limits.llm_timeout_ms` | Must be > 0 |

Add tests following the existing pattern (4 tests already exist):

```rust
#[test]
fn test_zero_buffer_size_rejected() {
    let mut config = Config::default();
    config.pipeline.buffer_size = 0;
    assert!(config.validate().is_err());
}
```

### 4B — Pipeline Edge Case Tests

**File:** New integration test or extend `tests/integration/`

| Test | What it verifies |
|------|-----------------|
| `test_zero_length_file` | Returns `FileNotFound` or appropriate error, no panic |
| `test_1x1_pixel_image` | Processes successfully (degenerate but valid) |
| `test_corrupt_jpeg_header` | Valid magic bytes, bad data → `Decode` error with context |
| `test_unicode_file_path` | Path with emoji/CJK characters processes correctly |

### Implementation Notes

- Use the existing `tests/fixtures/images/test.png` (70B minimal) as a base
- For zero-length: `File::create(path)` with no content
- For 1x1: generate programmatically with `image::RgbImage::new(1, 1)`
- For corrupt JPEG: write `FF D8 FF` header followed by garbage bytes
- For unicode: copy `test.png` to a path like `tests/fixtures/images/日本語テスト.png`

### Verification

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
```

---

## Commit Strategy

One commit per phase:

1. `refactor: split processor.rs into processor/tagging_loader/scoring modules`
2. `test: batch output hash loading — JSON array, JSONL, dual-stream formats`
3. `test: enricher — mock provider with retry, timeout, concurrency tests`
4. `test: edge cases — config validation, zero-length file, 1x1 image, corrupt JPEG, unicode paths`

After all phases: update `docs/plans/code-assessment.md` to mark Phase 3+4 resolved, bump changelog.
