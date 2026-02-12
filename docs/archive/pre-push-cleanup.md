# Pre-Push Cleanup

> Context: Two independent code assessments confirmed 185 tests passing, zero clippy warnings, zero fmt violations. All 10 MEDIUM correctness fixes are real and verified. However, the structure assessment (`docs/completions/assessment-structure.md`) describes work that was partially or never completed. This plan addresses the gap before pushing.

---

## Blocking (must fix before push)

### Task 1 — Delete orphaned pipeline files

**Severity:** Medium — dead code duplicating production code, maintenance hazard

**Problem:** `scoring.rs` (76 lines) and `tagging_loader.rs` (242 lines) were created under `crates/photon-core/src/pipeline/` as part of a processor.rs decomposition that was never completed:
- Neither file is declared in `pipeline/mod.rs` (no `mod scoring;` or `mod tagging_loader;`)
- The original code was never removed from `processor.rs` (still 561 lines)
- Result: two orphaned files containing exact duplicates of production code

**Fix:** Delete both files. The code they contain still lives in `processor.rs` where it's actually compiled and used. A proper decomposition can be revisited later as a standalone task.

**Files:**
- Delete `crates/photon-core/src/pipeline/scoring.rs`
- Delete `crates/photon-core/src/pipeline/tagging_loader.rs`

**Verification:** `cargo test --workspace` — count stays at 185, no compilation errors.

---

### Task 2 — Correct assessment-structure.md

**Severity:** Medium — document claims completed work that doesn't exist

**Problem:** `docs/completions/assessment-structure.md` claims:
- 195 total tests (actual: 185)
- processor.rs reduced from 559→282 lines (actual: still 561)
- 6 enricher tests with MockProvider (none exist)
- 4 integration edge case tests (none exist)
- Phase 1 split "complete" (files created but never wired in)

Phases 2 (batch tests +2) and 4A (config validation tests +5) *were* completed. Phases 1, 3, and 4B were not.

**Fix:** Rewrite the document to reflect actual state. Update:
- Total tests: 185 (not 195)
- Phase 1: Mark as **not completed** — files were created but never wired into `mod.rs`, original code not removed. Now deleted (Task 1).
- Phase 2: Correct as-is (2 batch tests added, verified)
- Phase 3: Mark as **not completed** — zero enricher tests exist
- Phase 4A: Correct as-is (5 config validation tests added, verified)
- Phase 4B: Mark as **not completed** — zero integration edge case tests exist
- Final metrics table: update to accurate numbers

---

### Task 3 — Upgrade bare `.unwrap()` in progressive.rs

**Severity:** Low — inconsistent with codebase conventions, poor diagnostics on panic

**Problem:** Two `RwLock` unwrap calls in `crates/photon-core/src/tagging/progressive.rs` lack context messages:
- Line 120: `ctx.scorer_slot.read().unwrap()`
- Line 168: `ctx.scorer_slot.write().unwrap()`

Every other lock operation in the codebase uses `.expect("descriptive message")` (see `processor.rs` lines 432, 435, 444, 457, 491, 297, 298).

**Fix:** Replace with `.expect()`:
- Line 120: `.expect("TagScorer lock poisoned during background encoding read")`
- Line 168: `.expect("TagScorer lock poisoned during background encoding swap")`

**Verification:** `cargo clippy --workspace -- -D warnings` stays clean.

---

## Recommended (not blocking, but worth tracking)

### Task 4 — Tighten module visibility in lib.rs

**Severity:** Low — leaks internal types as public API

**Problem:** `crates/photon-core/src/lib.rs` declares all modules as `pub mod`. The intended public API is the explicit re-exports (`pub use config::Config`, etc.), but `pub mod pipeline` means downstream crates can also access `photon_core::pipeline::decode::ImageDecoder` and every other internal type. This makes it hard to refactor internals without breaking semver.

**Fix:** Change internal modules to `pub(crate) mod`:
```rust
// Keep pub — these have re-exported types consumers need
pub mod config;
pub mod error;
pub mod types;

// Change to pub(crate) — internal implementation
pub(crate) mod embedding;
pub(crate) mod llm;
pub(crate) mod math;
pub(crate) mod output;
pub(crate) mod pipeline;
pub(crate) mod tagging;
```

**Risk:** May break the CLI crate if it reaches through module paths instead of using re-exports. Requires checking all `use photon_core::` imports in `crates/photon/`.

**Note:** `EmbeddingEngine` and `OutputFormat`/`OutputWriter` are re-exported from `lib.rs`, so `pub(crate) mod embedding` and `pub(crate) mod output` should work. But verify before committing.

---

### Task 5 — Add missing enricher tests (from structure Phase 3)

**Severity:** Low — the enricher module has zero unit tests; retry logic is tested separately in `retry.rs`

**What was claimed but never written:**
- `test_enricher_basic_success` — single image enrichment returns correct description
- `test_enricher_retry_on_transient_error` — 429 → retry → success
- `test_enricher_no_retry_on_auth_error` — 401 fails immediately, no retries
- `test_enricher_timeout` — slow provider hits timeout
- `test_enricher_batch_partial_failure` — mixed success/failure batch
- `test_enricher_missing_image_file` — nonexistent file path → graceful failure

**Approach:** Create a `MockProvider` implementing `LlmProvider` with configurable responses, call counter, and optional delay. Tests need `#[tokio::test(flavor = "multi_thread")]` because `enrich_batch()` uses `tokio::spawn`.

---

### Task 6 — Add missing integration edge case tests (from structure Phase 4B)

**Severity:** Low — edge cases are handled by the validation layer which is tested, but explicit integration coverage is better

**What was claimed but never written:**
- `process_zero_length_file` — empty file → error (no panic)
- `process_1x1_pixel_image` — 1x1 PNG processes correctly
- `process_corrupt_jpeg_header` — `FF D8 FF` + garbage → `PipelineError::Decode`
- `process_unicode_file_path` — CJK characters in filename → correct processing

**File:** `crates/photon-core/tests/integration.rs`

---

## Execution order

1. Task 1 (delete orphans) — prerequisite for Task 2
2. Task 3 (`.expect()` upgrade) — independent, quick
3. Task 2 (fix assessment doc) — depends on Task 1 being done
4. `cargo test --workspace && cargo clippy --workspace -- -D warnings && cargo fmt --all -- --check`
5. Tasks 4–6 as follow-up if desired
