# Speed Improvement — Phase 5: Batch ONNX Inference + Benchmark Validation

> Completed: 2026-02-13
> Ref: `docs/executing/speed-improvement-plan.md`, Phase 5 (Tasks 5.1–5.2)
> Tests: 226 passing (40 CLI + 166 core + 20 integration), zero clippy warnings

---

## Summary

Two additions completing the speed improvement plan. Batch ONNX embedding inference amortizes session dispatch overhead by stacking N preprocessed tensors into a single `[N, 3, H, W]` ONNX call — following the exact pattern already used by `SigLipTextEncoder::encode_batch()`. The benchmark suite is fixed (5 existing benchmarks referenced stale `pub(crate)` APIs) and expanded with 5 new benchmarks covering scoring throughput, preprocessing, end-to-end pipeline, and batch throughput.

Five files changed. No new dependencies. +6 net new tests. 10 benchmarks (5 fixed + 5 new).

---

## Task 5.1: Batch ONNX Embedding Inference

### Approach: API-only

Batch methods added to `SigLipSession` and `EmbeddingEngine`. The processor pipeline continues using single-image `embed_preprocessed()` — the batch API is available for direct library users and future GPU support.

**Why not restructure the pipeline?** On CPU (our only target), ONNX Runtime parallelizes single-image compute internally via its thread pool. Batch inference amortizes dispatch overhead (mutex acquisition, tensor creation, output extraction — ~1ms per image) but does not unlock new parallelism. The `buffer_unordered` concurrency from Phase 2 already overlaps decode/hash/thumbnail across images while one holds the ONNX mutex. A channel-based batch accumulator would add latency (images wait for a full batch) and complexity for marginal CPU benefit.

### `SigLipSession::embed_batch()`

**File changed:** `embedding/siglip.rs`

New method accepts `&[Array4<f32>]` tensors and `&[PathBuf]` paths:

1. **Validate shapes** — all tensors must have identical `[1, 3, H, W]` shape. Mismatch returns an error before any ONNX dispatch.

2. **Build flat batch tensor** — iterate tensors, extend a single `Vec<f32>` with capacity `N × single_len`. Construct shape `[N, 3, H, W]`. Passed to `Value::from_array((shape, flat_data))` — same `(Vec<i64>, Vec<f32>)` tuple pattern as the single-image path.

3. **Single ONNX call** — one `session.run()` with the batched input. Mutex acquired once for the entire batch.

4. **Split output** — `pooler_output` is `[N, 768]`. Chunk by embedding dimension, take N, L2-normalize each. Identical to `SigLipTextEncoder::encode_batch()` lines 144–148.

5. **Edge cases** — empty input returns empty vec immediately. Batch of 1 is a degenerate case that works identically to the single-image path (shape `[1, 3, H, W]`).

### `EmbeddingEngine` batch methods

**File changed:** `embedding/mod.rs`

- **`embed_batch_preprocessed(tensors, paths)`** — delegates directly to `SigLipSession::embed_batch()`. For callers that have already preprocessed tensors (e.g., from `spawn_blocking` preprocessing).

- **`embed_batch(images)`** — convenience wrapper that preprocesses each `(&DynamicImage, &Path)`, then calls the batch session method. Single-step API for library users.

### Tests (+6 new)

| Test | Requires Model | What it validates |
|------|---------------|-------------------|
| `test_embed_batch_empty_returns_empty` | No | Empty input fast path |
| `test_embed_batch_shape_mismatch_detected` | No | Mismatched 224/384 tensors caught |
| `test_batch_tensor_stacking` | No | Flat data layout: correct interleaving of N tensors |
| `test_embed_batch_empty_input` | Yes | Engine-level empty batch through ONNX |
| `test_embed_batch_single_matches_single` | Yes | Batch-of-1 produces identical embedding to single `embed_preprocessed()` |
| `test_embed_batch_multiple_normalized` | Yes | Batch of 2 distinct images returns 2 L2-normalized 768-dim vectors |

Model-dependent tests skip gracefully when ONNX models are not on disk.

---

## Task 5.2: Benchmark Validation

### Visibility fixes for benchmarks

Benchmarks are external binaries that can only access `pub` types. Several pipeline types used by existing benchmarks were `pub(crate)`.

**`pipeline/mod.rs`** — Added re-exports: `ImageDecoder`, `ThumbnailGenerator`, `MetadataExtractor`.

**`lib.rs`** — Extended re-exports to include these three types plus `preprocess_image` (re-export of `embedding::preprocess::preprocess`). No module visibility changes — `lib.rs` can re-export `pub(crate)` items as `pub` since it's inside the crate.

### Fixed 5 existing benchmarks

- `photon_core::pipeline::Hasher` → `photon_core::Hasher` (all hash benchmarks)
- `photon_core::pipeline::ImageDecoder` → `photon_core::ImageDecoder`
- `photon_core::pipeline::ThumbnailGenerator` → `photon_core::ThumbnailGenerator`
- `photon_core::pipeline::MetadataExtractor` → `photon_core::MetadataExtractor`
- `Hasher::perceptual_hash(&img)` → `hasher.perceptual_hash(&img)` (changed from associated function to instance method in Phase 1)
- `decoder.decode(&path)` → `decoder.decode_from_bytes(bytes, &path)` (API changed in Phase 1's read-once I/O)

### Added 5 new benchmarks

| Benchmark | Models Required | Data Source | What it measures |
|-----------|----------------|-------------|------------------|
| `score_68k_matvec` | No | Synthetic 68K×768 `ndarray::Array2` | Raw BLAS mat-vec — the exact operation `TagScorer::score()` performs |
| `preprocess_224` | No | Synthetic 4032×3024 image | Resize + normalize to 224×224 tensor |
| `preprocess_384` | No | Synthetic 4032×3024 image | Resize + normalize to 384×384 tensor |
| `process_e2e_dog_jpg` | Yes (skip if absent) | `dog.jpg` fixture + ONNX models | Full pipeline: decode → hash → embed → tag |
| `batch_4_images` | Yes (skip if absent) | All 4 fixtures + ONNX models | 4 images via `buffer_unordered(4)` — concurrent pipeline throughput |

**Scoring benchmark** uses raw `ndarray::Array2::dot()` with a synthetic matrix. No internal types needed — this measures the exact BLAS operation that powers the tagging hot path. On macOS with Accelerate, this dispatches to `sgemv`.

**E2E and batch benchmarks** use the public `ImageProcessor` API with a `tokio::runtime::Runtime`. Both skip gracefully with `eprintln` if models are not found, so CI runs without model files.

---

## Files Modified

| File | Changes |
|------|---------|
| `crates/photon-core/src/embedding/siglip.rs` | `embed_batch()` method (~95 lines): shape validation, flat tensor stacking, single ONNX call, output splitting with L2 normalization. 3 unit tests. |
| `crates/photon-core/src/embedding/mod.rs` | `embed_batch_preprocessed()` and `embed_batch()` on `EmbeddingEngine` (~25 lines). 3 integration tests (model-dependent, skip if absent). |
| `crates/photon-core/src/pipeline/mod.rs` | 3 new re-exports: `ImageDecoder`, `ThumbnailGenerator`, `MetadataExtractor` |
| `crates/photon-core/src/lib.rs` | 4 new re-exports: `ImageDecoder`, `ThumbnailGenerator`, `MetadataExtractor`, `preprocess_image` |
| `crates/photon-core/benches/pipeline.rs` | 5 existing benchmarks fixed (stale API paths), 5 new benchmarks added. Total: 10 benchmarks. |
