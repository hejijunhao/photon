# Photon: Architecture Blueprint

> Pure image processing pipeline for AI-powered tagging and embeddings

## Executive Summary

Photon is an open-source, embeddable image processing engine. It takes images as input and outputs structured data: vector embeddings, semantic tags, metadata, and descriptions. **Photon does not manage a database** — it's a pure processing pipeline that outputs data for your backend to store and search however you choose.

**Key Design Decisions:**
- **Rust** for performance and single-binary distribution
- **Pure pipeline** — no database, just input → output
- **SigLIP** for embeddings (bundled, runs locally via ONNX)
- **BYOK** for LLMs (Ollama, Hyperbolic, Anthropic, OpenAI)
- **Apple Silicon optimized** (Metal acceleration)

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                           PHOTON                                 │
│                  Pure Image Processing Pipeline                  │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   INPUT                           OUTPUT                        │
│   ─────                           ──────                        │
│   Image file(s)          →        Structured data (JSON):       │
│   - jpg, png, webp                - embedding vector [768]      │
│   - heic, raw formats             - semantic tags + confidence  │
│   - batch directories             - EXIF metadata               │
│                                   - description (via LLM)       │
│                                   - content hash                │
│                                   - thumbnail (optional)        │
│                                                                 │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   ┌───────────┐   ┌───────────┐   ┌───────────┐                │
│   │  Decode   │──▶│  Extract  │──▶│   Embed   │                │
│   │  Image    │   │  Metadata │   │  (SigLIP) │                │
│   └───────────┘   └───────────┘   └───────────┘                │
│         │               │               │                       │
│         ▼               ▼               ▼                       │
│   ┌───────────┐   ┌───────────┐   ┌───────────┐                │
│   │ Thumbnail │   │   EXIF    │   │  Vector   │                │
│   │  (WebP)   │   │   JSON    │   │  [768]    │                │
│   └───────────┘   └───────────┘   └───────────┘                │
│                                         │                       │
│                         ┌───────────────┴───────────────┐       │
│                         ▼                               ▼       │
│                   ┌───────────┐                 ┌───────────┐   │
│                   │Zero-Shot  │                 │    LLM    │   │
│                   │  Tags     │                 │Description│   │
│                   │ (SigLIP)  │                 │  (BYOK)   │   │
│                   └───────────┘                 └───────────┘   │
│                                                                 │
│         ════════════ Bounded Channels ════════════              │
│         (Backpressure between stages prevents OOM)              │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼ JSON output
                    ┌─────────────────────┐
                    │    YOUR BACKEND     │
                    │                     │
                    │  - Store in any DB  │
                    │  - Index vectors    │
                    │  - Build your search│
                    │  - Your API/UI      │
                    └─────────────────────┘
```

---

## Technology Stack

| Component | Choice | Why |
|-----------|--------|-----|
| Language | **Rust** | Memory safety, ONNX bindings, single static binary, no GC |
| Embedding Model | **SigLIP** | Newer than CLIP, better performance, bundled via ONNX |
| ML Runtime | **ONNX Runtime** | Run models natively without Python, Metal support |
| LLM Integration | **BYOK** | Ollama (local), Hyperbolic (hosted), Anthropic/OpenAI (commercial) |
| Target Hardware | **Apple Silicon** | M1-M4 with Metal acceleration |

### Core Dependencies

```toml
[dependencies]
tokio = { version = "1", features = ["full"] }  # Async runtime
clap = { version = "4", features = ["derive"] }  # CLI
image = "0.25"                    # Image decoding
kamadak-exif = "0.5"              # EXIF extraction
blake3 = "1"                      # Content hashing
image_hasher = "2"                # Perceptual hashing
ort = "2"                         # ONNX Runtime (SigLIP)
reqwest = { version = "0.12", features = ["json"] }  # HTTP for LLM APIs
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
tracing = "0.1"                   # Structured logging
tracing-subscriber = "0.3"
rayon = "1.10"                    # Parallel processing
```

**Note:** No database dependencies. Photon is a pure processing pipeline.

---

## Project Structure

```
photon/
├── Cargo.toml                    # Workspace manifest
├── crates/
│   ├── photon/                   # CLI binary
│   │   └── src/
│   │       ├── main.rs
│   │       └── cli/
│   │           ├── process.rs    # Main processing command
│   │           ├── config.rs     # Configuration management
│   │           └── models.rs     # Model download/management
│   │
│   └── photon-core/              # Embeddable library
│       └── src/
│           ├── lib.rs            # Public API
│           ├── config.rs         # Configuration types
│           ├── error.rs          # Error types (per-stage)
│           │
│           ├── pipeline/         # Image processing pipeline
│           │   ├── mod.rs        # Pipeline orchestration
│           │   ├── decode.rs     # Image decoding (all formats)
│           │   ├── metadata.rs   # EXIF extraction
│           │   ├── hash.rs       # Content + perceptual hashing
│           │   ├── thumbnail.rs  # Thumbnail generation
│           │   └── channel.rs    # Bounded channels (backpressure)
│           │
│           ├── embedding/        # SigLIP embedding generation
│           │   ├── mod.rs
│           │   ├── siglip.rs     # ONNX model inference
│           │   └── preprocess.rs # Image normalization
│           │
│           ├── tagging/          # Tag generation
│           │   ├── mod.rs
│           │   ├── zero_shot.rs  # SigLIP zero-shot classification
│           │   └── taxonomy.rs   # Tag vocabulary
│           │
│           ├── llm/              # LLM provider abstraction (BYOK)
│           │   ├── mod.rs        # Provider trait
│           │   ├── ollama.rs     # Local models
│           │   ├── hyperbolic.rs # Self-hosted cloud
│           │   ├── anthropic.rs  # Commercial API
│           │   └── openai.rs     # Commercial API
│           │
│           └── output.rs         # Output formatting (JSON, JSONL)
│
├── models/                       # ONNX models (downloaded on first run)
│   └── siglip-base-patch16/
│       ├── visual.onnx
│       └── textual.onnx
│
├── docs/
│   ├── vision.md
│   └── blueprint.md              # This file
│
└── tests/
    ├── integration/
    └── fixtures/
        └── images/
```

---

## Pipeline Reliability

### Parallel Processing

Images are processed in parallel using a configurable worker pool:

```bash
photon process ./photos/ --parallel 8  # 8 workers
```

Each worker processes images end-to-end (decode → embed → output). This keeps implementation simple while utilizing available CPU/GPU resources.

### Backpressure

Bounded channels between pipeline stages prevent memory exhaustion when processing large batches:

```
Decode (fast) ──[buffer: 100]──▶ Embed (slow) ──[buffer: 100]──▶ Output
```

If embedding is slower than decoding, the decode stage blocks once the buffer fills, preventing unbounded memory growth.

```toml
[pipeline]
buffer_size = 100  # Max images buffered between stages
```

### Retry Strategy

Transient failures (network timeouts, temporary API errors) are retried with a simple fixed-delay strategy:

```toml
[pipeline]
retry_attempts = 3      # Max retries per image
retry_delay_ms = 1000   # Wait 1 second between retries
```

If all retries fail, the image is marked as failed, logged, and processing continues with the next image.

### Failure Handling

Each image either **fully succeeds** or **fully fails** — no partial state. Failures are logged with context:

```
[ERROR] Failed: /photos/corrupt.jpg - stage: decode - invalid JPEG header
[ERROR] Failed: /photos/large.png - stage: embed - timeout after 5000ms
```

At the end of processing, a summary is printed:

```
[INFO] Completed: 998 succeeded, 2 failed (45.2 img/sec)
```

### Skip Already-Processed Images

When re-running on a folder, use `--skip-existing` to avoid reprocessing:

```bash
photon process ./photos/ --output results.jsonl --skip-existing
```

This reads `results.jsonl` and skips any images whose `content_hash` is already present. Useful for incremental processing.

### Input Limits

Guard against problematic files that could slow down or crash the pipeline:

```toml
[limits]
max_file_size_mb = 100       # Skip files larger than 100MB
max_image_dimension = 10000  # Skip images wider/taller than 10000px
decode_timeout_ms = 5000     # Fail decode if it takes > 5 seconds
embed_timeout_ms = 30000     # Fail embedding if it takes > 30 seconds
llm_timeout_ms = 60000       # Fail LLM call if it takes > 60 seconds
```

---

## Output Format

### Rust Struct

```rust
pub struct ProcessedImage {
    // File identification
    pub file_path: PathBuf,
    pub file_name: String,
    pub content_hash: String,         // blake3 hash for deduplication

    // Image properties
    pub width: u32,
    pub height: u32,
    pub format: String,               // "jpeg", "png", "webp", etc.
    pub file_size: u64,

    // Vector embedding (main output for semantic search)
    pub embedding: Vec<f32>,          // 768 floats (SigLIP)

    // Metadata
    pub exif: Option<ExifData>,

    // AI-generated content
    pub tags: Vec<Tag>,
    pub description: Option<String>,  // If LLM enabled

    // Optional outputs
    pub thumbnail: Option<Vec<u8>>,   // WebP bytes, base64 in JSON
    pub perceptual_hash: Option<String>,
}

pub struct Tag {
    pub name: String,
    pub confidence: f32,              // 0.0 to 1.0
    pub category: Option<String>,     // "object", "scene", "color", "style"
}
```

### JSON Output

```json
{
  "file_path": "/photos/vacation/beach.jpg",
  "file_name": "beach.jpg",
  "content_hash": "a7f3b2c1d4e5f6a8b9c0d1e2f3...",
  "width": 4032,
  "height": 3024,
  "format": "jpeg",
  "file_size": 2458624,

  "embedding": [0.023, -0.156, 0.089, ...],

  "exif": {
    "captured_at": "2024-07-15T14:32:00Z",
    "camera_make": "Apple",
    "camera_model": "iPhone 15 Pro",
    "gps_latitude": 25.7617,
    "gps_longitude": -80.1918,
    "iso": 50,
    "aperture": "f/1.8"
  },

  "tags": [
    { "name": "beach", "confidence": 0.94, "category": "scene" },
    { "name": "ocean", "confidence": 0.87, "category": "scene" },
    { "name": "tropical", "confidence": 0.76, "category": "style" },
    { "name": "blue", "confidence": 0.82, "category": "color" }
  ],

  "description": "A sandy tropical beach with turquoise water and palm trees swaying in the breeze. The scene captures a serene vacation atmosphere.",

  "thumbnail": "base64-encoded-webp-bytes...",
  "perceptual_hash": "d4c3b2a1e5f6..."
}
```

---

## CLI Interface

```bash
# Process single image → JSON to stdout
photon process image.jpg

# Process directory → JSON Lines to file
photon process ./photos/ --output results.jsonl

# Stream for piping to your backend
photon process ./photos/ --format jsonl | your-ingestion-script

# Skip already-processed images (reads output file to check hashes)
photon process ./photos/ --output results.jsonl --skip-existing

# Control outputs
photon process image.jpg --no-thumbnail
photon process image.jpg --no-description
photon process image.jpg --thumbnail-size 256

# Use LLM for descriptions (BYOK)
photon process image.jpg --llm ollama --llm-model llama3.2-vision
photon process image.jpg --llm hyperbolic --llm-model meta-llama/Llama-3.2-11B-Vision-Instruct
photon process image.jpg --llm anthropic --llm-model claude-sonnet-4-20250514

# Batch processing with parallelism
photon process ./photos/ --parallel 8

# Verbose logging (debug info)
photon process ./photos/ --verbose

# Model management
photon models download          # Download SigLIP model
photon models list              # Show installed models
photon models path              # Show model directory

# Configuration
photon config show              # Display current config
photon config path              # Show config file location
```

---

## Model Strategy

### Two Types of Models

| Type | Purpose | Distribution | User Choice |
|------|---------|--------------|-------------|
| **Embedding (SigLIP)** | Image → vector for similarity search | Downloaded on first run (~350MB) | No — we pick SigLIP |
| **LLM (BYOK)** | Rich descriptions, query understanding | User provides | Yes — Ollama, Hyperbolic, Anthropic, etc. |

### SigLIP (Embedding Model)

- **What:** Successor to CLIP, trained by Google
- **Why:** Better zero-shot performance, cleaner embeddings
- **How:** Exported to ONNX, runs via `ort` crate
- **Size:** ~350MB for base model
- **Distribution:** Downloaded to `~/.photon/models/` on first use

### LLM Providers (BYOK)

```toml
# ~/.photon/config.toml

# Local via Ollama
[llm.ollama]
enabled = true
endpoint = "http://localhost:11434"
model = "llama3.2-vision"

# Self-hosted cloud via Hyperbolic
[llm.hyperbolic]
enabled = false
endpoint = "https://api.hyperbolic.xyz/v1"
api_key = "${HYPERBOLIC_API_KEY}"
model = "meta-llama/Llama-3.2-11B-Vision-Instruct"

# Commercial APIs
[llm.anthropic]
enabled = false
api_key = "${ANTHROPIC_API_KEY}"
model = "claude-sonnet-4-20250514"

[llm.openai]
enabled = false
api_key = "${OPENAI_API_KEY}"
model = "gpt-4o-mini"
```

---

## Configuration

```toml
# ~/.photon/config.toml

[general]
# Where to store downloaded models
model_dir = "~/.photon/models"

[processing]
# Default parallel workers
parallel_workers = 4
# Supported input formats
supported_formats = ["jpg", "jpeg", "png", "webp", "heic", "raw", "cr2", "nef", "arw"]

[pipeline]
# Backpressure: max images buffered between stages
buffer_size = 100
# Retry on transient failures
retry_attempts = 3
retry_delay_ms = 1000

[limits]
# Skip files larger than this
max_file_size_mb = 100
# Skip images with dimensions exceeding this
max_image_dimension = 10000
# Timeouts per stage
decode_timeout_ms = 5000
embed_timeout_ms = 30000
llm_timeout_ms = 60000

[embedding]
# SigLIP model variant
model = "siglip-base-patch16"  # or "siglip-large-patch16" for higher quality
# Inference device
device = "metal"  # "cpu", "metal" (Apple), "cuda" (NVIDIA)

[thumbnail]
# Generate thumbnails
enabled = true
size = 256
format = "webp"
quality = 80

[tagging]
# Minimum confidence to include a tag
min_confidence = 0.5
# Maximum tags per image
max_tags = 20
# Enable zero-shot tagging via SigLIP
zero_shot_enabled = true

[output]
# Default output format
format = "json"  # "json" or "jsonl"
# Pretty print JSON
pretty = false
# Include embedding in output (large!)
include_embedding = true

[logging]
# Log level: error, warn, info, debug, trace
level = "info"
# Format: pretty (human-readable) or json (structured)
format = "pretty"
```

---

## Logging

### Default Output (level: info)

```
[INFO] Photon v0.1.0
[INFO] Processing 1000 images with 4 workers...
[INFO] Progress: 500/1000 (50%) - 45.2 img/sec
[ERROR] Failed: /photos/corrupt.jpg - decode - invalid JPEG header
[INFO] Completed: 998 succeeded, 2 failed (45.2 img/sec)
```

### Verbose Output (--verbose or level: debug)

```
[DEBUG] Loading config from ~/.photon/config.toml
[DEBUG] Using device: metal
[DEBUG] Loading SigLIP model from ~/.photon/models/siglip-base-patch16/
[DEBUG] Processing: /photos/beach.jpg
[DEBUG]   Decode: 12ms
[DEBUG]   EXIF: 2ms
[DEBUG]   Embed: 85ms
[DEBUG]   Tags: 15ms
[DEBUG]   Total: 114ms
```

### JSON Logging (for machine parsing)

```toml
[logging]
format = "json"
```

```json
{"level":"INFO","message":"Processing 1000 images","workers":4}
{"level":"ERROR","message":"Failed","path":"/photos/corrupt.jpg","stage":"decode","error":"invalid JPEG header"}
{"level":"INFO","message":"Completed","succeeded":998,"failed":2,"rate":45.2}
```

---

## Embedding in Your Backend

### Python Example

```python
import subprocess
import json

def process_image(image_path: str) -> dict:
    """Process an image with Photon and return structured data."""
    result = subprocess.run(
        ["photon", "process", image_path],
        capture_output=True,
        text=True
    )
    return json.loads(result.stdout)

# Usage
data = process_image("/photos/beach.jpg")

# Store in your database
db.images.insert({
    "path": data["file_path"],
    "hash": data["content_hash"],
    "embedding": data["embedding"],  # Store in pgvector, Pinecone, etc.
    "tags": data["tags"],
    "metadata": data["exif"]
})
```

### Rust Example (Direct Library Use)

```rust
use photon_core::{Photon, Config, ProcessOptions};

#[tokio::main]
async fn main() -> Result<()> {
    let photon = Photon::new(Config::default()).await?;

    let result = photon.process("./image.jpg", ProcessOptions {
        generate_thumbnail: true,
        use_llm: Some("anthropic"),
        ..Default::default()
    }).await?;

    println!("Embedding: {:?}", result.embedding);
    println!("Tags: {:?}", result.tags);

    // Store in your database...

    Ok(())
}
```

### Node.js Example

```javascript
const { execSync } = require('child_process');

function processImage(imagePath) {
  const output = execSync(`photon process "${imagePath}"`);
  return JSON.parse(output.toString());
}

// Usage
const data = processImage('/photos/beach.jpg');

// Store in your database (e.g., with pgvector)
await db.query(
  `INSERT INTO images (path, hash, embedding, tags)
   VALUES ($1, $2, $3, $4)`,
  [data.file_path, data.content_hash, data.embedding, data.tags]
);
```

---

## Implementation Phases

### Phase 1: Foundation (1 week)
- [ ] Cargo workspace with `photon` and `photon-core` crates
- [ ] CLI skeleton with clap (`photon process`, `photon models`, `photon config`)
- [ ] Configuration system (TOML-based)
- [ ] Output formatting (JSON, JSONL)
- [ ] Structured logging with tracing
- [ ] Basic error types with thiserror

**Milestone:** `photon --help` works, `photon config show` displays config

### Phase 2: Image Pipeline (2 weeks)
- [ ] Image decoding with `image` crate (jpg, png, webp, heic)
- [ ] EXIF extraction with `kamadak-exif`
- [ ] Content hashing with blake3
- [ ] Perceptual hashing with `image_hasher`
- [ ] Thumbnail generation (WebP output)
- [ ] Batch file discovery with filtering
- [ ] Input validation (file size, dimensions)
- [ ] Decode timeout

**Milestone:** `photon process image.jpg` outputs metadata, hash, thumbnail

### Phase 3: SigLIP Embedding (2 weeks)
- [ ] ONNX Runtime setup with Metal acceleration
- [ ] SigLIP model download on first run
- [ ] Image preprocessing (resize, normalize)
- [ ] Embedding generation
- [ ] Batch processing with bounded channels (backpressure)
- [ ] Embedding timeout

**Milestone:** `photon process image.jpg` outputs 768-dim embedding vector

### Phase 4: Zero-Shot Tagging (1 week)
- [ ] SigLIP text encoder integration
- [ ] Tag taxonomy/vocabulary
- [ ] Zero-shot classification
- [ ] Confidence scoring and filtering

**Milestone:** `photon process image.jpg` outputs semantic tags

### Phase 5: LLM Integration (2 weeks)
- [ ] LLM provider trait abstraction
- [ ] Ollama integration (local)
- [ ] Hyperbolic integration (self-hosted cloud)
- [ ] Anthropic integration (commercial)
- [ ] OpenAI integration (commercial)
- [ ] Description generation
- [ ] Retry logic for transient failures
- [ ] LLM timeout

**Milestone:** `photon process image.jpg --llm anthropic` outputs AI description

### Phase 6: Polish & Release (1 week)
- [ ] Parallel batch processing with progress bar
- [ ] `--skip-existing` flag for incremental processing
- [ ] Summary stats at end of run
- [ ] Comprehensive error messages
- [ ] Performance optimization
- [ ] Documentation and examples
- [ ] GitHub release with binaries

**Milestone:** v0.1.0 release

---

## Performance Targets

| Operation | Target | Hardware |
|-----------|--------|----------|
| Image decode + metadata | 200 img/sec | M1 Mac |
| SigLIP embedding | 50-100 img/min | M1 Mac (Metal) |
| Full pipeline (no LLM) | 50 img/min | M1 Mac |
| Full pipeline (with LLM) | 10-20 img/min | Depends on LLM provider |

---

## What Photon Does NOT Do

To keep scope focused, Photon intentionally excludes:

- ❌ **Database management** — Your backend handles storage
- ❌ **Vector search** — Use pgvector, Pinecone, Qdrant in your backend
- ❌ **API server** — Photon is a CLI/library, not a service
- ❌ **File watching** — Your backend triggers processing
- ❌ **User authentication** — Not applicable
- ❌ **Web UI** — Build your own on top

---

## Future Considerations

After v1.0, potential additions:

- **CUDA support** — For non-Apple hardware
- **Additional models** — OpenCLIP variants, DINOv2
- **Video support** — Frame extraction and processing
- **OCR** — Text extraction from images
- **Face detection** — Optional face embedding output
- **WASM build** — Run in browser
- **Pluggable enhancers** — Custom processing stages

---

## Next Steps

1. ✅ Finalize this blueprint
2. Begin Phase 1: Foundation
3. Set up CI/CD pipeline
4. Create initial release structure
