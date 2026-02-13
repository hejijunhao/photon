# Speed Improvement Plan

> Systematic performance overhaul for Photon's image processing pipeline.
> Baseline: sequential processing, scalar scoring, redundant I/O.
> Target: full multi-core utilization, hardware-accelerated scoring, zero waste on the hot path.

---

## Phase 1 — Eliminate per-image waste (low effort, immediate wins)

Quick fixes that remove redundant work from every single image processed.
No architectural changes required. Each task is independent.

### Task 1.1: Cache `HasherConfig` on `ImageProcessor`

**File:** `crates/photon-core/src/pipeline/hash.rs:41-44`

**Problem:** A new `HasherConfig` + hasher object is constructed for every image's perceptual hash. This allocates and configures the same thing thousands of times in a batch.

**Fix:** Add a `hasher: image_hasher::Hasher` field to `ImageProcessor` (or change `Hasher` from a unit struct to one holding the configured hasher). Construct once in `ImageProcessor::new()`, reuse via `&self.hasher` in `perceptual_hash()`.

**Scope:** `hash.rs`, `processor.rs` (add field + pass reference)

---

### Task 1.2: Preprocess before `spawn_blocking` to avoid full image clone

**File:** `crates/photon-core/src/pipeline/processor.rs:393-401`

**Problem:** The entire decoded `DynamicImage` (~49MB for a 4032×3024 image) is cloned to move into `spawn_blocking` for embedding. But `spawn_blocking` only needs the preprocessed 224×224 tensor (~600KB).

**Fix:** Call `preprocess(&decoded.image, self.image_size)` *before* the `spawn_blocking` boundary. Move only the resulting `Array4<f32>` into the closure. Change `EmbeddingEngine::embed()` to accept the preprocessed tensor directly (add an `embed_tensor()` method or split preprocess from embed).

**Scope:** `processor.rs`, `embedding/mod.rs`, `embedding/siglip.rs`

---

### Task 1.3: Read file once — hash from bytes, decode from bytes

**Files:** `processor.rs:349-364`, `hash.rs:18-33`, `decode.rs:40-86`

**Problem:** Every image is read from disk twice: once for BLAKE3 hashing (streaming read), once for image decoding (`image::ImageReader::open()`). For large files (up to 100MB limit) this is significant.

**Fix:** Read the file into a `Vec<u8>` once. Compute BLAKE3 hash from the byte buffer. Decode the image from a `Cursor<&[u8]>` using `image::ImageReader::new(Cursor::new(&bytes)).with_guessed_format()`. This also eliminates the redundant `std::fs::metadata` call in `decode_sync()` (line 120).

**Scope:** `processor.rs` (orchestration change), `hash.rs` (add `content_hash_from_bytes()`), `decode.rs` (add `decode_from_bytes()`)

---

### Task 1.4: Raw buffer iteration in image preprocessing

**File:** `crates/photon-core/src/embedding/preprocess.rs:36-43`

**Problem:** Per-pixel `get_pixel()` with bounds checking + 4D ndarray indexing with bounds checking. On 224×224 that's 150K bounds-checked reads and 450K bounds-checked writes.

**Fix:** Access the raw RGB byte slice via `rgb.as_raw()` and iterate with `.chunks_exact(3)`. Write into the tensor's raw `as_slice_mut()` with computed flat offsets (NCHW layout: `c * size * size + y * size + x`). Eliminates all bounds checking on the inner loop.

```rust
let raw = rgb.as_raw();
let tensor_data = tensor.as_slice_mut().unwrap();
for (i, pixel) in raw.chunks_exact(3).enumerate() {
    let y = i / size;
    let x = i % size;
    for c in 0..3 {
        let idx = c * size * size + y * size + x;
        tensor_data[idx] = (pixel[c] as f32 / 255.0 - 0.5) / 0.5;
    }
}
```

**Scope:** `preprocess.rs` only

---

### Task 1.5: Reduce `ProcessedImage` cloning in batch output

**File:** `crates/photon/src/cli/process/batch.rs:89-110`

**Problem:** When LLM is enabled, each `ProcessedImage` (768-float embedding + base64 thumbnail) is cloned up to 3 times per image in the batch loop.

**Fix:** Wrap `ProcessedImage` in `Arc` when LLM is enabled. Clone the `Arc` (cheap pointer bump) instead of the struct. Alternatively, restructure the output branches to avoid the multiple-ownership need — e.g., serialize to JSON string once, write to both stdout and file from the same string.

**Scope:** `batch.rs`

---

## Phase 2 — Parallel batch processing (medium effort, highest impact)

This is the single biggest performance win. Currently images are processed sequentially;
with 8 cores available, the pipeline uses ~12.5% of CPU.

### Task 2.1: Concurrent pipeline with `tokio::JoinSet` / `buffer_unordered`

**File:** `crates/photon/src/cli/process/batch.rs:66-127`

**Problem:** `for file in &files { processor.process(...).await }` — one image at a time.

**Fix:** Replace the sequential loop with concurrent processing bounded by `args.parallel`:

```rust
use futures_util::stream::{self, StreamExt};

let results = stream::iter(files)
    .map(|file| {
        let processor = &ctx.processor;
        let options = &ctx.options;
        async move {
            (file.path.clone(), processor.process_with_options(&file.path, options).await)
        }
    })
    .buffer_unordered(args.parallel)
```

Key considerations:
- `ImageProcessor` is already `&self` (shared reference) for `process()` — safe to share across tasks.
- `EmbeddingEngine` is behind `Arc<>` with `Mutex<Session>` — serializes ONNX calls naturally.
- `TagScorer` is behind `Arc<RwLock<>>` — concurrent reads are fine.
- Progress bar updates, skip-existing checks, and output writes need to happen in the result-handling stream (ordered or unordered depending on output format).
- JSONL output can be unordered (each line is independent). JSON array output must collect all results.
- `--skip-existing` hash check should happen *before* entering the async pipeline to avoid wasting a concurrency slot.

**Scope:** `batch.rs` (main rewrite), possibly `enrichment.rs` (adjust interface)

**Impact:** Expected **4-8x throughput** on multi-core machines. Decode, hash, thumbnail, and preprocess all run concurrently on different images while one image occupies the ONNX session.

---

### Task 2.2: Wire `args.parallel` through to batch concurrency

**Files:** `batch.rs`, `setup.rs:146`

**Problem:** `args.parallel` only controls LLM enrichment concurrency. The core pipeline ignores it.

**Fix:** Pass `args.parallel` into `process_batch()` and use it as the `buffer_unordered` limit. Keep the existing LLM cap at `args.parallel.min(8)`.

**Scope:** `batch.rs`, `setup.rs`

---

## Phase 3 — Scoring acceleration (medium effort, high impact on tagging)

The scoring hot path computes 68K dot products of 768-dim vectors per image.
Currently scalar — this is the most SIMD-friendly code in the entire codebase.

### Task 3.1: SIMD/BLAS matrix-vector multiply for `score()`

**File:** `crates/photon-core/src/tagging/scorer.rs:126-136`

**Problem:** Scalar loop: `(0..dim).map(|j| image_embedding[j] * matrix[offset + j]).sum()` repeated 68K times. ~52M multiply-adds per image, no autovectorization guarantee.

**Fix (option A — recommended):** Use `ndarray` (already a dependency) with BLAS backend:
```rust
use ndarray::{Array1, Array2, Axis};
let matrix = Array2::from_shape_vec((n, dim), self.label_bank.matrix().to_vec()).unwrap();
let image = Array1::from_vec(image_embedding.to_vec());
let scores = matrix.dot(&image); // single BLAS sgemv call
```
Then add `ndarray = { version = "0.16", features = ["blas"] }` and `blas-src` with the appropriate backend (Accelerate on macOS, OpenBLAS on Linux).

**Fix (option B — zero dependencies):** Manual SIMD using `std::simd` (nightly) or `std::arch::aarch64::vfmaq_f32` for ARM NEON. Process 4 floats at a time.

**Fix (option C — simple improvement):** Process chunks of 8 with explicit unrolling to help the autovectorizer:
```rust
let cosine: f32 = image_embedding.chunks_exact(8)
    .zip(matrix[offset..offset+dim].chunks_exact(8))
    .map(|(a, b)| a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>())
    .sum();
```

**Impact:** Option A gives ~8-16x on Apple Silicon (AMX/NEON via Accelerate). Option C gives ~2-4x from better autovectorization.

**Scope:** `scorer.rs`, `Cargo.toml` (if adding BLAS)

---

### Task 3.2: Precomputed pool index lists

**File:** `crates/photon-core/src/tagging/scorer.rs:160-163`

**Problem:** `score_pool()` iterates all 68K terms checking `tracker.pool(i) != pool` for each. With ~2K Active terms, 97% of iterations are wasted.

**Fix:** Add `active_indices: Vec<usize>`, `warm_indices: Vec<usize>` to `RelevanceTracker`. Update these lists in `sweep()` and `promote_to_warm()`. Change `score_pool()` to accept `&[usize]` indices directly:

```rust
pub fn score_indices(&self, image_embedding: &[f32], indices: &[usize]) -> Vec<(usize, f32)> {
    indices.iter().filter_map(|&i| {
        let offset = i * dim;
        let cosine: f32 = /* dot product */;
        let confidence = Self::cosine_to_confidence(cosine);
        (confidence >= self.config.min_confidence).then_some((i, confidence))
    }).collect()
}
```

**Impact:** Scoring active pool goes from iterating 68K to iterating ~2K — **~30x fewer iterations** for the per-image hot path.

**Scope:** `scorer.rs`, `relevance.rs` (maintain index lists), `processor.rs` (pass indices)

---

## Phase 4 — I/O and allocation optimizations (low effort, cumulative wins)

### Task 4.1: Cheap `--skip-existing` pre-filter

**File:** `crates/photon/src/cli/process/batch.rs:68-76`

**Problem:** Every file is fully read and BLAKE3-hashed just to check if it should be skipped. For 10K files where 9K are done, this reads and hashes 9K files for nothing.

**Fix:** Build a secondary index of `(file_path, file_size)` from the existing output file. Before hashing, check if the file path + size match an already-processed entry. Only compute the expensive BLAKE3 hash for files that pass the cheap pre-filter (new files, or files whose size changed). This is a heuristic fast-path — the hash remains the source of truth.

**Scope:** `batch.rs`

---

### Task 4.2: Zero-copy label bank save/load

**File:** `crates/photon-core/src/tagging/label_bank.rs:133, 179`

**Problem:** Save allocates a full `Vec<u8>` copy of the matrix (~200MB). Load converts byte-by-byte.

**Fix:** On little-endian platforms (all supported targets: aarch64, x86_64):
```rust
// Save: write raw f32 memory directly
let byte_slice = unsafe {
    std::slice::from_raw_parts(self.matrix.as_ptr() as *const u8, self.matrix.len() * 4)
};
std::fs::write(path, byte_slice)?;

// Load: read into Vec<f32> directly
let mut matrix = vec![0f32; term_count * embedding_dim];
let byte_slice = unsafe {
    std::slice::from_raw_parts_mut(matrix.as_mut_ptr() as *mut u8, matrix.len() * 4)
};
std::fs::File::open(path)?.read_exact(byte_slice)?;
```

Add a compile-time assert: `const _: () = assert!(cfg!(target_endian = "little"));`

**Impact:** Eliminates ~200MB temporary allocation on save and per-element conversion on load.

**Scope:** `label_bank.rs`

---

### Task 4.3: Avoid label bank clone in progressive encoding

**File:** `crates/photon-core/src/tagging/progressive.rs:168`

**Problem:** Each chunk swap clones the entire running label bank (~200MB at full size). With ~13 chunks, that's ~2.6GB of intermediate allocations.

**Fix:** Instead of cloning the running bank into the new scorer, use `Arc<LabelBank>` shared between the progressive encoder and the scorer. On swap, create a new `TagScorer` that shares the same `Arc<LabelBank>`. This requires changing `TagScorer.label_bank` from owned to `Arc<LabelBank>`.

Alternative simpler fix: build the new scorer by moving `running_bank` into it, then immediately clone the scorer's bank back for the next iteration. This halves the clones (N clones instead of 2N).

**Scope:** `progressive.rs`, `scorer.rs` (if using Arc approach)

---

## Phase 5 — Advanced (higher effort, situational impact)

### Task 5.1: Batch ONNX embedding inference

**Files:** `embedding/siglip.rs`, `embedding/mod.rs`, `processor.rs`

**Problem:** Each image is embedded individually through the ONNX session (batch=1). ONNX Runtime supports batched inference which amortizes session dispatch overhead.

**Fix:** Accumulate N preprocessed tensors, stack into `[N, 3, 224, 224]`, run a single ONNX call. This only becomes meaningful after Phase 2 (parallel processing) is implemented, since you need multiple images ready simultaneously.

**Prerequisite:** Phase 2 (parallel batch processing)

**Scope:** New batching layer between processor and embedding engine

---

### Task 5.2: Benchmark-driven validation

**File:** `crates/photon-core/benches/pipeline.rs`

**Problem:** Current benchmarks only cover individual stages (hash, decode, thumbnail, metadata). No end-to-end throughput benchmark. No scoring benchmark.

**Fix:** Add benchmarks for:
- `score()` with realistic 68K vocabulary (synthetic matrix)
- `preprocess()` at 224 and 384
- End-to-end `process()` per image
- Batch throughput (N images / second)

These benchmarks should be run before and after each phase to validate improvements.

**Scope:** `benches/pipeline.rs`

---

## Implementation order

```
Phase 1 (tasks 1.1–1.5)     ← do first, independent, low risk
    ↓
Phase 2 (tasks 2.1–2.2)     ← highest single impact, medium effort
    ↓
Phase 3 (tasks 3.1–3.2)     ← biggest per-image win for tagging workloads
    ↓
Phase 4 (tasks 4.1–4.3)     ← cumulative I/O and allocation savings
    ↓
Phase 5 (tasks 5.1–5.2)     ← advanced, depends on Phase 2
```

Within each phase, tasks are independent and can be done in any order (except 5.1 depends on 2.1).

## Expected cumulative impact

| After phase | Estimated throughput improvement |
|-------------|-------------------------------|
| Phase 1     | ~1.5-2x (less waste per image) |
| Phase 2     | ~6-12x (multi-core utilization) |
| Phase 3     | ~8-16x (scoring acceleration) |
| Phase 4     | Additional ~10-20% (I/O + allocation) |
| Phase 5     | Additional ~10-30% (batched ONNX) |

Phases 2 and 3 are multiplicative where they overlap (parallel images × faster scoring per image).
