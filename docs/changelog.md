# Changelog

All notable changes to Photon are documented here.

---

## Index

- **[0.3.3](#033---2026-02-10)** — Pre-Phase 5 cleanup: clippy fixes, streaming downloads, cache invalidation, dead code removal
- **[0.3.2](#032---2026-02-09)** — Zero-shot tagging: 68K-term vocabulary, SigLIP text encoder, label bank caching
- **[0.3.1](#031---2026-02-09)** — Text encoder alignment spike: cross-modal verification, scoring parameter derivation
- **[0.3.0](#030---2026-02-09)** — SigLIP embedding: ONNX Runtime integration, 768-dim vector generation
- **[0.2.0](#020---2026-02-09)** — Image processing pipeline: decode, EXIF, hashing, thumbnails
- **[0.1.0](#010---2026-02-09)** — Project foundation: CLI, configuration, logging, error handling

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

[0.3.2]: https://github.com/crimsonsun/photon/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/crimsonsun/photon/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/crimsonsun/photon/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/crimsonsun/photon/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/crimsonsun/photon/releases/tag/v0.1.0
