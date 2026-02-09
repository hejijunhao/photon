# Phase 1: Foundation — Completion Report

> **Status:** ✅ Complete
> **Date:** 2026-02-09
> **Milestone:** `photon --help` works, `photon config show` displays config

---

## Summary

Phase 1 establishes the project scaffolding: Cargo workspace, CLI structure, configuration system, output formatting, logging, and error handling. No image processing yet — just the skeleton that everything else builds on.

---

## What Was Built

### Workspace Structure

```
photon/
├── Cargo.toml                    # Workspace manifest
├── crates/
│   ├── photon/                   # CLI binary
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs           # CLI entry point
│   │       ├── logging.rs        # Logging setup (tracing)
│   │       └── cli/
│   │           ├── mod.rs
│   │           ├── process.rs    # Process command
│   │           ├── models.rs     # Models command
│   │           └── config.rs     # Config command
│   │
│   └── photon-core/              # Embeddable library
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs            # Library exports + Photon struct
│           ├── config.rs         # Configuration types + loading
│           ├── error.rs          # Error types (PhotonError, PipelineError, etc.)
│           ├── types.rs          # Data types (ProcessedImage, Tag, ExifData)
│           └── output.rs         # Output formatting (JSON, JSONL)
```

---

## Files Created

### Root

| File | Purpose |
|------|---------|
| `Cargo.toml` | Workspace manifest with shared dependencies |

### photon (CLI binary)

| File | Purpose |
|------|---------|
| `crates/photon/Cargo.toml` | Binary crate config, depends on photon-core |
| `crates/photon/src/main.rs` | CLI entry point with clap, dispatches to commands |
| `crates/photon/src/logging.rs` | Tracing initialization (pretty/JSON, verbose mode) |
| `crates/photon/src/cli/mod.rs` | CLI module exports |
| `crates/photon/src/cli/process.rs` | `photon process` command with all flags from blueprint |
| `crates/photon/src/cli/models.rs` | `photon models [download|list|path]` commands |
| `crates/photon/src/cli/config.rs` | `photon config [show|path|init]` commands |

### photon-core (library)

| File | Purpose |
|------|---------|
| `crates/photon-core/Cargo.toml` | Library crate config |
| `crates/photon-core/src/lib.rs` | Public API, Photon struct stub, re-exports |
| `crates/photon-core/src/config.rs` | Full configuration system with all options from blueprint |
| `crates/photon-core/src/error.rs` | Comprehensive error types (PhotonError, PipelineError, ConfigError) |
| `crates/photon-core/src/types.rs` | ProcessedImage, Tag, ExifData, ProcessingStats structs |
| `crates/photon-core/src/output.rs` | OutputWriter for JSON/JSONL, OutputFormat enum |

---

## Dependencies Added

### Workspace-level (shared)

| Dependency | Version | Purpose |
|------------|---------|---------|
| tokio | 1 (full) | Async runtime |
| serde | 1 (derive) | Serialization framework |
| serde_json | 1 | JSON serialization |
| toml | 0.8 | TOML config parsing |
| thiserror | 2 | Error derive macros |
| anyhow | 1 | Ergonomic error handling (CLI) |
| tracing | 0.1 | Structured logging |
| tracing-subscriber | 0.3 (env-filter, json) | Log formatting |
| directories | 5 | Platform config directories |
| shellexpand | 3 | Tilde (~) path expansion |

### photon (CLI only)

| Dependency | Version | Purpose |
|------------|---------|---------|
| clap | 4 (derive, env) | CLI argument parsing |

---

## Configuration System

The configuration system (`photon-core/src/config.rs`) implements all settings from the blueprint:

```toml
[general]
model_dir = "~/.photon/models"

[processing]
parallel_workers = 4
supported_formats = ["jpg", "jpeg", "png", "webp", "heic", "raw", "cr2", "nef", "arw"]

[pipeline]
buffer_size = 100
retry_attempts = 3
retry_delay_ms = 1000

[limits]
max_file_size_mb = 100
max_image_dimension = 10000
decode_timeout_ms = 5000
embed_timeout_ms = 30000
llm_timeout_ms = 60000

[embedding]
model = "siglip-base-patch16"
device = "metal"

[thumbnail]
enabled = true
size = 256
format = "webp"
quality = 80

[tagging]
min_confidence = 0.5
max_tags = 20
zero_shot_enabled = true

[output]
format = "json"
pretty = false
include_embedding = true

[logging]
level = "info"
format = "pretty"

[llm]
# Supports ollama, hyperbolic, anthropic, openai providers
```

### Config Loading

- **Default path:** Platform-specific (macOS: `~/Library/Application Support/com.photon.photon/config.toml`)
- **Fallback:** `~/.photon/config.toml`
- **Missing file:** Falls back to defaults (no error)
- **Tilde expansion:** Handled via `shellexpand` crate

---

## CLI Commands Implemented

### `photon --help`

```
Pure image processing pipeline for AI-powered tagging and embeddings

Usage: photon [OPTIONS] <COMMAND>

Commands:
  process  Process images and generate embeddings, tags, and metadata
  models   Manage AI models (download, list, etc.)
  config   View and manage configuration
  help     Print this message or the help of the given subcommand(s)

Options:
  -v, --verbose    Enable verbose (debug) logging
      --json-logs  Output logs in JSON format
  -h, --help       Print help
  -V, --version    Print version
```

### `photon process --help`

All flags from the blueprint are implemented:
- `-o, --output <FILE>` — Output file (defaults to stdout)
- `-f, --format <json|jsonl>` — Output format
- `-p, --parallel <N>` — Parallel workers (default: 4)
- `--skip-existing` — Skip already-processed images
- `--no-thumbnail` — Disable thumbnail generation
- `--no-description` — Disable LLM descriptions
- `--thumbnail-size <PX>` — Thumbnail size (default: 256)
- `--llm <ollama|hyperbolic|anthropic|openai>` — LLM provider
- `--llm-model <NAME>` — LLM model name

### `photon models <subcommand>`

- `download` — Download required models (stub, Phase 3)
- `list` — List installed models
- `path` — Show model directory path

### `photon config <subcommand>`

- `show` — Display current configuration (TOML format)
- `path` — Show config file path
- `init [--force]` — Initialize config file with defaults

---

## Error Types

The error system (`photon-core/src/error.rs`) provides granular, per-stage errors:

```rust
PhotonError           // Top-level error
├── Config(ConfigError)
│   ├── ReadError     // Failed to read config file
│   ├── ParseError    // Failed to parse TOML
│   └── ValidationError
├── Pipeline(PipelineError)
│   ├── Decode        // Image decoding failed
│   ├── Metadata      // EXIF extraction failed
│   ├── Embedding     // Embedding generation failed
│   ├── Tagging       // Tag generation failed
│   ├── Llm           // LLM call failed
│   ├── Timeout       // Operation timed out
│   ├── FileTooLarge  // File exceeds size limit
│   ├── ImageTooLarge // Image exceeds dimension limit
│   ├── UnsupportedFormat
│   └── FileNotFound
├── Io(std::io::Error)
└── Json(serde_json::Error)
```

All errors include relevant context (file path, stage, specific message).

---

## Output Types

The data types (`photon-core/src/types.rs`) match the blueprint:

```rust
ProcessedImage {
    file_path, file_name, content_hash,     // Identification
    width, height, format, file_size,       // Properties
    embedding: Vec<f32>,                    // 768-dim vector
    exif: Option<ExifData>,                 // EXIF metadata
    tags: Vec<Tag>,                         // Semantic tags
    description: Option<String>,            // LLM description
    thumbnail: Option<String>,              // Base64 WebP
    perceptual_hash: Option<String>,        // Similarity hash
}

ExifData {
    captured_at, camera_make, camera_model,
    gps_latitude, gps_longitude,
    iso, aperture, shutter_speed, focal_length, orientation
}

Tag {
    name: String,
    confidence: f32,  // 0.0 to 1.0
    category: Option<String>,  // "object", "scene", "color", "style"
}
```

---

## Logging

Logging (`photon/src/logging.rs`) uses tracing with:

- **Default:** INFO level, pretty format, output to stderr
- **Verbose (-v):** DEBUG level
- **JSON logs (--json-logs):** Structured JSON format
- **RUST_LOG:** Environment variable override supported

Example output:
```
2026-02-09T01:06:54.769Z DEBUG Photon v0.1.0
2026-02-09T01:06:54.769Z INFO  Processing input: "."
2026-02-09T01:06:54.769Z DEBUG Output format: json
2026-02-09T01:06:54.769Z DEBUG Parallel workers: 4
```

---

## Tests

8 unit tests implemented and passing:

| Test | Location | Purpose |
|------|----------|---------|
| `test_default_config` | config.rs | Verify default values match blueprint |
| `test_config_to_toml` | config.rs | Verify TOML serialization |
| `test_version` | lib.rs | Verify version constant |
| `test_photon_new` | lib.rs | Verify Photon struct initialization |
| `test_write_json` | output.rs | Verify JSON output |
| `test_write_jsonl` | output.rs | Verify JSONL output |
| `test_write_all_json_array` | output.rs | Verify array JSON output |
| `test_format_parse` | output.rs | Verify format string parsing |

---

## Verification Checklist

| Criteria | Status |
|----------|--------|
| `cargo build --release` succeeds | ✅ |
| `photon --help` displays usage | ✅ |
| `photon --version` displays version | ✅ |
| `photon process --help` shows all options | ✅ |
| `photon models path` outputs model directory | ✅ |
| `photon config path` outputs config path | ✅ |
| `photon config show` displays configuration | ✅ |
| Verbose logging works with `-v` | ✅ |
| Unit tests pass: `cargo test` | ✅ (8 tests) |
| Code formatted: `cargo fmt --check` | ✅ |
| Lints pass: `cargo clippy` | ✅ (1 expected warning) |

---

## Design Decisions

### Why separate crates?

- **photon-core:** Pure library, can be embedded in other Rust projects
- **photon:** Thin CLI wrapper, uses anyhow for ergonomic errors
- This matches the blueprint's goal of being "embeddable"

### Why tracing over log?

- Structured logging with spans (useful for pipeline stages in Phase 2+)
- JSON output format for machine parsing
- env-filter for runtime log level control

### Why thiserror + anyhow?

- **thiserror:** Typed errors in the library (good for embedding)
- **anyhow:** Ergonomic error handling in the CLI binary

### Config path strategy

- Uses `directories` crate for platform-appropriate paths
- macOS: `~/Library/Application Support/com.photon.photon/`
- Linux: `~/.config/photon/`
- Falls back to `~/.photon/` if detection fails

---

## Known Limitations

1. **Unused function warning:** `init_from_config` is defined but not yet used (will be used when config-based logging is needed)
2. **Stubs:** Process, models download commands are stubs (Phase 2+)

---

## Next Steps (Phase 2)

Phase 2 will implement the image pipeline:
- Image decoding with `image` crate
- EXIF extraction with `kamadak-exif`
- Content hashing with blake3
- Perceptual hashing with `image_hasher`
- Thumbnail generation
- Batch file discovery
- Input validation

**Milestone:** `photon process image.jpg` outputs metadata, hash, thumbnail
