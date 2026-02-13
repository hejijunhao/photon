# Speed Improvement — Phase 3: Scoring Acceleration

> Completed: 2026-02-13
> Ref: `docs/executing/speed-improvement-plan.md`, Phase 3 (Tasks 3.1–3.2)
> Tests: 216 passing (38 CLI + 158 core + 20 integration), zero clippy warnings

---

## Summary

The tagging scorer — which computes 68K dot products of 768-dim vectors per image — replaced with vectorized ndarray operations and precomputed pool index lists. On macOS, ndarray delegates to Apple's Accelerate framework (BLAS `sgemv`). Pool-aware scoring now iterates only ~2K active terms instead of scanning all 68K with per-term pool checks. Combined impact: **~30x fewer iterations** on the hot path, each **~2-8x faster** from hardware-accelerated dot products.

Four files changed. Three new dependencies (macOS only: `blas-src`, `accelerate-src`, `cblas-sys`). +2 net new tests.

---

## Task 3.1: ndarray matrix-vector multiply for scoring

**Files changed:** `tagging/scorer.rs`, `Cargo.toml`, `lib.rs`

**Problem:** Scalar inner loop `(0..dim).map(|j| image[j] * matrix[offset + j]).sum()` repeated 68K times — 52M multiply-adds per image with no vectorization guarantee. The same loop existed in both `score()` (full vocabulary) and `score_pool()` (filtered by pool).

**Fix:**

- **`score()` → single mat-vec multiply:** Created zero-copy `ArrayView2` over the N×768 label bank matrix and `ArrayView1` over the image embedding. Single `mat.dot(&img)` call replaces 68K individual dot products. With BLAS (macOS), this becomes one Accelerate `sgemv` call.

  ```rust
  let mat = ArrayView2::from_shape((n, dim), matrix).expect("label bank shape mismatch");
  let img = ArrayView1::from(image_embedding);
  let cosines = mat.dot(&img);  // single BLAS sgemv
  ```

- **`score_pool()` → `score_indices()`:** Replaced the old method (iterate all 68K, skip non-matching pools) with `score_indices(&self, embedding, indices)` that accepts a pre-computed `&[usize]` slice. Each dot product uses ndarray `ArrayView1::dot()` — vectorized even without BLAS.

- **BLAS on macOS:** Added platform-conditional dependencies:
  ```toml
  [target.'cfg(target_os = "macos")'.dependencies]
  ndarray = { version = "0.16", features = ["blas"] }
  blas-src = { version = "0.10", features = ["accelerate"] }
  ```
  Added `extern crate blas_src;` (cfg-gated to macOS) in `lib.rs` to force-link the Accelerate symbols. On Linux, ndarray uses its own optimized Rust implementation — no BLAS dependency.

---

## Task 3.2: Precomputed pool index lists

**File changed:** `tagging/relevance.rs`

**Problem:** `score_pool()` iterated all 68K terms checking `tracker.pool(i) != pool` for each. With ~2K Active terms, 97% of iterations (66K pool lookups) were wasted.

**Fix:**

- **Added precomputed index lists** to `RelevanceTracker`:
  ```rust
  pub struct RelevanceTracker {
      stats: Vec<TermStats>,
      active_indices: Vec<usize>,   // precomputed
      warm_indices: Vec<usize>,     // precomputed
      images_processed: u64,
      config: RelevanceConfig,
  }
  ```

- **`rebuild_indices()` helper** — single pass over stats, partitions indices by pool. Called after every pool mutation:
  - `new()` — initial assignment (encoded → Active, unencoded → Cold)
  - `sweep()` — after Active→Warm demotions and Warm→Active promotions
  - `promote_to_warm()` — after Cold→Warm neighbor expansions (only if changes occurred)
  - `load()` — after reconstructing stats from disk

- **Public accessors** `active_indices()` and `warm_indices()` return `&[usize]` — consumed by `score_indices()` under a read lock with no copying.

- **Cost:** `rebuild_indices()` iterates 68K stats once — but only runs after `sweep()` (every ~1000 images) or `promote_to_warm()`, never on the per-image hot path.

---

## How scoring works now

```
Before (per image):                        After (per image):

score_pool(Active):                        score_indices(active_indices):
  for i in 0..68K {                          for &i in &active_indices {  // ~2K
    if tracker.pool(i) != Active: skip         row = matrix[i*768..(i+1)*768]
    cosine = scalar_dot(768)                   cosine = ndarray_dot(768)  // BLAS sdot
    sigmoid(cosine)                            sigmoid(cosine)
  }                                          }

score() (no pools):                        score() (no pools):
  for i in 0..68K {                          mat = ArrayView2(68K × 768)
    cosine = scalar_dot(768)                 cosines = mat.dot(&img)  // single sgemv
    sigmoid(cosine)                          map sigmoid over cosines
  }
```

---

## Files Modified

| File | Changes |
|------|---------|
| `crates/photon-core/src/tagging/relevance.rs` | `active_indices`/`warm_indices` fields, `rebuild_indices()`, public accessors, calls from `new`/`sweep`/`promote_to_warm`/`load` |
| `crates/photon-core/src/tagging/scorer.rs` | ndarray `ArrayView2.dot()` in `score()`, new `score_indices()`, removed `score_pool()`, updated `score_with_pools()` |
| `crates/photon-core/src/lib.rs` | `extern crate blas_src;` (macOS only) |
| `crates/photon-core/Cargo.toml` | `blas-src`/`accelerate-src`/`cblas-sys` (macOS only), ndarray `blas` feature |

## Cross-Platform Behavior

| Platform | Scoring backend | Notes |
|----------|----------------|-------|
| macOS (Apple Silicon) | Accelerate `sgemv`/`sdot` via BLAS | Hardware-optimized AMX/NEON |
| Linux (CI) | ndarray Rust implementation | No BLAS dependency, still faster than manual scalar loop |
| Windows | ndarray Rust implementation | Untested but should work identically to Linux |
