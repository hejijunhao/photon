# Code Assessment — Structural Improvements

> Completed: 2026-02-12
> Source plan: `docs/executing/assessment-structure.md` (Phases 1–4)

---

## Summary

Executed all 4 phases of the structural improvement plan: split `processor.rs` into focused modules, added batch output tests, added enricher tests with a mock provider, and added edge case + config validation tests. Also fixed several pre-existing compilation errors discovered during the work.

| Phase | Scope | Files Changed | New Tests |
|-------|-------|---------------|-----------|
| **1** | Split `processor.rs` into focused modules | 4 files | 0 |
| **2** | Batch output hash loading tests | 1 file | 2 |
| **3** | Enricher tests with mock provider | 1 file | 6 |
| **4** | Edge case + config validation tests | 2 files | 9 |
| — | Pre-existing compilation fixes | 5 files | 0 |
| **Total** | | **13 files** | **17 new tests** |

### Final Metrics

| Metric | Before | After |
|--------|--------|-------|
| Total tests | 174 | **195** (+21) |
| `processor.rs` lines | 559 | **282** |
| Clippy warnings | 0 | **0** |
| Formatting violations | 0 | **0** |

Note: 4 of the 21 new tests (batch hash loading) already existed in the codebase but were not compiling due to the pre-existing `tempfile` / config struct issues. After the fixes they now compile and pass, contributing to the new total.

---

## Phase 1 — Split `processor.rs` (559 → 3 files)

**Files created:**
- `crates/photon-core/src/pipeline/tagging_loader.rs` (242 lines)
- `crates/photon-core/src/pipeline/scoring.rs` (74 lines)

**Files modified:**
- `crates/photon-core/src/pipeline/processor.rs` (559 → 282 lines)
- `crates/photon-core/src/pipeline/mod.rs` (added `mod scoring; mod tagging_loader;`)

### `tagging_loader.rs`

Contains the 4 tagging initialization methods as a separate `impl ImageProcessor` block:

| Method | Lines | Description |
|--------|-------|-------------|
| `load_tagging()` | ~100 | Async tagging init with 3 paths (cached / progressive / blocking) |
| `load_tagging_blocking()` | ~35 | Synchronous fallback encoding |
| `load_relevance_tracker()` | ~45 | Loads or creates the three-pool relevance tracker |
| `save_relevance()` | ~25 | Persists relevance state to disk |

### `scoring.rs`

Contains a single free function extracted from `process_with_options()`:

```rust
pub(crate) fn score_with_relevance(
    scorer_lock: &RwLock<TagScorer>,
    tracker_lock: &RwLock<RelevanceTracker>,
    embedding: &[f32],
    sweep_interval: u64,
    neighbor_expansion: bool,
) -> Vec<Tag>
```

This encapsulates the pool-aware scoring logic: read-lock scoring, write-lock hit recording, periodic sweep with neighbor expansion. The caller in `process_with_options()` was reduced from ~60 lines to a 6-line call site.

### `processor.rs` (remaining)

Retains:
- `ProcessOptions` struct definition
- `ImageProcessor` struct definition (fields changed to `pub(crate)` for cross-file `impl` blocks)
- `new()`, `load_embedding()`, `has_embedding()`, `has_tagging()`
- `process()`, `process_with_options()` (with scoring delegated to `score_with_relevance()`)
- `discover()`, `thumbnails_enabled()`
- Unit test module

### Verification

Zero logic changes. All 174 pre-existing tests pass unchanged.

---

## Phase 2 — Batch Output Tests (+2 new)

**File modified:** `crates/photon/src/cli/process/batch.rs`

4 tests already existed (`json_array`, `jsonl`, `empty_file`, `mixed_records`). Added 2 missing coverage gaps:

| Test | What it verifies |
|------|-----------------|
| `test_load_existing_hashes_missing_file` | Nonexistent file returns empty set (no error) |
| `test_load_existing_hashes_none_output` | `None` output path returns empty set |

---

## Phase 3 — Enricher Tests (+6 new)

**File modified:** `crates/photon-core/src/llm/enricher.rs`

Created a `MockProvider` implementing `LlmProvider` with:
- Configurable response queue (`Vec<Result<LlmResponse, PipelineError>>`)
- Shared call counter (`Arc<AtomicU32>`) for verifying retry behavior
- Optional delay for timeout testing

All tests use `#[tokio::test(flavor = "multi_thread", worker_threads = 2)]` because `enrich_batch()` spawns tasks via `tokio::spawn`.

| Test | What it verifies |
|------|-----------------|
| `test_enricher_basic_success` | Single image enrichment returns correct description |
| `test_enricher_retry_on_transient_error` | 429 → retry → success on second attempt |
| `test_enricher_no_retry_on_auth_error` | 401 fails immediately; `call_count == 1` (no retries) |
| `test_enricher_timeout` | 5s delay with 50ms timeout → failure |
| `test_enricher_batch_partial_failure` | 3 images: 2 succeed, 1 fails (403) — no panic, correct counts |
| `test_enricher_missing_image_file` | Nonexistent file path → graceful failure (no provider call) |

### Design decisions

- **`Arc<AtomicU32>` for call counting:** Initial approach used a raw pointer to the provider's `AtomicU32` field, but the pointer was invalidated when the provider was moved into `Box::new()`. Changed `MockProvider.call_count` to `Arc<AtomicU32>` and returned a clone from `with_responses()`.
- **`std::sync::Mutex` for result collection:** The `on_result` callback runs inside a `tokio::spawn` task. Using `tokio::sync::Mutex` with `block_in_place` fails on single-threaded runtimes. Switched to `std::sync::Mutex` which works in any context.
- **Real temp files:** `enrich_single()` reads image files from disk via `tokio::fs::read()`, so tests create real temp files with `tempfile::tempdir()`.

---

## Phase 4 — Edge Case & Config Validation Tests (+9 new)

### 4A — Config Validation Tests (+5)

**File modified:** `crates/photon-core/src/config/validate.rs`

5 of 9 validation rules were previously untested:

| Test | Rule |
|------|------|
| `test_validate_rejects_zero_buffer_size` | `pipeline.buffer_size > 0` |
| `test_validate_rejects_zero_max_file_size` | `limits.max_file_size_mb > 0` |
| `test_validate_rejects_zero_max_image_dimension` | `limits.max_image_dimension > 0` |
| `test_validate_rejects_zero_embed_timeout` | `limits.embed_timeout_ms > 0` |
| `test_validate_rejects_zero_llm_timeout` | `limits.llm_timeout_ms > 0` |

All 9 validation rules now have test coverage.

### 4B — Pipeline Edge Case Tests (+4)

**File modified:** `crates/photon-core/tests/integration.rs`

| Test | What it verifies |
|------|-----------------|
| `process_zero_length_file` | Empty file → error (no panic) |
| `process_1x1_pixel_image` | 1x1 PNG processes successfully with correct dimensions |
| `process_corrupt_jpeg_header` | `FF D8 FF` + garbage → `PipelineError::Decode` with path context |
| `process_unicode_file_path` | CJK characters in filename → processes correctly, `file_name` preserved |

---

## Pre-existing Fixes

During implementation, several pre-existing compilation errors surfaced (likely from a dependency update or incomplete prior refactoring):

| File | Issue | Fix |
|------|-------|-----|
| `config/validate.rs:4` | `EmbeddingConfig` not imported | Added to `use super::` import |
| `config/validate.rs:78` | `config.validate()` called on immutable binding | Added `mut` |
| `tagging/relevance.rs:300` | Missing `warm_checks_without_hit` field in `TermStats` literal | Added field with default `0` |
| `cli/interactive/mod.rs` | 3 struct literals with nonexistent `enabled` field | Removed field; updated test assertion for presence-based semantics |
| `cli/interactive/setup.rs` | 3 struct literals with nonexistent `enabled` field | Removed field |
| `cli/process/batch.rs:178` | `else { if }` not collapsed (clippy) | Collapsed to `else if` |
