# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Test Commands

```bash
cargo build                         # Dev build
cargo build --release               # Release build
cargo check                         # Type-check without building
cargo test                          # Run all tests (50 tests across workspace)
cargo test -p photon-core           # Test core library only
cargo test -p photon                # Test CLI only
cargo test <test_name>              # Run a single test by name
cargo test -- --nocapture           # Show println/tracing output
cargo run -- process image.jpg      # Run CLI
cargo run -- models download        # Download SigLIP model from HuggingFace
```

No custom lint or format configuration — standard `cargo fmt` and `cargo clippy`.

## Architecture

**Rust workspace** with two crates:
- `crates/photon-core` — Embeddable library. All processing logic lives here.
- `crates/photon` — CLI binary using `clap`. Thin wrapper that calls into photon-core.

### Processing Pipeline

`ImageProcessor` orchestrates a sequential pipeline:

```
Validate → Decode → EXIF → Hash (BLAKE3) → Perceptual Hash → Thumbnail (WebP) → Embed (SigLIP) → Tag (SigLIP) → ProcessedImage
                                                                                                                       ↓
                                                                                                              Enricher (LLM) → EnrichmentPatch
```

Each stage is a separate module under `photon-core/src/pipeline/`. The processor produces a `ProcessedImage` struct (defined in `types.rs`) serialized as JSON/JSONL via `OutputWriter`.

When `--llm` is active, the CLI uses a **dual-stream output** model: core pipeline results (`OutputRecord::Core`) emit immediately at full speed, then LLM descriptions follow as `OutputRecord::Enrichment` patches. Without `--llm`, output is identical to pre-LLM behavior.

### Embedding System

- `EmbeddingEngine` wraps a `SigLipSession` which holds `Mutex<ort::Session>` (ONNX Runtime)
- Loaded optionally via `ImageProcessor::load_embedding()` — processor works without it
- Inference runs in `tokio::task::spawn_blocking` with configurable timeout
- Model: `Xenova/siglip-base-patch16-224`, stored at `~/.photon/models/siglip-base-patch16/visual.onnx`
- Input preprocessing: resize to 224x224 or 384x384 (Lanczos3), normalize to [-1, 1]
- Output: 768-dim L2-normalized `Vec<f32>` from `pooler_output`
- Two model variants: 224 (fast, default) and 384 (higher detail, `--quality high`)

### Tagging System

- `TagScorer` holds `Vocabulary` + `LabelBank` + `TaggingConfig`
- Loaded optionally via `ImageProcessor::load_tagging()` — same opt-in pattern as embedding
- **Implicit dependency**: Tagging requires embedding — `load_tagging()` checks `has_embedding()` first
- `SigLipTextEncoder` encodes vocabulary terms through SigLIP text model (`text_model.onnx`)
- `LabelBank` stores pre-computed N×768 text embedding matrix, cached at `~/.photon/taxonomy/label_bank.bin`
- Cache invalidated via vocabulary hash stored in `label_bank.meta` sidecar file
- Scoring: dot product of image embedding × vocabulary matrix → SigLIP sigmoid → confidence
- SigLIP scaling: `logit = 117.33 * cosine + (-12.93)`, then `sigmoid(logit)` (learned constants, not standard sigmoid)
- Vocabulary: ~68K WordNet nouns + ~260 supplemental terms at `~/.photon/vocabulary/`

### LLM Integration (BYOK)

- `LlmProvider` async trait (`#[async_trait]`) with four implementations: Anthropic, Ollama, OpenAI, Hyperbolic
- `LlmProviderFactory::create()` produces `Box<dyn LlmProvider>` from provider name + config, with `${ENV_VAR}` expansion for API keys
- `Enricher` (`llm/enricher.rs`) orchestrates concurrent LLM calls with `tokio::Semaphore` (capped at 8), retry + exponential backoff, per-request timeouts
- Retry logic (`llm/retry.rs`): classifies errors by `status_code: Option<u16>` first (429/5xx retryable, 401/403 not), falls back to message substring matching for non-HTTP errors (e.g. connection failures)
- Tag-aware prompts: `LlmRequest::describe_image()` includes zero-shot tags for more focused descriptions

### Key Patterns

- **Optional components via Arc+Option**: `ImageProcessor` holds `Option<Arc<EmbeddingEngine>>` and `Option<Arc<TagScorer>>`. `new()` is sync/infallible; components loaded separately via `load_embedding()`/`load_tagging()`.
- **Async timeout pattern**: `tokio::time::timeout(duration, spawn_blocking(|| blocking_op()))` — used for embedding and decode to avoid blocking the async runtime while still enforcing time limits.
- **ort ONNX input**: Uses `(Vec<i64>, Vec<f32>)` tuples instead of ndarray feature to avoid coupling to ort's internal ndarray version.
- **Error types**: `PhotonError` (top-level) wraps `PipelineError` (per-stage variants with context: Decode, Metadata, Embedding, Llm, Timeout, etc.) and `ConfigError`.
- **Config hierarchy**: Code defaults → TOML config file → CLI flags. Config path is platform-specific via `directories` crate, with `shellexpand::tilde()` for `~` paths.

### Data Directory Layout

```
~/.photon/
  models/
    siglip-base-patch16/          # Default 224px model
      visual.onnx
      text_model.onnx
      tokenizer.json
    siglip-base-patch16-384/      # High-quality 384px model
      visual.onnx
  vocabulary/
    wordnet_nouns.txt
    supplemental.txt
  taxonomy/
    label_bank.bin                # Pre-computed text embedding matrix (N×768 flat f32)
    label_bank.meta               # Vocab hash for cache invalidation
```

## Platform Notes

- Developed on aarch64 Apple Silicon (macOS / Asahi Linux)
- `ort` v2.0.0-rc.11 downloads ONNX Runtime pre-built binaries at build time
- fp16 ONNX models crash on aarch64 — always use fp32 variants
- SigLIP text encoder: must use `pooler_output` (2nd output), not `last_hidden_state` — they are not aligned across modalities

## Project Status

Phases 1–5 complete (foundation, image pipeline, SigLIP embedding, zero-shot tagging, LLM integration with two rounds of bug fixes). Phase 4b–4e are planned optimizations (progressive encoding, relevance pruning, hierarchy dedup). Detailed plans in `docs/plans/`, completed phase docs in `docs/archive/`.

## Test Fixtures

Test images at `tests/fixtures/images/` — `test.png`, `dog.jpg`, `beach.jpg`, `car.jpg`.
