# Changelog

All notable changes to Photon are documented here.

---

## Index

- **[0.2.0](#020---2026-02-09)** — Image processing pipeline: decode, EXIF, hashing, thumbnails
- **[0.1.0](#010---2026-02-09)** — Project foundation: CLI, configuration, logging, error handling

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

[0.2.0]: https://github.com/crimsonsun/photon/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/crimsonsun/photon/releases/tag/v0.1.0
