# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Test Commands

```bash
cargo build                         # Dev build
cargo build --release               # Release build
cargo check                         # Type-check without building
cargo test                          # Run all tests (32 tests across workspace)
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
Validate → Decode → EXIF Metadata → Content Hash (BLAKE3) → Perceptual Hash → Thumbnail (WebP) → Embed (SigLIP) → Tag (SigLIP) → [Description Phase 5] → ProcessedImage
```

Each stage is a separate module under `photon-core/src/pipeline/`. The processor produces a `ProcessedImage` struct (defined in `types.rs`) serialized as JSON/JSONL via `OutputWriter`.

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
- `SigLipTextEncoder` encodes vocabulary terms through SigLIP text model (`text_model.onnx`)
- `LabelBank` stores pre-computed N×768 text embedding matrix, cached at `~/.photon/taxonomy/label_bank.bin`
- Scoring: dot product of image embedding × vocabulary matrix → SigLIP sigmoid → confidence
- SigLIP scaling: `logit = 117.33 * cosine + (-12.93)`, then `sigmoid(logit)`
- Vocabulary: ~68K WordNet nouns + ~260 supplemental terms at `~/.photon/vocabulary/`

### Key Patterns

- **Optional components**: `ImageProcessor::new()` is sync/infallible. Embedding and tagging loaded separately so phases build incrementally.
- **ort ONNX input**: Uses `(Vec<i64>, Vec<f32>)` tuples instead of ndarray feature to avoid coupling to ort's internal ndarray version.
- **Error types**: `PhotonError` (top-level) wraps `PipelineError` (per-stage variants: Decode, Metadata, Embedding, Timeout, etc.) and `ConfigError`.
- **Config hierarchy**: Code defaults → TOML config file → CLI flags. Config path is platform-specific via `directories` crate.

## Platform Notes

- Developed on Asahi Linux (aarch64, Apple Silicon)
- `ort` v2.0.0-rc.11 downloads ONNX Runtime pre-built binaries at build time
- fp16 ONNX models crash on aarch64 — use fp32 variants
- SigLIP text encoder (Phase 4): must use `pooler_output` (2nd output), not `last_hidden_state`

## Project Status

Phases 1-4a complete (foundation, image pipeline, SigLIP embedding, zero-shot tagging). Phase 4b-4e are optimizations (progressive encoding, relevance pruning, hierarchy dedup). Phase 5 (LLM integration) is next. Detailed plans in `docs/plans/`, completed phase docs in `docs/completions/`.

## Test Fixtures

Test images at `tests/fixtures/images/` — `test.png`, `dog.jpg`, `beach.jpg`, `car.jpg`.
