# Phase 3: SigLIP Embedding

> Completed 2026-02-09

## Milestone

`photon process image.jpg` outputs a 768-dimensional embedding vector (when the model is downloaded).

## What Was Built

ONNX Runtime integration for running the SigLIP visual encoder locally, producing L2-normalized 768-float embedding vectors from images. Model download from HuggingFace. Full pipeline integration with timeout and graceful fallback when the model isn't present.

## Files Created

| File | Purpose |
|------|---------|
| `crates/photon-core/src/embedding/mod.rs` | `EmbeddingEngine` — public API wrapping the SigLIP session. `load()`, `embed()`, `model_exists()`, `model_path()`. |
| `crates/photon-core/src/embedding/preprocess.rs` | Image preprocessing for SigLIP: resize to 224×224 (Lanczos3), normalize pixels to [-1, 1] via `(p/255 - 0.5) / 0.5`, lay out as NCHW `[1, 3, 224, 224]` tensor. |
| `crates/photon-core/src/embedding/siglip.rs` | `SigLipSession` — loads an ONNX model file, runs inference, extracts embedding from output tensor, L2-normalizes. Handles 1D/2D/3D output shapes (direct vector, batch, or mean-pooled sequence). |

## Files Modified

| File | Change |
|------|--------|
| `Cargo.toml` (workspace) | Added `reqwest` to workspace dependencies. |
| `crates/photon-core/Cargo.toml` | Added `ort = "2.0.0-rc.11"` and `ndarray = "0.16"`. |
| `crates/photon/Cargo.toml` | Added `reqwest.workspace = true` for model download. |
| `crates/photon-core/src/lib.rs` | Declared `embedding` module, re-exported `EmbeddingEngine`. |
| `crates/photon-core/src/pipeline/processor.rs` | `ImageProcessor` now holds `Option<Arc<EmbeddingEngine>>`. Added `load_embedding()`, `has_embedding()`. Embedding step runs inside `spawn_blocking` with configurable timeout. Added `skip_embedding` to `ProcessOptions`. |
| `crates/photon/src/cli/process.rs` | Added `--no-embedding` flag. Auto-loads model if present, warns if missing, sets `skip_embedding` accordingly. |
| `crates/photon/src/cli/models.rs` | Implemented `photon models download` — downloads `vision_model.onnx` from `Xenova/siglip-base-patch16-224` on HuggingFace, saves as `visual.onnx` under `~/.photon/models/siglip-base-patch16/`. `list` subcommand now shows model readiness status. |

## Design Decisions

### Why `Mutex<Session>`

`ort::Session::run()` requires `&mut self`. Since the `EmbeddingEngine` is shared across the pipeline via `Arc`, interior mutability is needed. `Mutex` was chosen over making the session per-worker because:
- Embedding is already the bottleneck (CPU-bound ONNX inference), so lock contention is minimal.
- Keeps memory footprint low (one model instance ≈ 350 MB).
- Simpler than a session pool.

### Why `(Vec<i64>, Vec<f32>)` instead of ndarray for ort input

The `ort` crate's ndarray integration is behind a `#[cfg(feature = "ndarray")]` feature gate. Rather than enabling that feature (which couples ort's internal ndarray version to ours), we convert the preprocessed `Array4<f32>` to a flat `(shape, data)` tuple. This uses ort's always-available `OwnedTensorArrayData` impl for `(D, Vec<T>)` where `D: ToShape`.

### Why optional embedding in the processor

`ImageProcessor::new()` stays synchronous and infallible. The embedding model is loaded separately via `load_embedding()`. This means:
- Phase 1/2 code doesn't break — processing works without a model.
- The CLI can detect whether the model exists and warn the user.
- Library consumers choose when/whether to load the model.

### Why `spawn_blocking` with timeout

ONNX inference is CPU-bound and would block the tokio async runtime. `spawn_blocking` moves it to a dedicated thread pool. The configurable timeout (`limits.embed_timeout_ms`, default 30s) prevents a single problematic image from stalling the pipeline.

### Output shape handling

SigLIP ONNX exports vary in output tensor shape depending on the export tool:
- `[768]` — direct embedding vector
- `[1, 768]` — batched single image
- `[1, 197, 768]` — patch tokens (mean-pooled to get a single vector)

The code handles all three, taking the first output tensor from the model regardless of its name.

## Model Download

- **Source**: `https://huggingface.co/Xenova/siglip-base-patch16-224/resolve/main/onnx/vision_model.onnx`
- **Local path**: `~/.photon/models/siglip-base-patch16/visual.onnx`
- **Size**: ~350 MB
- Downloads with `reqwest`, checks existence before downloading, skips if already present.

## ort Crate Notes (v2.0.0-rc.11)

The `ort` v2 API differs significantly from v1 and from the stable v2 that hasn't shipped yet:

- `Session::run()` takes `&mut self` (not `&self`).
- `Session::inputs()` / `outputs()` return `&[Outlet]`; field access is via methods (`outlet.name()`), not public fields.
- `ort::inputs!` macro returns `Vec<(Cow<str>, SessionInputValue)>`, not a `Result`.
- `try_extract_tensor::<f32>()` returns `(&Shape, &[f32])` where `Shape` derefs to `[i64]`.
- `Value::from_array()` accepts `impl OwnedTensorArrayData<T>` — ndarray `Array` only works with the `ndarray` feature; `(D, Vec<T>)` always works.

## Tests Added

| Test | Location |
|------|----------|
| `test_preprocess_shape` | Verifies output tensor is `[1, 3, 224, 224]` regardless of input size. |
| `test_preprocess_normalization_range` | White pixels → 1.0, black pixels → -1.0. |
| `test_l2_normalize` | `[3, 4]` → `[0.6, 0.8]` with unit norm. |
| `test_l2_normalize_zero_vector` | Zero vector stays zero (no division by zero). |
| `test_process_options_default` | Updated to verify `skip_embedding` defaults to `false`. |

All 29 tests pass. No integration test for actual ONNX inference (requires model file on disk).
