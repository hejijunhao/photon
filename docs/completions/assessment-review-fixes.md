# Assessment Review — Fixes Completion

> Completed: 2026-02-13
> Source plan: `docs/executing/assessment-review-fixes.md` (Phases 1–6)
> Prerequisite: v0.5.5 (195 tests, zero clippy warnings)

---

## Summary

Implemented all 6 phases from the assessment review plan, addressing documentation fabrication, missing test assertions, and test coverage gaps across 9 findings (F1–F9, with F8–F9 deferred).

| Phase | Scope | Files Changed | Tests Added |
|-------|-------|---------------|-------------|
| **1** | Fix documentation fabrication (F1) | 2 docs | 0 |
| **2** | Strengthen enricher assertions (F2) | 1 file | 0 (existing tests strengthened) |
| **3** | Add enricher test coverage (F3) | 1 file | +4 new tests |
| **4** | Tighten integration assertions (F4, F5) | 1 file | 0 (existing tests strengthened) |
| **5** | Add boundary condition tests (F6) | 1 file | +4 new tests |
| **6** | Add skip-options tests (F7) | 1 file | +2 new tests |
| **Total** | | **4 files** | **+10 new tests** |

### Final Metrics

| Metric | Before | After |
|--------|--------|-------|
| Enricher tests | 6 | **10** (+4) |
| Integration tests | 14 | **20** (+6) |
| Total tests | 195 | **205** (+10) |
| Documentation inaccuracies | 2 docs | **0** |
| Clippy warnings | 0 | **0** |
| Formatting violations | 0 | **0** |

---

## Phase 1 — Fix documentation fabrication (F1)

**Finding:** `assessment-structure.md` and `changelog.md` v0.5.5 claimed `processor.rs` was split into 3 files (559 -> 282 lines) with `tagging_loader.rs` and `scoring.rs` extracted. Neither file existed; `processor.rs` was still 561 lines.

### Files modified

**`docs/completions/assessment-structure.md`:**
- Summary: "Executed all 4 phases" -> "Executed Phases 2-4. Phase 1 (processor split) was descoped."
- Phase 1 table row: marked "Descoped" with reference to `final-cleanup.md` Phase 2
- Files changed count: 13 -> 9
- Metrics table: `processor.rs` lines 559 -> 282 changed to 561 -> 561 (unchanged)
- Phase 1 section: full fabricated description replaced with descoped note
- Phase 3 MockProvider description: corrected from "response queue" to "factory function pattern"
- Design decisions: corrected constructor names to `success()`, `failing()`, `fail_then_succeed()`

**`docs/changelog.md` (v0.5.5 entry):**
- Index line: removed "processor.rs split" from summary
- Summary paragraph: removed "split `processor.rs` into 3 focused files (559 -> 282 lines)"
- Changed section: removed the `processor.rs` split bullet entirely

---

## Phase 2 — Strengthen enricher test assertions (F2)

**Finding:** `test_enricher_no_retry_on_auth_error` and `test_enricher_missing_image_file` tested only final results, not provider call counts — they'd pass even if retries happened or the provider was called.

### Changes to `crates/photon-core/src/llm/enricher.rs`

**MockProvider refactored:**
- `call_count` field changed from `AtomicU32` to `Arc<AtomicU32>` for shared access
- Added `call_count_handle()` method returning `Arc<AtomicU32>` clone
- Added `in_flight` field `Option<(Arc<AtomicU32>, Arc<AtomicU32>)>` for concurrency tracking in `generate()`

**`test_enricher_no_retry_on_auth_error`:**
- Captures `call_count` handle before provider is consumed
- Added: `assert_eq!(call_count.load(Ordering::SeqCst), 1)` — verifies exactly one call (no retries on 401)

**`test_enricher_missing_image_file`:**
- Captures `call_count` handle before provider is consumed
- Added: `assert_eq!(call_count.load(Ordering::SeqCst), 0)` — verifies provider was never called (file read short-circuits)

---

## Phase 3 — Add enricher test coverage (F3)

**Finding:** Enricher's semaphore-based concurrency, retry exhaustion, empty batch, and 5xx retry paths were completely untested.

### 4 new tests in `crates/photon-core/src/llm/enricher.rs`

| Test | What it verifies |
|------|-----------------|
| `test_enricher_semaphore_bounds_concurrency` | 6 images with `parallel=2` and 200ms delay: `in_flight` counter never exceeds 2. Uses `multi_thread` with 4 worker threads for real parallelism. |
| `test_enricher_exhausts_retries` | Always-failing 429 with `retry_attempts=2`: asserts `call_count == 3` (1 initial + 2 retries) and error message preserved. |
| `test_enricher_empty_batch` | `enrich_batch(&[], ...)` returns `(0, 0)`, callback never invoked, provider never called. |
| `test_enricher_retry_on_server_error` | 500 on first call, success on retry: asserts `call_count == 2` and success result. Complements existing 429 test. |

---

## Phase 4 — Tighten integration test assertions (F4, F5)

**Finding:** Several integration tests used overly lenient error matching or missing assertions.

### Changes to `crates/photon-core/tests/integration.rs`

| Test | Change |
|------|--------|
| `process_zero_length_file` | Removed `FileTooLarge` as acceptable variant — 0-byte files always hit `Decode` (magic bytes check). Added path assertion. |
| `process_corrupt_jpeg_header` | Added `assert!(!message.is_empty())` to verify decode error carries a descriptive message. |
| `process_1x1_pixel_image` | Added `assert!(result.perceptual_hash.is_some())` and `assert!(result.thumbnail.is_some())`. |
| `process_unicode_file_path` | Added `assert!(result.width > 0)`, `height > 0`, `file_size > 0` to guard against silent partial processing. |

---

## Phase 5 — Add boundary condition tests (F6)

**Finding:** The v0.5.3 off-by-one fix for file size validation had no regression test at the exact boundary.

### 4 new tests in `crates/photon-core/tests/integration.rs`

| Test | Setup | Assertion |
|------|-------|-----------|
| `test_file_size_at_exact_limit` | 2x2 PNG padded to exactly `1 MB` with `max_file_size_mb=1` | Succeeds |
| `test_file_size_one_byte_over_limit` | Same PNG padded to `1 MB + 1 byte` | Fails with `FileTooLarge` |
| `test_image_dimension_at_exact_limit` | 100x1 PNG with `max_image_dimension=100` | Succeeds, correct dimensions |
| `test_image_dimension_one_over_limit` | 101x1 PNG with `max_image_dimension=100` | Fails with `ImageTooLarge` |

Implementation: uses `image::RgbImage::new()` for real PNGs, file padding via `OpenOptions::append` for size tests.

---

## Phase 6 — Add skip-options tests (F7)

**Finding:** `ProcessOptions` with multiple skip flags was never exercised in combination.

### 2 new tests in `crates/photon-core/tests/integration.rs`

| Test | Skip flags | Key assertions |
|------|-----------|----------------|
| `test_process_with_all_skips` | All 4 flags `true` | `thumbnail=None`, `perceptual_hash=None`, `embedding=[]`, `tags=[]`; core fields (`content_hash`, `width`, `height`, `file_size`) still populated |
| `test_process_with_selective_skips` | `skip_thumbnail=true`, `skip_embedding=true` (others false) | `thumbnail=None`, `perceptual_hash=Some(...)`, `embedding=[]`, `tags=[]` (embedding skipped -> tagging has no input) |

---

## Out of Scope (deferred)

| Item | Reference |
|------|-----------|
| Processor.rs decomposition | F1 — covered by `final-cleanup.md` Phase 2 |
| Enricher triple-unwrap hardening | Covered by `final-cleanup.md` Phase 3 |
| Sweep logic -> RelevanceTracker refactor | F9 — architectural change, needs design |
| Output roundtrip struct equality | F8 — low impact, deferred |
