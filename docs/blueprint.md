# Photon — Architecture & Product Blueprint

> AI-powered image processing pipeline. Pure Rust. Single binary. No Python runtime, no Docker, no cloud dependency.

---

## What Is Photon?

Photon is an open-source image processing engine that takes images as input and outputs structured, AI-enriched data: **vector embeddings**, **semantic tags**, **metadata**, **perceptual hashes**, **thumbnails**, and optional **LLM-generated descriptions** — all as clean JSON.

It is not a database. It is not a search engine. It is not a web service. Photon is a **pure processing pipeline** — a single binary you point at images, and it emits structured data for your backend to store, index, and search however you choose.

```
                    ┌──────────────────────────────────────────────┐
   Image files      │                  PHOTON                      │        Structured JSON
   ─────────── ───▶ │  Decode → EXIF → Hash → Embed → Tag → [LLM] │ ───▶  ────────────────
   jpg, png,        │         Pure Processing Pipeline             │        embeddings, tags,
   webp, heic       └──────────────────────────────────────────────┘        metadata, hashes,
                                                                            thumbnails, descriptions
                                         │
                                         ▼
                               ┌───────────────────┐
                               │   YOUR BACKEND    │
                               │                   │
                               │  pgvector, Qdrant │
                               │  Pinecone, custom │
                               └───────────────────┘
```

### Why Photon Exists

Teams accumulate thousands of images across folders, drives, and cloud storage. Finding the right photo means scrolling, guessing filenames, or relying on manual tags. Photon solves this by processing every image through a local AI pipeline that understands what's in the photo — not just its filename.

A photo of red sneakers gets tagged with `sneakers`, `footwear`, `red`, `fashion`, `product photography`. Search for any of those terms and it surfaces. The 768-dimensional embedding vector enables semantic similarity search: find photos _like_ this one, even if no tags overlap.

---

## Core Design Principles

| Principle | What It Means |
|---|---|
| **Pure pipeline** | No database, no state management, no API server. Input images, output JSON. Your backend owns storage and search. |
| **Single binary** | One executable. No Python runtime, no Docker container, no system dependencies. Download and run. |
| **Local-first AI** | SigLIP runs entirely on-device via ONNX Runtime. No cloud calls for embeddings or tags. Your images never leave your machine. |
| **BYOK for LLMs** | Bring Your Own Key. Descriptions are optional and use whichever LLM provider you prefer — local (Ollama) or commercial (Anthropic, OpenAI, Hyperbolic). |
| **Embeddable** | The core library (`photon-core`) is a standalone Rust crate. Import it into your own Rust project and call it directly — no CLI needed. |

---

## Architecture

### Workspace Structure

Photon is a Rust workspace with two crates:

```
photon/
├── crates/
│   ├── photon-core/          # Embeddable library (~7,000 lines)
│   │   └── src/
│   │       ├── lib.rs            # Public API surface
│   │       ├── config/           # Layered configuration system
│   │       ├── pipeline/         # Processing stages (decode, hash, metadata, thumbnail)
│   │       ├── embedding/        # SigLIP visual encoder (ONNX Runtime)
│   │       ├── tagging/          # Zero-shot classification (~2,800 lines, 10 files)
│   │       ├── llm/              # BYOK provider abstraction (4 providers)
│   │       ├── output.rs         # JSON/JSONL streaming writer
│   │       ├── types.rs          # ProcessedImage, Tag, OutputRecord
│   │       └── error.rs          # Per-stage error types with recovery hints
│   │
│   └── photon/               # CLI binary (~1,400 lines)
│       └── src/
│           ├── main.rs
│           └── cli/
│               ├── process/      # Batch processing, dual-stream output
│               ├── interactive/  # Guided wizard (bare `photon` invocation)
│               ├── models.rs     # Model download with BLAKE3 verification
│               └── config.rs     # Config init/show/path
│
├── data/vocabulary/          # WordNet nouns + supplemental terms (source)
├── tests/fixtures/images/    # Test images (dog.jpg, beach.jpg, car.jpg, test.png)
└── pyproject.toml            # maturin config for PyPI binary distribution
```

### Public API

The library exposes a focused surface through `photon-core`:

```rust
pub use config::Config;
pub use embedding::EmbeddingEngine;
pub use error::{PhotonError, PipelineError, ConfigError};
pub use output::{OutputFormat, OutputWriter};
pub use pipeline::{ImageProcessor, ProcessOptions};
pub use types::{ProcessedImage, ExifData, Tag, EnrichmentPatch, OutputRecord};
```

`ImageProcessor` is the main entry point. It holds optional components via `Arc<Option<...>>` — the processor works without embedding or tagging models loaded, producing metadata-only output. Components are loaded separately via `load_embedding()` and `load_tagging()`, keeping construction sync and infallible.

---

## The Processing Pipeline

Every image passes through a sequential pipeline of independent stages. Each stage is a separate module, independently testable, and individually skippable via `ProcessOptions`.

```
┌──────────┐   ┌──────────┐   ┌──────────┐   ┌──────────┐   ┌──────────┐   ┌──────────┐   ┌──────────┐
│ Validate │──▶│  Decode  │──▶│   EXIF   │──▶│   Hash   │──▶│Thumbnail │──▶│  Embed   │──▶│   Tag    │
│          │   │          │   │          │   │          │   │          │   │ (SigLIP) │   │ (SigLIP) │
└──────────┘   └──────────┘   └──────────┘   └──────────┘   └──────────┘   └──────────┘   └──────────┘
     │              │              │              │              │              │              │
     ▼              ▼              ▼              ▼              ▼              ▼              ▼
  Magic byte    DynamicImage   ExifData      BLAKE3 hash    WebP base64   768-dim vec    Scored tags
  validation    + dimensions   (camera,      + perceptual   thumbnail     (L2-normed)   with hierarchy
  + size check                 GPS, ISO)     hash (pHash)                               paths
```

### Stage Details

| Stage | What It Does | Output |
|---|---|---|
| **Validate** | Checks file existence, size limits, and magic bytes (JPEG `FF D8 FF`, PNG `89 50 4E 47`, WebP `RIFF...WEBP`, GIF, TIFF, HEIC/AVIF). Rejects bad files before wasting decode time. | Pass/fail |
| **Decode** | Decodes the image from in-memory bytes (single read — bytes are reused for hashing). Enforces dimension limits and decode timeout. | `DynamicImage`, format, dimensions |
| **EXIF** | Extracts metadata leniently — returns partial data if some fields are missing. Captures timestamp, camera make/model, GPS coordinates, ISO, aperture, shutter speed, focal length, orientation. | `ExifData` (optional) |
| **Hash** | BLAKE3 content hash for exact deduplication. DoubleGradient perceptual hash (16x16, base64-encoded) for near-duplicate detection via Hamming distance. | `content_hash`, `perceptual_hash` |
| **Thumbnail** | Generates a WebP thumbnail at configurable size (default 256px). Base64-encoded for embedding directly in JSON output. | Base64 string |
| **Embed** | Runs the image through SigLIP's visual encoder via ONNX Runtime. Preprocesses to model resolution (224x224 or 384x384), normalizes to [-1, 1], outputs a 768-dimensional L2-normalized embedding vector. | `Vec<f32>` (768 dims) |
| **Tag** | Scores the image embedding against a vocabulary of ~68,000 terms. Applies SigLIP sigmoid scaling, hierarchy deduplication, and optional path annotation. | `Vec<Tag>` |

### Single-Read I/O

Photon reads each file exactly once. The raw bytes feed both the BLAKE3 content hash and the image decoder — no second disk read. This matters at scale: processing thousands of images, eliminating redundant I/O adds up.

### Async Timeout Pattern

CPU-heavy stages (decode, embedding) run inside `tokio::task::spawn_blocking` wrapped in `tokio::time::timeout`. This prevents a single slow or corrupt image from blocking the async runtime while still enforcing configurable time limits:

```rust
tokio::time::timeout(duration, tokio::task::spawn_blocking(|| {
    // CPU-heavy ONNX inference or image decode
}))
```

---

## SigLIP Embedding Engine

Photon uses Google's **SigLIP** (Sigmoid Language-Image Pre-training) model for both image embeddings and zero-shot tagging. SigLIP is the successor to CLIP, with better zero-shot classification accuracy and cleaner cross-modal alignment.

### How It Works

1. **Preprocessing**: Resize to model resolution using Lanczos3 interpolation, convert to RGB, normalize pixels to [-1, 1] via `(pixel/255 - 0.5) / 0.5`, arrange in NCHW tensor layout.
2. **Inference**: Run through ONNX Runtime session. Extract `pooler_output` (the 2nd output tensor — critically, _not_ `last_hidden_state`, which is misaligned across vision and text modalities).
3. **Normalization**: L2-normalize the 768-dim output vector.

### Two Model Variants

| Variant | Resolution | Speed | Use Case |
|---|---|---|---|
| `siglip-base-patch16` (default) | 224×224 | Fast | General processing, batch jobs |
| `siglip-base-patch16-384` | 384×384 | ~3-4× slower | Higher detail, fine-grained classification |

Selected via `--quality fast` (default) or `--quality high`.

### Performance Optimization

The preprocessing step converts a `DynamicImage` (~49MB for a 4K photo) into a compact float tensor (~600KB for 224×224×3×f32). This preprocessing happens _outside_ `spawn_blocking`, so only the small tensor crosses the thread boundary — not the full image.

### Batch Embedding

`embed_batch()` stacks multiple preprocessed tensors into a single `[N, 3, H, W]` input, amortizing ONNX dispatch overhead across images. The ONNX session is behind a `Mutex` (required by `ort`'s API), so batch calls reduce lock contention.

---

## Zero-Shot Tagging System

The tagging subsystem is the most technically sophisticated part of Photon — approximately 2,800 lines across 10 files. It transforms raw embedding vectors into human-readable semantic tags using SigLIP's cross-modal capabilities, a 68,000-term vocabulary, and a self-organizing relevance system.

### The Core Idea

SigLIP was trained so that image embeddings and text embeddings of matching concepts land near each other in the same 768-dimensional space. Photon exploits this by pre-encoding a large vocabulary of text descriptions into embeddings, then scoring each image against the entire vocabulary via dot product.

```
Image embedding (768-dim)  ×  Vocabulary matrix (68K × 768)  =  68K confidence scores
```

The raw cosine similarity is converted to a meaningful confidence via SigLIP's learned sigmoid scaling:

```
logit = 117.33 × cosine_similarity + (−12.93)
confidence = sigmoid(logit) = 1 / (1 + e^(−logit))
```

The constants `117.33` and `−12.93` are derived from SigLIP's training — they calibrate the raw cosine into a probability that the image actually depicts the concept.

### Vocabulary: 68,000+ Visual Terms

The vocabulary combines two sources:

- **~68,000 WordNet nouns** — the full noun hierarchy from Princeton's WordNet lexical database. Each term carries its synset ID and full hypernym chain (ancestor lineage). Format: `labrador_retriever → retriever → sporting_dog → dog → canine → mammal → ...`
- **~260 supplemental terms** — scenes, moods, styles, weather, and temporal concepts not covered by WordNet nouns: `sunset`, `vintage`, `aerial view`, `rainy`, `minimalist`, etc. Categorized into scene, mood, style, weather, time, activity, color, and composition.

### Label Bank: Pre-Computed Text Embeddings

The vocabulary is pre-encoded into a flat N×768 matrix (`label_bank.bin`, ~209MB) stored at `~/.photon/taxonomy/`. Each row is the SigLIP text embedding for the prompt `"a photo of a {term}"`.

**Cache invalidation**: A sidecar file (`label_bank.meta`) stores a BLAKE3 hash of the vocabulary content. If the vocabulary changes — even by a single term — the hash changes and the label bank is automatically rebuilt.

Scoring an image against 68,000 terms reduces to a single matrix-vector multiply. On macOS, this dispatches to Apple's Accelerate framework via BLAS, executing as an optimized `sgemv` call.

### Progressive Encoding: Cold Start in 30 Seconds

Encoding 68,000 terms through SigLIP's text encoder takes approximately 90 minutes on first run. Photon solves this with a two-stage progressive encoding strategy:

**Stage 1 — Seed encoding (~30 seconds, synchronous):**
A `SeedSelector` picks ~2,000 high-value terms: all supplemental terms first (scenes, moods, styles are inherently high visual relevance), then curated seed terms from a hand-picked list, then deterministic random fill from remaining WordNet nouns. These are encoded synchronously, and image processing begins immediately with this partial vocabulary.

**Stage 2 — Background encoding (asynchronous):**
A tokio background task encodes the remaining ~66,000 terms in chunks of 5,000. After each chunk completes:
1. The new embeddings are appended to the running label bank (incremental — O(N) total work).
2. A new `TagScorer` is constructed with the expanded vocabulary.
3. The scorer is atomically swapped into the shared `Arc<RwLock<TagScorer>>`.

Images being processed always use the best available scorer. Early images get tags from ~2,000 terms; later images benefit from the full 68,000. The label bank is cached to disk on completion — subsequent runs load instantly.

### Self-Organizing Vocabulary: Three-Pool Relevance Tracking

Not all 68,000 terms are equally useful for a given image collection. A dataset of product photos will never trigger `glacier` or `tundra`. Photon's `RelevanceTracker` adapts the vocabulary to the data by organizing terms into three pools:

```
┌─────────────────────────────────────────────────────────────────┐
│                    VOCABULARY POOLS                              │
│                                                                 │
│  ┌─────────┐        ┌─────────┐        ┌─────────┐             │
│  │ ACTIVE  │ ─────▶ │  WARM   │ ─────▶ │  COLD   │             │
│  │ (~2K)   │ demote │ (sample)│ demote │ (rest)  │             │
│  │ scored  │        │ scored  │        │ not     │             │
│  │ every   │ ◀───── │ every   │ ◀───── │ scored  │             │
│  │ image   │promote │ Nth img │promote │         │             │
│  └─────────┘        └─────────┘        └─────────┘             │
│                                              ▲                  │
│                                              │                  │
│                                    neighbor expansion           │
│                                    (WordNet siblings)           │
└─────────────────────────────────────────────────────────────────┘
```

| Pool | Behavior | Purpose |
|---|---|---|
| **Active** | Scored against every image | The hot path — terms that matter for this dataset |
| **Warm** | Scored every Nth image (sampling) | Candidates being evaluated for promotion |
| **Cold** | Not scored at all | Dormant terms, promoted only via neighbor expansion |

**Pool transitions** happen during periodic sweeps (every ~1,000 images):

- **Active → Warm**: Term hasn't matched any image in 90 days, or never matched after 1,000 images processed.
- **Warm → Active**: Term's average confidence exceeds the promotion threshold (default 0.3).
- **Warm → Cold**: 50 consecutive sampling checks with no match.
- **Cold → Warm**: Triggered by **neighbor expansion** — when a term is promoted to Active, its WordNet siblings (terms sharing the same parent hypernym) are promoted to Warm for evaluation.

This means the vocabulary self-organizes: after processing a batch of dog photos, `labrador retriever` gets promoted to Active, which triggers `golden retriever`, `poodle`, and other retriever breeds to enter the Warm pool. Meanwhile, `submarine` and `telescope` quietly demote to Cold.

Each term tracks per-term statistics: hit count, running score sum, last hit timestamp, and warm checks without hit. Precomputed index lists ensure scoring iterates only relevant terms — not all 68,000.

### Hierarchy Deduplication

When both `dog` and `animal` score above threshold, reporting both is redundant. Photon's `HierarchyDedup` suppresses ancestor tags when more specific descendants are present, using WordNet's hypernym chains.

If the tags `labrador retriever` (0.92), `dog` (0.85), and `animal` (0.71) all score, only `labrador retriever` survives — it's the most specific term in the chain. Overly generic ancestors like `entity`, `physical_entity`, `object`, and `organism` are always filtered.

**Optional path annotation** shows the surviving tag's lineage:

```json
{ "name": "labrador retriever", "confidence": 0.92, "path": "canine > retriever > labrador retriever" }
```

---

## LLM Integration (BYOK)

Photon supports optional image descriptions via external LLM providers. This is strictly BYOK (Bring Your Own Key) — Photon never bundles or requires a specific LLM.

### Supported Providers

| Provider | Type | Endpoint | Auth |
|---|---|---|---|
| **Ollama** | Local | `localhost:11434` | None |
| **Anthropic** | Commercial | `api.anthropic.com` | API key |
| **OpenAI** | Commercial | `api.openai.com` | API key |
| **Hyperbolic** | Self-hosted cloud | Configurable | API key |

All providers implement a common `LlmProvider` trait with `generate()`, `is_available()`, and `timeout()` methods. A factory creates the right provider from config + CLI flags, with `${ENV_VAR}` expansion for API keys.

### Tag-Aware Prompts

LLM requests include the zero-shot tags in the prompt, giving the LLM context about what Photon already detected. This produces more focused, accurate descriptions rather than generic "I see an image" responses.

### Dual-Stream Output

When LLM enrichment is active, Photon uses a dual-stream output model to avoid blocking the fast pipeline on slow LLM calls:

```
Stream 1 (immediate):   Core records — embeddings, tags, metadata, hashes
Stream 2 (async):       Enrichment patches — LLM descriptions, keyed by content_hash
```

**Core records** (`OutputRecord::Core`) emit at full pipeline speed. **Enrichment patches** (`OutputRecord::Enrichment`) follow asynchronously as LLM calls complete, cross-referenced by `content_hash` for client-side joining. In JSONL mode, this enables true real-time streaming — your backend starts receiving and indexing results before the LLM has finished.

### Resilient Execution

- **Concurrent calls**: Bounded by a semaphore (default 4 parallel, max 8) to avoid overwhelming providers.
- **Structured retry**: Classifies errors by HTTP status code (429/5xx retryable, 401/403 not), falls back to message substring matching for connection errors. Exponential backoff capped at 30 seconds.
- **Per-request timeout**: Configurable (default 60s). A slow LLM response doesn't stall the pipeline.
- **File size guard**: Skips enrichment for images exceeding the configured max file size.

---

## Batch Processing

Photon is designed for batch processing of large image collections, not just single files.

### Concurrent Pipeline

Images are processed concurrently via `futures::stream::buffer_unordered(parallel)`. While one image waits for the ONNX mutex (embedding inference), others decode, hash, and generate thumbnails on the blocking thread pool. The `--parallel` flag controls concurrency (default 4).

### Skip-Existing Pre-Filter

The `--skip-existing` flag enables incremental processing. Before entering the concurrent pipeline, Photon pre-filters the file list by matching `(path, file_size)` tuples against already-processed entries in the output file — zero I/O, no hashing required. Only new or changed files enter the pipeline.

### Streaming Output

| Output Mode | Behavior |
|---|---|
| JSONL to file | Core records stream immediately per-image; enrichment patches append after LLM completes |
| JSONL to stdout | Core records stream in real-time for piping to other tools |
| JSON to file | Collects all results, merges with existing if `--skip-existing`, writes array |
| JSON to stdout | Collects all results, emits as pretty-printed array |

### Progress & Summary

Batch processing displays a real-time progress bar (via `indicatif`) showing elapsed time, completion percentage, and current throughput in images/second. After completion, a summary table reports succeeded, failed, skipped counts, total duration, and throughput in both images/sec and MB/sec.

---

## CLI Interface

### Commands

```bash
# Process a single image (JSON to stdout)
photon process photo.jpg

# Process a directory (JSONL to file, 8 parallel workers)
photon process ./photos/ -o results.jsonl -f jsonl -p 8

# High-quality mode (384px SigLIP model)
photon process photo.jpg --quality high

# With LLM descriptions
photon process photo.jpg --llm anthropic --llm-model claude-sonnet-4-20250514

# Incremental processing (skip already-processed images)
photon process ./photos/ -o results.jsonl --skip-existing

# Selective output
photon process photo.jpg --no-thumbnail --no-embedding

# Show hierarchy paths in tags
photon process photo.jpg --show-tag-paths

# Stream for piping to your backend
photon process ./photos/ -f jsonl | your-ingestion-script

# Model management
photon models download     # Download SigLIP models + vocabulary
photon models list         # Show installed models with sizes
photon models path         # Print model directory

# Configuration
photon config show         # Display current config (TOML)
photon config init         # Create default config file
photon config path         # Print config file location
```

### Interactive Wizard

Invoking `photon` with no arguments on a TTY launches a guided interactive wizard:

1. **Main menu**: Process images, download models, configure settings, or exit.
2. **Guided processing**: Step-by-step walkthrough — select input path, verify models are installed, choose quality preset, configure LLM provider (with masked API key entry and option to save to config), select output format and destination, review summary, and process.
3. **Model management**: Visual status indicators (green checkmarks / red crosses) for each model component, with targeted download options.
4. **Settings viewer**: Summary of all configuration with option to view full TOML.

The wizard uses a custom color theme with cyan prompts, green success indicators, and box-drawing characters for the banner.

---

## Output Format

Every processed image produces a `ProcessedImage` struct serialized to JSON:

```json
{
  "file_path": "/photos/vacation/beach.jpg",
  "file_name": "beach.jpg",
  "content_hash": "a7f3b2c1d4e5f6...",
  "width": 4032,
  "height": 3024,
  "format": "jpeg",
  "file_size": 2458624,

  "embedding": [0.023, -0.156, 0.089, "... 768 floats"],

  "exif": {
    "captured_at": "2024-07-15T14:32:00Z",
    "camera_make": "Apple",
    "camera_model": "iPhone 15 Pro",
    "gps_latitude": 25.7617,
    "gps_longitude": -80.1918,
    "iso": 50,
    "aperture": "f/1.8",
    "focal_length": "6.765mm",
    "shutter_speed": "1/1000"
  },

  "tags": [
    { "name": "beach", "confidence": 0.94, "category": "scene" },
    { "name": "ocean", "confidence": 0.87 },
    { "name": "tropical", "confidence": 0.76, "category": "style" },
    { "name": "palm tree", "confidence": 0.71 }
  ],

  "description": "A sandy tropical beach with turquoise water and palm trees. The scene captures a serene vacation atmosphere with clear skies.",

  "thumbnail": "base64-encoded-webp-bytes...",
  "perceptual_hash": "d4c3b2a1e5f6..."
}
```

When LLM enrichment is active with JSONL output, enrichment patches follow as separate records:

```json
{"type": "enrichment", "content_hash": "a7f3b2c1d4e5f6...", "description": "A sandy tropical beach...", "llm_model": "claude-sonnet-4-20250514", "llm_latency_ms": 2340}
```

---

## Configuration

Photon uses a layered configuration system: **code defaults → TOML config file → CLI flags**. The config file lives at the platform-standard location (e.g., `~/Library/Application Support/com.photon.photon/config.toml` on macOS) with fallback to `~/.photon/config.toml`.

```toml
[general]
model_dir = "~/.photon/models"

[processing]
parallel_workers = 4
supported_formats = ["jpg", "jpeg", "png", "webp", "heic", "raw", "cr2", "nef", "arw"]

[limits]
max_file_size_mb = 100
max_image_dimension = 10000
decode_timeout_ms = 5000
embed_timeout_ms = 30000
llm_timeout_ms = 60000

[embedding]
model = "siglip-base-patch16"

[thumbnail]
enabled = true
size = 256
format = "webp"

[tagging]
enabled = true
min_confidence = 0.0
max_tags = 15
deduplicate_ancestors = false

[output]
format = "json"
pretty = false
include_embedding = true

[llm.anthropic]
api_key = "${ANTHROPIC_API_KEY}"
model = "claude-sonnet-4-20250514"

[llm.ollama]
endpoint = "http://localhost:11434"
model = "llama3.2-vision"
```

All numeric configuration fields are validated with range checks. `~` paths are expanded via `shellexpand`. Environment variable references (`${VAR}`) in API keys are expanded at runtime.

---

## Error Handling

Errors are structured per-pipeline-stage with specific variants: `Decode`, `Metadata`, `Embedding`, `Tagging`, `Timeout`, `FileTooLarge`, `ImageTooLarge`, `UnsupportedFormat`, `FileNotFound`, `Llm`, and `Model`. Each error carries the file path and a descriptive message.

A `.hint()` method on recoverable errors provides actionable recovery suggestions:

```
Error: Model not found at ~/.photon/models/siglip-base-patch16/visual.onnx
Hint:  Run `photon models download` to install the required models.
```

In batch processing, each image either **fully succeeds** or **fully fails** — no partial state. Failed images are logged and processing continues with the next image.

---

## Data Directory

```
~/.photon/
├── models/
│   ├── siglip-base-patch16/          # Default 224px visual encoder
│   │   └── visual.onnx              # ~348 MB
│   ├── siglip-base-patch16-384/      # Optional 384px visual encoder
│   │   └── visual.onnx
│   ├── text_model.onnx              # Shared text encoder (~443 MB)
│   └── tokenizer.json               # Shared tokenizer (~1 MB)
├── vocabulary/
│   ├── wordnet_nouns.txt            # ~68,000 WordNet nouns
│   └── supplemental.txt            # ~260 supplemental visual terms
└── taxonomy/
    ├── label_bank.bin               # Pre-computed text embeddings (N×768, ~209 MB)
    ├── label_bank.meta              # Vocabulary hash for cache invalidation
    └── relevance.json               # Relevance tracker state (per-term pool stats)
```

Model downloads are verified with BLAKE3 checksums. Corrupt or incomplete downloads are automatically removed and re-downloaded.

---

## Installation & Distribution

### From PyPI (recommended)

```bash
pip install photon-imager
```

Pre-built wheels for macOS (Apple Silicon) and Linux (x86_64, aarch64). Python 3.8+.

### From Source

```bash
git clone https://github.com/hejijunhao/photon
cd photon
cargo build --release
# Binary at target/release/photon
```

### From GitHub Releases

Pre-built binaries for macOS (aarch64), Linux (x86_64), and Linux (aarch64) attached to each release.

### First Run

```bash
photon models download    # Downloads ~800 MB of models + vocabulary
photon process photo.jpg  # Process your first image
```

---

## Technology Stack

| Component | Choice | Rationale |
|---|---|---|
| Language | **Rust** | Memory safety, no GC pauses, single static binary, native ONNX bindings |
| ML Runtime | **ONNX Runtime** (via `ort` crate) | Run SigLIP natively without Python, Metal acceleration on Apple Silicon |
| Embedding Model | **SigLIP** (Google) | Successor to CLIP, better zero-shot accuracy, aligned vision-text embeddings |
| Content Hash | **BLAKE3** | Cryptographic strength at near-memcpy speed |
| Perceptual Hash | **DoubleGradient** (16×16) | Robust near-duplicate detection via Hamming distance |
| Image Decode | **image** crate | JPEG, PNG, WebP, GIF, TIFF, HEIC support |
| EXIF | **kamadak-exif** | Lenient extraction with partial data recovery |
| Async Runtime | **tokio** | Full-featured async runtime with spawn_blocking for CPU-bound work |
| Matrix Ops | **ndarray** + **BLAS** | Accelerate framework on macOS for optimized matrix-vector multiply |
| CLI Framework | **clap** (derive) | Type-safe argument parsing |
| Progress UI | **indicatif** + **dialoguer** | Progress bars and interactive prompts |
| Config Format | **TOML** | Human-readable, comment-preserving edits via `toml_edit` |
| HTTP Client | **reqwest** (rustls-tls) | Pure-Rust TLS, no system OpenSSL dependency |
| Python Dist | **maturin** | Binary wheel builds for PyPI |

---

## Platform Support

| Platform | Status | Notes |
|---|---|---|
| macOS (Apple Silicon) | Primary | Metal acceleration via ONNX Runtime, Accelerate BLAS for scoring |
| Linux x86_64 | Supported | Pre-built binaries and PyPI wheels |
| Linux aarch64 | Supported | Native ARM builds on GitHub's arm64 runners |
| macOS (Intel) | Not supported | No pre-built ONNX Runtime binaries for cross-compilation |

### Important Constraints

- **fp32 models only** — fp16 ONNX models crash on aarch64. All shipped models are fp32.
- **SigLIP `pooler_output`** — The text encoder must use the 2nd output tensor (`pooler_output`), not `last_hidden_state`. They are not aligned across vision and text modalities.

---

## Testing & CI

### Test Suite

226 tests across the workspace:
- **40 CLI tests** — argument parsing, interactive flows, config management
- **166 core library tests** — per-stage unit tests, error handling, serialization
- **20 integration tests** — end-to-end pipeline against real image fixtures

### CI Pipeline (GitHub Actions)

| Workflow | Trigger | What It Does |
|---|---|---|
| **ci.yml** | Push/PR to `master` | `cargo check` + `cargo test` on macOS-14 and Ubuntu; `cargo fmt --check` + `cargo clippy -D warnings` on Ubuntu |
| **pypi.yml** | Git tag `v*` | Build wheels for 3 platforms, publish to PyPI via trusted publisher |
| **release.yml** | Git tag `v*` | Build release binaries for 3 platforms, upload to GitHub Releases |

### Benchmarks

Criterion benchmarks covering the pipeline hot path:

- `content_hash_blake3` — Hashing throughput
- `perceptual_hash` — DoubleGradient computation
- `decode_image` — Image decode from bytes
- `thumbnail_256px` — WebP generation
- `score_68k_matvec` — Full vocabulary scoring (68K×768 matrix-vector multiply)
- `preprocess_224` / `preprocess_384` — SigLIP input preparation
- `process_e2e_dog_jpg` — Full single-image pipeline
- `batch_4_images` — Concurrent batch throughput

Run with `cargo bench -p photon-core`.

---

## What Photon Does Not Do

Photon is intentionally scoped as a pure processing pipeline:

- **No database** — Your backend handles storage (pgvector, Pinecone, Qdrant, SQLite, whatever you want).
- **No search** — Use your database's vector similarity search on the embeddings Photon produces.
- **No API server** — Photon is a CLI and library, not a web service. Build your API on top.
- **No file watching** — Your backend triggers processing when new images arrive.
- **No web UI** — Build your own frontend. Photon outputs the data.

This separation of concerns means Photon fits into any architecture. Pipe its JSONL output into a Python script, a Go service, a Node.js ingestor, or a shell pipeline. It doesn't care what's downstream.
