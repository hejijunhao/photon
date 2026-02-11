# Changelog

All notable changes to Photon are documented here.

---

## Index

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

[0.4.1]: https://github.com/crimsonsun/photon/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/crimsonsun/photon/compare/v0.3.3...v0.4.0
[0.3.3]: https://github.com/crimsonsun/photon/compare/v0.3.2...v0.3.3
[0.3.2]: https://github.com/crimsonsun/photon/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/crimsonsun/photon/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/crimsonsun/photon/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/crimsonsun/photon/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/crimsonsun/photon/releases/tag/v0.1.0
