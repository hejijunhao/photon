# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Test Commands

```bash
cargo build                         # Dev build
cargo build --release               # Release build
cargo check                         # Type-check without building
cargo test                          # Run all tests (~138 tests across workspace)
cargo test -p photon-core           # Test core library only
cargo test -p photon                # Test CLI only
cargo test <test_name>              # Run a single test by name
cargo test -- --nocapture           # Show println/tracing output
cargo bench -p photon-core          # Run criterion benchmarks
cargo run -- process image.jpg      # Run CLI
cargo run -- models download        # Download SigLIP model from HuggingFace
```

No custom lint or format configuration — standard `cargo fmt` and `cargo clippy`. CI enforces:

```bash
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
```

## Architecture

**Rust workspace** with two crates:
- `crates/photon-core` — Embeddable library (~7K lines). All processing logic lives here.
- `crates/photon` — CLI binary (~1.4K lines) using `clap`. Thin wrapper that calls into photon-core.

### Public API Surface (lib.rs re-exports)

```rust
pub use config::Config;
pub use embedding::EmbeddingEngine;
pub use error::{ConfigError, PhotonError, PipelineError, PipelineResult, Result};
pub use output::{OutputFormat, OutputWriter};
pub use pipeline::{ImageProcessor, ProcessOptions};
pub use types::{EnrichmentPatch, ExifData, OutputRecord, ProcessedImage, ProcessingStats, Tag};
```

### Processing Pipeline

`ImageProcessor` (`pipeline/processor.rs`) orchestrates a sequential pipeline:

```
Validate → Decode → EXIF → Hash (BLAKE3) → Perceptual Hash → Thumbnail (WebP) → Embed (SigLIP) → Tag (SigLIP) → ProcessedImage
                                                                                                                       ↓
                                                                                                              Enricher (LLM) → EnrichmentPatch
```

Each stage is a separate module under `photon-core/src/pipeline/`. The processor produces a `ProcessedImage` struct (defined in `types.rs`) serialized as JSON/JSONL via `OutputWriter`.

When `--llm` is active, the CLI uses a **dual-stream output** model: core pipeline results (`OutputRecord::Core`) emit immediately at full speed, then LLM descriptions follow as `OutputRecord::Enrichment` patches. Without `--llm`, output is identical to pre-LLM behavior.

The CLI's `process.rs` is decomposed into a `ProcessContext` struct and helper functions: `setup_processor()`, `process_single()`, `process_batch()`, `run_enrichment_collect()`, `run_enrichment_stdout()`. The actual `execute()` function is ~16 lines of orchestration.

### Embedding System

- `EmbeddingEngine` wraps a `SigLipSession` which holds `Mutex<ort::Session>` (ONNX Runtime)
- Loaded optionally via `ImageProcessor::load_embedding()` — processor works without it
- Inference runs in `tokio::task::spawn_blocking` with configurable timeout
- Model: `Xenova/siglip-base-patch16-224`, stored at `~/.photon/models/siglip-base-patch16/visual.onnx`
- Input preprocessing: resize to 224x224 or 384x384 (Lanczos3), normalize to [-1, 1]
- Output: 768-dim L2-normalized `Vec<f32>` from `pooler_output`
- Two model variants: 224 (fast, default) and 384 (higher detail, `--quality high`)

### Tagging System

The tagging subsystem is the most complex part of photon-core (~2.8K lines across 10 files in `tagging/`):

- **`TagScorer`** (`scorer.rs`) — holds `Vocabulary` + `LabelBank` + `TaggingConfig`. Brute-force scoring: dot product of image embedding × vocabulary matrix → SigLIP sigmoid → confidence. SigLIP scaling: `logit = 117.33 * cosine + (-12.93)`, then `sigmoid(logit)` (learned constants).
- **`Vocabulary`** (`vocabulary.rs`) — ~68K WordNet nouns + ~260 supplemental terms. Source files in `data/vocabulary/`, installed to `~/.photon/vocabulary/`.
- **`LabelBank`** (`label_bank.rs`) — pre-computed N×768 text embedding matrix cached at `~/.photon/taxonomy/label_bank.bin`. Cache invalidated via vocabulary hash in `label_bank.meta` sidecar.
- **`SigLipTextEncoder`** (`text_encoder.rs`) — encodes vocabulary terms through SigLIP text model (`text_model.onnx`). Must use `pooler_output` (2nd output), not `last_hidden_state`.
- **`RelevanceTracker`** (`relevance.rs`, 670 lines) — self-organizing three-pool system (Active/Warm/Cold) that promotes frequently-hit terms and demotes idle ones, reducing scoring cost over time.
- **`HierarchyDedup`** (`hierarchy.rs`) — suppresses ancestor tags when more specific descendants are present (e.g., removes "animal" when "dog" scores higher), with WordNet path annotations.
- **`ProgressiveEncoder`** (`progressive.rs`) — reduces cold-start from ~90min to ~30s by encoding a seed vocabulary first, then background-encoding remaining terms in chunks.
- **`NeighborExpander`** (`neighbors.rs`) — expands vocabulary graph with WordNet neighbors of high-scoring terms.
- **`SeedSelector`** (`seed.rs`) — picks high-value seed terms for progressive encoding.

**Implicit dependency**: Tagging requires embedding — `load_tagging()` checks `has_embedding()` first.

### LLM Integration (BYOK)

- `LlmProvider` async trait (`#[async_trait]`) with four implementations: Anthropic, Ollama, OpenAI, Hyperbolic
- `LlmProviderFactory::create()` produces `Box<dyn LlmProvider>` from provider name + config, with `${ENV_VAR}` expansion for API keys
- `Enricher` (`llm/enricher.rs`) orchestrates concurrent LLM calls with `tokio::Semaphore` (capped at 8), retry + exponential backoff, per-request timeouts
- Retry logic (`llm/retry.rs`): classifies errors by `status_code: Option<u16>` first (429/5xx retryable, 401/403 not), falls back to message substring matching for non-HTTP errors (e.g. connection failures)
- Tag-aware prompts: `LlmRequest::describe_image()` includes zero-shot tags for more focused descriptions

### Key Patterns

- **Optional components via Arc+Option**: `ImageProcessor` holds `Option<Arc<EmbeddingEngine>>` and `Option<Arc<RwLock<TagScorer>>>`. Note `TagScorer` uses `RwLock` because the relevance tracker mutates pool assignments during scoring. `new()` is sync/infallible; components loaded separately via `load_embedding()`/`load_tagging()`.
- **Async timeout pattern**: `tokio::time::timeout(duration, spawn_blocking(|| blocking_op()))` — used for embedding and decode to avoid blocking the async runtime while still enforcing time limits.
- **ort ONNX input**: Uses `(Vec<i64>, Vec<f32>)` tuples instead of ndarray feature to avoid coupling to ort's internal ndarray version.
- **Error types**: `PhotonError` (top-level) wraps `PipelineError` (per-stage variants with context: Decode, Metadata, Embedding, Tagging, Llm, Timeout, FileTooLarge, ImageTooLarge, UnsupportedFormat, FileNotFound) and `ConfigError`.
- **Config hierarchy**: Code defaults → TOML config file → CLI flags. Config path is platform-specific via `directories` crate, with `shellexpand::tilde()` for `~` paths. Config validation with range checks on 9 fields (`config.rs`, ~667 lines).
- **ProcessOptions**: Controls which pipeline stages to skip (`skip_thumbnail`, `skip_perceptual_hash`, `skip_embedding`, `skip_tagging`). Defaults to all-enabled.

### Data Directory Layout

```
~/.photon/
  models/
    text_model.onnx               # Shared text encoder (at root, not in variant dir)
    tokenizer.json                # Shared tokenizer
    siglip-base-patch16/          # Default 224px model
      visual.onnx
    siglip-base-patch16-384/      # High-quality 384px model
      visual.onnx
  vocabulary/
    wordnet_nouns.txt
    supplemental.txt
  taxonomy/
    label_bank.bin                # Pre-computed text embedding matrix (N×768 flat f32)
    label_bank.meta               # Vocab hash for cache invalidation
```

Source vocabulary files live in `data/vocabulary/` in the repo (`wordnet_nouns.txt`, `supplemental.txt`, `seed_terms.txt`).

## Platform Notes

- Developed on aarch64 Apple Silicon (macOS / Asahi Linux)
- `ort` v2.0.0-rc.11 downloads ONNX Runtime pre-built binaries at build time
- fp16 ONNX models crash on aarch64 — always use fp32 variants
- SigLIP text encoder: must use `pooler_output` (2nd output), not `last_hidden_state` — they are not aligned across modalities

## CI

GitHub Actions on `master` branch (`.github/workflows/ci.yml`):
- **Check & Test**: `cargo check --workspace` + `cargo test --workspace` on macOS-14 and Ubuntu
- **Lint**: `cargo fmt --all -- --check` + `cargo clippy --workspace -- -D warnings` on Ubuntu

## Test Fixtures

Test images at `tests/fixtures/images/` — `test.png` (70B minimal), `dog.jpg` (252KB), `beach.jpg` (44KB), `car.jpg` (40KB).
