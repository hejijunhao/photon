# MEDIUM-Severity Fixes — Completion Notes

> Source plan: `docs/executing/medium-severity-fixes.md`
> Baseline: v0.5.7 (210 tests, zero clippy warnings)

---

## Phase 1 — Silent failure logging (M1, M3, M8)

### M1: WalkDir error logging (`discovery.rs`)

**What:** `WalkDir::into_iter().filter_map(|e| e.ok())` silently dropped directory traversal errors (permission denied, broken symlinks, I/O errors).

**Fix:** Replaced `.filter_map(|e| e.ok())` with explicit `match` + `tracing::warn!("Skipping directory entry: {e}")`. Added `.max_depth(256)` as defense-in-depth against pathological symlink cycles.

**File:** `crates/photon-core/src/pipeline/discovery.rs:47-57`

**Test added:** `test_discover_logs_on_permission_error` — creates a temp directory with a readable and an unreadable (`0o000`) subdirectory, verifies the readable file is still discovered and the count is correct. Unix-only (`#[cfg(unix)]`).

### M3: Skip-existing parse failure warnings (`batch.rs`)

**What:** `load_existing_hashes()` tried JSON array, then JSONL line-by-line, with no logging when either parse path failed. Corrupt lines were silently skipped, causing images to be reprocessed without explanation.

**Fix (3 changes):**
1. Added `tracing::debug!("Output file is not a JSON array — trying JSONL line-by-line")` after both JSON array parse attempts fail.
2. Added `skipped_lines` counter in the JSONL loop — logs `tracing::warn!("--skip-existing: {N} lines in output file could not be parsed — those images will be reprocessed")` when corrupt lines are found.
3. Added `tracing::warn!` in `process_batch()` when the JSON merge-load for `--skip-existing` fails to parse existing records (lines 154-160).

**File:** `crates/photon/src/cli/process/batch.rs:272-299` (JSONL loop), `batch.rs:154-160` (merge-load)

**Test added:** `test_load_existing_hashes_warns_on_corrupt_jsonl` — writes 2 valid + 1 corrupt JSONL line, verifies both valid hashes are returned and the count is 2.

### M8: Progressive encoder failure summary (`progressive.rs`)

**What:** `background_encode()` tracked failures with a boolean `all_chunks_succeeded` flag. When chunks failed, only per-chunk errors were logged — no summary showing how many chunks failed out of how many total.

**Fix:** Replaced `all_chunks_succeeded: bool` with `failed_chunks: usize` counter + `total_chunks` count. Added summary warning: `"Progressive encoding: {failed}/{total} chunks failed — vocabulary is incomplete ({encoded} of {total_terms} terms encoded). Skipping cache save."` Consolidated the two separate failure branches (old summary + new summary) into a single `if/else` on `failed_chunks`.

**File:** `crates/photon-core/src/tagging/progressive.rs:128-129` (counter), `progressive.rs:186-193` (summary)

**No new test** — the encoding paths require a real text encoder; the change is logging-only and covered by existing chunk error tests.

### Verification

- **212 tests passing** (38 CLI + 154 core + 20 integration) — +2 from v0.5.7 baseline
- Zero clippy warnings
- Zero formatting violations

---

## Phase 2 — Enricher resource bounding (M4)

### M4: File size check before enrichment read (`enricher.rs`)

**What:** `enrich_single()` called `tokio::fs::read(&image.file_path)` without any size check. A large file would be loaded entirely into memory before base64-encoding and sending to the LLM.

**Fix (3 changes):**
1. Added `max_file_size_mb: u64` field to `EnrichOptions` (default: 100, matching `LimitsConfig`).
2. Added `tokio::fs::metadata()` check before `tokio::fs::read()` in `enrich_single()` — files exceeding the limit return `EnrichResult::Failure` with a descriptive message. Stat errors also return failure (covers nonexistent files before the read).
3. Wired `config.limits.max_file_size_mb` into `EnrichOptions` in `cli/process/setup.rs:145-150`.

**Files:**
- `crates/photon-core/src/llm/enricher.rs:16-36` (struct + default)
- `crates/photon-core/src/llm/enricher.rs:124-144` (metadata guard)
- `crates/photon/src/cli/process/setup.rs:145-150` (CLI wiring)

**Side effect:** Existing `test_enricher_missing_image_file` assertion updated — nonexistent files now hit the stat check first ("Failed to stat image") instead of the read ("Failed to read image"). Same behavior, different error message.

**Test added:** `test_enricher_skips_oversized_file` — creates a temp file of 1 MB + 1 byte with `max_file_size_mb: 1`, verifies `EnrichResult::Failure` with "too large" message and `call_count == 0` (provider never called).

**Cleanup:** Simplified two test `EnrichOptions` literals to use `..fast_options()` instead of repeating all fields.

### Verification

- **213 tests passing** (38 CLI + 155 core + 20 integration) — +3 from v0.5.7 baseline
- Zero clippy warnings
- Zero formatting violations

---

## Phase 3 — Embedding error context (M7)

### M7: ONNX embedding errors now include file path (`siglip.rs`, `mod.rs`, `processor.rs`)

**What:** `SigLipSession::embed()` had 6 error sites using `path: Default::default()` (empty `PathBuf`). ONNX inference failures gave no indication which image triggered them.

**Fix (3 changes):**
1. Added `path: &Path` parameter to `SigLipSession::embed()` (`siglip.rs:66`). All 6 `path: Default::default()` replaced with `path: path.to_path_buf()`.
2. Added `path: &Path` parameter to `EmbeddingEngine::embed()` (`mod.rs:69`), threaded through to `session.embed()`.
3. Updated the sole call site in `processor.rs:399` — cloned `embed_path` into `embed_path_inner` for the `spawn_blocking` closure, keeping the original for the outer error handlers (task panic and timeout).

**Files:**
- `crates/photon-core/src/embedding/siglip.rs:66` (signature + 6 error sites)
- `crates/photon-core/src/embedding/mod.rs:69-71` (signature + passthrough)
- `crates/photon-core/src/pipeline/processor.rs:396-400` (caller update)

**No new test** — this is a pure debuggability improvement. Existing embedding tests don't exercise the ONNX session (no model in test env), and the path propagation is verified by inspection.

### Verification

- **213 tests passing** (38 CLI + 155 core + 20 integration) — unchanged from Phase 2
- Zero clippy warnings
- Zero formatting violations

---

## Phase 4 — Config validation & lock ordering (M5, M6)

### M5: Progressive + Relevance config warning (`validate.rs`)

**What:** When both `tagging.progressive.enabled` and `tagging.relevance.enabled` are `true`, relevance pruning is effectively disabled during progressive encoding (the full label bank isn't ready). No warning was given.

**Fix:** Added `tracing::warn!` at the end of `validate()` when both flags are `true`. Warning-only — does not return an error.

**File:** `crates/photon-core/src/config/validate.rs:68-74`

**Test added:** `test_validate_warns_on_progressive_and_relevance` — enables both, confirms `validate()` returns `Ok(())`.

### M6: Nested lock elimination (`processor.rs`)

**What:** The pool-aware scoring path acquired `tracker_lock.write()` (line 446) then `scorer_lock.read()` (line 457) while still holding the write lock. This created a nested lock pattern — while not currently deadlock-prone (both are reads on scorer), it's fragile. If anyone ever adds `scorer_lock.write()` + `tracker_lock.read()` elsewhere, classic ABBA deadlock results.

**Fix:** Restructured into three independent lock phases:

1. **Phase 2a** — `tracker_lock.write()`: record hits + sweep. Returns `Option<Vec<usize>>` (promoted indices). Lock released at end of `if let` block.
2. **Phase 2b** — `scorer_lock.read()`: expand neighbors of promoted terms. Then `tracker_lock.read()`: filter to cold-only siblings. Both released.
3. **Phase 2c** — `tracker_lock.write()`: promote cold siblings to warm. Brief lock, only if needed.

No lock is ever held while acquiring another. Added a comment documenting the lock ordering invariant.

**File:** `crates/photon-core/src/pipeline/processor.rs:444-510`

**No new test** — this is a refactoring that preserves existing behavior. All 20 integration tests validate the tagging pipeline still works correctly. The restructuring was verified via clippy and full test suite.

### Verification

- **214 tests passing** (38 CLI + 156 core + 20 integration) — +4 from v0.5.7 baseline
- Zero clippy warnings
- Zero formatting violations

---

## Phase 5 — CLI enrichment bounded channel (M2)

### M2a: Bounded channel in `run_enrichment_collect()` (`enrichment.rs`)

**What:** `std::sync::mpsc::channel()` is unbounded — if the enricher produces patches faster than the consumer reads them, memory grows without limit.

**Fix:** Replaced `channel()` with `sync_channel(64)`. The sender now blocks when 64 items are buffered, applying backpressure. 64 enrichment patches at ~200 bytes each = ~12KB — generous enough to avoid throttling while preventing unbounded growth.

**File:** `crates/photon/src/cli/process/enrichment.rs:14`

### M2b: Document batch results Vec rationale (`batch.rs`)

**What:** The `results` Vec in `process_batch()` collects all `ProcessedImage` structs in memory when JSON output or LLM enrichment is active. This was flagged as potential unbounded memory but is fundamentally required.

**Fix:** Added a clarifying comment explaining why the Vec exists and when it stays empty (JSONL-only without LLM streams zero-copy).

**File:** `crates/photon/src/cli/process/batch.rs:42-46`

**No new test** — `sync_channel` is a drop-in replacement with identical API (only adds backpressure). Existing enrichment and batch tests cover the channel usage.

### Verification

- **214 tests passing** (38 CLI + 156 core + 20 integration) — unchanged from Phase 4
- Zero clippy warnings
- Zero formatting violations

---

## Final Summary

| Metric | Before (v0.5.7) | After |
|--------|-----------------|-------|
| Total tests | 210 | **214** (+4) |
| Discovery tests | 2 | **3** (+1) |
| Enricher tests | 11 | **12** (+1) |
| CLI batch tests | 6 | **7** (+1) |
| Config validation tests | 11 | **12** (+1) |
| Clippy warnings | 0 | **0** |
| MEDIUM findings remaining | 8 | **0** |

### Files modified

| File | Phase | Changes |
|------|-------|---------|
| `photon-core/src/pipeline/discovery.rs` | 1 | WalkDir error logging + max_depth + test |
| `photon/src/cli/process/batch.rs` | 1, 5 | JSONL parse warnings, JSON merge warning, results comment |
| `photon-core/src/tagging/progressive.rs` | 1 | Failed chunk counter + summary |
| `photon-core/src/llm/enricher.rs` | 2 | File size guard + max_file_size_mb field + test |
| `photon/src/cli/process/setup.rs` | 2 | Wire max_file_size_mb |
| `photon-core/src/embedding/siglip.rs` | 3 | Path parameter + 6 error sites |
| `photon-core/src/embedding/mod.rs` | 3 | Path parameter passthrough |
| `photon-core/src/pipeline/processor.rs` | 3, 4 | embed path threading, lock ordering refactor |
| `photon-core/src/config/validate.rs` | 4 | Progressive+relevance warning + test |
| `photon/src/cli/process/enrichment.rs` | 5 | Bounded sync_channel(64) |
