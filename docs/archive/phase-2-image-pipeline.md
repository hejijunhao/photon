# Phase 2: Image Pipeline — Completion Report

> **Status:** ✅ Complete
> **Date:** 2026-02-09
> **Milestone:** `photon process image.jpg` outputs metadata, hash, thumbnail

---

## Summary

Phase 2 implements the core image processing pipeline: decoding images in various formats, extracting EXIF metadata, generating content and perceptual hashes, creating thumbnails, and handling batch file discovery. This is the foundation that all AI features (embedding, tagging, LLM descriptions) will build upon.

---

## What Was Built

### Pipeline Components

```
crates/photon-core/src/pipeline/
├── mod.rs           # Module exports
├── decode.rs        # Image decoding with timeout
├── metadata.rs      # EXIF extraction
├── hash.rs          # Content (blake3) + perceptual hashing
├── thumbnail.rs     # WebP thumbnail generation
├── discovery.rs     # File discovery with format filtering
├── validate.rs      # Input validation (size, format, magic bytes)
├── processor.rs     # Pipeline orchestration
└── channel.rs       # Bounded channels for backpressure
```

---

## Files Created/Modified

### New Files (Phase 2)

| File | Lines | Purpose |
|------|-------|---------|
| `pipeline/mod.rs` | 30 | Module exports and re-exports |
| `pipeline/decode.rs` | 135 | Image decoding with limits and timeout |
| `pipeline/metadata.rs` | 130 | EXIF extraction (camera, GPS, datetime, etc.) |
| `pipeline/hash.rs` | 75 | BLAKE3 content hash + perceptual hash |
| `pipeline/thumbnail.rs` | 90 | WebP thumbnail with base64 encoding |
| `pipeline/discovery.rs` | 90 | Recursive file discovery |
| `pipeline/validate.rs` | 160 | Magic byte validation, size checks |
| `pipeline/processor.rs` | 145 | Full pipeline orchestration |
| `pipeline/channel.rs` | 100 | Bounded async channels |
| `tests/fixtures/images/test.png` | — | Test fixture (1x1 PNG) |

### Modified Files

| File | Changes |
|------|---------|
| `photon-core/Cargo.toml` | Added image processing dependencies |
| `photon-core/src/lib.rs` | Export `pipeline` module and `ProcessOptions` |
| `photon/src/cli/process.rs` | Full implementation using `ImageProcessor` |

---

## Dependencies Added

| Dependency | Version | Purpose |
|------------|---------|---------|
| `image` | 0.25 | Image decoding (JPEG, PNG, WebP, GIF, TIFF, BMP) |
| `kamadak-exif` | 0.5 | EXIF metadata extraction |
| `blake3` | 1 | Fast cryptographic hashing for deduplication |
| `image_hasher` | 2 | Perceptual hashing for similarity detection |
| `base64` | 0.22 | Thumbnail encoding for JSON output |
| `walkdir` | 2 | Recursive directory traversal |

---

## Pipeline Stages

### 1. Validation (`validate.rs`)

Quick pre-flight checks before expensive operations:

```rust
Validator::validate(path)
  ├── Check file exists
  ├── Check file size < max_file_size_mb
  └── Check magic bytes (JPEG, PNG, WebP, GIF, BMP, TIFF, HEIC)
```

**Magic bytes detected:**
- JPEG: `FF D8 FF`
- PNG: `89 50 4E 47`
- GIF: `GIF8`
- WebP: `RIFF....WEBP`
- BMP: `BM`
- TIFF: `II` or `MM`
- HEIC/AVIF: `ftyp` at offset 4

### 2. Decoding (`decode.rs`)

```rust
ImageDecoder::decode(path)
  ├── File size validation
  ├── spawn_blocking (avoid blocking async runtime)
  ├── timeout(decode_timeout_ms)
  ├── Dimension validation (max_image_dimension)
  └── Return DecodedImage { image, format, width, height, file_size }
```

**Supported formats:** JPEG, PNG, WebP, GIF, TIFF, BMP, ICO, PNM, AVIF

### 3. Metadata Extraction (`metadata.rs`)

```rust
MetadataExtractor::extract(path) -> Option<ExifData>
  ├── DateTimeOriginal / DateTime
  ├── Make / Model (camera)
  ├── GPS coordinates (with N/S/E/W handling)
  ├── ISO, Aperture, Shutter Speed
  ├── Focal Length
  └── Orientation
```

**GPS conversion:** Degrees/minutes/seconds → decimal degrees with sign based on reference.

### 4. Hashing (`hash.rs`)

```rust
Hasher::content_hash(path) -> String
  └── BLAKE3, streamed in 64KB chunks
      → 64-character hex string

Hasher::perceptual_hash(image) -> String
  └── DoubleGradient algorithm, 16x16 hash
      → Base64 encoded

Hasher::perceptual_distance(hash1, hash2) -> Option<u32>
  └── Hamming distance (0 = identical)
```

### 5. Thumbnail Generation (`thumbnail.rs`)

```rust
ThumbnailGenerator::generate(image) -> Option<String>
  ├── Resize maintaining aspect ratio
  ├── Encode to WebP
  └── Base64 encode
```

**Configuration:**
- `thumbnail.enabled` — Enable/disable
- `thumbnail.size` — Max dimension (default: 256px)
- `thumbnail.quality` — WebP quality (default: 80)

### 6. File Discovery (`discovery.rs`)

```rust
FileDiscovery::discover(path) -> Vec<DiscoveredFile>
  ├── Single file: validate extension
  ├── Directory: recursive walk with symlink following
  ├── Filter by supported_formats config
  └── Sort by path (deterministic ordering)
```

### 7. Pipeline Orchestration (`processor.rs`)

```rust
ImageProcessor::process(path) -> Result<ProcessedImage>
  ├── validate()
  ├── decode()
  ├── MetadataExtractor::extract()
  ├── Hasher::content_hash()
  ├── Hasher::perceptual_hash()
  ├── ThumbnailGenerator::generate()
  └── Return ProcessedImage
```

With `ProcessOptions`:
- `skip_thumbnail` — Disable thumbnail
- `skip_perceptual_hash` — Disable perceptual hash

---

## CLI Implementation

### Single File Processing

```bash
photon process image.jpg
```

Output: Pretty-printed JSON to stdout

### Batch Processing

```bash
photon process ./photos/ --output results.jsonl --format jsonl
```

- Discovers all images recursively
- Processes each file
- Streams JSONL or writes JSON array
- Reports success/failure summary with rate

### Flags Working

| Flag | Effect |
|------|--------|
| `--no-thumbnail` | Excludes thumbnail from output |
| `--thumbnail-size 128` | Sets thumbnail max dimension |
| `-o output.json` | Writes to file instead of stdout |
| `-f jsonl` | Uses JSON Lines format |
| `-v` | Verbose mode with timing info |

---

## Output Format

```json
{
  "file_path": "tests/fixtures/images/test.png",
  "file_name": "test.png",
  "content_hash": "9aab4a27bde3bd1ce7e2b29b6e01aef4f4a35d604bbe592c0c66a8f0ed50847c",
  "width": 1,
  "height": 1,
  "format": "png",
  "file_size": 70,
  "embedding": [],
  "tags": [],
  "thumbnail": "UklGRuAAAABXRUJQVlA4TNM...",
  "perceptual_hash": "AAAAAAAAAAAAAAAAAAAAAAAA"
}
```

**Notes:**
- `embedding` is empty (Phase 3)
- `tags` is empty (Phase 4)
- `description` is omitted when null (Phase 5)
- `exif` is omitted when null (no EXIF data)
- `thumbnail` is omitted when disabled

---

## Tests

**25 tests passing:**

| Module | Tests |
|--------|-------|
| `config` | 2 (default config, TOML serialization) |
| `output` | 4 (JSON, JSONL, write_all, format parsing) |
| `pipeline::decode` | 1 (format_to_string) |
| `pipeline::metadata` | 1 (missing file handling) |
| `pipeline::hash` | 3 (consistency, distance, invalid hash) |
| `pipeline::thumbnail` | 3 (generation, disabled, bytes) |
| `pipeline::discovery` | 2 (is_supported, total_size) |
| `pipeline::validate` | 4 (magic bytes for JPEG, PNG, WebP, invalid) |
| `pipeline::channel` | 2 (bounded channel, pipeline stage) |
| `pipeline::processor` | 1 (default options) |
| `lib` | 2 (version, photon_new) |

---

## Verification Checklist

| Criteria | Status |
|----------|--------|
| `photon process image.jpg` outputs JSON | ✅ |
| JPEG, PNG, WebP formats decode | ✅ |
| Content hash is 64 hex characters (BLAKE3) | ✅ |
| Perceptual hash is base64 encoded | ✅ |
| Thumbnails are base64-encoded WebP | ✅ |
| `--no-thumbnail` flag works | ✅ |
| Verbose mode shows timing | ✅ |
| Batch processing with summary | ✅ |
| `cargo build --release` | ✅ |
| `cargo test` — 25 tests pass | ✅ |
| `cargo fmt --check` | ✅ |
| `cargo clippy` | ✅ (1 expected warning) |

---

## Performance Notes

- **Decoding:** Runs in `spawn_blocking` to avoid blocking async runtime
- **Hashing:** BLAKE3 streams in 64KB chunks (handles large files)
- **Timeout:** Configurable decode timeout (default: 5 seconds)
- **Memory:** Each image decoded, processed, then dropped before next

---

## Architecture Decisions

### Why spawn_blocking for decode?

The `image` crate performs CPU-intensive decoding. Running this on the Tokio runtime would block other async tasks. `spawn_blocking` moves it to a dedicated thread pool.

### Why BLAKE3 over SHA-256?

- 3-4x faster on modern CPUs
- Cryptographically secure
- 64-character hex output (same as SHA-256)
- Perfect for content deduplication

### Why DoubleGradient perceptual hash?

- More robust to resizing/cropping than simple average hash
- Good balance of accuracy and speed
- 16x16 hash size gives 256 bits of similarity data

### Why WebP thumbnails?

- Smaller file size than JPEG at same quality
- Supports transparency
- Modern format with good browser support
- `image` crate has built-in WebP encoding

---

## Known Limitations

1. **HEIC/RAW not yet supported** — Requires additional system libraries
2. **No parallel batch processing** — Sequential for now (Phase 6)
3. **No skip-existing** — Will be implemented in Phase 6
4. **EXIF GPS only works for photos with coordinates** — Gracefully skipped otherwise

---

## Next Steps (Phase 3)

Phase 3 will implement SigLIP embedding:
- ONNX Runtime setup with Metal acceleration
- SigLIP model download on first run
- Image preprocessing (resize, normalize)
- 768-dimensional embedding generation
- Batch processing with bounded channels

**Milestone:** `photon process image.jpg` outputs 768-dim embedding vector
