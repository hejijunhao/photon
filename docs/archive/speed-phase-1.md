# Speed Improvement — Phase 1: Eliminate Per-Image Waste

> Completed: 2026-02-13
> Ref: `docs/executing/speed-improvement-plan.md`, Phase 1 (Tasks 1.1–1.5)
> Tests: 214 passing (38 CLI + 156 core + 20 integration), zero clippy warnings

---

## Summary

Five independent optimizations that eliminate redundant work from every image processed. No architectural changes — purely mechanical waste removal on the hot path.

---

## Task 1.1: Cache `HasherConfig` on `ImageProcessor`

**Files changed:** `pipeline/hash.rs`, `pipeline/processor.rs`

**Problem:** `HasherConfig::new().hash_alg(...).hash_size(...).to_hasher()` was constructed fresh for every `perceptual_hash()` call — identical allocation repeated thousands of times in a batch.

**Fix:**
- Changed `Hasher` from a unit struct to hold a pre-built `image_hasher::Hasher` field (`phash_hasher`).
- Added `Hasher::new()` constructor + `Default` impl.
- Changed `perceptual_hash()` from an associated function to an instance method (`&self`).
- Added `hasher: Hasher` field to `ImageProcessor`, constructed once in `new()`.

---

## Task 1.2: Preprocess before `spawn_blocking`

**Files changed:** `pipeline/processor.rs`, `embedding/mod.rs`

**Problem:** The full decoded `DynamicImage` (~49 MB for 4032x3024) was cloned via `.clone()` to move into `spawn_blocking` for embedding. The blocking task only needs the preprocessed 224x224 tensor (~600 KB).

**Fix:**
- Added `EmbeddingEngine::image_size()` getter to expose the model input size.
- Added `EmbeddingEngine::embed_preprocessed(&self, tensor, path)` that skips preprocessing and calls the ONNX session directly.
- In `processor.rs`, call `preprocess()` *before* `spawn_blocking`. Move only the `Array4<f32>` tensor (~600 KB) into the closure instead of the full `DynamicImage` (~49 MB). **~80x less data moved across the thread boundary.**

---

## Task 1.3: Read file once — hash from bytes, decode from bytes

**Files changed:** `pipeline/hash.rs`, `pipeline/decode.rs`, `pipeline/processor.rs`

**Problem:** Every image was read from disk twice: once for BLAKE3 hashing (streaming read), once for decoding (`image::ImageReader::open()`). The redundant `fs::metadata()` call in `decode()` added a third syscall. For large files near the 100 MB limit, this doubled I/O.

**Fix:**
- Added `Hasher::content_hash_from_bytes(&[u8]) -> String` — hashes an in-memory buffer instead of opening a file.
- Added `ImageDecoder::decode_from_bytes(bytes, path)` — decodes from a `Cursor<Vec<u8>>` with the same timeout and dimension validation as the original `decode()`.
- Added `ImageDecoder::decode_bytes_sync()` — sync inner implementation for `spawn_blocking`.
- Removed the old `decode()` and `decode_sync()` methods (fully superseded).
- In `processor.rs`, the pipeline now reads the file once with `std::fs::read()`, hashes from the byte buffer, then passes the bytes into `decode_from_bytes()` which moves them into `spawn_blocking`.
- Kept `Hasher::content_hash(path)` (the streaming version) for the CLI's `--skip-existing` path in `batch.rs`, where reading the entire file into memory just to check a hash would be wasteful.

New pipeline order: `validate → read file → hash from bytes → decode from bytes → metadata → phash → thumbnail → embed`.

---

## Task 1.4: Raw buffer iteration in preprocessing

**File changed:** `embedding/preprocess.rs`

**Problem:** Per-pixel `get_pixel(x, y)` with bounds checking + 4D ndarray `tensor[[0, c, y, x]]` with bounds checking. At 224x224 that's 150K bounds-checked reads and 450K bounds-checked writes.

**Fix:**
- Access raw RGB byte slice via `rgb.as_raw()` and iterate with `.chunks_exact(3)`.
- Write into tensor's raw `as_slice_mut()` with computed flat offsets (NCHW layout: `c * size * size + y * size + x`).
- Inner loop uses `pixel.iter().enumerate()` to satisfy clippy's `needless_range_loop`.
- Eliminates all bounds checking on the inner loop.

---

## Task 1.5: Reduce `ProcessedImage` cloning in batch output

**Files changed:** `cli/process/batch.rs`, `cli/process/enrichment.rs`, `cli/process/mod.rs`

**Problem:** In LLM-enabled batch processing, the post-loop output code cloned `Vec<ProcessedImage>` up to 2N times (once via `.iter().map(clone)` for building `all_records`, once via `results.clone()` for the enricher). For a 1000-image batch with embeddings (~15 KB/image), that's ~30 MB of wasted copies.

**Fix:**
- Changed `run_enrichment_collect()` return type from `Vec<OutputRecord>` to `(Vec<OutputRecord>, Vec<ProcessedImage>)`. The spawned task now moves results in and returns them back, allowing callers to consume by move instead of cloning.
- In all post-loop branches that build `all_records`, replaced `.iter().map(|r| r.clone())` with `.into_iter()` (zero-copy move).
- Moved the final JSONL stdout enrichment block from a separate `if` into an `else` clause of the main output branch chain, satisfying the borrow checker's proof that `results` hasn't been consumed.
- Updated all 3 call sites in `mod.rs` and `batch.rs` to destructure the new tuple return.
- **Net result: 0 clones of `ProcessedImage` in the post-loop output handling** (down from 2N).

---

## Files Modified

| File | Changes |
|------|---------|
| `crates/photon-core/src/pipeline/hash.rs` | `Hasher` struct with cached `phash_hasher`, `content_hash_from_bytes()`, `Default` impl |
| `crates/photon-core/src/pipeline/processor.rs` | `hasher` field, read-once pipeline, preprocess before `spawn_blocking` |
| `crates/photon-core/src/pipeline/decode.rs` | `decode_from_bytes()`, `decode_bytes_sync()`, removed old `decode()`/`decode_sync()` |
| `crates/photon-core/src/embedding/preprocess.rs` | Raw buffer iteration via `chunks_exact(3)` + flat NCHW indexing |
| `crates/photon-core/src/embedding/mod.rs` | `image_size()` getter, `embed_preprocessed()` method |
| `crates/photon/src/cli/process/batch.rs` | `into_iter()` instead of `.iter().clone()`, restructured output branches |
| `crates/photon/src/cli/process/enrichment.rs` | Return `(Vec<OutputRecord>, Vec<ProcessedImage>)` from `run_enrichment_collect()` |
| `crates/photon/src/cli/process/mod.rs` | Updated `run_enrichment_collect()` call sites |
