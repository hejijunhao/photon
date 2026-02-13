# Technical Overview

A complete guide to the Photon codebase: what sits where, what each file does, and how they all work together.

---

## What Photon Is

Photon is a **pure image processing pipeline** — it takes images in and produces structured JSON out. It does not manage a database, serve an API, or provide a UI. Every image flows through a deterministic sequence of stages (decode, hash, embed, tag, optionally describe with an LLM) and exits as a self-contained JSON record that your backend can store and search however it likes.

The codebase is a Rust workspace with two crates: a library (`photon-core`) containing all processing logic, and a thin CLI binary (`photon`) that orchestrates I/O and user interaction on top of it.

---

## Repository Root

```
photon/
├── Cargo.toml              # Workspace manifest (two members, shared deps)
├── Cargo.lock              # Pinned dependency versions
├── CLAUDE.md               # AI assistant context (architecture summary)
├── README.md               # User-facing docs: install, usage, examples
├── LICENSE-MIT              # Dual-licensed
├── LICENSE-APACHE           #
├── .gitignore               #
│
├── crates/
│   ├── photon-core/        # The library — all processing logic (~8K lines)
│   └── photon/             # The CLI binary — I/O and UX (~1.4K lines)
│
├── data/                   # Source vocabulary files shipped with the repo
│   └── vocabulary/
│       ├── wordnet_nouns.txt    # ~68K WordNet nouns (name, synset, hypernyms)
│       ├── supplemental.txt     # ~260 curated visual terms (scenes, moods, styles)
│       └── seed_terms.txt       # Hand-picked common terms for fast first-run startup
│
├── tests/
│   └── fixtures/images/    # Test images used by integration tests + benchmarks
│       ├── test.png        # 70-byte minimal PNG
│       ├── dog.jpg         # 252 KB
│       ├── beach.jpg       # 44 KB
│       └── car.jpg         # 40 KB
│
├── docs/                   # Project documentation
│   ├── vision.md           # Original product vision
│   ├── blueprint.md        # Architecture blueprint & design decisions
│   ├── changelog.md        # Detailed per-version change history
│   ├── archive/            # Historical code assessments and fix plans
│   ├── completions/        # Post-mortems for completed optimization phases
│   ├── executing/          # In-progress plans (speed improvements, publishing)
│   └── plans/              # Assessment reports
│
├── scripts/                # Utilities (e.g., WordNet vocabulary generation)
├── assets/                 # Project logo
│
└── .github/workflows/
    ├── ci.yml              # Check, test, fmt, clippy on macOS-14 + Ubuntu
    └── release.yml         # Binary release builds
```

---

## The Two Crates

### `photon-core` — The Library

Everything that processes an image lives here. Zero I/O policy decisions, zero CLI concerns. If you wanted to embed Photon's processing into another Rust application, this is the only crate you'd depend on.

### `photon` — The CLI Binary

A thin wrapper that handles file discovery, output formatting, progress bars, interactive mode, model downloads, and config management. It calls into `photon-core` for all actual image processing. The `photon` binary has a single dependency on `photon-core = { path = "../photon-core" }`.

The design intent: **all intelligence in the library, all UX in the binary**.

---

## photon-core: File-by-File

```
crates/photon-core/src/
├── lib.rs                  # Public API surface + BLAS linkage
├── types.rs                # Core data structures (ProcessedImage, Tag, etc.)
├── error.rs                # Error hierarchy (PhotonError → PipelineError, ConfigError)
├── math.rs                 # L2 normalization helpers
├── output.rs               # JSON/JSONL serializer (OutputWriter)
│
├── config/                 # Configuration system
│   ├── mod.rs              # Config loading, platform paths, defaults
│   ├── types.rs            # All config sub-structs with defaults
│   └── validate.rs         # Range validation + cross-field consistency
│
├── pipeline/               # Image processing stages
│   ├── mod.rs              # Module re-exports
│   ├── processor.rs        # ★ The orchestrator — wires all stages together
│   ├── validate.rs         # Pre-decode file validation (size, magic bytes)
│   ├── decode.rs           # Async image decoding with format detection
│   ├── metadata.rs         # EXIF extraction (camera, GPS, exposure)
│   ├── hash.rs             # BLAKE3 content hash + perceptual hash
│   ├── thumbnail.rs        # WebP thumbnail generation
│   └── discovery.rs        # Recursive file discovery with format filtering
│
├── embedding/              # SigLIP visual embedding
│   ├── mod.rs              # EmbeddingEngine public interface
│   ├── siglip.rs           # ONNX Runtime session management + inference
│   └── preprocess.rs       # Image → normalized NCHW tensor
│
├── tagging/                # Zero-shot tagging system (most complex subsystem)
│   ├── mod.rs              # Module re-exports
│   ├── vocabulary.rs       # 68K-term vocabulary loader (WordNet + supplemental)
│   ├── scorer.rs           # Embedding × label matrix → confidence scores
│   ├── label_bank.rs       # Pre-computed N×768 text embedding matrix + caching
│   ├── text_encoder.rs     # SigLIP text model (encodes tag names to vectors)
│   ├── progressive.rs      # Incremental vocabulary encoding (cold-start fix)
│   ├── relevance.rs        # Three-pool self-organizing vocabulary (Active/Warm/Cold)
│   ├── hierarchy.rs        # WordNet ancestor deduplication
│   ├── neighbors.rs        # WordNet sibling expansion
│   └── seed.rs             # Seed term selection for progressive encoding
│
└── llm/                    # LLM integration (BYOK)
    ├── mod.rs              # Module re-exports
    ├── provider.rs         # LlmProvider trait + factory + request/response types
    ├── anthropic.rs        # Anthropic Messages API
    ├── openai.rs           # OpenAI Chat Completions API
    ├── hyperbolic.rs       # Hyperbolic (OpenAI-compatible wrapper)
    ├── ollama.rs           # Local Ollama API
    ├── enricher.rs         # Concurrent batch enrichment engine
    └── retry.rs            # Error classification + exponential backoff
```

### lib.rs — The Public API Surface

The entry point for anyone consuming `photon-core`. It does two things:

1. **Force-links BLAS** on macOS (`extern crate blas_src`) so ndarray's dot products dispatch to Apple's Accelerate framework.
2. **Re-exports** the public API — everything a consumer needs without reaching into submodules:

```rust
pub use config::Config;
pub use embedding::EmbeddingEngine;
pub use error::{ConfigError, PhotonError, PipelineError, PipelineResult, Result};
pub use llm::{EnrichOptions, EnrichResult, Enricher, LlmProviderFactory};
pub use output::{OutputFormat, OutputWriter};
pub use pipeline::{DiscoveredFile, FileDiscovery, Hasher, ImageProcessor, ProcessOptions};
pub use types::{EnrichmentPatch, ExifData, OutputRecord, ProcessedImage, ProcessingStats, Tag};
```

Most submodules are `pub(crate)` — only `config`, `error`, and `types` are `pub mod` (because downstream code may need to inspect config fields or match error variants).

### types.rs — The Data Model

Defines the **output contract** — what comes out of the pipeline:

| Type | Purpose |
|------|---------|
| `ProcessedImage` | Complete per-image result: path, hash, dimensions, embedding vector, EXIF, tags, description, thumbnail |
| `Tag` | A semantic label with confidence score, optional category and hierarchy path |
| `ExifData` | Camera metadata: datetime, make/model, GPS, ISO, aperture, shutter, focal length |
| `EnrichmentPatch` | LLM description keyed by content_hash (joins with core record) |
| `OutputRecord` | Tagged union: `Core(ProcessedImage)` or `Enrichment(EnrichmentPatch)` for dual-stream output |
| `ProcessingStats` | Batch summary: succeeded, failed, skipped, throughput |

### error.rs — The Error Hierarchy

Two-level error design:

- **`PhotonError`** (top-level) — wraps `PipelineError`, `ConfigError`, and generic I/O
- **`PipelineError`** — stage-specific variants, each carrying the file path and a message:
  - `Decode`, `Metadata`, `Embedding`, `Tagging`, `Model`, `Llm` (with optional HTTP status code)
  - `Timeout` (stage + duration), `FileTooLarge`, `ImageTooLarge`, `UnsupportedFormat`, `FileNotFound`
- **`ConfigError`** — file read, TOML parse, and validation failures

Every error includes enough context (file path, stage name, timeout value) for the CLI to produce a helpful error message without re-deriving context.

### config/ — Configuration System

Three files implementing a **three-tier hierarchy**: code defaults → TOML file → CLI flags.

- **`mod.rs`** — `Config::load()` reads `~/.photon/config.toml` (platform-aware via the `directories` crate), falls back to defaults if missing. Provides `model_dir()`, `vocabulary_dir()`, `taxonomy_dir()` with `~` expansion.
- **`types.rs`** — All sub-structs (`LimitsConfig`, `TaggingConfig`, `EmbeddingConfig`, etc.) with `#[derive(Default)]` providing sensible values. Key defaults: 4 parallel workers, 100 MB max file size, 30s embed timeout, 50 max tags, 0.5 min confidence.
- **`validate.rs`** — Range checks on 9 fields, auto-derives `image_size` from model name to prevent desync, warns on conflicting progressive + relevance settings.

---

## The Pipeline: How an Image Flows Through

The central orchestrator is **`pipeline/processor.rs`** → `ImageProcessor`. Here is the exact sequence for a single image:

```
                    ImageProcessor::process_with_options(path)
                                      │
          ┌───────────────────────────┼───────────────────────────┐
          ▼                           ▼                           ▼
   1. Validate               2. Read file once             3. Content hash
   (size, magic bytes)       (Vec<u8> in memory)           (BLAKE3 from buffer)
          │                           │
          │                           ▼
          │                    4. Decode from buffer
          │                    (async, with timeout)
          │                           │
          │              ┌────────────┼────────────────┐
          │              ▼            ▼                 ▼
          │       5. EXIF        6. Perceptual     7. Thumbnail
          │       extraction     hash (16x16       (resize → WebP
          │       (sync)         DoubleGradient)    → base64)
          │                           │
          │                           ▼
          │                    8. Preprocess image
          │                    (resize 224/384, normalize [-1,1])
          │                           │
          │                           ▼
          │                    9. Embed (SigLIP ONNX)
          │                    (spawn_blocking + timeout)
          │                    → 768-dim L2-normalized vector
          │                           │
          │                           ▼
          │                   10. Tag (embedding × label bank)
          │                    → SigLIP sigmoid → filter/sort/dedup
          │                           │
          └───────────┬───────────────┘
                      ▼
               ProcessedImage
                      │
                      ▼  (optional, CLI-level)
               11. Enrich (LLM)
               → EnrichmentPatch
```

### Key design choices in the pipeline:

**Read-once I/O** — The file is read into a `Vec<u8>` once. Both the BLAKE3 hash and the image decoder consume this buffer. No second disk read.

**Preprocess before `spawn_blocking`** — The image is resized and normalized (224x224, ~600 KB tensor) *before* crossing the thread boundary, instead of sending the full decoded image (~49 MB for a 4032x3024 photo). This is an 80x reduction in data moved across the async/blocking boundary.

**Optional components via `Option<Arc<T>>`** — The embedding engine and tag scorer are loaded separately via `load_embedding()` and `load_tagging()`. If neither is loaded, the pipeline still works — it just produces hashes, metadata, and thumbnails.

**`RwLock` on `TagScorer`** — The tag scorer uses `RwLock` (not `Mutex`) because the relevance tracker needs to mutate pool assignments during scoring, but scoring itself is a read-only operation. Multiple images can score concurrently under a read lock.

---

## Pipeline Stages: File by File

### pipeline/validate.rs — Gate Keeper

Runs before anything expensive. Checks:
- File exists and is readable
- File size within `max_file_size_mb` limit
- First 12 bytes match a known image magic signature (JPEG, PNG, WebP, HEIC, GIF, BMP, TIFF, etc.)

Rejects early with specific `PipelineError` variants (`FileNotFound`, `FileTooLarge`, `UnsupportedFormat`).

### pipeline/decode.rs — Image Decoder

Async decoding with timeout protection:
- `decode_from_bytes()` wraps the `image` crate's decoder in `spawn_blocking` + `tokio::time::timeout`
- Uses `with_guessed_format()` for content-based format detection (not just extension)
- Returns `DecodedImage` with the `DynamicImage`, detected format string, dimensions, and file size

### pipeline/metadata.rs — EXIF Extraction

Lenient extraction — partial data is fine, missing fields are `None`. Extracts: captured datetime, camera make/model, GPS coordinates (with hemisphere-aware conversion), ISO, aperture, shutter speed, focal length, orientation.

### pipeline/hash.rs — Content + Perceptual Hashing

Two independent hash types:
- **BLAKE3** content hash — cryptographic, for deduplication and `--skip-existing`
- **Perceptual hash** — DoubleGradient 16x16, for near-duplicate detection across visual similarity

The `Hasher` struct holds a pre-built `image_hasher::Hasher` (constructed once, reused for every image — avoids per-image allocation overhead).

### pipeline/thumbnail.rs — WebP Thumbnails

Resizes maintaining aspect ratio, encodes to WebP at configurable quality, returns base64-encoded string. Respects `enabled` flag from config.

### pipeline/discovery.rs — File Discovery

Recursively walks directories (WalkDir with symlink following, max depth 256), filters by supported extensions from config, logs warnings on permission errors instead of silently skipping. Returns sorted `Vec<DiscoveredFile>`.

---

## Embedding System

```
crates/photon-core/src/embedding/
├── mod.rs              # EmbeddingEngine — public wrapper
├── siglip.rs           # SigLipSession — ONNX Runtime management
└── preprocess.rs       # Image → NCHW tensor normalization
```

### How embedding works:

1. **Preprocess** (`preprocess.rs`) — Resize to 224x224 or 384x384 (Lanczos3), convert to RGB, normalize pixels to `[-1, 1]` via `(pixel/255 - 0.5) / 0.5`, output NCHW layout. Uses raw buffer iteration (`as_raw().chunks_exact(3)`) to avoid per-pixel bounds checking.

2. **Inference** (`siglip.rs`) — `SigLipSession` holds a `Mutex<ort::Session>`. Input is passed as `(Vec<i64>, Vec<f32>)` tuples (avoids coupling to ort's internal ndarray version). The **`pooler_output`** (2nd model output) is used — not `last_hidden_state` — because SigLIP's cross-modal projection lives in the pooler.

3. **Output** (`mod.rs`) — 768-dimensional L2-normalized `Vec<f32>`. This vector can be stored in any vector database for semantic similarity search.

### Model files (at runtime):

```
~/.photon/models/
├── siglip-base-patch16/visual.onnx       # Default 224px model
├── siglip-base-patch16-384/visual.onnx   # High-quality 384px model
├── text_model.onnx                       # Shared text encoder (for tagging)
└── tokenizer.json                        # Shared tokenizer
```

---

## Tagging System

The most complex subsystem (~2,800 lines across 10 files). Takes a 768-dim image embedding and produces human-readable semantic tags with confidence scores.

```
crates/photon-core/src/tagging/
├── vocabulary.rs       # What terms exist (~68K)
├── label_bank.rs       # Pre-computed text embeddings for all terms (N×768 matrix)
├── text_encoder.rs     # SigLIP text model that produces those embeddings
├── scorer.rs           # Image embedding × label bank → scores
├── progressive.rs      # First-run optimization (encode seed, background-encode rest)
├── relevance.rs        # Runtime optimization (prune cold terms from scoring)
├── hierarchy.rs        # Post-processing (remove ancestor redundancy)
├── neighbors.rs        # Vocabulary expansion (WordNet siblings)
└── seed.rs             # Seed selection for progressive encoding
```

### The scoring pipeline:

```
Image embedding (768-dim)
        ×
Label bank matrix (68K × 768)     ← pre-computed text embeddings
        =
68K raw cosine similarities
        │
        ▼
SigLIP sigmoid: logit = 117.33 * cosine - 12.93, then sigmoid(logit)
        │
        ▼
Filter (min_confidence), sort (descending), truncate (max_tags)
        │
        ▼
Hierarchy dedup (suppress ancestors when descendants present)
        │
        ▼
Vec<Tag> output
```

### How the pieces connect:

**`Vocabulary`** loads ~68K WordNet nouns + ~260 supplemental terms from text files. Each term has a name, WordNet synset ID, and hypernym chain (parent → grandparent → ... → "entity").

**`LabelBank`** is an N×768 flat `f32` matrix — one row per vocabulary term. Each row is the L2-normalized text embedding of `"a photo of a {term}"`. This matrix is computed once by the `SigLipTextEncoder` and cached to disk at `~/.photon/taxonomy/label_bank.bin` with a vocabulary hash in a `.meta` sidecar for cache invalidation.

**`TagScorer`** performs the actual scoring. On macOS, the matrix-vector multiply (`label_bank × image_embedding`) dispatches to Accelerate's `sgemv` via ndarray's BLAS backend — replacing 68K individual scalar dot products with a single hardware-accelerated operation.

**`ProgressiveEncoder`** solves the cold-start problem. On first run, encoding all 68K terms through the text model takes ~90 minutes. Instead, it encodes ~2K high-value seed terms synchronously (~30s), creates an initial scorer, then background-encodes the remaining terms in chunks — swapping in progressively larger scorers via `RwLock` as each chunk completes.

**`RelevanceTracker`** is a runtime optimization. It maintains three pools:
- **Active** (~2K terms) — scored every image
- **Warm** — sampled periodically (every N images)
- **Cold** — not scored at all

Terms self-organize: frequently-hit terms stay active, idle terms demote to warm, then cold. This reduces per-image scoring cost from 68K to ~2K dot products after warmup. Pool assignments persist across sessions via `~/.photon/taxonomy/relevance.json`.

**`HierarchyDedup`** removes redundant ancestor tags. If both "labrador retriever" (0.92) and "dog" (0.85) score above threshold, "dog" is suppressed because it's an ancestor. Optionally annotates surviving tags with abbreviated hierarchy paths (e.g., `"animal > dog > labrador retriever"`).

**`NeighborExpander`** uses WordNet's graph structure to expand coverage. When a term gets promoted to Active, its siblings (terms sharing the same parent) get promoted to Warm for sampling — helping the system discover related terms it hasn't seen yet.

---

## LLM Integration

```
crates/photon-core/src/llm/
├── provider.rs         # LlmProvider trait + LlmProviderFactory
├── anthropic.rs        # Anthropic Messages API
├── openai.rs           # OpenAI Chat Completions API
├── hyperbolic.rs       # Hyperbolic (wraps OpenAI provider with custom endpoint)
├── ollama.rs           # Local Ollama API
├── enricher.rs         # Concurrent batch enrichment with backpressure
└── retry.rs            # Error classification + exponential backoff
```

### Architecture:

**`LlmProvider`** is an async trait (`#[async_trait]`) with four implementations. `LlmProviderFactory::create()` produces a `Box<dyn LlmProvider>` from a provider name + config, with `${ENV_VAR}` expansion for API keys.

**`Enricher`** orchestrates concurrent LLM calls. It wraps a provider and processes images in parallel, bounded by a `tokio::Semaphore` (default 4, max 8). Each image is read, base64-encoded, and sent with a prompt that includes zero-shot tags for context. The semaphore permit is dropped *before* the result callback runs, so the next LLM request starts while the callback executes.

**`retry.rs`** classifies errors: HTTP 429/5xx are retryable, 401/403 are not. Non-HTTP errors (connection failures, timeouts) fall back to message substring matching. Backoff is exponential: `2^attempt * base_delay`, capped at 30 seconds.

### Dual-stream output model:

When `--llm` is active, the CLI uses two record types:
1. `OutputRecord::Core(ProcessedImage)` — emitted immediately at full pipeline speed
2. `OutputRecord::Enrichment(EnrichmentPatch)` — follows later as LLM calls complete

The `EnrichmentPatch` is keyed by `content_hash`, so the consumer can join it with the core record. Without `--llm`, output is plain `ProcessedImage` — backward compatible.

---

## The CLI Binary: File by File

```
crates/photon/src/
├── main.rs                     # Entry point: parse CLI, dispatch to command
├── logging.rs                  # tracing/tracing-subscriber setup
│
└── cli/
    ├── mod.rs                  # Module declarations
    ├── config.rs               # `photon config {show,path,init}`
    ├── models.rs               # `photon models {download,list,path}` + HuggingFace downloads
    │
    ├── process/                # `photon process` — the main command
    │   ├── mod.rs              # ProcessArgs, execute(), process_single()
    │   ├── types.rs            # CLI enums (OutputFormat, Quality, LlmProvider)
    │   ├── setup.rs            # Build ProcessContext (load models, apply CLI overrides)
    │   ├── batch.rs            # Concurrent batch processing (buffer_unordered)
    │   └── enrichment.rs       # LLM enrichment helpers (collect vs stream modes)
    │
    └── interactive/            # `photon` with no args (guided wizard)
        ├── mod.rs              # Main menu loop
        ├── process.rs          # Guided processing wizard (8 steps)
        ├── models.rs           # Guided model management
        ├── setup.rs            # Path input, model selection helpers
        └── theme.rs            # Custom dialoguer color scheme + ASCII banner
```

### main.rs — Entry Point

Parses CLI via clap, loads config (with graceful fallback), initializes logging, and dispatches:

| Input | Handler |
|-------|---------|
| `photon process ...` | `cli::process::execute()` |
| `photon models ...` | `cli::models::execute()` |
| `photon config ...` | `cli::config::execute()` |
| `photon` (TTY) | `cli::interactive::run()` — guided wizard |
| `photon` (piped) | Print help and exit |

### process/ — The Main Command

The `execute()` function is ~16 lines of orchestration:

1. `setup_processor()` builds a `ProcessContext` — loads config, applies CLI overrides, loads embedding + tagging models
2. `discover()` finds all image files at the input path
3. Branch: single file → `process_single()`, directory → `process_batch()`
4. Save relevance tracking data on completion

**`batch.rs`** is where concurrency happens. Images flow through `futures::stream::iter(files).map(process).buffer_unordered(parallel)`. While one image waits on the ONNX mutex for embedding, others decode, hash, and generate thumbnails. Results are consumed single-threaded — stdout/file writes need no synchronization.

**Skip-existing pre-filtering**: existing content hashes are loaded from the output file *before* the concurrent pipeline, so skipped files don't waste concurrency slots.

### models.rs — Model Management

Downloads SigLIP models from HuggingFace (`Xenova/siglip-base-patch16-*`):
- Visual encoders (224px and 384px variants)
- Shared text encoder + tokenizer
- Vocabulary files

Every download is verified with a BLAKE3 checksum. Corrupt files are auto-removed.

### interactive/ — Guided Wizard

When you run bare `photon` in a terminal, an 8-step guided wizard walks you through:
1. Input path selection (with file discovery preview)
2. Model installation check
3. Quality preset (fast/high)
4. LLM provider selection
5. Output format + destination
6. Confirmation + execution

Uses `dialoguer` for prompts with a custom Photon color theme.

---

## Data Flow: End to End

Here is how a `photon process ./photos/ --output results.jsonl --llm anthropic` invocation flows through the codebase:

```
main.rs
  │ parse CLI, load config, init logging
  ▼
cli/process/mod.rs::execute()
  │ setup_processor() → ProcessContext
  │ discover() → Vec<DiscoveredFile>
  ▼
cli/process/batch.rs::process_batch()
  │ load existing hashes (if --skip-existing)
  │ pre-filter already-processed files
  │
  │  ┌─── buffer_unordered(parallel) ──────────────────────┐
  │  │                                                      │
  │  │  For each image file:                                │
  │  │    pipeline/processor.rs::process_with_options()     │
  │  │      ├── validate.rs    → check size + magic bytes   │
  │  │      ├── fs::read()     → Vec<u8> (read once)        │
  │  │      ├── hash.rs        → BLAKE3 content hash        │
  │  │      ├── decode.rs      → DynamicImage               │
  │  │      ├── metadata.rs    → ExifData                   │
  │  │      ├── hash.rs        → perceptual hash            │
  │  │      ├── thumbnail.rs   → base64 WebP                │
  │  │      ├── preprocess.rs  → NCHW tensor                │
  │  │      ├── siglip.rs      → 768-dim embedding          │
  │  │      └── scorer.rs      → Vec<Tag>                   │
  │  │    → ProcessedImage                                  │
  │  │                                                      │
  │  └──────────────────────────────────────────────────────┘
  │
  │  Write Core records to results.jsonl (streaming)
  │
  ▼
cli/process/enrichment.rs::run_enrichment_collect()
  │ For each ProcessedImage:
  │   llm/enricher.rs → read image, base64 encode, send to LLM
  │   llm/anthropic.rs → Anthropic Messages API
  │   → EnrichmentPatch { content_hash, description }
  │
  │ Append Enrichment records to results.jsonl
  ▼
Save relevance data → ~/.photon/taxonomy/relevance.json
Print summary stats
```

---

## Key Architectural Patterns

### Optional Components via `Option<Arc<T>>`

`ImageProcessor` holds models as optional shared references:

```rust
embedding_engine: Option<Arc<EmbeddingEngine>>,         // Mutex internally
tag_scorer: Option<Arc<RwLock<TagScorer>>>,              // RwLock for concurrent reads
relevance_tracker: Option<RwLock<RelevanceTracker>>,     // Mutable pool assignments
```

`new()` is sync and infallible — it creates the processor without any models. Models are loaded separately via `load_embedding()` and `load_tagging()`, so the processor degrades gracefully (no embedding → no tags, but hashes + metadata + thumbnails still work).

### Async Timeout Pattern

Blocking ONNX inference runs inside `spawn_blocking` wrapped in `tokio::time::timeout`:

```rust
tokio::time::timeout(duration, async {
    tokio::task::spawn_blocking(move || engine.embed_preprocessed(&tensor, &path))
        .await
})
```

This avoids blocking the async runtime while still enforcing time limits. Used for both embedding and decode stages.

### Lock Ordering Discipline

When both `tag_scorer` and `relevance_tracker` need write access during pool-aware scoring, the code follows strict lock ordering:

1. Read lock on both (concurrent scoring — the hot path)
2. Release both
3. Write lock on tracker only (record hits, sweep)
4. Release tracker
5. Read lock on scorer (find neighbors)
6. Write lock on tracker (promote neighbors)

No lock is ever held while acquiring another lock as a write lock.

### Configuration Hierarchy

```
Code defaults (types.rs)  →  TOML file (~/.photon/config.toml)  →  CLI flags
     lowest priority                                                highest priority
```

The CLI's `setup.rs` applies CLI flags as overrides on top of the loaded config before passing it to `ImageProcessor::new()`.

---

## Runtime Data Layout

When Photon runs, it reads from and writes to `~/.photon/`:

```
~/.photon/
├── config.toml                 # User configuration
│
├── models/
│   ├── siglip-base-patch16/
│   │   └── visual.onnx        # 224px visual encoder (~87 MB)
│   ├── siglip-base-patch16-384/
│   │   └── visual.onnx        # 384px visual encoder (~87 MB)
│   ├── text_model.onnx        # Shared text encoder (~130 MB)
│   └── tokenizer.json         # Shared tokenizer (~700 KB)
│
├── vocabulary/                 # Installed from data/vocabulary/
│   ├── wordnet_nouns.txt
│   ├── supplemental.txt
│   └── seed_terms.txt
│
└── taxonomy/
    ├── label_bank.bin          # Pre-computed text embeddings (N×768 flat f32, ~200 MB)
    ├── label_bank.meta         # Vocabulary hash for cache invalidation
    └── relevance.json          # Pool assignments + term statistics (persisted)
```

---

## Dependency Map

### photon-core (library)

| Category | Crate | Purpose |
|----------|-------|---------|
| Async | `tokio` | Runtime, `spawn_blocking`, `time::timeout` |
| Image | `image` | Decoding all formats, resizing |
| Image | `kamadak-exif` | EXIF metadata extraction |
| Image | `image_hasher` | Perceptual hashing (DoubleGradient) |
| Hashing | `blake3` | Content hashing for dedup |
| ML | `ort` (2.0.0-rc.11) | ONNX Runtime — runs SigLIP models |
| ML | `ndarray` | Matrix operations for scoring |
| ML | `blas-src` (macOS) | Accelerate framework for BLAS |
| ML | `tokenizers` | SigLIP text tokenization |
| Serialization | `serde`, `serde_json`, `toml` | JSON output, config parsing |
| Error | `thiserror` | Derive macro for error types |
| HTTP | `reqwest` | LLM API calls |
| Async | `async-trait` | Object-safe async trait for `LlmProvider` |
| I/O | `walkdir` | Recursive directory traversal |
| I/O | `base64` | Thumbnail encoding |
| Paths | `directories`, `shellexpand` | Platform-aware paths, `~` expansion |
| Logging | `tracing` | Structured logging |

### photon (CLI binary)

| Crate | Purpose |
|-------|---------|
| `photon-core` | All processing logic |
| `clap` | CLI argument parsing |
| `anyhow` | Top-level error handling |
| `indicatif` | Progress bars |
| `dialoguer`, `console` | Interactive prompts + color |
| `toml_edit` | Comment-preserving config editing |
| `tracing-subscriber` | Log formatting (human + JSON) |

---

## Test Structure

```
216 tests total:
├── 38 CLI tests (crates/photon/src/**/tests)
├── 158 core tests (crates/photon-core/src/**/tests)
└── 20 integration tests (crates/photon-core/tests/integration.rs)
```

Integration tests in `crates/photon-core/tests/integration.rs` exercise the full `ImageProcessor::process()` pipeline against real fixture images (`tests/fixtures/images/`).

Benchmarks in `crates/photon-core/benches/pipeline.rs` cover: BLAKE3 hashing, perceptual hashing, image decoding, thumbnail generation, and metadata extraction via Criterion.

---

## CI/CD

`.github/workflows/ci.yml` runs on every push to `master` and on pull requests:

| Job | What it does |
|-----|-------------|
| **Check & Test** | `cargo check --workspace` + `cargo test --workspace` on **macOS-14** and **Ubuntu** |
| **Lint** | `cargo fmt --all -- --check` + `cargo clippy --workspace -- -D warnings` on Ubuntu |

The macOS runner is required because the BLAS/Accelerate integration is platform-specific.
