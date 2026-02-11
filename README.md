<p align="center">
  <img src="assets/logo.svg" width="140" alt="Photon">
</p>

<h1 align="center">Photon</h1>

<p align="center">
  <strong>AI-powered image processing pipeline written in Rust</strong><br>
  Analyze, embed, and tag images locally using SigLIP — no cloud required.
</p>

<p align="center">
  <a href="#quick-start">Quick Start</a> &nbsp;&bull;&nbsp;
  <a href="#usage">Usage</a> &nbsp;&bull;&nbsp;
  <a href="#how-it-works">How It Works</a> &nbsp;&bull;&nbsp;
  <a href="#configuration">Configuration</a> &nbsp;&bull;&nbsp;
  <a href="#library-usage">Library Usage</a>
</p>

<p align="center">
  <a href="LICENSE-APACHE"><img src="https://img.shields.io/badge/license-MIT%2FApache--2.0-blue" alt="License"></a>
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/rust-2021_edition-orange" alt="Rust"></a>
</p>

---

Photon takes images as input and outputs structured JSON: **768-dim vector embeddings**, **semantic tags**, **EXIF metadata**, **content hashes**, and **thumbnails**. It's a pure processing pipeline — no database, no server, no cloud dependency. Process locally, store wherever you want.

```
image.jpg ──▶ Photon ──▶ { embedding, tags, metadata, hash, thumbnail }
```

## Features

- **SigLIP Embeddings** — 768-dimensional vectors for semantic similarity search, powered by ONNX Runtime
- **Zero-Shot Tagging** — 68,000+ term vocabulary (WordNet + curated visual terms) scored locally via SigLIP
- **EXIF Extraction** — Camera, GPS coordinates, datetime, ISO, aperture, focal length
- **Content Hashing** — BLAKE3 cryptographic hash + perceptual hash for deduplication and similarity
- **Thumbnails** — WebP generation with configurable size and quality
- **LLM Descriptions** — BYOK enrichment via Ollama, Anthropic, OpenAI, Hyperbolic
- **Batch Processing** — Parallel workers with progress bar and skip-existing support
- **Single Binary** — No Python, no Docker, no runtime dependencies

## Quick Start

```bash
# Build from source
git clone https://github.com/hejijunhao/photon.git
cd photon
cargo build --release

# Download the SigLIP model (~350 MB, one-time)
cargo run --release -- models download

# Process a single image
cargo run --release -- process photo.jpg

# Process an entire directory
cargo run --release -- process ./photos/ --format jsonl --output results.jsonl
```

## Usage

### Process Images

```bash
# Single image → JSON to stdout
photon process image.jpg

# Directory → JSONL file (one JSON object per line)
photon process ./photos/ --format jsonl --output results.jsonl

# Parallel processing with 8 workers
photon process ./photos/ --parallel 8 --output results.jsonl

# Skip already-processed images on re-runs
photon process ./photos/ --output results.jsonl --skip-existing

# Higher quality embeddings (384px model, slower but more detailed)
photon process image.jpg --quality high
```

### LLM Descriptions (BYOK)

```bash
# Local via Ollama
photon process image.jpg --llm ollama --llm-model llama3.2-vision

# Anthropic API
photon process image.jpg --llm anthropic --llm-model claude-sonnet-4-5-20250929

# OpenAI API
photon process image.jpg --llm openai --llm-model gpt-4o-mini

# Batch with LLM enrichment
photon process ./photos/ --format jsonl --output results.jsonl --llm anthropic
```

### Control What Gets Generated

```bash
# Metadata and hashes only (no AI)
photon process image.jpg --no-embedding --no-tagging

# Skip thumbnail generation
photon process image.jpg --no-thumbnail

# Custom thumbnail size
photon process image.jpg --thumbnail-size 128
```

### Manage Models

```bash
photon models download    # Download SigLIP models from HuggingFace
photon models list        # Show installed models and status
photon models path        # Show model storage directory
```

### Configuration

```bash
photon config init        # Create config file with defaults
photon config show        # Display current settings
photon config path        # Show config file location
```

## How It Works

Photon runs a sequential pipeline where each stage is independent and optional:

```
 Input        ┌──────────┐ ┌──────┐ ┌──────┐ ┌───────────┐ ┌───────┐ ┌───────┐
 image.jpg ──▶│ Validate │▶│Decode│▶│ EXIF │▶│   Hash    │▶│Thumb- │▶│ Embed │──▶ ...
              │          │ │      │ │      │ │BLAKE3+pHash│ │ nail  │ │SigLIP │
              └──────────┘ └──────┘ └──────┘ └───────────┘ └───────┘ └───────┘

 ... ──▶ ┌──────────┐ ┌─────────────┐        Output
         │Zero-Shot │▶│  LLM Enrich │──▶  Structured JSON
         │  Tags    │ │  (BYOK)     │     { embedding, tags,
         │ (SigLIP) │ │             │       metadata, hash, ... }
         └──────────┘ └─────────────┘
```

| Stage | What it does | Speed |
|-------|-------------|-------|
| **Validate** | Check file exists, size limits, format detection via magic bytes | <1ms |
| **Decode** | Load image pixels (JPEG, PNG, WebP, GIF, TIFF, BMP, AVIF) | ~5ms |
| **EXIF** | Extract camera, GPS, datetime, shooting parameters | ~2ms |
| **Hash** | BLAKE3 content hash (dedup) + perceptual hash (similarity) | ~3ms |
| **Thumbnail** | Aspect-preserving resize to WebP, base64 encoded | ~5ms |
| **Embed** | SigLIP vision encoder → 768-dim L2-normalized vector | ~200ms |
| **Tag** | Dot product against 68K vocabulary, SigLIP sigmoid scoring | ~2ms |

## Output Format

Each processed image produces a JSON object:

```json
{
  "file_path": "/photos/beach.jpg",
  "file_name": "beach.jpg",
  "content_hash": "a7f3b2c1d4e5...",
  "width": 4032,
  "height": 3024,
  "format": "jpeg",
  "file_size": 2458624,
  "embedding": [0.023, -0.156, 0.089, "... 768 floats"],
  "tags": [
    { "name": "beach", "confidence": 0.94, "category": "scene" },
    { "name": "ocean", "confidence": 0.87, "category": "scene" },
    { "name": "tropical", "confidence": 0.76, "category": "style" }
  ],
  "exif": {
    "captured_at": "2024-07-15T14:32:00",
    "camera_model": "iPhone 15 Pro",
    "gps_latitude": 25.7617,
    "gps_longitude": -80.1918
  },
  "thumbnail": "base64-encoded-webp...",
  "perceptual_hash": "d4c3b2a1..."
}
```

Use `--format jsonl` for batch processing — one JSON object per line, streamed as each image completes.

## Configuration

Photon uses a layered configuration system: **code defaults < config file < CLI flags**.

```bash
photon config init    # Creates ~/.photon/config.toml (or platform-appropriate path)
```

Key settings in `config.toml`:

```toml
[processing]
parallel_workers = 4
supported_formats = ["jpg", "jpeg", "png", "webp", "heic", "raw", "cr2", "nef", "arw"]

[limits]
max_file_size_mb = 100
max_image_dimension = 10000
embed_timeout_ms = 30000

[embedding]
model = "siglip-base-patch16"         # or "siglip-base-patch16-384" for higher quality
device = "cpu"                        # "cpu", "metal" (Apple Silicon), "cuda" (NVIDIA)

[thumbnail]
enabled = true
size = 256
quality = 80

[tagging]
enabled = true
max_tags = 15

[logging]
level = "info"                        # error, warn, info, debug, trace
```

## Library Usage

Photon's processing engine lives in the `photon-core` crate and can be embedded directly in Rust applications:

```rust
use photon_core::{Config, ImageProcessor};
use std::path::Path;

#[tokio::main]
async fn main() -> photon_core::Result<()> {
    let config = Config::load()?;
    let mut processor = ImageProcessor::new(&config);

    // Load AI components (optional — pipeline works without them)
    processor.load_embedding(&config)?;
    processor.load_tagging(&config)?;

    let result = processor.process(Path::new("photo.jpg")).await?;

    println!("Hash:      {}", result.content_hash);
    println!("Embedding: {} dimensions", result.embedding.len());
    println!("Tags:      {:?}", result.tags.iter().map(|t| &t.name).collect::<Vec<_>>());

    Ok(())
}
```

Add to your `Cargo.toml`:

```toml
[dependencies]
photon-core = { git = "https://github.com/hejijunhao/photon.git" }
tokio = { version = "1", features = ["full"] }
```

## Integrating with Your Backend

Photon is designed to feed into your own storage and search infrastructure. Pipe the output to your ingestion scripts:

```bash
# Stream results into your backend
photon process ./photos/ --format jsonl | your-ingestion-script

# Or process to file, then ingest
photon process ./photos/ --format jsonl --output results.jsonl
python ingest.py results.jsonl
```

Example — storing embeddings in PostgreSQL with pgvector:

```python
import subprocess, json

result = subprocess.run(
    ["photon", "process", "photo.jpg"],
    capture_output=True, text=True
)
data = json.loads(result.stdout)

db.execute(
    "INSERT INTO images (path, hash, embedding, tags) VALUES (%s, %s, %s, %s)",
    [data["file_path"], data["content_hash"], data["embedding"], json.dumps(data["tags"])]
)
```

## Architecture

```
photon/
├── crates/
│   ├── photon/              # CLI binary (thin clap wrapper)
│   └── photon-core/         # Embeddable library
│       └── src/
│           ├── pipeline/    # Processing stages (decode, metadata, hash, thumbnail)
│           ├── embedding/   # SigLIP vision encoder (ONNX Runtime)
│           ├── tagging/     # Zero-shot classification (68K vocabulary)
│           └── output.rs    # JSON/JSONL serialization
├── data/vocabulary/         # WordNet nouns + supplemental visual terms
├── tests/fixtures/          # Test images
└── docs/                    # Phase plans and changelogs
```

**Two-crate design:** `photon-core` contains all processing logic and can be used as a library. `photon` is a thin CLI that calls into it. This means you can embed Photon's pipeline directly in your Rust application without pulling in CLI dependencies.

## Project Status

| Phase | Status |
|-------|--------|
| Foundation (CLI, config, logging) | Complete |
| Image pipeline (decode, EXIF, hashing, thumbnails) | Complete |
| SigLIP embedding (768-dim vectors via ONNX) | Complete |
| Zero-shot tagging (68K vocabulary, self-organizing pools) | Complete |
| LLM enrichment (BYOK descriptions) | Complete |
| Polish & release (progress bar, skip-existing, benchmarks) | Complete |

## Requirements

- **Rust** 2021 edition (stable)
- ~350 MB disk for SigLIP model (downloaded on first `models download`)
- Tested on macOS (Apple Silicon) and Linux (aarch64/x86_64)

## Contributing

Contributions are welcome. Please open an issue to discuss significant changes before submitting a PR.

```bash
cargo test              # Run all tests (120+ across workspace)
cargo clippy            # Lint
cargo fmt               # Format
cargo bench -p photon-core  # Run benchmarks
```

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE), at your option.
