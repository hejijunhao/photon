# Changelog

All notable changes to Photon are documented here.

---

## Index

- **[0.7.4](#074---2026-02-15)** — CI fix: `manylinux: auto` → `manylinux: 2_28` for `aarch64-unknown-linux-gnu` PyPI wheel — `ring` crate fails to cross-compile with manylinux2014's GCC 4.8 (missing `__ARM_ARCH`)
- **[0.7.3](#073---2026-02-15)** — CI fix: drop `x86_64-apple-darwin` target — `ort` has no prebuilt ONNX Runtime binaries for Intel Mac cross-compilation. Build matrix reduced to 3 targets
- **[0.7.2](#072---2026-02-15)** — CI fix: `ort` TLS backend `tls-native` → `tls-rustls` (eliminates `openssl-sys` from entire dep tree), `macos-13` → `macos-14` cross-compilation for x86_64-apple-darwin
- **[0.7.1](#071---2026-02-15)** — PyPI package rename: `photon-ai` → `photon-imager` (name collision with existing `photonai` package)
- **[0.7.0](#070---2026-02-13)** — PyPI publishing: maturin `bindings = "bin"` config, CI workflow for 4-platform wheel builds with trusted publishing, `native-tls` → `rustls-tls` for fully self-contained binary
- **[0.6.5](#065---2026-02-13)** — Speed Phase 5: batch ONNX embedding API (`embed_batch()`), benchmark suite overhaul — 5 broken benchmarks fixed, 5 new benchmarks added (scoring, preprocessing, e2e, batch throughput), 4 new public re-exports (+6 tests)
- **[0.6.4](#064---2026-02-13)** — CI fix: TOML section scoping bug — 5 dependencies (`tokenizers`, `rand`, `async-trait`, `reqwest`, `futures-util`) accidentally scoped to macOS-only, breaking Linux builds
- **[0.6.3](#063---2026-02-13)** — Speed Phase 4: cheap `--skip-existing` pre-filter (path+size matching, zero I/O), zero-copy label bank save/load (~209MB allocation eliminated), reduced peak memory in progressive encoding (move semantics over clone)
- **[0.6.2](#062---2026-02-13)** — Speed Phase 3: ndarray vectorized scoring with BLAS/Accelerate on macOS, precomputed pool index lists (~30x fewer iterations on tagging hot path)
- **[0.6.1](#061---2026-02-13)** — Speed Phase 2: concurrent batch processing via `buffer_unordered(parallel)`, skip-existing pre-filtering, `--parallel` now controls pipeline concurrency (expected 4-8x throughput)
- **[0.6.0](#060---2026-02-13)** — Speed Phase 1: read-once I/O, cached perceptual hasher, preprocess before `spawn_blocking` (~80x less data across thread boundary), raw buffer preprocessing, zero-clone batch output
- **[0.5.8](#058---2026-02-13)** — MEDIUM-severity fixes: silent failure logging (WalkDir, skip-existing, progressive encoder), enricher file size guard, embedding error context, nested lock elimination, bounded enrichment channel (+4 tests)
- **[0.5.7](#057---2026-02-13)** — HIGH-severity fixes: lock poisoning graceful degradation, embedding dimension validation, semaphore leak prevention, single-authority timeouts, error propagation over silent swallowing (+5 tests)
- **[0.5.6](#056---2026-02-13)** — Assessment review fixes: documentation correction, strengthened assertions, +10 tests (enricher concurrency, boundary conditions, skip options)
- **[0.5.5](#055---2026-02-13)** — Structural cleanup: module visibility tightening, dead code removal, +10 tests (enricher, integration edge cases)
- **[0.5.4](#054---2026-02-13)** — MEDIUM-severity correctness fixes: 10 bugs across config, tagging, pipeline, and CLI — `--skip-existing` JSON support, Warm→Cold demotion, content-based format detection
- **[0.5.3](#053---2026-02-12)** — HIGH-severity bug fixes: progressive encoder race condition, invalid JSON output for LLM dual-stream, file size validation off-by-one
- **[0.5.2](#052---2026-02-12)** — Code assessment fixes: progressive encoding cache bug, text encoder unwrap, `cli/process.rs` → 5-file module, `config.rs` → 3-file module (8/10 → 9/10)
- **[0.5.1](#051---2026-02-12)** — Interactive CLI hardening: 28 unit tests, `unsafe set_var` elimination, `toml_edit` comment-preserving config, output path validation, recursive async → loop
- **[0.5.0](#050---2026-02-12)** — Interactive CLI: guided mode via bare `photon` invocation — 8-step process wizard, model management, LLM setup, config viewer, custom theme
- **[0.4.17](#0417---2026-02-12)** — process.rs decomposition: `execute()` reduced from ~475 to 16 lines, 5 enrichment duplicates consolidated into 2 helpers
- **[0.4.16](#0416---2026-02-12)** — Streaming batch output: JSONL file writes stream per-image instead of collecting all results in memory
- **[0.4.15](#0415---2026-02-12)** — Model download checksum verification: BLAKE3 integrity checks for all 4 model files, auto-removal of corrupt downloads
- **[0.4.14](#0414---2026-02-12)** — Integration tests: 10 end-to-end tests exercising `ImageProcessor::process()` against real fixtures
- **[0.4.13](#0413---2026-02-11)** — Hardening: lock poisoning diagnostics, config validation with range checks, malformed config warning
- **[0.4.12](#0412---2026-02-11)** — Benchmark fix: fixture paths now resolve via `CARGO_MANIFEST_DIR` so all 5 benchmarks run from any working directory
- **[0.4.11](#0411---2026-02-11)** — Final assessment: comprehensive code review (7.5/10), README config fix, test verification
- **[0.4.10](#0410---2026-02-11)** — CI fix: replace `is_some()`+`unwrap()` with `Option::filter()` to satisfy `clippy::unnecessary_unwrap`
- **[0.4.9](#049---2026-02-11)** — Code polishing: hot-path clone elimination, O(N×K)→O(N+K) sibling lookups, dead code removal, test hygiene
- **[0.4.8](#048---2026-02-11)** — Benchmark fix: correct API calls for `ImageDecoder` and `ThumbnailGenerator` instance methods
- **[0.4.7](#047---2026-02-11)** — Polish & release: progress bar, `--skip-existing`, summary stats, error hints, benchmarks, CI/CD, MIT license
- **[0.4.6](#046---2026-02-11)** — Housekeeping: `cargo fmt` across workspace
- **[0.4.5](#045---2026-02-11)** — Hierarchy dedup: ancestor suppression and WordNet path annotation for cleaner tag output
- **[0.4.4](#044---2026-02-11)** — Relevance pruning: self-organizing three-pool vocabulary (Active/Warm/Cold) with WordNet neighbor expansion
- **[0.4.3](#043---2026-02-11)** — Progressive encoding: first-run cold-start reduced from ~90min to ~30s via seed vocabulary + background chunked encoding
- **[0.4.2](#042---2026-02-11)** — Post-fix review: invalid JSON in batch+file+LLM, empty response guards, dead field removal
- **[0.4.1](#041---2026-02-11)** — Post-Phase-5 bug fixes: structured retry classification, async I/O, enricher counting, stdout data loss fix
- **[0.4.0](#040---2026-02-11)** — LLM integration: BYOK description enrichment with dual-stream output (Ollama, Anthropic, OpenAI, Hyperbolic)
- **[0.3.3](#033---2026-02-10)** — Pre-Phase 5 cleanup: clippy fixes, streaming downloads, cache invalidation, dead code removal
- **[0.3.2](#032---2026-02-09)** — Zero-shot tagging: 68K-term vocabulary, SigLIP text encoder, label bank caching
- **[0.3.1](#031---2026-02-09)** — Text encoder alignment spike: cross-modal verification, scoring parameter derivation
- **[0.3.0](#030---2026-02-09)** — SigLIP embedding: ONNX Runtime integration, 768-dim vector generation
- **[0.2.0](#020---2026-02-09)** — Image processing pipeline: decode, EXIF, hashing, thumbnails
- **[0.1.0](#010---2026-02-09)** — Project foundation: CLI, configuration, logging, error handling

---

## [0.7.4] - 2026-02-15

### Summary

CI fix — the v0.7.3 PyPI workflow failed on `aarch64-unknown-linux-gnu` because the `ring` crate (v0.17.14, pulled in via `ort` → `rustls` → `ring`) requires `__ARM_ARCH` to be defined by the assembler. The `manylinux2014-cross` Docker image uses CentOS 7's GCC 4.8, which doesn't define this macro for aarch64 targets. Fixed by switching to `manylinux: 2_28` (AlmaLinux 8, GCC 8.5) for the `aarch64-unknown-linux-gnu` target only — `x86_64-unknown-linux-gnu` and `aarch64-apple-darwin` remain on `manylinux: auto`. The `2_28` tag requires glibc 2.28+ on the target system (Ubuntu 20.04+, Debian 10+, RHEL 8+). 1 file changed, no code changes.

### Changed

- **`manylinux: auto` → `manylinux: 2_28`** for `aarch64-unknown-linux-gnu` (`.github/workflows/pypi.yml`) — manylinux per-target matrix variable replaces the global setting. The manylinux2014 base image (CentOS 7, GCC 4.8) cannot cross-compile `ring`'s aarch64 assembly; manylinux_2_28 (AlmaLinux 8, GCC 8.5) defines `__ARM_ARCH` correctly.

### Tests

226 tests passing (40 CLI + 166 core + 20 integration), zero clippy warnings, zero formatting violations. No code changes — CI only.

---

## [0.7.3] - 2026-02-15

### Summary

CI fix — dropped `x86_64-apple-darwin` from both `pypi.yml` and `release.yml` build matrices. The v0.7.2 `macos-13` → `macos-14` change attempted cross-compilation from ARM to Intel, but `ort-sys` has no prebuilt ONNX Runtime binaries for the `x86_64-apple-darwin` target when cross-compiling. Previously this worked only because `macos-13` was native Intel hardware. Build matrix reduced from 4 to 3 targets: `aarch64-apple-darwin`, `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`. Pre-M1 Mac users can still build from source via `cargo install`. 2 files changed, no code changes.

### Removed

- **`x86_64-apple-darwin` build target** (`.github/workflows/pypi.yml`, `.github/workflows/release.yml`) — `ort` does not distribute prebuilt ONNX Runtime binaries for Intel Mac cross-compilation from ARM runners. GitHub deprecated the native `macos-13` Intel runners, making this target unbuildable without compiling ONNX Runtime from source.

### Tests

226 tests passing (40 CLI + 166 core + 20 integration), zero clippy warnings, zero formatting violations. No code changes — CI only.

---

## [0.7.2] - 2026-02-15

### Summary

CI fix — the v0.7.1 PyPI workflow failed on all 4 targets due to two issues. First, `ort` (ONNX Runtime) defaults to `tls-native`, which pulls `openssl-sys` via `ort-sys` → `ureq` → `native-tls`. The v0.7.0 `reqwest` rustls switch only eliminated OpenSSL from the HTTP client, not from `ort`'s build-time model downloader. The manylinux Docker container lacks OpenSSL dev headers, causing Linux builds to fail; macOS builds also failed at the `openssl-sys` build script. Fixed by switching `ort` to `default-features = false` with explicit `tls-rustls` — `openssl-sys` is now completely absent from the dependency tree. Second, GitHub has deprecated `macos-13` runners, breaking the `x86_64-apple-darwin` build. Fixed by using `macos-14` (ARM) with maturin cross-compilation for both `pypi.yml` and `release.yml`. 3 files changed, no new dependencies.

### Changed

- **`ort`: `tls-native` → `tls-rustls`** (`photon-core/Cargo.toml`) — `default-features = false` drops the default `tls-native` feature; explicit feature list: `std`, `ndarray`, `download-binaries`, `copy-dylibs`, `tls-rustls`. Eliminates `openssl-sys` from the entire dependency tree (previously pulled via `ort-sys` → `ureq` → `native-tls` → `openssl-sys`).
- **`macos-13` → `macos-14`** (`.github/workflows/pypi.yml`, `.github/workflows/release.yml`) — `x86_64-apple-darwin` target now cross-compiles from ARM runner. GitHub deprecated `macos-13` Intel runners.

### Tests

226 tests passing (40 CLI + 166 core + 20 integration), zero clippy warnings, zero formatting violations.

---

## [0.7.1] - 2026-02-15

### Summary

PyPI package rename. The original name `photon-ai` collides with the existing [`photonai`](https://pypi.org/project/photonai/) package (a machine learning pipeline library) due to PyPI's PEP 503 name normalization, which treats hyphens, underscores, and case as equivalent. Renamed to `photon-imager` across `pyproject.toml` and all publishing docs.

### Changed

- **`pyproject.toml`** — package name `photon-ai` → `photon-imager`.
- **`docs/executing/publish-pypi-next-steps.md`** — updated PyPI project name and install commands.
- **`docs/executing/publishing.md`** — updated all PyPI references (npm `@photon-ai` scope unchanged — separate namespace).
- **`docs/completions/publishing-pypi.md`** — updated package name references.

### Tests

226 tests passing (40 CLI + 166 core + 20 integration), zero clippy warnings, zero formatting violations. No code changes — docs only.

---

## [0.7.0] - 2026-02-13

### Summary

PyPI publishing infrastructure. Photon can now be installed via `pip install photon-ai` — the native Rust binary is packaged into platform-specific Python wheels using maturin's `bindings = "bin"` mode (the same pattern used by ruff and uv). A new CI workflow builds wheels for 4 targets and publishes to PyPI via OIDC trusted publishing. The `reqwest` TLS backend was switched from `native-tls` (OpenSSL, dynamically linked) to `rustls-tls` (statically compiled) to produce a fully self-contained binary with no shared library dependencies. 4 files changed, no new Rust dependencies.

### Added

- **`pyproject.toml`** (workspace root) — maturin build configuration with `bindings = "bin"`, package name `photon-ai`, full PyPI metadata (description, license, keywords, classifiers, repository URL).
- **`.github/workflows/pypi.yml`** — CI workflow triggered on `v*` tags and `workflow_dispatch`. Build matrix: `aarch64-apple-darwin` (macOS-14), `x86_64-apple-darwin` (macOS-13), `x86_64-unknown-linux-gnu` (Ubuntu), `aarch64-unknown-linux-gnu` (Ubuntu cross). Publish job uses OIDC trusted publishing (`id-token: write`) — no API token needed.
- **`docs/executing/publish-pypi-next-steps.md`** — Checklist for first publish: create GitHub `pypi` environment, configure trusted publisher on pypi.org, tag release.
- **`docs/completions/publishing-pypi.md`** — Implementation notes and local verification results.

### Changed

- **`reqwest`: `native-tls` → `rustls-tls`** (`Cargo.toml`) — `default-features = false` disables OpenSSL; `rustls-tls` feature statically compiles TLS via ring/rustls. Eliminates `libssl.so`/`libcrypto.so` bundling in wheels. Binary is fully self-contained (39MB stripped). This fixes a maturin bin-bindings issue where the RPATH (`$ORIGIN/../photon-ai.libs`) doesn't resolve when pip installs the binary to `bin/` and shared libs to `site-packages/.libs/`.

### Tests

226 tests passing (40 CLI + 166 core + 20 integration), zero clippy warnings, zero formatting violations.

---

## [0.6.5] - 2026-02-13

### Summary

Speed Phase 5 — batch ONNX embedding inference and benchmark validation. Completes the speed improvement plan. New `embed_batch()` / `embed_batch_preprocessed()` methods on `EmbeddingEngine` stack N preprocessed tensors into a single `[N, 3, H, W]` ONNX call, amortizing session dispatch overhead. Mirrors the existing `SigLipTextEncoder::encode_batch()` pattern. The processor pipeline is unchanged — batch API is for direct library users and future GPU support. The benchmark suite is fixed (5 existing benchmarks referenced stale `pub(crate)` APIs from Phase 1 changes) and expanded with 5 new benchmarks. 5 files changed, no new dependencies. +6 tests.

### Added

- **Batch ONNX embedding API** (`embedding/siglip.rs`, `embedding/mod.rs`) — `SigLipSession::embed_batch()` validates tensor shapes, builds a flat `[N, 3, H, W]` batch tensor, runs a single `session.run()`, extracts `pooler_output` `[N, 768]`, chunks by embedding dimension, and L2-normalizes each vector. `EmbeddingEngine::embed_batch_preprocessed()` delegates to the session; `embed_batch()` is a convenience wrapper that also handles preprocessing.
- **5 new benchmarks** (`benches/pipeline.rs`) — `score_68k_matvec` (synthetic 68K×768 ndarray mat-vec — the BLAS operation powering `TagScorer::score()`), `preprocess_224` and `preprocess_384` (resize+normalize a 4032×3024 image), `process_e2e_dog_jpg` (full pipeline with model, skips if absent), `batch_4_images` (concurrent 4-image throughput via `buffer_unordered`, skips if absent).
- **4 new public re-exports** (`lib.rs`) — `ImageDecoder`, `ThumbnailGenerator`, `MetadataExtractor`, `preprocess_image`. Enables benchmarks and library users to access pipeline components without `pub(crate)` module visibility changes.

### Fixed

- **5 broken benchmarks** (`benches/pipeline.rs`) — stale `photon_core::pipeline::*` paths replaced with `photon_core::*` re-exports; `Hasher::perceptual_hash()` updated from associated function to instance method (changed in Phase 1); `decoder.decode()` updated to `decoder.decode_from_bytes()` (API changed in Phase 1's read-once I/O).

### Tests

226 tests passing (40 CLI + 166 core + 20 integration), zero clippy warnings, zero formatting violations. 10 benchmarks compile and run (e2e/batch skip gracefully when ONNX models not on disk).

---

## [0.6.4] - 2026-02-13

### Summary

CI fix — TOML section scoping bug in `photon-core/Cargo.toml`. The `[target.'cfg(target_os = "macos")'.dependencies]` section header for BLAS/Accelerate was placed mid-file, causing all subsequent dependencies (`tokenizers`, `rand`, `async-trait`, `reqwest`, `futures-util`) to be scoped to macOS-only. This broke Linux CI with 52 compilation errors: unresolved crates, dyn-incompatible `LlmProvider` trait (missing `#[async_trait]`), and cascading type inference failures. Fix: moved the 5 platform-independent deps above the target-specific section. 1 file changed, no code changes.

### Fixed

- **TOML section scoping** (`photon-core/Cargo.toml`) — `tokenizers`, `rand`, `async-trait`, `reqwest`, and `futures-util` were listed after `[target.'cfg(target_os = "macos")'.dependencies]`, making them macOS-only. In TOML, all key-value pairs belong to the most recent section header until a new one appears. Reordered so all platform-independent deps sit under `[dependencies]` and the macOS-specific `ndarray`+`blas-src` section comes last.

### Tests

220 tests passing (40 CLI + 160 core + 20 integration), zero clippy warnings, zero formatting violations.

---

## [0.6.3] - 2026-02-13

### Summary

Speed Phase 4 — I/O and allocation optimizations. The `--skip-existing` pre-filter no longer reads or hashes files — it matches by (path, size) from the output file in microseconds instead of reading each file through BLAKE3. Label bank save/load uses `unsafe` byte reinterpretation to eliminate ~209MB temporary allocations on save and halve peak memory on load. Progressive encoding uses move semantics (`std::mem::replace`) instead of cloning the growing label bank per chunk, reducing peak memory per swap. 3 files changed, no new dependencies. 220 tests (+4), zero clippy warnings.

### Changed

- **Cheap `--skip-existing` pre-filter** (`cli/process/batch.rs`) — `load_existing_hashes()` renamed to `load_existing_entries()`, now returns `HashMap<(PathBuf, u64), ()>` keyed by (file_path, file_size). Pre-filter uses `contains_key()` instead of computing BLAKE3 hashes — zero I/O, just a HashMap lookup against metadata already available from WalkDir discovery.
- **Zero-copy label bank save** (`tagging/label_bank.rs`) — `save()` reinterprets `&[f32]` as `&[u8]` via `std::slice::from_raw_parts` and writes directly. Eliminates ~209MB intermediate `Vec<u8>` allocation. Compile-time endianness assert: `const _: () = assert!(cfg!(target_endian = "little"));`.
- **Zero-copy label bank load** (`tagging/label_bank.rs`) — `load()` allocates `Vec<f32>` directly, creates an `unsafe` mutable byte view, reads with `read_exact()`. Peak memory halved from ~418MB (`Vec<u8>` + `Vec<f32>`) to ~209MB (single `Vec<f32>`). File size validated via `fs::metadata()` before allocation.
- **Move semantics in progressive encoding** (`tagging/progressive.rs`) — seed bank cloned before moving into scorer and passed directly to background task via `ProgressiveContext`, eliminating the previous read-lock + clone. Per-chunk swap uses `std::mem::replace(&mut running_bank, LabelBank::empty())` to move the bank into the scorer at zero cost, then clones back only for non-last iterations. Post-loop cache save reads from `scorer_slot` directly.

### Tests

220 tests passing (40 CLI + 160 core + 20 integration), zero clippy warnings, zero formatting violations.

---

## [0.6.2] - 2026-02-13

### Summary

Speed Phase 3 — scoring acceleration. The tagging scorer's 68K scalar dot products replaced with ndarray vectorized operations backed by Apple's Accelerate framework (BLAS) on macOS. Pool-aware scoring now uses precomputed index lists, iterating only ~2K active terms instead of scanning all 68K with per-term pool checks. Full-vocabulary `score()` becomes a single `sgemv` mat-vec multiply. Expected **~30x fewer iterations** on the pool-aware hot path, each **~2-8x faster** from hardware-accelerated dot products. 4 files changed, 3 new macOS-only dependencies. 216 tests (+2), zero clippy warnings.

### Changed

- **ndarray matrix-vector scoring** (`tagging/scorer.rs`) — `score()` now creates zero-copy `ArrayView2` / `ArrayView1` views over the label bank matrix and image embedding, replacing 68K individual scalar dot products with a single `mat.dot(&img)` call. On macOS this dispatches to Accelerate's `sgemv`; on Linux it uses ndarray's optimized Rust implementation.
- **`score_indices()` replaces `score_pool()`** (`tagging/scorer.rs`) — new method accepts a `&[usize]` slice of term indices directly instead of iterating all 68K terms and checking pool membership per-term. Each dot product uses ndarray `ArrayView1::dot()`. `score_with_pools()` updated to pass `tracker.active_indices()` / `tracker.warm_indices()`.
- **Precomputed pool index lists** (`tagging/relevance.rs`) — `RelevanceTracker` now maintains `active_indices: Vec<usize>` and `warm_indices: Vec<usize>`, rebuilt via `rebuild_indices()` after any pool mutation (`new`, `sweep`, `promote_to_warm`, `load`). Cost: one 68K-iteration rebuild per sweep (~every 1000 images), not per-image.
- **BLAS/Accelerate on macOS** (`Cargo.toml`, `lib.rs`) — platform-conditional `blas-src` with `accelerate` feature enables ndarray's BLAS backend on macOS. `extern crate blas_src;` in `lib.rs` (cfg-gated) forces the linker to include Accelerate symbols. No BLAS dependency on Linux — ndarray falls back to its Rust implementation.

### Removed

- **`score_pool()`** (`tagging/scorer.rs`) — replaced by `score_indices()` which accepts precomputed index lists instead of scanning all terms with per-term pool checks.

### Tests

216 tests passing (38 CLI + 158 core + 20 integration), zero clippy warnings, zero formatting violations.

---

## [0.6.1] - 2026-02-13

### Summary

Speed Phase 2 — batch processing converted from sequential to concurrent. Images are now processed in parallel using `futures_util::StreamExt::buffer_unordered(parallel)`, bounded by `--parallel` (default 4). While one image waits on the ONNX mutex for embedding, others decode, hash, and generate thumbnails on tokio's blocking thread pool. Skip-existing files are pre-filtered before the concurrent pipeline to avoid wasting concurrency slots. Expected **4-8x throughput** on multi-core machines. Single file changed, no new dependencies. 214 tests, zero clippy warnings.

### Changed

- **Concurrent batch pipeline** (`cli/process/batch.rs`) — replaced sequential `for file in &files { process().await }` loop with `stream::iter(files).map(async process).buffer_unordered(parallel)`. `ProcessContext` destructured to wrap `ImageProcessor` and `ProcessOptions` in `Arc` for concurrent sharing. Results consumed single-threaded — stdout/file writes and counters need no synchronization.
- **Skip-existing pre-filtering** (`cli/process/batch.rs`) — hash checks now run before the concurrent stream. Previously, skipped files entered the loop, computed their hash, and `continue`d — wasting a concurrency slot. Now only files that need processing enter the pipeline.
- **`--parallel` controls pipeline concurrency** (`cli/process/batch.rs`) — `args.parallel` (default 4, clamped to `min 1`) now sets the `buffer_unordered` limit for image processing. Previously it only controlled LLM enrichment concurrency (that behavior is unchanged in `setup.rs`).

### Tests

214 tests passing (38 CLI + 156 core + 20 integration), zero clippy warnings, zero formatting violations.

---

## [0.6.0] - 2026-02-13

### Summary

Speed Phase 1 — five independent optimizations that eliminate redundant work from every image processed. Each image is now read from disk once instead of twice, the perceptual hasher is constructed once per session instead of per image, only a ~600 KB preprocessed tensor crosses the `spawn_blocking` boundary instead of a ~49 MB decoded image, pixel preprocessing uses raw buffer iteration without bounds checking, and batch output handling performs zero `ProcessedImage` clones. Estimated ~1.5-2x throughput improvement. 214 tests, zero clippy warnings.

### Changed

- **Read file once** (`pipeline/processor.rs`, `pipeline/hash.rs`, `pipeline/decode.rs`) — pipeline now reads each file into a `Vec<u8>` once, computes BLAKE3 from the buffer, and decodes from a `Cursor<Vec<u8>>`. Removed old `decode()`/`decode_sync()` methods. Added `Hasher::content_hash_from_bytes()` and `ImageDecoder::decode_from_bytes()`. The streaming `content_hash(path)` is retained for `--skip-existing` where full file reads would be wasteful.
- **Cached perceptual hasher** (`pipeline/hash.rs`, `pipeline/processor.rs`) — `Hasher` changed from a unit struct to hold a pre-built `image_hasher::Hasher` field. Constructed once in `ImageProcessor::new()`, reused via `&self.hasher` for every image.
- **Preprocess before `spawn_blocking`** (`pipeline/processor.rs`, `embedding/mod.rs`) — preprocessing (resize to 224x224, normalize) now runs before the blocking boundary. Only the resulting `Array4<f32>` (~600 KB) moves into the closure instead of the full `DynamicImage` (~49 MB). Added `EmbeddingEngine::image_size()` and `embed_preprocessed()`.
- **Raw buffer preprocessing** (`embedding/preprocess.rs`) — replaced per-pixel `get_pixel()` + 4D ndarray indexing (both bounds-checked) with raw slice access via `rgb.as_raw().chunks_exact(3)` and flat NCHW offset writes into `as_slice_mut()`. Eliminates all bounds checking on the inner loop.
- **Zero-clone batch output** (`cli/process/batch.rs`, `cli/process/enrichment.rs`, `cli/process/mod.rs`) — `run_enrichment_collect()` now returns owned results via move instead of requiring callers to clone. All post-loop output branches use `.into_iter()` instead of `.iter().map(clone)`. Net result: 0 `ProcessedImage` clones in output handling (down from 2N).

### Tests

214 tests passing (38 CLI + 156 core + 20 integration), zero clippy warnings, zero formatting violations.

---

## [0.5.8] - 2026-02-13

### Summary

All 8 MEDIUM-severity findings from the merged codebase assessment resolved. Silent failures now log warnings (directory traversal errors, skip-existing parse failures, progressive encoding chunk failures), the enricher guards against oversized file reads, ONNX embedding errors include the triggering file path, nested lock acquisition in the scoring path is eliminated, and the CLI enrichment channel is bounded. 214 tests (+4), zero clippy warnings.

### Fixed

- **WalkDir error logging** (M1, `pipeline/discovery.rs`) — replaced `.filter_map(|e| e.ok())` with explicit `match` + `tracing::warn!`. Added `.max_depth(256)` as defense-in-depth against symlink cycles. Directory traversal errors (permission denied, broken symlinks) are now visible instead of silently skipped.
- **Skip-existing parse failure warnings** (M3, `cli/process/batch.rs`) — `load_existing_hashes()` now logs when JSON array parse fails and falls back to JSONL, counts unparseable JSONL lines and warns with reprocessing notice, and warns when the JSON merge-load for `--skip-existing` fails.
- **Progressive encoder failure summary** (M8, `tagging/progressive.rs`) — replaced boolean `all_chunks_succeeded` with `failed_chunks` counter. Logs a summary: `"Progressive encoding: {failed}/{total} chunks failed — vocabulary is incomplete"`.
- **Enricher file size guard** (M4, `llm/enricher.rs`) — added `max_file_size_mb` to `EnrichOptions` (default: 100). `enrich_single()` now checks `tokio::fs::metadata()` before reading — oversized files return `EnrichResult::Failure` without loading into memory.
- **Embedding error context** (M7, `embedding/siglip.rs`, `embedding/mod.rs`) — added `path: &Path` parameter to `SigLipSession::embed()` and `EmbeddingEngine::embed()`. All 6 ONNX error sites now include the triggering image path instead of empty `PathBuf`.
- **Progressive + relevance config warning** (M5, `config/validate.rs`) — `validate()` now warns when both `progressive.enabled` and `relevance.enabled` are true, since relevance pruning is inactive during progressive encoding.
- **Nested lock elimination** (M6, `pipeline/processor.rs`) — restructured pool-aware scoring into three independent lock phases (record hits → expand neighbors → promote). No lock is ever held while acquiring another. Documented lock ordering invariant.
- **Bounded enrichment channel** (M2, `cli/process/enrichment.rs`) — replaced unbounded `channel()` with `sync_channel(64)`, applying backpressure when enrichment patches outpace consumption (~12KB buffer cap).

### Added

- **4 tests** — `test_discover_logs_on_permission_error` (discovery.rs, `#[cfg(unix)]`), `test_load_existing_hashes_warns_on_corrupt_jsonl` (batch.rs), `test_enricher_skips_oversized_file` (enricher.rs), `test_validate_warns_on_progressive_and_relevance` (validate.rs).

### Tests

214 tests passing (38 CLI + 156 core + 20 integration), zero clippy warnings, zero formatting violations.

---

## [0.5.7] - 2026-02-13

### Summary

All 5 HIGH-severity findings from the merged codebase assessment (`docs/plans/merged-assessment.md`) resolved. Lock poisoning no longer cascades through batches, embedding dimension mismatches return errors instead of panicking, the enricher semaphore can't leak on callback panic, provider timeouts no longer compete with the enricher timeout, and I/O errors in validation are properly propagated instead of silently swallowed. 210 tests (+5), zero clippy warnings.

### Fixed

- **Lock poisoning graceful degradation** (H1, `pipeline/processor.rs`) — replaced 7 `.expect()` calls on `RwLock` with `map_err()` / `.ok()?` / `if let Ok()`. Poisoned locks skip tagging and return empty `Vec<Tag>` — all other pipeline outputs (hash, embedding, metadata, thumbnail) preserved. `save_relevance()` propagates lock errors as `PipelineError::Tagging`.
- **Embedding dimension validation** (H2, `tagging/scorer.rs`, `tagging/relevance.rs`) — added `validate_embedding()` check at entry to `score()` and `score_with_pools()`, returning `PipelineError::Tagging` on mismatch. `score_pool()` uses `debug_assert_eq!` (private, called after validation). `record_hits()`, `pool()`, and `promote_to_warm()` bounds-check indices with `tracing::warn` + skip on out-of-bounds.
- **Enricher semaphore leak prevention** (H4, `llm/enricher.rs`) — moved `drop(permit)` before `on_result(result)` so the concurrency permit is released even if the callback panics. Also improves throughput — next LLM request starts immediately while the callback runs. Added `tracing::warn!` on unexpected semaphore closure.
- **Single-authority timeout** (H6, `llm/anthropic.rs`, `llm/openai.rs`, `llm/ollama.rs`) — removed `.timeout()` from `generate()` in all 3 providers. The enricher's `tokio::time::timeout` is now the single source of truth, eliminating silent inner-timeout capping and ensuring the retry logic fires correctly. Ollama's `is_available()` 5-second health-check timeout retained (separate concern).
- **Error propagation over silent swallowing** (H5, `pipeline/validate.rs`, `pipeline/decode.rs`) — `validate.rs:61`: replaced `.unwrap_or(0)` with `.map_err()` → `PipelineError::Decode` preserving the OS error message. `decode.rs`: replaced JPEG fallback with content-based detection via `with_guessed_format()`, falling back to `ImageFormat::from_path()` → `PipelineError::UnsupportedFormat`. Added clarifying comment on benign `decode.rs:120` (value overwritten by caller).

### Added

- **5 tests** — `test_score_dimension_mismatch`, `test_score_with_pools_dimension_mismatch` (scorer.rs); `test_record_hits_out_of_bounds_skips`, `test_pool_out_of_bounds_returns_cold`, `test_promote_to_warm_out_of_bounds_skips` (relevance.rs).
- **`ScoringResult` type alias** (`tagging/scorer.rs`, re-exported from `tagging/mod.rs`) — `(Vec<Tag>, Vec<(usize, f32)>)` to satisfy clippy `type_complexity` after `score_with_pools()` return type changed to `Result`.

### Tests

210 tests passing (37 CLI + 153 core + 20 integration), zero clippy warnings, zero formatting violations.

---

## [0.5.6] - 2026-02-13

### Summary

Assessment review fixes addressing 9 findings (F1–F7, with F8–F9 deferred). Corrected documentation fabrication in v0.5.5's changelog and assessment-structure.md, strengthened existing test assertions with call-count verification and stricter error matching, and added 10 new tests covering enricher concurrency bounds, retry exhaustion, boundary conditions, and skip-option interactions. 205 tests (+10), zero clippy warnings.

### Fixed

- **Documentation fabrication corrected** (F1, `docs/completions/assessment-structure.md`, `docs/changelog.md`) — v0.5.5 changelog and assessment-structure.md falsely claimed `processor.rs` was split into 3 files (559 → 282 lines) with `tagging_loader.rs` and `scoring.rs` extracted. Neither file existed; `processor.rs` remains at 561 lines. Removed fabricated claims, marked Phase 1 as descoped, corrected MockProvider description from "response queue" to factory function pattern.

### Changed

- **Enricher test assertions strengthened** (F2, `llm/enricher.rs`) — `test_enricher_no_retry_on_auth_error` now verifies `call_count == 1` (exactly one call, no retries on 401). `test_enricher_missing_image_file` now verifies `call_count == 0` (provider never called, file read short-circuits). `MockProvider.call_count` changed from `AtomicU32` to `Arc<AtomicU32>` with `call_count_handle()` accessor for shared access.
- **Integration test assertions tightened** (F4/F5, `tests/integration.rs`) — `process_zero_length_file`: removed `FileTooLarge` as acceptable variant (0-byte files always hit `Decode`), added path assertion. `process_corrupt_jpeg_header`: added `!message.is_empty()` check. `process_1x1_pixel_image`: added `perceptual_hash.is_some()` and `thumbnail.is_some()`. `process_unicode_file_path`: added `width > 0`, `height > 0`, `file_size > 0` to guard against silent partial processing.

### Added

- **4 enricher tests** (F3, `llm/enricher.rs`) — `test_enricher_semaphore_bounds_concurrency` (6 images with `parallel=2` and 200ms delay; `in_flight` counter never exceeds 2; uses `multi_thread` with 4 workers), `test_enricher_exhausts_retries` (always-failing 429 with `retry_attempts=2`; asserts `call_count == 3`), `test_enricher_empty_batch` (empty input returns `(0, 0)`, provider never called), `test_enricher_retry_on_server_error` (500 → retry → success; asserts `call_count == 2`).
- **4 boundary condition tests** (F6, `tests/integration.rs`) — `test_file_size_at_exact_limit` (2x2 PNG padded to exactly 1 MB with `max_file_size_mb=1` → succeeds), `test_file_size_one_byte_over_limit` (1 MB + 1 byte → `FileTooLarge`), `test_image_dimension_at_exact_limit` (100x1 PNG with `max_image_dimension=100` → succeeds), `test_image_dimension_one_over_limit` (101x1 → `ImageTooLarge`).
- **2 skip-options tests** (F7, `tests/integration.rs`) — `test_process_with_all_skips` (all 4 skip flags `true`: `thumbnail=None`, `perceptual_hash=None`, `embedding=[]`, `tags=[]`; core fields still populated), `test_process_with_selective_skips` (`skip_thumbnail` + `skip_embedding` true, others false: `thumbnail=None`, `perceptual_hash=Some(...)`, `tags=[]` since tagging depends on embedding).

### Tests

205 tests passing (37 CLI + 148 core + 20 integration), zero clippy warnings, zero formatting violations.

### Deferred

- Processor.rs decomposition — covered by `docs/executing/final-cleanup.md` Phase 2
- Enricher triple-unwrap hardening — covered by `docs/executing/final-cleanup.md` Phase 3
- Sweep logic → RelevanceTracker refactor (F9) — architectural change, needs design
- Output roundtrip struct equality (F8) — low impact

---

## [0.5.5] - 2026-02-13

### Summary

Structural cleanup and test hardening. Tightened module visibility across `photon-core` (5 modules changed from `pub mod` to `pub(crate) mod`), removed dead code and unused re-exports, and added 10 new tests covering the LLM enricher and pipeline edge cases. 195 tests, zero clippy warnings.

### Changed

- **Module visibility tightened** (`lib.rs`) — `embedding`, `llm`, `math`, `output`, `pipeline`, `tagging` changed from `pub mod` to `pub(crate) mod`. All consumer-facing types re-exported from `lib.rs`. All CLI imports updated to use re-export paths.
- **Submodule visibility tightened** — all submodules within `embedding/`, `llm/`, `pipeline/`, `tagging/` changed from `pub mod` to `pub(crate) mod`.
- **Unused re-exports removed** from `llm/mod.rs` (`ImageInput`, `LlmProvider`, `LlmRequest`, `LlmResponse`), `pipeline/mod.rs` (`DecodedImage`, `ImageDecoder`, `MetadataExtractor`, `ThumbnailGenerator`, `Validator`), `tagging/mod.rs` (`Pool`, `RelevanceConfig`, `RelevanceTracker`).

### Removed

- **Dead code**: `SIGLIP_IMAGE_SIZE` constant, `to_json()`/`to_jsonl()` functions in `output.rs`, `encode()` method in `text_encoder.rs`
- **Test-only items gated** with `#[cfg(test)]`: `ThumbnailGenerator::generate_bytes()`, `LabelBank::from_raw()`, `NeighborExpander::find_siblings()`

### Added

- **6 enricher unit tests** (`llm/enricher.rs`) — `MockProvider` implementing `LlmProvider` with configurable responses, call counting, and artificial delay. Covers: basic success, retry on 429, no retry on 401, timeout, partial batch failure, missing file.
- **4 integration edge case tests** (`tests/integration.rs`) — zero-length file → error (not panic), 1x1 pixel image processes correctly, corrupt JPEG header → `PipelineError::Decode`, unicode (CJK) file path preserved in output.

### Tests

195 tests passing (37 CLI + 144 core + 14 integration), zero clippy warnings, zero formatting violations.

---

## [0.5.4] - 2026-02-13

### Summary

Fixed 10 MEDIUM-severity issues identified in the code assessment. Covers `--skip-existing` broken with JSON format, dead Warm→Cold demotion code, `LabelBank::append()` panics, content-based format detection, incomplete EXIF presence checks, config auto-correction, TIFF false positives, dead `enabled` field, and misleading download menu. 185 tests (+21), zero clippy warnings.

### Fixed

- **`--skip-existing` fails on JSON array files** (M2, `cli/process/batch.rs`) — `load_existing_hashes()` parsed line-by-line, which silently failed on JSON arrays. Added two-pass approach: try array parsing first, fall back to line-by-line JSONL.
- **JSON append produces invalid `[...][...]`** (M3, `cli/process/batch.rs`) — appending a second JSON array to an existing file. Now reads existing records, merges, and overwrites. JSONL streaming paths unchanged.
- **`image_size` not auto-derived from model name** (M5, `config/validate.rs`) — setting `model = "siglip-base-patch16-384"` without updating `image_size` silently produced wrong embeddings. `validate()` now auto-corrects with a tracing warning. Signature changed from `&self` to `&mut self`.
- **Warm→Cold demotion never executed** (M1, `tagging/relevance.rs`) — `warm_demotion_checks` config was dead code; terms entering Warm pool stayed forever. Added `warm_checks_without_hit` counter to `TermStats`; `sweep()` now demotes Warm→Cold when threshold exceeded.
- **`LabelBank::append()` panics on dimension mismatch** (M6, `tagging/label_bank.rs`) — `assert_eq!` replaced with `Result<(), PipelineError>`. Caller in `progressive.rs` handles error gracefully.
- **Content-based image format detection** (M9, `pipeline/decode.rs`) — `ImageFormat::from_path()` used extension only. A misnamed file (e.g., JPEG saved as `.png`) was mislabeled. Now uses `ImageReader::with_guessed_format()` with extension fallback.
- **EXIF field presence check incomplete** (M10, `pipeline/metadata.rs`) — only 4 of 10 fields checked. Images with only `iso`, `aperture`, `shutter_speed`, `focal_length`, `gps_longitude`, or `orientation` had EXIF silently dropped. All 10 fields now checked.
- **TIFF magic bytes false positives** (M11, `pipeline/validate.rs`) — only checked `II`/`MM` at bytes 0-1. Now requires full 4-byte TIFF signature including version 42.
- **Dead `enabled` field on LLM configs** (M12, `config/types.rs`) — removed `pub enabled: bool` from all 4 provider configs. Provider selection is purely via `--llm` flag. Interactive module updated to presence-based semantics (`is_some()`).
- **Download command misleading menu** (M8, `cli/models.rs`) — printed a 3-option menu then immediately downloaded option 1. Removed misleading menu; non-interactive path now just logs the download action.

### Tests

185 tests passing (37 CLI + 138 core + 10 integration), zero clippy warnings, zero formatting violations.

---

## [0.5.3] - 2026-02-12

### Summary

HIGH-severity bug fixes from the code assessment review. Fixed a race condition in progressive encoding that could corrupt the label bank cache, and invalid JSON output when using `--llm` with JSON format. Also fixed a minor file size validation off-by-one (downgraded from HIGH to LOW). 164 tests, zero clippy warnings.

### Fixed

- **Progressive encoder race condition** (`tagging/progressive.rs`, `pipeline/processor.rs`) — `start()` spawned a background task that read from `scorer_slot` before the caller could install the seed scorer, risking a vocabulary/bank dimension mismatch on multi-threaded runtimes. The corrupted state would persist to the label bank cache. Seed scorer is now installed inside `start()` *before* `tokio::spawn`; return type changed from `Result<TagScorer>` to `Result<()>`. Caller-side installation in `processor.rs` removed.
- **JSON + LLM stdout emits invalid output** (`cli/process/batch.rs`, `cli/process/mod.rs`) — batch mode printed a JSON array of core records followed by loose enrichment JSON objects — unparseable by any consumer. Single-file mode had the same pattern (sequential `writer.write()` calls producing concatenated JSON objects). Both paths now collect enrichment via `run_enrichment_collect()` and emit a single combined JSON array. JSONL streaming paths unchanged (already correct). File output switched to `write_all()` for proper array wrapping.
- **File size validation off-by-one** (`pipeline/validate.rs`) — integer division `len() / (1024*1024) > limit` truncated, allowing files up to ~1 MB over the limit. Changed to exact byte comparison `len() > limit * 1024 * 1024`. Downgraded from HIGH to LOW — practical impact negligible with default 100 MB limit.

### Tests

164 tests passing (31 CLI + 123 core + 10 integration), zero clippy warnings, zero formatting violations.

---

## [0.5.2] - 2026-02-12

### Summary

Code assessment fixes raising quality from 8/10 to ~9/10. Fixed two bugs (one HIGH, one LOW) and split two oversized files into module directories. Zero known bugs remaining, zero files over 500 production lines, zero unsafe unwraps on fallible paths. 164 tests, zero clippy warnings, zero formatting violations.

### Fixed

- **Progressive encoding cache corruption** (`tagging/progressive.rs`) — when a chunk encoding failed mid-batch, `background_encode()` skipped the chunk via `continue` but still saved the incomplete label bank with the full vocabulary hash, creating a sticky broken state requiring manual deletion of `~/.photon/taxonomy/label_bank.*`. Added `all_chunks_succeeded` tracking; cache save now skipped on partial failure, allowing self-healing re-encode on next startup.
- **Text encoder panic on empty result** (`tagging/text_encoder.rs`) — `.unwrap()` on `batch.into_iter().next()` replaced with `.ok_or_else(|| PipelineError::Model { ... })` to propagate a typed error instead of panicking if ONNX returned an empty tensor.

### Changed

- **`cli/process.rs` → `cli/process/` module** (843 → 5 files) — pure structural refactoring, zero logic changes. `mod.rs` (259), `types.rs` (55), `setup.rs` (176), `batch.rs` (309), `enrichment.rs` (77). All external imports unchanged via `pub use` re-exports.
- **`config.rs` → `config/` module** (667 → 3 files) — pure structural refactoring, zero logic changes. `mod.rs` (166), `types.rs` (418), `validate.rs` (104). All `crate::config::*` and `photon_core::config::*` imports unchanged via `pub use types::*`.

### Metrics

| Metric | Before | After |
|--------|--------|-------|
| Known bugs | 2 (1 HIGH, 1 LOW) | **0** |
| Files >500 production lines | 2 | **0** |
| Unsafe unwraps on fallible paths | 1 | **0** |
| Tests | 164 | **164** |
| Assessed quality | 8/10 | **~9/10** |

### Tests

164 tests passing (31 CLI + 123 core + 10 integration), zero clippy warnings, zero formatting violations.

---

## [0.5.1] - 2026-02-12

### Summary

Interactive CLI hardening across 4 phases — raising assessed quality from 7/10 to 9/10. Added 28 unit tests for all pure functions in the interactive module (test count: 136 → 164). Eliminated both `unsafe set_var` calls by threading API keys through the type system. Replaced `toml` with `toml_edit` to preserve config file comments during API key saves. Made download errors graceful instead of session-killing. Added output path validation with overwrite confirmation. Replaced recursive `Box::pin()` async with a simple loop. Standardized MB display units. Zero `unsafe`, zero clippy warnings, zero formatting violations.

### Added

- **28 unit tests** across 4 files (`mod.rs`, `setup.rs`, `models.rs`, `process.rs`) — covers `handle_interrupt()` (3 tests), `llm_summary()` (4 tests), `config_has_key()` (5 tests), `env_var_for()` + `provider_label()` (2 tests), `InstalledModels::can_process()` (7 tests), `ProcessArgs::default()` (7 tests)
- **`api_key: Option<String>`** field on `ProcessArgs` (`#[arg(skip)]`) — carries session-only API keys from interactive mode without mutating environment variables
- **`inject_api_key()` helper** in `cli/process.rs` — sets the API key on the appropriate provider config section before factory creation, keeping the key in the type system end-to-end
- **Output path validation** in `prompt_output_path()` — re-prompts if parent directory doesn't exist; `Confirm` dialog before overwriting existing files (default: no)
- **Empty model name guards** — Ollama and Hyperbolic `select_model()` arms now filter whitespace-only input via `m.trim().is_empty()`

### Changed

- **`unsafe set_var` → type-threaded API key** — both `unsafe { std::env::set_var(...) }` calls in `setup.rs` replaced with `LlmSelection.api_key` → `ProcessArgs.api_key` → `inject_api_key()` into cloned config. Zero global state mutation.
- **`toml` → `toml_edit 0.22`** in `Cargo.toml` — `save_key_to_config()` rewritten with `toml_edit::DocumentMut` to preserve comments, whitespace, and key ordering during round-trips
- **Graceful download errors** in `interactive/models.rs` — all three download actions (`DownloadVision`, `DownloadShared`, `InstallVocabulary`) wrapped in `match` blocks; failures show `✗ Download failed: <error>` and return to model menu instead of crashing the session
- **`unreachable!()` → safe fallbacks** — `run()` main menu: `_ => {}` (re-shows menu); `show_config()`: `_ => break` (treats as "Back")
- **Recursive async → loop** in `guided_process()` — `Box::pin(guided_process(config)).await` replaced with `loop { ... break }`, eliminating heap allocation and unbounded recursion; `theme`/`dim`/`warn`/`bold` styles created once before the loop
- **Consistent SI MB** — `models.rs` download sizes changed from binary mebibytes (`1024.0 * 1024.0`) to SI megabytes (`1_000_000.0`) to match `process.rs` throughput display
- **Style allocation dedup** — hoisted repeated `Style::new().for_stderr().*` constructions to function scope in `interactive/process.rs` and `interactive/setup.rs`
- **Clippy fix** — `save_key_to_config()`: `.map_or(false, |t| ...)` → `.is_some_and(|t| ...)` to satisfy `unnecessary_map_or` lint
- **Visibility** — `config_has_key`, `env_var_for`, `provider_label` in `setup.rs` changed to `pub(crate)` for testability

### Dependency Changes

| Crate | Before | After | Why |
|-------|--------|-------|-----|
| `toml` | `0.8` (workspace) | Removed | Only used by `save_key_to_config()` |
| `toml_edit` | — | `0.22` | Comment-preserving TOML round-trips |

### Tests

164 tests passing (31 CLI + 123 core + 10 integration), zero clippy warnings, zero formatting violations.

---

## [0.5.0] - 2026-02-12

### Summary

Interactive CLI mode. Running bare `photon` (no subcommand) on a TTY now launches a guided interactive experience using `dialoguer` prompts, walking users through model management, image processing, and LLM configuration — all without memorizing CLI flags. Non-TTY invocations (pipes, scripts) print help as before. The interactive module delegates entirely to the existing `cli::process::execute()` pipeline — zero duplication of processing logic. Implemented across 10 phases: foundation, theme, main menu, prerequisite refactors, guided models, guided process, LLM setup, post-process menu, config viewer, and polish. 136 tests passing, zero clippy warnings.

### Added

- **Interactive entry point** (`cli/interactive/mod.rs`, ~180 lines) — `Option<Commands>` routing in `main.rs` with `std::io::IsTerminal` TTY detection; bare `photon` on TTY → `interactive::run()`, non-TTY → help text
- **Custom theme** (`cli/interactive/theme.rs`, ~50 lines) — `photon_theme()` returns a `ColorfulTheme` with cyan `?` prompt prefix, `▸` active indicator, green `✓` success, red `✗` error; `print_banner()` renders a box-drawn version banner to stderr using `photon_core::VERSION`
- **Main menu loop** — `Select` with 4 options (Process images / Download models / Configure settings / Exit) using `interact_opt()` for clean Esc/Ctrl+C handling
- **Guided model management** (`cli/interactive/models.rs`, ~147 lines) — displays installed/missing status with checkmarks and file sizes; dynamic menu adapts based on what's missing (individual downloads, "download both", re-download all); delegates to extracted `download_vision()` / `download_shared()` / `install_vocabulary()` functions
- **Guided process flow** (`cli/interactive/process.rs`, ~250 lines) — 8-step wizard: input path (with `shellexpand::tilde()` and re-prompt on invalid/empty), file discovery with count+size display, model check with inline download offer, quality preset selection (Fast 224px / High 384px), LLM provider setup (delegates to `setup.rs`), output format selection (adaptive: single-file defaults to stdout, batch defaults to JSONL file), confirmation summary, and `ProcessArgs { ... } → execute()` delegation
- **LLM setup flow** (`cli/interactive/setup.rs`, ~278 lines) — provider selection (Skip / Anthropic / OpenAI / Ollama / Hyperbolic); API key handling via `Password` prompt with env var and config file detection (`config_has_key()` treats `${...}` placeholders as unset); save-to-config option using `toml::Table` manipulation (`save_key_to_config()`); provider-specific model presets with "Custom model name" option
- **Post-process menu** — "Process more images" (recursive via `Box::pin()`) or "Back to main menu" after processing completes
- **Config viewer** (~100 lines in `mod.rs`) — read-only summary of 8 key settings (config file existence, model dir, parallel workers, thumbnail, embedding model, tagging, log level, LLM providers via `llm_summary()` helper); "View full config (TOML)" and "Show config file path" actions
- **`handle_interrupt()` helper** — converts `dialoguer::Error::IO(Interrupted)` → `Ok(None)` for clean Ctrl+C handling across all 5 `Input::interact_text()` call sites
- **Empty directory re-prompt** — combined input path + file discovery into a single validation loop that re-prompts on missing paths or empty directories instead of exiting

### Changed

- **`main.rs`** — `Cli.command` type changed from `Commands` to `Option<Commands>`; existing `Some(Commands::Process(...))` / `Some(Commands::Models(...))` / `Some(Commands::Config(...))` arms unchanged; `None` arm added for interactive routing
- **`cli/mod.rs`** — added `pub mod interactive;`
- **`cli/process.rs`** — added manual `Default` impl for `ProcessArgs` (15 fields matching all `#[arg(default_value = ...)]` annotations) so the interactive module can build args via struct update syntax
- **`cli/models.rs`** — extracted 4 public items from monolithic `execute()`: `InstalledModels` struct (5 bool fields), `check_installed(config)`, `download_vision(indices, config, client)`, `download_shared(config, client)`; added `InstalledModels::can_process()` (requires ≥1 vision model + text encoder + tokenizer); added `VARIANT_LABELS` const; changed `install_vocabulary()` visibility to `pub`; `ModelsCommand::Download` arm refactored from ~90 inline lines to 3 function calls

### New Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `dialoguer` | 0.11 | Terminal prompts: Select, Input, Confirm, Password |
| `console` | 0.15 | Styled terminal output (colors, bold) — also transitive dep of dialoguer |
| `toml` | 0.8 | Config file editing for LLM API key persistence |
| `shellexpand` | 3 | `~` path expansion in user-entered paths |

### New Files

| File | Lines | Purpose |
|------|-------|---------|
| `crates/photon/src/cli/interactive/mod.rs` | ~180 | Entry point, main menu, config viewer, `handle_interrupt()` |
| `crates/photon/src/cli/interactive/theme.rs` | ~50 | Custom `ColorfulTheme` + banner |
| `crates/photon/src/cli/interactive/process.rs` | ~250 | 8-step guided process wizard + post-process menu |
| `crates/photon/src/cli/interactive/models.rs` | ~147 | Guided model management with dynamic menus |
| `crates/photon/src/cli/interactive/setup.rs` | ~278 | LLM provider selection, API key handling, model picker |

### Design Decisions

- **`interact_opt()` everywhere** — all `Select` and `Confirm` widgets use `_opt` variants, returning `Ok(None)` on Esc/Ctrl+C instead of propagating errors. `Input` widgets (which lack `_opt`) use the `handle_interrupt()` wrapper.
- **`ColorfulTheme` customization over `Theme` trait impl** — overriding 10 fields on `ColorfulTheme` gives all 20+ trait methods for free in ~15 lines, vs ~150 lines of manual `Theme` impl.
- **Zero processing duplication** — the interactive module only handles user interaction. All processing goes through `ProcessArgs → execute()`. The interactive module never touches `ImageProcessor`, `EmbeddingEngine`, or any pipeline type directly.
- **`unsafe { std::env::set_var() }`** — required since Rust 1.83 deprecated safe `set_var`. Used only for session-only API key persistence in the single-threaded CLI context. Documented in code.
- **`Box::pin()` for recursive async** — "Process more images" recurses into `guided_process()`, requiring `Pin<Box<dyn Future>>` since the compiler can't determine the future's size for recursive async functions.
- **TTY detection via `std::io::IsTerminal`** — stable since Rust 1.70, no external crate needed. Ensures `echo "test" | photon` prints help instead of hanging on interactive prompts.

### Tests

136 tests passing (unchanged — interactive flows are inherently manual), zero clippy warnings, zero formatting violations.

---

## [0.4.17] - 2026-02-12

### Summary

process.rs decomposition. The monolithic `execute()` function (previously ~475 lines managing a 2x2x2 matrix of single/batch x LLM/no-LLM x file/stdout) reduced to 16 lines of pure orchestration. Five near-identical enrichment blocks consolidated into two reusable helpers. Pure refactor — zero behavior changes, output byte-identical for all flag combinations. 136 tests passing, zero clippy warnings.

### Added

- **`ProcessContext` struct** — bundles processor, options, enricher, config, output format, and LLM flag into a single context passed between functions
- **`setup_processor()`** — extracted ~110 lines of input validation, config loading, quality preset, model loading, options construction, and enricher creation
- **`process_single()`** — extracted ~50 lines handling single-file processing with LLM/no-LLM and file/stdout branching
- **`process_batch()`** — extracted ~210 lines handling batch processing: skip-existing, progress bar, streaming loop, post-loop output, and summary
- **`run_enrichment_collect()`** — consolidated 3 duplicated spawn+channel enrichment blocks into one function that returns `Vec<OutputRecord>` (used for file-targeted enrichment)
- **`run_enrichment_stdout()`** — consolidated 2 duplicated inline-callback enrichment blocks into one function with `pretty: bool` parameter (used for stdout streaming)

### Changed

- **`execute()`** — replaced ~475-line body with 16-line orchestration: `setup_processor()` → `discover()` → `process_single()` or `process_batch()`
- **Module-level imports** — `photon_core` imports previously scoped inside `execute()` moved to module level since they're now shared across multiple functions

### Metrics

| Metric | Before | After |
|--------|--------|-------|
| `execute()` length | ~475 lines | 16 lines |
| Enrichment code copies | 5 | 2 |
| Total file length | 722 lines | 726 lines |

### Tests

136 tests passing (unchanged), zero clippy warnings, zero formatting violations.

---

## [0.4.16] - 2026-02-12

### Summary

Streaming batch output for JSONL format. Batch processing now writes each `ProcessedImage` to the output file as it's produced, instead of collecting all results in a `Vec` and writing after the loop. For 1000 images without LLM, memory usage drops from ~6 MB to ~0 MB; with LLM, from ~24 MB to ~6 MB. JSON array format unchanged (collecting is inherent to `[...]`). 136 tests passing, zero clippy warnings.

### Changed

- **`process.rs` — JSONL file streaming** — added `stream_to_file` flag and pre-loop `OutputWriter` initialization; each result written to file immediately inside the processing loop rather than collected in `results` Vec
- **`process.rs` — collection condition simplified** — `results.push(result)` now only triggers when `llm_enabled || JSON format`; JSONL file output without LLM skips collection entirely
- **`process.rs` — streaming LLM enrichment** — in JSONL streaming path, `results` Vec moved into enricher spawn (no `.clone()` needed since core records are already on disk); enrichment patches appended to the same file after enricher completes
- **`process.rs` — post-loop restructured** — output handling split into `if stream_to_file { ... } else { ... }` branches; non-streaming path (JSON format, stdout) wrapped in else with zero logic changes

### Memory impact

| Scenario | Before | After |
|----------|--------|-------|
| 1000 images, JSONL, no LLM | ~6 MB in Vec + file write | ~0 MB, streamed to file |
| 1000 images, JSONL, with LLM | ~24 MB (Vec + clone + all_records) | ~6 MB (Vec for enricher only) |
| 1000 images, JSON array | ~6 MB in Vec | ~6 MB in Vec (unchanged) |

### Tests

136 tests passing (unchanged), zero clippy warnings, zero formatting violations.

---

## [0.4.15] - 2026-02-12

### Summary

Model download integrity verification. All 4 model files (`visual.onnx` ×2, `text_model.onnx`, `tokenizer.json`) are now verified against embedded BLAKE3 checksums after download. Corrupt or truncated files are automatically removed with a clear error message guiding the user to re-download. Reuses the existing `blake3` crate (no new dependencies) with streaming 64 KB buffer verification. 136 tests passing (+3 verification tests), zero clippy warnings.

### Added

- **`models.rs` — `verify_blake3()`** — standalone function that computes the BLAKE3 hash via streaming `Hasher::content_hash()` (64 KB buffer), compares against the expected digest, and on mismatch removes the corrupt file and returns an error with expected vs actual hashes
- **`models.rs` — checksum constants** — 4 compile-time BLAKE3 digests for `siglip-base-patch16/visual.onnx`, `siglip-base-patch16-384/visual.onnx`, `text_model.onnx`, and `tokenizer.json`
- **`models.rs` — `ModelVariant.blake3`** — new `&'static str` field on the vision model variant struct holding the expected hex digest

### Changed

- **`models.rs` — `download_file()` signature** — now accepts `expected_blake3: Option<&str>` as 4th parameter; verification runs after `file.flush()` completes; all 3 call sites updated

### Tests

136 tests passing (+3 from 133), zero clippy warnings, zero formatting violations.

| New test | Validates |
|----------|-----------|
| `verify_blake3_correct_hash` | Correct hash passes, file preserved |
| `verify_blake3_wrong_hash_removes_file` | Wrong hash returns error, file deleted, error contains "Checksum mismatch" and "Corrupt file removed" |
| `verify_blake3_missing_file` | Nonexistent file returns error gracefully |

---

## [0.4.14] - 2026-02-12

### Summary

First integration tests for `photon-core`. 10 `#[tokio::test]` functions exercise `ImageProcessor::process()` end-to-end against real fixture images, covering the full pipeline (decode → EXIF → hash → thumbnail), option toggling, error paths, output serialization roundtrips, and hash determinism. All tests run without ML models installed. 133 tests passing (+10 integration tests), zero clippy warnings.

### Added

- **`crates/photon-core/tests/integration.rs`** — 10 end-to-end integration tests using shared fixtures at `tests/fixtures/images/`

### Tests

133 tests passing (+10 from 123), zero clippy warnings, zero formatting violations.

| New test | Validates |
|----------|-----------|
| `full_pipeline_without_models` | Decode, EXIF, hash, thumbnail populate correctly; embedding/tags empty without models; description is None |
| `full_pipeline_skips_thumbnail` | `ProcessOptions::skip_thumbnail` produces `None` thumbnail while other fields remain populated |
| `full_pipeline_skips_perceptual_hash` | `ProcessOptions::skip_perceptual_hash` produces `None` while thumbnail still generates |
| `process_multiple_formats` | All 4 fixtures (`test.png`, `beach.jpg`, `dog.jpg`, `car.jpg`) succeed with correct format strings |
| `process_nonexistent_file` | Returns `PhotonError::Pipeline(PipelineError::FileNotFound)` with correct path |
| `process_rejects_oversized_dimensions` | Returns `PipelineError::ImageTooLarge` when `max_image_dimension = 1` |
| `discover_finds_fixtures` | `discover()` returns all 4 test images from fixtures directory |
| `output_roundtrip_json` | Process → serialize → deserialize → all fields match |
| `output_roundtrip_jsonl` | Process 2 images → JSONL → parse each line → content hashes match |
| `deterministic_content_hash` | Process same file twice → both content_hash and perceptual_hash identical |

---

## [0.4.13] - 2026-02-11

### Summary

Hardening pass addressing three quick-win items from the code assessment (`docs/executing/finish-testing.md`). Replaced 8 bare `.unwrap()` calls on `RwLock` with descriptive `.expect()` messages, added config validation with range checks on 9 fields, and surfaced a warning when `config.toml` is malformed instead of silently falling back to defaults. 123 tests passing (+5 new validation tests), zero clippy warnings.

### Fixed

- **`processor.rs` — lock poisoning diagnostics** — 8 `.unwrap()` calls on `RwLock::read()`/`write()` replaced with `.expect()` messages that identify which lock (`TagScorer` vs `RelevanceTracker`) and which operation (scoring, seed installation, hit recording, neighbor expansion, save) was in progress when the lock was found poisoned
- **`main.rs` — silent config error** — `Config::load().unwrap_or_default()` replaced with explicit `match` that prints a warning to stderr (`eprintln!`) before falling back to defaults; uses stderr because logging isn't initialized yet at this point in startup

### Added

- **`config.rs` — `Config::validate()`** — range validation called from `Config::load_from()`, rejects zero values for `parallel_workers`, `buffer_size`, `max_file_size_mb`, `max_image_dimension`, `decode_timeout_ms`, `embed_timeout_ms`, `llm_timeout_ms`, `thumbnail.size`, and out-of-range `tagging.min_confidence` (must be 0.0–1.0); returns `ConfigError::ValidationError` with field-specific messages

### Tests

123 tests passing (+5 from 118), zero clippy warnings, zero formatting violations.

| New test | Validates |
|----------|-----------|
| `test_default_config_passes_validation` | Default config always valid |
| `test_validate_rejects_zero_parallel_workers` | Workers = 0 rejected |
| `test_validate_rejects_zero_thumbnail_size` | Thumbnail size = 0 rejected |
| `test_validate_rejects_zero_timeout` | Timeout = 0 rejected |
| `test_validate_rejects_invalid_min_confidence` | Confidence outside 0.0–1.0 rejected |

---

## [0.4.12] - 2026-02-11

### Summary

Benchmark fixture path fix. Three of five criterion benchmarks (`content_hash_blake3`, `decode_image`, `metadata_extract`) were silently skipped because they used relative paths (`tests/fixtures/images/test.png`) that only resolve from the workspace root. The benchmark binary's working directory is the crate directory (`crates/photon-core/`), so the fixtures were invisible. Fixed by resolving paths through `env!("CARGO_MANIFEST_DIR")`. All 5 benchmarks now execute. 118 tests passing, zero clippy warnings.

### Fixed

- **`benches/pipeline.rs` — fixture path resolution** — added `fixture_path()` helper that constructs absolute paths via `env!("CARGO_MANIFEST_DIR")` + `../../tests/fixtures/images/`; applied to `content_hash_blake3`, `decode_image`, and `metadata_extract` benchmarks

### Benchmark Results (Apple Silicon)

| Benchmark | Time | Input |
|-----------|------|-------|
| `content_hash_blake3` | ~5.0 µs | `test.png` (70 B) |
| `perceptual_hash` | ~219 µs | Synthetic 256x256 |
| `decode_image` | ~16.8 µs | `test.png` (70 B) |
| `thumbnail_256px` | ~2.90 ms | Synthetic 1920x1080 |
| `metadata_extract` | ~5.4 µs | `test.png` (70 B) |

---

## [0.4.11] - 2026-02-11

### Summary

Final comprehensive code assessment across the entire workspace. Full review of all source files in both crates (photon-core library, photon CLI), tests, CI/CD, dependencies, and documentation. Overall rating: **7.5/10** — strong architecture, solid test coverage, a few concrete gaps to address. Fixed stale config references in README (`device` and `quality` fields removed in v0.4.9 but still shown in config example). 118 tests passing, zero clippy warnings.

### Fixed

- **`README.md` — stale config fields** — removed `device = "cpu"` from `[embedding]` section and `quality = 80` from `[thumbnail]` section; both fields were dead code removed in v0.4.9 but still appeared in the README config example

### Added

- **`docs/executing/finish-testing.md` — code assessment** — comprehensive review covering architecture (9/10), code quality (8/10), error handling (9/10), testing (7/10), documentation (7/10), safety (7/10), performance (8.5/10), CI/CD (9/10), dependencies (8/10), CLI UX (7.5/10); identified 10 issues (0 critical, 3 high, 4 medium, 3 low)

### Tests

118 tests passing, zero clippy warnings, zero formatting violations.

---

## [0.4.10] - 2026-02-11

### Summary

CI clippy fix. Rust 1.93's `clippy::unnecessary_unwrap` lint (promoted to deny via `-D warnings`) rejected the `is_some()` guard + `.unwrap()` pattern in batch output writing. Replaced with `Option::filter()` to destructure via `if let` in one expression. 118 tests passing, zero clippy warnings.

### Fixed

- **`process.rs` — `unnecessary_unwrap` lint** — `args.output.is_some()` guard followed by `args.output.as_ref().unwrap()` replaced with `args.output.as_ref().filter(|_| !results.is_empty())` destructured via `if let Some(output_path)`

---

## [0.4.9] - 2026-02-11

### Summary

Code polishing pass addressing all 9 open items from the code assessment (`docs/executing/code-assessment.md`). Two performance fixes on the tagging hot path (clone elimination, O(N×K) → O(N+K) sibling lookups), dead code and config removal (~120 LOC), test hygiene improvements, and CLI control flow cleanup. 118 tests passing, zero clippy warnings.

### Fixed

- **`scorer.rs` — hot-path clone eliminated** — `hits_to_tags()` now takes `&[(usize, f32)]` instead of `Vec<(usize, f32)>`, removing a per-image `.clone()` of the full hit vector in `score_with_pools()`
- **`neighbors.rs` — O(N×K) → O(N+K) sibling lookups** — `expand_all()` now builds the parent index once via `vocabulary.build_parent_index()` and uses it for all K promoted terms, instead of linearly scanning 68K terms per promotion
- **`CLAUDE.md` — corrected Data Directory Layout** — `text_model.onnx` and `tokenizer.json` are at the models root (`~/.photon/models/`), not inside the variant subdirectory; also updated stale test count (50 → 120+)
- **`scorer.rs`, `neighbors.rs` — test tempdir leak** — replaced `std::mem::forget(dir)` with returning `TempDir` alongside results so temporary directories are properly cleaned up after each test run
- **`process.rs` — enricher created once** — `create_enricher()` was called at 4 separate branch sites; now created once before branching and consumed via `Option::take()` in the executing branch

### Removed

- **`ThumbnailConfig.quality`** — dead config field; the `image` crate v0.25's WebP encoder only supports lossless encoding with no quality parameter
- **`EmbeddingConfig.device`** — dead config field; ONNX Runtime auto-selects execution providers at build time, this field was never referenced
- **`pipeline/channel.rs`** — `PipelineStage` and `bounded_channel` were defined and tested but never used by production code (116 LOC, 2 tests removed)

### Changed

- **`models.rs` — `reqwest::Client` reuse** — single client instance shared across all 3 HuggingFace download calls for HTTP connection pooling

### Tests

118 tests passing (−2 from removed `channel.rs` dead code), zero clippy warnings.

---

## [0.4.8] - 2026-02-11

### Summary

Benchmark compilation fix. Two benchmarks (`decode_image`, `thumbnail_256px`) called `ImageDecoder::decode` and `ThumbnailGenerator::generate` as static methods, but both are instance methods that take `&self`. Fixed by constructing instances with default configs and bridging async via `tokio::runtime::Runtime::block_on()` for the async decode path. Zero logic changes to library code.

### Fixed

- **`benchmark_decode`** — `ImageDecoder::decode(path)` → construct `ImageDecoder::new(LimitsConfig::default())` + `rt.block_on(decoder.decode(path))` to match the actual `async fn(&self, &Path)` signature
- **`benchmark_thumbnail`** — `ThumbnailGenerator::generate(&img, 256, 80)` → construct `ThumbnailGenerator::new(ThumbnailConfig::default())` + `generator.generate(&img)` to match the actual `fn(&self, &DynamicImage)` signature

---

## [0.4.7] - 2026-02-11

### Summary

Production UX polish and release infrastructure. Adds an indicatif progress bar for batch processing, working `--skip-existing` flag (BLAKE3 hash-based dedup against existing output), formatted summary statistics on stderr, contextual error hints for every error variant, criterion benchmarks for core pipeline stages, GitHub Actions CI/CD (check + lint + multi-platform release), and MIT license file. 120 tests passing, zero clippy warnings.

### Added

- **Progress bar** (`indicatif`) — spinner + elapsed + bar + rate display during batch processing; placed in CLI crate (not core) as a terminal UI concern
- **`--skip-existing` flag** — reads existing output file, extracts `content_hash` values into a `HashSet`, skips already-processed images; appends to output file instead of truncating; handles both JSONL and `OutputRecord` dual-stream formats
- **Summary statistics** — formatted table printed to stderr after batch processing showing succeeded/failed/skipped counts, duration, rate (img/sec), and throughput (MB/sec); failed/skipped rows omitted when zero
- **`PipelineError::hint()` / `PhotonError::hint()`** — contextual recovery suggestions per error variant (e.g. "Run `photon models download`" for model errors, "Check your API key" for 401/403 LLM errors, format list for unsupported formats)
- **Criterion benchmarks** (`crates/photon-core/benches/pipeline.rs`) — `content_hash_blake3`, `perceptual_hash`, `decode_image`, `thumbnail_256px`, `metadata_extract`; gracefully skip when fixtures missing
- **GitHub Actions CI** (`.github/workflows/ci.yml`) — `cargo check` + `cargo test` on macOS-14 ARM and ubuntu-latest; `cargo fmt --check` + `cargo clippy -D warnings` lint job
- **GitHub Actions Release** (`.github/workflows/release.yml`) — triggered on `v*` tags; builds release binaries for `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu`; packages as `.tar.gz`; creates GitHub Release with auto-generated notes
- **`LICENSE-MIT`** — MIT license text (workspace declares `MIT OR Apache-2.0`)

### Changed

- **`README.md`** — LLM section no longer says "coming soon", added LLM usage examples, updated test count to 120+, added benchmark instructions, added Hyperbolic to provider list

---

## [0.4.6] - 2026-02-11

### Summary

Housekeeping pass: ran `cargo fmt` across the workspace to fix 6 formatting violations (line-length wrapping, closure argument formatting). No logic changes. 120 tests passing, zero clippy warnings.

### Changed

- **`crates/photon/src/cli/process.rs`** — reformatted long lines and closure arguments (3 sites)
- **`crates/photon-core/src/llm/enricher.rs`** — reformatted closure wrapping (1 site)
- **`crates/photon-core/src/llm/anthropic.rs`** — reformatted long line (1 site)
- **`crates/photon-core/src/llm/ollama.rs`** — reformatted long line (1 site)
- **`crates/photon-core/src/tagging/seed.rs`** — reformatted test data arrays
- **`crates/photon-core/src/tagging/vocabulary.rs`** — reformatted test data arrays

---

## [0.4.5] - 2026-02-11

### Summary

Post-processing step that suppresses redundant ancestor tags and optionally annotates surviving tags with abbreviated WordNet hierarchy paths. When both "labrador retriever" (0.87) and "dog" (0.68) pass threshold, the ancestor "dog" is suppressed — the specific term is strictly more informative. Surviving tags can optionally display their hierarchy: `"animal > canine > labrador retriever"`. Both features are off by default; existing JSON output is byte-identical when disabled. 120 tests passing (+20 new), zero clippy warnings.

### Added

- **`HierarchyDedup`** (`tagging/hierarchy.rs`) — unit struct with two pure functions: `deduplicate()` suppresses tags that are WordNet ancestors of other tags in the list; `add_paths()` annotates surviving tags with abbreviated hierarchy paths (generic terms like "entity", "object", "organism" filtered out)
- **`Tag.path`** (`types.rs`) — new `Option<String>` field with `#[serde(skip_serializing_if = "Option::is_none")]` for backward-compatible hierarchy path display
- **CLI flags** — `--show-tag-paths` enables path annotation, `--no-dedup-tags` disables ancestor suppression

### Changed

- **`TagScorer::hits_to_tags()`** — wired dedup and path annotation at the end of the shared helper, so both `score()` and `score_with_pools()` benefit automatically
- **`TaggingConfig`** — added `deduplicate_ancestors: bool` (default `false`), `show_paths: bool` (default `false`), `path_max_depth: usize` (default `2`)

### Configuration

```toml
[tagging]
deduplicate_ancestors = false   # Suppress tags that are ancestors of more specific tags
show_paths = false              # Annotate tags with WordNet hierarchy paths
path_max_depth = 2              # Max ancestor levels shown in path strings
```

### Tests

120 tests passing (+20 new):

- **Hierarchy — is_ancestor** (5): `test_is_ancestor_direct_parent`, `test_is_ancestor_grandparent`, `test_is_ancestor_unrelated`, `test_is_ancestor_self`, `test_is_ancestor_supplemental`
- **Hierarchy — deduplicate** (6): `test_dedup_suppresses_ancestors`, `test_dedup_preserves_unrelated`, `test_dedup_multiple_chains`, `test_dedup_no_hypernyms`, `test_dedup_empty_tags`, `test_dedup_preserves_order`
- **Hierarchy — add_paths** (6): `test_add_paths_basic`, `test_add_paths_skips_generic`, `test_add_paths_max_ancestors`, `test_add_paths_supplemental_no_path`, `test_add_paths_short_chain`, `test_add_paths_all_generic_hypernyms`
- **Types** (2): `test_tag_serde_without_path`, `test_tag_serde_with_path`
- **Config** (1): `test_tagging_config_hierarchy_defaults`

---

## [0.4.4] - 2026-02-11

### Summary

Self-organizing vocabulary with three-pool system (Active/Warm/Cold) for relevance-based scoring optimization. Terms that never match are demoted to reduce scoring cost; WordNet neighbors of active terms are promoted for deeper coverage. Per-term statistics (hit count, average confidence, last match timestamp) drive pool transitions. Relevance data persists across runs via `~/.photon/taxonomy/relevance.json`. Backward compatible — `relevance.enabled = false` (the default) gives identical behavior to Phase 4a/4b. 100 tests passing (+32 new), zero clippy warnings.

### Added

- **`RelevanceTracker`** (`tagging/relevance.rs`) — per-term statistics tracking with `Pool` enum (`Active`/`Warm`/`Cold`), `TermStats` (hit count, score sum, last hit timestamp), pool transition logic via `sweep()`, and JSON persistence by term name for cross-run stability
- **`NeighborExpander`** (`tagging/neighbors.rs`) — WordNet sibling lookup via shared first hypernym; when a term is promoted to Active, its siblings are promoted to Warm for evaluation
- **`TagScorer::score_with_pools()`** — pool-aware scoring that scores Active terms every image, Warm terms every Nth image, and skips Cold terms entirely; returns raw hits for separate recording
- **`TagScorer::score_pool()`** — single-pool scoring with configurable pool filter
- **`TagScorer::hits_to_tags()`** — extracted shared filter → sort → truncate helper used by both `score()` and `score_with_pools()` to prevent logic divergence
- **`Vocabulary::parent_of()`** and **`Vocabulary::build_parent_index()`** — hypernym-based parent lookup for neighbor expansion
- **`LabelBank::from_raw()`** — constructor for test and external use
- **`RelevanceConfig`** — new sub-section of `TaggingConfig` with `enabled`, `warm_check_interval`, `promotion_threshold`, `active_demotion_days`, `warm_demotion_checks`, `neighbor_expansion`

### Changed

- **`ImageProcessor`** — added `relevance_tracker: Option<RwLock<RelevanceTracker>>`, `sweep_interval`, and `neighbor_expansion` fields; added `load_relevance_tracker()` and `save_relevance()` methods
- **`process_with_options()` tagging stage** — rewritten with split read-lock scoring / write-lock recording pattern: scoring runs under read lock (concurrent), only the brief `record_hits()` call needs a write lock; periodic sweep + neighbor expansion inside the write lock
- **CLI (`process.rs`)** — calls `save_relevance()` after both single-file and batch processing to persist pool state

### Configuration

```toml
[tagging.relevance]
enabled = false              # Opt-in (default off until stable)
warm_check_interval = 100    # Score warm pool every N images
promotion_threshold = 0.3    # Min confidence for warm → active
active_demotion_days = 90    # Demote active terms with no hits in N days
warm_demotion_checks = 50    # Demote warm terms after N checks with no hits
neighbor_expansion = true    # Auto-expand WordNet siblings of promoted terms
```

### Tests

100 tests passing (+32 new):

- **Relevance** (20): `test_avg_confidence_zero_hits`, `test_avg_confidence_calculation`, `test_pool_serde_roundtrip`, `test_new_encoded_terms_active`, `test_new_unencoded_terms_cold`, `test_record_hits_updates_stats`, `test_record_hits_increments_image_count`, `test_sweep_demotes_stale_active`, `test_sweep_demotes_never_hit_active`, `test_sweep_promotes_warm_with_hits`, `test_sweep_returns_promoted_indices`, `test_sweep_preserves_recent_active`, `test_should_check_warm_interval`, `test_pool_counts`, `test_promote_to_warm`, `test_save_load_roundtrip`, `test_load_with_vocabulary_change`, `test_load_missing_file_error`, `test_relevance_config_defaults`
- **Neighbors** (6): `test_find_siblings_shared_parent`, `test_find_siblings_excludes_self`, `test_find_siblings_no_hypernyms`, `test_find_siblings_different_parent`, `test_expand_all_deduplicates`, `test_expand_all_excludes_promoted`
- **Scorer** (3): `test_hits_to_tags_filters_sorts_truncates`, `test_score_pool_filters_by_pool`, `test_score_with_pools_returns_hits`
- **Vocabulary** (3): `test_parent_of_wordnet_term`, `test_parent_of_supplemental_term`, `test_build_parent_index`
- **Config** (1): `test_tagging_config_includes_relevance`

---

## [0.4.3] - 2026-02-11

### Summary

Progressive encoding for first-run cold-start optimization. Previously, `load_tagging()` encoded all ~68K vocabulary terms in a single blocking call (~90 minutes on CPU) before the first image could be processed. Now encodes a seed set of ~2K high-value terms synchronously (~30 seconds), starts processing immediately, then background-encodes remaining terms in 5K-term chunks — progressively swapping in richer scorers via `RwLock`. Subsequent runs are unchanged (cached `label_bank.bin` loads instantly). 68 tests passing (+18 new), zero clippy warnings.

### Added

- **`SeedSelector`** (`tagging/seed.rs`) — deterministic seed term selection with three-tier priority: all supplemental terms (scenes, moods, styles), curated seed file matches, then seeded random fill to reach target size
- **`ProgressiveEncoder`** (`tagging/progressive.rs`) — background encoding orchestration using `tokio::spawn` + `spawn_blocking`; encodes in chunks, appends to a running `LabelBank`, and atomically swaps progressively larger `TagScorer` instances via `RwLock`
- **`seed_terms.txt`** (`data/vocabulary/`) — 1041 curated common visual nouns (animals, vehicles, nature, food, people, buildings, furniture, clothing, sports, technology, music, weather), all validated against `wordnet_nouns.txt`
- **`ProgressiveConfig`** — new sub-section of `TaggingConfig` with `enabled` (default: true), `seed_size` (default: 2000), `chunk_size` (default: 5000)
- **`Vocabulary::empty()`** and **`Vocabulary::subset(indices)`** — create empty vocabularies and sub-vocabularies from index lists with rebuilt `by_name` index
- **`LabelBank::empty()`** and **`LabelBank::append(other)`** — placeholder creation and incremental matrix growth for the append-only encoding pattern
- **`TagScorer::label_bank()`** and **`TagScorer::vocabulary()`** — accessor methods for progressive encoder to clone/inspect scorer state

### Changed

- **`ImageProcessor::tag_scorer`** — type changed from `Option<Arc<TagScorer>>` to `Option<Arc<RwLock<TagScorer>>>` to support concurrent reads with progressive write swaps
- **`load_tagging()`** — rewritten with three code paths: cached (fast, unchanged), progressive (new, default on cache miss), and blocking (legacy fallback when `progressive.enabled = false` or no tokio runtime)
- **`process_with_options()` scoring** — now acquires `read()` lock on the scorer before calling `score()`, enabling safe concurrent access during background swaps
- **`LabelBank`** — now derives `Clone` (needed for running bank pattern in progressive encoder)

### Dependencies

- **`rand` 0.8** (new) — seeded deterministic random sampling for seed term selection
- **`tempfile` 3** (dev, new) — test helpers for temporary vocabulary files

### Tests

68 tests passing (+18 new):

- **Vocabulary**: `test_empty_vocabulary`, `test_subset_preserves_terms`, `test_subset_empty`, `test_subset_rebuilds_index`, `test_subset_preserves_hypernyms`
- **LabelBank**: `test_empty_label_bank`, `test_append_grows_matrix`, `test_append_preserves_existing`, `test_append_empty_to_empty`, `test_append_to_empty`, `test_append_dimension_mismatch`
- **SeedSelector**: `test_select_includes_supplemental`, `test_select_respects_target_size`, `test_select_deterministic`, `test_select_without_seed_file`, `test_select_with_seed_file`
- **Config**: `test_progressive_config_defaults`, `test_tagging_config_includes_progressive`

---

## [0.4.2] - 2026-02-11

### Summary

Second-pass review of the LLM integration layer, resolving 5 bugs (1 medium, 3 low, 1 cosmetic) and 2 code smells. Key fix: `--format json --output file.json --llm <provider>` now produces a valid JSON array instead of concatenated objects. Also adds empty-response guards to Anthropic/Ollama (matching the existing OpenAI fix), removes a dead field from `PipelineError::Llm`, and restores missing enrichment stats logging. 50 tests passing, zero clippy warnings.

### Fixed

- **Batch + file + JSON + `--llm` produces invalid JSON** (medium) — core and enrichment records were written individually via `writer.write()`, producing concatenated JSON objects instead of a valid array; now buffers all `OutputRecord`s and calls `writer.write_all()` for correct `[...]` output
- **Anthropic/Ollama silently accept empty LLM responses** — both providers could produce `EnrichmentPatch` with a blank description; added empty/whitespace checks matching the existing OpenAI guard, returning `PipelineError::Llm` on empty content
- **Enrichment stats not logged in batch + stdout + `--llm` path** — the `(succeeded, failed)` return value from `enrich_batch()` was silently discarded; now captured and passed to `log_enrichment_stats()`
- **`PipelineError::Llm { path }` always `None`** — the `path: Option<PathBuf>` field was never populated anywhere in the codebase (path context is carried by `EnrichResult::Failure` instead); removed the dead field from the error variant and all 17 construction sites
- **Misleading comment on stdout enrichment block** — updated `// JSONL only` to `// JSON and JSONL` to match actual behavior

### Improved

- **Redundant retry substring check** — removed `message.contains("connection")` from retry fallback since `"connect"` already matches it as a substring
- **`EnrichResult` now derives `Debug`** — improves log and test output visibility

### Tests

50 tests passing (unchanged count — no new tests needed; existing coverage exercises the changed paths).

---

## [0.4.1] - 2026-02-11

### Summary

Post-Phase-5 code review resolving 7 bugs in the LLM integration layer. Key fixes: structured HTTP status codes for retry classification (replacing brittle string matching), correct enricher success/failure counting, silent data loss in batch JSON stdout mode, and async file I/O in the enricher. 50 tests passing (+2 new regression tests), zero clippy warnings.

### Fixed

- **Enricher success/failure counting** — spawned tasks now return a `bool` indicating LLM outcome; previously `Ok(())` was always returned regardless of success, making the `(succeeded, failed)` tuple meaningless
- **Batch JSON stdout data loss with `--llm`** — core records were silently dropped when using `--format json --llm <provider>` to stdout; removed the `!llm_enabled` guard and wrapped core records in `OutputRecord::Core` before printing
- **String-based retry classification** — added `status_code: Option<u16>` to `PipelineError::Llm`; retry logic now matches on typed HTTP status codes instead of substring matching (`"500"` in message body could false-positive)
- **Empty OpenAI `choices` array** — `unwrap_or_default()` silently produced blank descriptions; replaced with `ok_or_else(...)` to surface as a retryable error
- **Blocking `std::fs::read` in async context** — replaced with `tokio::fs::read().await` in `enrich_single` to avoid stalling tokio worker threads on large image reads
- **`HyperbolicProvider::timeout()` dead code** — returned a hardcoded 60s that was never called; now delegates to the inner `OpenAiProvider` for consistency
- **`PathBuf::new()` sentinel in LLM errors** — changed `path: PathBuf` to `path: Option<PathBuf>` in the `Llm` error variant; providers use `None` instead of empty paths, fixing error messages that read `"LLM error for : ..."`

### Tests

50 tests passing (+2 new regression tests):

- `test_message_with_500_in_body_not_retryable_without_status` — verifies string "500" in message body is not misclassified as retryable
- `test_connection_error_retryable_without_status` — verifies connection errors are retryable via message fallback

---

## [0.4.0] - 2026-02-11

### Summary

BYOK (Bring Your Own Key) LLM integration for AI-generated image descriptions. Supports four providers: Ollama (local), Anthropic, OpenAI, and Hyperbolic. Uses a **dual-stream output** model — core pipeline results emit immediately at full speed, then LLM descriptions follow as enrichment patches. Without `--llm`, output is identical to pre-Phase-5 (backward compatible).

### Added

- **LLM provider abstraction** — `LlmProvider` async trait with `#[async_trait]` for object-safe dynamic dispatch across providers at runtime
- **Ollama provider** — POST to `/api/generate` with base64 images, no auth, 120s default timeout for local vision models
- **Anthropic provider** — Messages API with `x-api-key` + `anthropic-version` headers, base64 image content blocks, token usage tracking
- **OpenAI provider** — Chat Completions with `Authorization: Bearer` header, data URL image content, custom endpoint support
- **Hyperbolic provider** — thin wrapper over `OpenAiProvider` with custom endpoint (`{config.endpoint}/chat/completions`)
- **`LlmProviderFactory`** — creates `Box<dyn LlmProvider>` from provider name + config, with `${ENV_VAR}` expansion for API keys
- **Enricher** — concurrent LLM orchestration engine using `tokio::Semaphore` for bounded parallelism (capped at 8), retry loop with exponential backoff, per-request timeouts
- **Retry logic** — classifies timeouts, HTTP 429, and 5xx as retryable; exponential backoff (1s, 2s, 4s, 8s…) capped at 30s
- **Tag-aware prompts** — `LlmRequest::describe_image()` includes Phase 4 zero-shot tags for more focused LLM descriptions
- **`EnrichmentPatch`** struct — content_hash, description, llm_model, llm_latency_ms, llm_tokens
- **`OutputRecord`** enum — internally tagged (`"type":"core"` / `"type":"enrichment"`) for dual-stream JSONL consumers
- **Dual-stream CLI output** — `process.rs` emits `OutputRecord::Core` records immediately, then `OutputRecord::Enrichment` patches as LLM calls complete

### Changed

- **`process.rs` `execute()`** rewritten for dual-stream output when `--llm` is active; backward compatible without `--llm`
- **`lib.rs`** — added `pub mod llm`, re-exported `EnrichmentPatch` and `OutputRecord`

### Pipeline Stages

```
Validate → Decode → EXIF → Hash → Thumbnail → Embed (SigLIP) → Tag (SigLIP) → JSON
                                                                                  ↓
                                                                         Enricher (LLM) → Enrichment JSONL
```

### Dependencies

- `async-trait` 0.1 (new) — object-safe async trait for `Box<dyn LlmProvider>`
- `reqwest` now also in photon-core (was CLI-only)
- `futures-util` now also in photon-core

### Tests

48 tests passing (+16 new over 0.3.3 baseline of 32):

- **provider.rs** (6): JPEG/PNG MIME detection, base64 encoding, data URL format, prompt generation with/without tags, `${ENV_VAR}` expansion
- **retry.rs** (5): timeout/429/5xx retryable, 401 not retryable, exponential backoff with 30s cap
- **types.rs** (3): core/enrichment serde roundtrips, optional `llm_tokens` skipped when `None`

---

## [0.3.3] - 2026-02-10

### Summary

Code quality cleanup before Phase 5 (LLM integration). Addresses all issues from post-Phase-4a code review.

### Fixed

- **Clippy warnings (0 remaining):** removed unnecessary `&*` deref in `siglip.rs`, replaced `unwrap()` after `is_none()` with `if let` in `processor.rs`, simplified redundant closure in `text_encoder.rs`
- **NaN safety** in tag scoring — `partial_cmp().unwrap()` replaced with `f32::total_cmp` in `scorer.rs`
- **`Config::load_from`** signature changed from `&PathBuf` to `&Path`
- **Double file-size validation** removed from `decode.rs` (already checked by `Validator`)

### Changed

- **Streaming model downloads** — `download_file` now streams response body to disk in chunks instead of buffering 350-441MB in RAM. Added `futures-util` dependency and `reqwest` `stream` feature.
- **Logging initialization** wired to config file — `init_from_config` now reads `[logging]` section from config TOML, with CLI `--verbose` and `--json-logs` as overrides
- **`l2_normalize` consolidated** into `math.rs` — removed duplicate implementations from `siglip.rs` and `text_encoder.rs`
- **`taxonomy_dir()`** now derives from `model_dir` parent instead of hardcoding `$HOME/.photon/taxonomy`
- **Label bank cache invalidation** — saves a `.meta` sidecar with vocabulary BLAKE3 hash; stale caches are automatically rebuilt when vocabulary changes

### Removed

- **Dead `Photon` struct** from `lib.rs` — `ImageProcessor` is the real public API
- **Dead `Vocabulary::prompts_for()`** — unused method removed
- Updated `lib.rs` doc example to use `ImageProcessor` directly

### Dependencies

- `futures-util` 0.3 (new)
- `reqwest` stream feature enabled

### Tests

32 tests passing (same count: removed `test_photon_new`, added `test_l2_normalize_in_place`)

---

## [0.3.2] - 2026-02-09

### Added

- **Zero-shot tagging** via SigLIP text encoder scoring against a 68K-term vocabulary
- **SigLIP text encoder** (`SigLipTextEncoder`) wrapping `text_model.onnx` with `tokenizers` crate for SentencePiece tokenization
- **Vocabulary system** loading ~67,893 WordNet nouns and ~259 supplemental visual terms (scenes, moods, styles, weather, colors, composition)
- **Label bank** — pre-computed N×768 text embedding matrix cached at `~/.photon/taxonomy/label_bank.bin` for instant reload (<1s)
- **Tag scorer** — flat brute-force dot product with SigLIP sigmoid scoring (`logit = 117.33 * cosine - 12.93`, then `sigmoid`), ~2ms for 68K terms
- **Vocabulary data files** at `data/vocabulary/`: `wordnet_nouns.txt` (6.4MB) and `supplemental.txt`
- **WordNet vocabulary generator** script at `scripts/generate_wordnet_vocab.py`
- `--no-tagging` flag to disable zero-shot tagging
- `--quality fast|high` flag to select 224 or 384 vision model variant
- `PipelineError::Model` variant for non-per-image errors (model load failures, lock poisoning)
- `TaggingConfig` with `enabled`, `min_confidence` (default 0.0), `max_tags` (default 15), nested `VocabularyConfig`
- `ImageProcessor::load_tagging()` — same opt-in pattern as `load_embedding()`
- `photon models download` now installs text encoder, tokenizer, and vocabulary files
- `photon models list` shows vision encoders, shared models, and vocabulary status
- Multi-resolution model support: 224 (default) and 384 (optional, `--quality high`) vision variants

### Changed

- `EmbeddingConfig` gained `image_size: u32` field (224 or 384)
- `preprocess()` now accepts `image_size` parameter (was hardcoded to 224)
- `EmbeddingEngine` stores and passes configurable `image_size`

### Pipeline Stages

```
Validate → Decode → EXIF → Hash → Thumbnail → Embed (SigLIP) → Tag (SigLIP) → JSON
```

### Dependencies

- `tokenizers` 0.20 — SentencePiece tokenizer for SigLIP text encoder

### Tests

32 tests passing (+3 new: `test_cosine_to_confidence_range`, `test_sigmoid_monotonic`, `test_preprocess_shape_384`)

---

## [0.3.1] - 2026-02-09

### Summary

Research spike verifying that the Xenova ONNX text encoder produces embeddings aligned with the vision encoder — a prerequisite for zero-shot tagging. No production code shipped; findings drove the architecture for 0.3.2.

### Findings

- Separate `vision_model.onnx` and `text_model.onnx` produce embeddings identical to the combined `model.onnx` (cosine = 1.000000)
- SigLIP cosine similarities are unusually small (~-0.05 to -0.10); compensated by learned scaling parameters: `logit_scale = 117.33`, `logit_bias = -12.93` (derived with max error 0.000004)
- Text model takes `input_ids` only (no `attention_mask` unlike CLIP)
- fp16 text model crashes on aarch64/Asahi Linux — must use fp32 variant
- `tokenizers` crate v0.20 correctly loads SigLIP's SentencePiece tokenizer

### Critical Fix Identified

- **`SigLipSession::embed()` was using the wrong output** — extracting `last_hidden_state` (1st output) and mean-pooling it, which breaks cross-modal alignment
- **Correct output:** `pooler_output` (2nd output, shape [1, 768]) — the model's intended cross-modal embedding projection
- Fix applied in 0.3.2

### Changed

- Documented model I/O specification for both vision and text ONNX models
- Test images (`dog.jpg`, `beach.jpg`, `car.jpg`) added to `tests/fixtures/images/` for ongoing use
- All spike artifacts cleaned up after completion

---

## [0.3.0] - 2026-02-09

### Added

- **SigLIP embedding generation** producing L2-normalized 768-dimensional vectors from images via ONNX Runtime
- **`EmbeddingEngine`** wrapping a `SigLipSession` with `Mutex<Session>` for thread-safe inference
- **`SigLipSession`** — loads ONNX model, runs inference, extracts embedding from output tensor, L2-normalizes
- **Image preprocessing** — resize to 224x224 (Lanczos3), normalize pixels to [-1, 1] via `(pixel/255 - 0.5) / 0.5`, NCHW tensor layout
- **Model download** — `photon models download` fetches `vision_model.onnx` (~350MB) from `Xenova/siglip-base-patch16-224` on HuggingFace
- **Optional embedding in processor** — `ImageProcessor::load_embedding()` opt-in; processing works without a model
- **Timeout + spawn_blocking** — ONNX inference runs off the async runtime with configurable timeout (`limits.embed_timeout_ms`, default 30s)
- `--no-embedding` flag to disable embedding generation
- `photon models list` shows model readiness status
- Output shape handling for varied ONNX exports: [768], [1, 768], and [1, 197, 768] (mean-pooled)

### Design Decisions

- `Mutex<Session>` chosen over per-worker sessions — embedding is the bottleneck, lock contention minimal, keeps memory low (~350MB single instance)
- `(Vec<i64>, Vec<f32>)` tuples for ONNX input instead of ndarray feature — avoids coupling to ort's internal ndarray version
- Embedding loaded separately from `ImageProcessor::new()` — processor stays sync/infallible, phases build incrementally

### Pipeline Stages

```
Validate → Decode → EXIF → Hash → Thumbnail → Embed (SigLIP) → JSON
```

### Dependencies

- `ort` 2.0.0-rc.11 — ONNX Runtime for local model inference
- `ndarray` 0.16 — Tensor preprocessing
- `reqwest` 0.12 — Model download from HuggingFace

### Tests

29 tests passing (+5 new: `test_preprocess_shape`, `test_preprocess_normalization_range`, `test_l2_normalize`, `test_l2_normalize_zero_vector`, updated `test_process_options_default`)

---

## [0.2.0] - 2026-02-09

### Added

- **Image decoding** with support for JPEG, PNG, WebP, GIF, TIFF, BMP formats
- **EXIF metadata extraction** including camera make/model, GPS coordinates, datetime, ISO, aperture, shutter speed, focal length
- **Content hashing** using BLAKE3 for exact deduplication (64-char hex)
- **Perceptual hashing** using DoubleGradient algorithm for similarity detection
- **Thumbnail generation** as base64-encoded WebP with configurable size
- **File discovery** with recursive directory traversal and format filtering
- **Input validation** with magic byte checking and size limits
- **Pipeline orchestration** via `ImageProcessor` struct
- **Bounded channels** infrastructure for future parallel processing
- `--no-thumbnail` flag to disable thumbnail generation
- `--thumbnail-size` flag to configure thumbnail dimensions
- Batch processing with success/failure summary and rate reporting
- Verbose timing output for each processing stage

### Pipeline Stages

```
Input → Validate → Decode → EXIF → Hash → Thumbnail → JSON
```

### Dependencies

- `image` 0.25 — Multi-format image decoding
- `kamadak-exif` 0.5 — EXIF metadata extraction
- `blake3` 1 — Fast cryptographic hashing
- `image_hasher` 2 — Perceptual hashing
- `base64` 0.22 — Thumbnail encoding
- `walkdir` 2 — Directory traversal

---

## [0.1.0] - 2026-02-09

### Added

- **Cargo workspace** with `photon` (CLI) and `photon-core` (library) crates
- **CLI skeleton** using clap with subcommands:
  - `photon process <input>` — Process images (stub in 0.1.0)
  - `photon models [download|list|path]` — Manage AI models
  - `photon config [show|path|init]` — Configuration management
- **Configuration system** with TOML support and sensible defaults:
  - Processing settings (parallel workers, supported formats)
  - Pipeline settings (buffer size, retry attempts)
  - Limits (file size, dimensions, timeouts)
  - Embedding, thumbnail, tagging, output, logging settings
  - LLM provider configurations (Ollama, Hyperbolic, Anthropic, OpenAI)
- **Output formatting** with JSON and JSONL support, pretty-print option
- **Structured logging** via tracing with human-readable and JSON formats
- **Error types** with granular per-stage errors (decode, metadata, embedding, tagging, LLM, timeout, size limits)
- **Core data types**: `ProcessedImage`, `Tag`, `ExifData`, `ProcessingStats`
- `-v, --verbose` flag for debug-level logging
- `--json-logs` flag for machine-parseable log output
- Platform-appropriate config paths via `directories` crate

### Project Structure

```
crates/
├── photon/           # CLI binary
│   └── src/
│       ├── main.rs
│       ├── logging.rs
│       └── cli/{process,models,config}.rs
└── photon-core/      # Embeddable library
    └── src/
        ├── lib.rs
        ├── config.rs
        ├── error.rs
        ├── types.rs
        └── output.rs
```

### Dependencies

- `tokio` 1 — Async runtime
- `clap` 4 — CLI argument parsing
- `serde` 1 + `serde_json` 1 — Serialization
- `toml` 0.8 — Configuration parsing
- `thiserror` 2 — Library error types
- `anyhow` 1 — CLI error handling
- `tracing` 0.1 + `tracing-subscriber` 0.3 — Logging
- `directories` 5 — Platform config paths
- `shellexpand` 3 — Tilde expansion

---

[0.4.1]: https://github.com/hejijunhao/photon/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/hejijunhao/photon/compare/v0.3.3...v0.4.0
[0.3.3]: https://github.com/hejijunhao/photon/compare/v0.3.2...v0.3.3
[0.3.2]: https://github.com/hejijunhao/photon/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/hejijunhao/photon/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/hejijunhao/photon/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/hejijunhao/photon/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/hejijunhao/photon/releases/tag/v0.1.0
