# Merged Codebase Assessment — 2026-02-13

> Consolidated from `code-assessment.md` (automated) and `manual-codebase-assessment-130226.md` (manual review). Deduplicated, reconciled where they disagreed, and filtered to functionally blocking issues only.
>
> **Reviewed 2026-02-13**: All 13 findings verified against source code. 10 fully confirmed, 3 corrected (H3 downgraded to MEDIUM, H5 decode.rs:110 reclassified as benign, M1 reclassified — WalkDir has cycle detection). See `[Reviewed]` annotations on corrected findings.
>
> **Updated 2026-02-13**: All 5 HIGH findings fixed — see `docs/completions/high-severity-fixes.md`. 8 MEDIUM findings remain.

**Scope**: 55 Rust files, ~11.4K lines, 210 tests

---

## HIGH — Crash or Data Loss in Normal Operation

### H1. Lock Poisoning Panics Cascade Through Batch Jobs `[FIXED]`

**Files**: `processor.rs:430-457`, `progressive.rs:73-86`

6+ `.expect()` calls on `RwLock` acquisition for `TagScorer` and `RelevanceTracker`. If any thread panics while holding a lock (e.g. tagging OOM, malformed embedding), the lock becomes poisoned and every subsequent image triggers a panic. One bad image in a 10,000-image batch kills the remaining 9,999.

> Note: The automated assessment claimed "zero `expect()` in non-test code" — this was incorrect. These `.expect()` calls are in production hot paths.

**Fix**: Replace all `.expect()` on `RwLock` with `.map_err()` returning `PipelineError`. The processor should skip the tagging stage for that image and continue.

**Resolution**: 7 `.expect()` calls replaced. `save_relevance()` uses `.map_err()` → `PipelineError::Tagging`. Pool-aware scoring uses closure with `.ok()?` for read locks, `if let Ok()` for write lock. Simple scoring uses `match`. Poisoned locks degrade to empty tags — remaining pipeline output preserved.

---

### H2. No Embedding Dimension Validation in Scorer `[FIXED]`

**Files**: `scorer.rs:106-107`, `relevance.rs:147`

The scoring hot loop indexes `image_embedding[j]` with no bounds check. A corrupted, truncated, or wrong-model embedding causes an out-of-bounds panic mid-batch. Same issue in `relevance.rs:record_hits()` — `self.stats[idx]` indexed without bounds validation.

**Fix**: Validate `image_embedding.len() == dim` at the top of `score()`. Add bounds check in `record_hits()`. Return `PipelineError` on mismatch.

**Resolution**: `validate_embedding()` helper added. `score()` and `score_with_pools()` return `Result`. `score_pool()` uses `debug_assert_eq!` (private, called after validation). `record_hits()`, `pool()`, and `promote_to_warm()` all bounds-check with graceful fallback. +5 tests.

---

### ~~H3.~~ M8. Progressive Encoder Incomplete Vocabulary on Chunk Failure `[Reviewed — downgraded from HIGH to MEDIUM]`

**Files**: `tagging/progressive.rs:155-160`

~~When `running_bank.append(&chunk_bank)` fails, the loop continues to the next chunk without updating `encoded_indices`. On the next successful append, a new `TagScorer` is built from `encoded_indices` that's out of sync with the actual `running_bank` contents. This can cause index-out-of-bounds panics during scoring.~~

**Correction**: The original analysis misread the control flow. When `running_bank.append()` fails at line 155, the `continue` at line 158 skips **both** the `encoded_indices.extend()` at line 160 **and** the scorer rebuild at lines 162-176. So `running_bank` and `encoded_indices` remain in sync — neither is updated on failure. No index-out-of-bounds panic can occur from this path.

The actual impact is **quality degradation**: failed chunks mean the in-memory scorer operates with a smaller vocabulary (missing terms from failed chunks). The `all_chunks_succeeded` flag correctly prevents writing an incomplete cache to disk. This is a graceful degradation, not a crash.

**Fix**: Log a warning summarizing skipped chunks at the end of background encoding so users know the vocabulary is incomplete. Consider retrying failed chunks once before giving up.

---

### H4. Enricher Semaphore Leak on Callback Panic `[FIXED]`

**Files**: `enricher.rs:86-91`

If the `on_result` callback panics, `drop(permit)` never executes. After `parallel` panics (default 8), all subsequent enrichment tasks hang forever waiting for permits that will never be released.

```rust
let handle = tokio::spawn(async move {
    let result = enrich_single(&provider, &image, &options).await;
    on_result(result);   // if this panics...
    drop(permit);        // ...this never runs
    success
});
```

Additionally, if the semaphore closes mid-batch (e.g. task panic), the acquire loop silently `break`s — remaining images are never enriched with no warning logged.

> **Review note**: The bug is real, but the callbacks in practice are `println!`/`tx.send()` — extremely unlikely to panic. The fix is trivial and worth doing as defense-in-depth, but this is not an urgent crash risk in current usage.

**Fix**: Move `drop(permit)` before `on_result`, or use a drop guard (`scopeguard`) to guarantee permit release. Log a warning on unexpected semaphore closure.

**Resolution**: `drop(permit)` moved before `on_result(result)`. Semaphore closure now emits `tracing::warn!`. Also improves throughput — next image can start while callback runs.

---

### H5. Silent Error Swallowing in Validation and Decode `[FIXED]`

**Files**: `validate.rs:61`, `decode.rs:103`, `decode.rs:110`

| Location | What happens | Status |
|---|---|---|
| `validate.rs:61` | `file.read(&mut header).unwrap_or(0)` — I/O errors (permission denied, NFS timeout) become "file too small" | **Confirmed** — misleading error |
| `decode.rs:103` | Unknown image format silently defaults to JPEG instead of returning an error | **Confirmed** — silent wrong guess |
| `decode.rs:110` | `fs::metadata(path).unwrap_or(0)` — reports 0 bytes for files that were just successfully decoded | **Benign** — see note |

> **Review note on `decode.rs:110`**: This `unwrap_or(0)` is in `decode_sync()`, but the public `decode()` method reads `file_size` separately at lines 41-47 with proper `map_err()` propagation, then overwrites the value at line 72 with `decoded.file_size = file_size`. The bad value from `decode_sync` never reaches the output. Still worth cleaning up, but not a data loss bug.

The first two items are real issues — permission errors and misnamed files are common in batch processing.

**Fix**: Replace `unwrap_or()` with `map_err()` propagation in `validate.rs:61`. Return `UnsupportedFormat` for unknown formats in `decode.rs:103` instead of guessing JPEG. Clean up `decode.rs:110` for consistency (low priority).

**Resolution**: `validate.rs:61` now uses `.map_err()` → `PipelineError::Decode`. `decode.rs` uses `with_guessed_format()` for content-based detection, falls back to `ImageFormat::from_path()` → `UnsupportedFormat`. `decode.rs:110` kept with clarifying comment (benign — overwritten by caller).

---

### H6. Double Timeout in Enricher + Providers `[FIXED]`

**Files**: `enricher.rs:149-152`, `anthropic.rs:123`, `openai.rs:132`, `ollama.rs:90`

The enricher wraps every `provider.generate()` call in `tokio::time::timeout(options.timeout_ms)`, but each provider *also* applies `.timeout()` on the reqwest HTTP client (60s for anthropic/openai, 120s for ollama). Two competing timeouts:

- If the user raises `llm_timeout_ms` above the provider's hardcoded timeout, the provider timeout silently caps the actual duration — the enricher's retry logic never fires because it sees a provider error, not a timeout.
- If the enricher timeout is shorter, the provider's timeout is unreachable dead code.

**Fix**: Remove `.timeout()` from all provider HTTP calls. The enricher's `tokio::time::timeout` is the single source of truth and already handles the timeout case with retry logic.

**Resolution**: Removed `.timeout()` from `generate()` in all 3 providers (anthropic, openai, ollama). Kept Ollama's `is_available()` 5-second health-check timeout (separate concern). Kept `timeout()` trait method (public interface).

---

## MEDIUM — Robustness Gaps Under Load or Unusual Input

### M1. Symlink Cycle Errors Silently Swallowed `[Reviewed — reclassified]`

**File**: `discovery.rs:47-50`

~~`WalkDir::new(path).follow_links(true)` with no `max_depth`. A symlink cycle on a user-provided path causes infinite directory traversal, eventually exhausting memory or file descriptors.~~

**Correction**: `walkdir` has **built-in symlink cycle detection** — it detects cycles and emits them as `WalkDir::Error` entries. The actual bug is that `.filter_map(|e| e.ok())` on line 50 **silently swallows** all walkdir errors, including cycle detection errors, permission errors, and broken symlinks. The traversal won't be infinite, but the user gets no warning that files or directories were skipped.

**Fix**: Replace `.filter_map(|e| e.ok())` with explicit error handling that logs warnings for skipped entries. Adding `.max_depth(256)` is still worthwhile as defense-in-depth.

---

### M2. Unbounded Memory in CLI Enrichment and Batch Collection

**Files**: `cli/process/enrichment.rs:14`, `cli/process/batch.rs:42`

Two unbounded collection points:
- `run_enrichment_collect` uses `std::sync::mpsc::channel` (unbounded) — all enrichment patches accumulate in memory before being written.
- Batch processing collects all `ProcessedImage` results in a `Vec` before writing.

For large batches (thousands of images) with slow LLM providers, this negates the streaming architecture.

**Fix**: Use bounded channels (`sync_channel(64)`) for enrichment. Stream batch results to file incrementally.

---

### M3. Silent JSON Parse Failure in `--skip-existing`

**Files**: `cli/process/batch.rs:149-156`

When loading existing records for `--skip-existing`, deserialization failure falls through silently — the hash set remains empty and all images are reprocessed from scratch. No warning logged.

**Fix**: Log a warning when the existing output file can't be parsed, so users know `--skip-existing` had no effect.

---

### M4. Unbounded Image Read in Enricher

**File**: `enricher.rs:123`

`tokio::fs::read(&image.file_path).await` loads the entire file into memory with no size limit. With `parallel=8` and large images (100MB RAW files), this allocates ~800MB just for file reads.

**Fix**: Check file size before reading. Reject or skip files exceeding a configurable threshold (or reuse the existing `max_file_size_mb` config).

---

### M5. Progressive + Relevance Mutual Exclusion Not Enforced

**Files**: `processor.rs:190-191`, config validation

Config allows both `progressive.enabled = true` and `relevance.enabled = true` simultaneously. When progressive encoding is active, `relevance_tracker` is `None` — relevance pruning is silently disabled with no warning to the user.

**Fix**: Either enforce mutual exclusion in config validation (return `ConfigError`), or log a warning explaining that relevance is disabled when progressive mode is active.

---

### M6. Concurrency Hazards in Relevance Tracker

**File**: `processor.rs:448-462`

Neighbor expansion acquires a read lock on `scorer_lock` while holding a write lock on `tracker_lock`. If any other code path acquires these locks in the opposite order, it deadlocks. Currently no deadlock exists, but the lock ordering is undocumented and fragile.

**Fix**: Document the lock ordering invariant (`tracker_lock` before `scorer_lock`). Consider combining them into a single `RwLock<(TagScorer, RelevanceTracker)>` to eliminate the risk.

---

### M7. Missing Path Context in SigLIP Embedding Errors

**File**: `embedding/siglip.rs:72-87`

ONNX tensor creation and inference failures use `path: Default::default()` — producing an empty `PathBuf` in error context. When processing images concurrently, it's impossible to correlate embedding errors to specific files.

**Fix**: Pass the image path into `SigLipSession::infer()` for error context, or map the error at the caller where the path is available.

---

## Test Coverage Gaps (Context)

Not a "finding" per se, but relevant context for prioritizing the remaining MEDIUM fixes:

| Module | Lines | Unit Tests | Note |
|---|---|---|---|
| `processor.rs` | 577 | **1** | Orchestrates the entire pipeline; H1 fixed here |
| `progressive.rs` | 213 | **0** | M8 (formerly H3) lives here |
| `decode.rs` | 171 | **3** | H5 fixed here; content-based format test added |
| `validate.rs` | 205 | **8** | H5 fixed here (magic bytes tests only) |
| `scorer.rs` | 371 | **9** | H2 fixed here; +2 dimension mismatch tests |
| `relevance.rs` | 805 | **24** | H2 fixed here; +3 bounds safety tests |
| `enricher.rs` | 617 | **11** | H4/H6 fixed here; comprehensive mock provider tests |
| `discovery.rs` | 121 | **2** | M1 lives here |

210 tests total (37 CLI + 153 core + 20 integration). The HIGH findings now have test coverage. The modules containing MEDIUM findings (`discovery.rs`, `progressive.rs`) still have minimal unit tests.
