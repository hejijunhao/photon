# Speed Improvement — Phase 2: Concurrent Batch Processing

> Completed: 2026-02-13
> Ref: `docs/executing/speed-improvement-plan.md`, Phase 2 (Tasks 2.1–2.2)
> Tests: 214 passing (38 CLI + 156 core + 20 integration), zero clippy warnings

---

## Summary

Batch image processing converted from sequential (one image at a time) to concurrent using `futures_util::StreamExt::buffer_unordered(parallel)`. While one image waits on the ONNX mutex for embedding, others decode/hash/thumbnail on tokio's blocking thread pool. Expected **4-8x throughput** on multi-core machines.

Single file changed. No changes to `photon-core`. No new dependencies.

---

## Task 2.1: Concurrent pipeline with `buffer_unordered`

**File changed:** `crates/photon/src/cli/process/batch.rs`

**Problem:** `for file in &files { processor.process(...).await }` — one image at a time. On an 8-core machine, ~12.5% CPU utilization.

**Fix:**

- **Destructured `ProcessContext`** — `processor` and `options` moved into `Arc` for concurrent sharing. Enricher, config, and flags remain as plain locals for the single-threaded post-loop code.

- **Pre-filtered skip-existing** — Hash checks now run before the concurrent pipeline so skipped files don't occupy a concurrency slot. Previously, every file entered the loop regardless.

- **Replaced `for` loop with `buffer_unordered` stream:**
  ```rust
  let mut result_stream = stream::iter(files_to_process)
      .map(|file| {
          let proc = Arc::clone(&processor);
          let opts = Arc::clone(&options);
          async move {
              let result = proc.process_with_options(&file.path, &opts).await;
              (file, result)
          }
      })
      .buffer_unordered(parallel);
  ```

- **Single-threaded result consumption** — The `while let Some(...)` loop handles stdout/file output, counters, and progress bar updates sequentially. No synchronization needed on the consumer side.

**Why this works without changes to photon-core:**
- `ImageProcessor::process_with_options()` takes `&self` — safe to share via `Arc`
- `EmbeddingEngine` wraps ONNX `Session` in `Mutex` — serializes inference automatically
- `TagScorer` behind `Arc<RwLock<>>` — concurrent read scoring
- `RelevanceTracker` behind `RwLock` — brief write locks for hit recording
- Decode/hash/thumbnail run in `spawn_blocking` — true OS-level parallelism

---

## Task 2.2: Wire `args.parallel` to batch concurrency

**File changed:** `crates/photon/src/cli/process/batch.rs`

**Problem:** `args.parallel` (default 4) only controlled LLM enrichment concurrency. The core pipeline ignored it entirely.

**Fix:** `args.parallel` now sets the `buffer_unordered` limit for concurrent image processing. Clamped to `max(1)` to prevent zero-concurrency. LLM enrichment cap remains at `args.parallel.min(8)` (unchanged, in `setup.rs`).

---

## How concurrency overlaps pipeline stages

```
Image A:  [Read] [Hash] [Decode] [EXIF] [Thumb] [Preprocess] [  ONNX  ] [Tag]
Image B:         [Read] [Hash]  [Decode] [EXIF] [Thumb] [Preprocess] [wait..] [ONNX] [Tag]
Image C:                [Read]  [Hash]  [Decode] [EXIF] [Thumb] [Preprocess] [wait.........] [ONNX]
Image D:                        [Read]  [Hash]  [Decode] [EXIF] [Thumb] [Preprocess] [wait..........]
                                                                        ↑
                                                               ONNX Mutex serializes,
                                                               but everything before it
                                                               runs in parallel on the
                                                               blocking thread pool.
```

---

## Files Modified

| File | Changes |
|------|---------|
| `crates/photon/src/cli/process/batch.rs` | `ProcessContext` destructuring, `Arc` wrapping, skip-existing pre-filter, `buffer_unordered` stream, post-loop reference updates |

## Edge Cases Handled

- `--parallel 0` → clamped to 1 (sequential fallback)
- `--parallel 1` → degenerates to sequential, negligible `Arc` overhead
- Empty file list after pre-filter → stream produces nothing, post-loop writes empty results
- Progress bar total remains `files.len()` (includes skipped files from pre-filter)
- JSONL output order may differ from input order (JSONL is order-independent by design)
