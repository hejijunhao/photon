# MEDIUM-Severity Fixes — Implementation Plan

> Date: 2026-02-13
> Source: 8 remaining MEDIUM findings from `docs/plans/merged-assessment.md`
> Prerequisite: v0.5.7 (210 tests, zero clippy warnings)

---

## Context

All 5 HIGH-severity findings from the merged codebase assessment have been resolved (v0.5.7). 8 MEDIUM findings remain — clustering around two themes: **silent failures** (M1, M3, M5, M7, M8) and **unbounded resource usage** (M2, M4). M6 is a latent concurrency hazard. This plan addresses all 8 in 5 phases, grouped by file proximity and dependency.

---

## Findings Summary

| Finding | Theme | File(s) | Effort |
|---------|-------|---------|--------|
| M1 | Silent failure | `discovery.rs` | Small |
| M2 | Unbounded memory | `cli/process/enrichment.rs`, `cli/process/batch.rs` | Medium |
| M3 | Silent failure | `cli/process/batch.rs` | Small |
| M4 | Unbounded memory | `llm/enricher.rs` | Small |
| M5 | Silent failure | `pipeline/processor.rs`, `config/validate.rs` | Small |
| M6 | Concurrency hazard | `pipeline/processor.rs` | Medium |
| M7 | Silent failure | `embedding/siglip.rs`, `embedding/mod.rs` | Small |
| M8 | Silent failure | `tagging/progressive.rs` | Small |

---

## Phase 1 — Silent failure logging (M1, M3, M8)

**Goal:** Add `tracing::warn!` to three modules where errors are silently swallowed. No behavioral changes — just visibility.

### Task 1a — WalkDir error logging (M1)

**File:** `crates/photon-core/src/pipeline/discovery.rs:47-51`

Replace `.filter_map(|e| e.ok())` with explicit error handling:

```rust
for entry in WalkDir::new(path)
    .follow_links(true)
    .max_depth(256)
    .into_iter()
{
    let entry = match entry {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("Skipping directory entry: {e}");
            continue;
        }
    };
    // ... rest unchanged
```

Changes:
- Add `.max_depth(256)` as defense-in-depth
- Replace `.filter_map(|e| e.ok())` with `match` + `tracing::warn!`

**Test:** Add `test_discover_logs_on_permission_error` — create a directory with a nested unreadable subdir (via `std::fs::set_permissions` on Unix), verify that discovered files from readable siblings are still found and the count is correct. This test would be platform-specific (`#[cfg(unix)]`).

### Task 1b — Skip-existing parse failure warnings (M3)

**File:** `crates/photon/src/cli/process/batch.rs`

**In `load_existing_hashes()` (lines 256-287):**
- After both JSON array parse attempts fail silently (lines 257, 265), before falling back to JSONL, add:
  ```rust
  tracing::debug!("Output file is not a JSON array — trying JSONL line-by-line");
  ```
- In the JSONL loop (lines 273-287), track unparseable lines:
  ```rust
  let mut skipped_lines = 0u64;
  // ... in the final if let Ok(...) else branch:
  skipped_lines += 1;
  // After loop:
  if skipped_lines > 0 {
      tracing::warn!(
          "--skip-existing: {skipped_lines} lines in output file could not be parsed — \
           those images will be reprocessed"
      );
  }
  ```

**In `process_batch()` (lines 152-156):**
- When the JSON merge-load fails, warn:
  ```rust
  if let Ok(records) = serde_json::from_str::<Vec<OutputRecord>>(&content) {
      existing_records = records;
  } else {
      tracing::warn!(
          "--skip-existing: failed to parse existing JSON output at {:?} — \
           existing records will not be merged",
          output_path
      );
  }
  ```

**Test:** Add `test_load_existing_hashes_warns_on_corrupt_jsonl` — write a file with 2 valid + 1 corrupt line, verify the 2 valid hashes are returned and the count is correct.

### Task 1c — Progressive encoder failure summary (M8)

**File:** `crates/photon-core/src/tagging/progressive.rs:127-211`

Track failed chunk count and log a summary at the end of `background_encode()`:

```rust
let mut all_chunks_succeeded = true;
let mut failed_chunks = 0usize;
let total_chunks = remaining_indices.chunks(ctx.chunk_size).len();

// ... in each failure branch:
failed_chunks += 1;

// After the for loop, before the cache-save decision:
if failed_chunks > 0 {
    tracing::warn!(
        "Progressive encoding: {failed_chunks}/{total_chunks} chunks failed — \
         vocabulary is incomplete ({} of {} terms encoded)",
        encoded_indices.len(),
        total_terms,
    );
}
```

**Test:** No new test needed — the existing logging paths are covered; this adds a summary that complements the per-chunk errors.

**Verification:** `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`

---

## Phase 2 — Enricher resource bounding (M4)

**Goal:** Prevent unbounded memory allocation when the enricher reads image files from disk.

### Task 2a — File size check before read

**File:** `crates/photon-core/src/llm/enricher.rs:118-132`

Add `max_file_size_mb` to `EnrichOptions`:

```rust
pub struct EnrichOptions {
    // ... existing fields ...
    /// Maximum file size in megabytes for enrichment reads.
    /// Files exceeding this are skipped with a warning.
    pub max_file_size_mb: u64,
}
```

Default to 100 (matching `LimitsConfig`).

In `enrich_single()`, before `tokio::fs::read()`:

```rust
// Guard: check file size before reading into memory
let metadata = match tokio::fs::metadata(&image.file_path).await {
    Ok(m) => m,
    Err(e) => {
        return EnrichResult::Failure(
            image.file_path.clone(),
            format!("Failed to stat image: {e}"),
        );
    }
};
let max_bytes = options.max_file_size_mb * 1024 * 1024;
if metadata.len() > max_bytes {
    return EnrichResult::Failure(
        image.file_path.clone(),
        format!(
            "Image too large for enrichment: {} MB (limit: {} MB)",
            metadata.len() / (1024 * 1024),
            options.max_file_size_mb
        ),
    );
}
```

### Task 2b — Wire `max_file_size_mb` through CLI

**File:** `crates/photon/src/cli/process/setup.rs` (or wherever `EnrichOptions` is constructed)

Pass `config.limits.max_file_size_mb` into `EnrichOptions::max_file_size_mb` when building the enricher.

**Test:** Add `test_enricher_skips_oversized_file` in `enricher.rs` — create a temp file over the limit, verify `EnrichResult::Failure` with "too large" message and `call_count == 0`.

**Verification:** `cargo test -p photon-core enricher`, `cargo clippy --workspace -- -D warnings`

---

## Phase 3 — Embedding error context (M7)

**Goal:** Ensure ONNX embedding errors include the image file path for debuggability.

### Task 3a — Add path parameter to `SigLipSession::embed()`

**File:** `crates/photon-core/src/embedding/siglip.rs:66`

Change signature:
```rust
pub fn embed(&self, preprocessed: &Array4<f32>, path: &Path) -> Result<Vec<f32>, PipelineError>
```

Replace all 6 instances of `path: Default::default()` with `path: path.to_path_buf()`.

### Task 3b — Thread path through `EmbeddingEngine::embed()`

**File:** `crates/photon-core/src/embedding/mod.rs:69-72`

Change signature:
```rust
pub fn embed(&self, image: &DynamicImage, path: &Path) -> Result<Vec<f32>, PipelineError> {
    let tensor = preprocess(image, self.image_size);
    self.session.embed(&tensor, path)
}
```

### Task 3c — Update callers

**File:** `crates/photon-core/src/pipeline/processor.rs:399`

The call `engine.embed(&image_clone)` becomes `engine.embed(&image_clone, &embed_path)` — `embed_path` already exists at line 396.

**Test:** No new test needed — the existing embedding error tests will now carry path context. Verify by inspection that all `PipelineError::Embedding` in `siglip.rs` now include a real path.

**Verification:** `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`

---

## Phase 4 — Config validation & lock ordering (M5, M6)

**Goal:** Warn on conflicting config and eliminate the nested lock hazard.

### Task 4a — Progressive + Relevance config warning (M5)

**File:** `crates/photon-core/src/config/validate.rs`

Add at the end of `validate()`, before `Ok(())`:

```rust
if self.tagging.progressive.enabled && self.tagging.relevance.enabled {
    tracing::warn!(
        "Both progressive encoding and relevance pruning are enabled. \
         Relevance pruning is disabled during progressive encoding — \
         it activates on subsequent runs when the full label bank is cached."
    );
}
```

**Test:** Add `test_validate_warns_on_progressive_and_relevance` — enable both, call `validate()`, assert it succeeds (warning only, not an error). The warning is logged via tracing, so we just confirm `validate()` returns `Ok(())`.

### Task 4b — Eliminate nested lock acquisition (M6)

**File:** `crates/photon-core/src/pipeline/processor.rs:442-474`

The current code acquires `scorer_lock.read()` (line 455) **while holding** `tracker_lock.write()` (line 444). Restructure to avoid nesting:

```rust
if let Some((tags, raw_hits)) = scoring_result {
    // Phase 2a: Record hits under WRITE lock
    let sweep_result = if let Ok(mut tracker) = tracker_lock.write() {
        tracker.record_hits(&raw_hits);

        // Periodic sweep (still under write lock — sweep mutates)
        if tracker.images_processed().is_multiple_of(self.sweep_interval)
            && tracker.images_processed() > 0
        {
            let promoted = tracker.sweep();
            let (active, warm, cold) = tracker.pool_counts();
            tracing::debug!("Pool sweep: {} active, {} warm, {} cold", active, warm, cold);
            Some(promoted)
        } else {
            None
        }
    } else {
        tracing::warn!("RelevanceTracker write lock poisoned — skipping hit recording");
        None
    };

    // Phase 2b: Neighbor expansion (NO lock held — read scorer, then write tracker)
    if let Some(promoted) = sweep_result {
        if !promoted.is_empty() && self.neighbor_expansion {
            // Read scorer WITHOUT holding tracker lock
            let cold_siblings = if let Ok(scorer) = scorer_lock.read() {
                let siblings = NeighborExpander::expand_all(
                    scorer.vocabulary(),
                    &promoted,
                );
                // Need tracker read access for pool() — use read lock (cheaper)
                if let Ok(tracker) = tracker_lock.read() {
                    siblings
                        .iter()
                        .filter(|&&i| tracker.pool(i) == Pool::Cold)
                        .copied()
                        .collect::<Vec<usize>>()
                } else {
                    vec![]
                }
            } else {
                vec![]
            };

            // Write-lock tracker only for the final promotion
            if !cold_siblings.is_empty() {
                if let Ok(mut tracker) = tracker_lock.write() {
                    tracker.promote_to_warm(&cold_siblings);
                    tracing::debug!(
                        "Neighbor expansion: {} promoted, {} siblings queued",
                        promoted.len(),
                        cold_siblings.len()
                    );
                }
            }
        }
    }

    tags
}
```

Key changes:
- The `tracker_lock.write()` is released before acquiring `scorer_lock.read()`
- Neighbor expansion uses a read lock on tracker for `pool()` checks, then a brief write lock for `promote_to_warm()`
- No nested locks — each lock acquisition is independent
- Add a comment documenting the lock ordering invariant:
  ```rust
  // LOCK ORDERING: scorer_lock and tracker_lock must never be held simultaneously
  // as write locks. Read-read is safe. Acquire one, release it, then acquire the other.
  ```

**Test:** No new test — this is a refactoring that preserves existing behavior. All 20 integration tests validate the tagging pipeline still works correctly.

**Verification:** `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`

---

## Phase 5 — CLI enrichment bounded channel (M2)

**Goal:** Replace unbounded channel with bounded `sync_channel` to cap memory during enrichment.

### Task 5a — Bounded channel in `run_enrichment_collect()`

**File:** `crates/photon/src/cli/process/enrichment.rs:14`

Replace:
```rust
let (tx, rx) = std::sync::mpsc::channel::<OutputRecord>();
```
With:
```rust
let (tx, rx) = std::sync::mpsc::sync_channel::<OutputRecord>(64);
```

This applies backpressure if the enricher produces results faster than they can be consumed. The buffer of 64 is generous — enrichment patches are small (~200 bytes each), so 64 patches ≈ 12KB.

### Task 5b — Document batch collection rationale

**File:** `crates/photon/src/cli/process/batch.rs:42`

The `results` Vec at line 42 is only populated when `ctx.llm_enabled || matches!(args.format, OutputFormat::Json)` (line 105). For JSON format, the full array must be held in memory (JSON requires `[...]` wrapper). For LLM enrichment, the enricher needs `Vec<ProcessedImage>` to know which images to enrich.

This is fundamentally required by the JSON output format. Add a clarifying comment:

```rust
// Collected results: needed for JSON array output (format requires all items)
// and for LLM enrichment (enricher needs image metadata). For JSONL-only
// without LLM, results are streamed directly (lines 84-91, 94-100) and
// this Vec stays empty.
let mut results = Vec::new();
```

No code change beyond the comment — the JSONL path already streams correctly.

**Test:** Verify existing `test_load_existing_hashes_*` tests still pass. No new test needed for the bounded channel — it's a drop-in replacement with identical API (only adds backpressure).

**Verification:** `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`, `cargo fmt --all -- --check`

---

## Phase Dependency Graph

```
Phase 1 (M1, M3, M8 — logging)     ── no deps
Phase 2 (M4 — enricher bounds)      ── no deps
Phase 3 (M7 — embedding path)       ── no deps
Phase 4 (M5, M6 — config + locks)   ── no deps
Phase 5 (M2 — bounded channel)      ── no deps
```

All phases are independent and can be executed in any order. Suggested serial order groups by risk: logging-only first, then small behavioral changes, then the lock restructuring.

---

## Expected Final Metrics

| Metric | Before | After |
|--------|--------|-------|
| Total tests | 210 | **~214** (+4) |
| Discovery tests | 2 | **3** (+1: permission error logging) |
| Enricher tests | 11 | **12** (+1: oversized file skip) |
| CLI batch tests | 6 | **7** (+1: corrupt JSONL warning) |
| Config validation tests | 11 | **12** (+1: progressive+relevance warning) |
| Clippy warnings | 0 | **0** |
| MEDIUM findings remaining | 8 | **0** |

---

## Verification (end-to-end)

After all phases:

```bash
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

All 8 MEDIUM findings resolved. Update `docs/plans/merged-assessment.md` with `[FIXED]` annotations.
