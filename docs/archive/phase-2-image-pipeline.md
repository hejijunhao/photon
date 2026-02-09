# Phase 2: Image Pipeline

> **Duration:** 2 weeks
> **Milestone:** `photon process image.jpg` outputs metadata, hash, thumbnail

---

## Overview

This phase implements the core image processing pipeline: decoding images in various formats, extracting EXIF metadata, generating content and perceptual hashes, creating thumbnails, and handling batch file discovery. This is the foundation that all AI features build upon.

---

## Prerequisites

- Phase 1 completed (CLI, config, logging, error types)
- Test images in various formats (JPEG, PNG, WebP, HEIC)

---

## Implementation Tasks

### 2.1 Image Decoding

**Goal:** Decode images from all supported formats into a unified in-memory representation.

**Steps:**

1. Add dependencies to `photon-core/Cargo.toml`:
   ```toml
   image = "0.25"
   # For HEIC/HEIF support (Apple formats)
   libheif-rs = { version = "1", optional = true }
   # For RAW formats
   rawloader = { version = "0.37", optional = true }

   [features]
   default = ["heic", "raw"]
   heic = ["libheif-rs"]
   raw = ["rawloader"]
   ```

2. Create `crates/photon-core/src/pipeline/mod.rs`:
   ```rust
   pub mod decode;
   pub mod metadata;
   pub mod hash;
   pub mod thumbnail;
   pub mod channel;

   pub use decode::ImageDecoder;
   pub use metadata::MetadataExtractor;
   pub use hash::Hasher;
   pub use thumbnail::ThumbnailGenerator;
   ```

3. Create `crates/photon-core/src/pipeline/decode.rs`:
   ```rust
   use image::{DynamicImage, ImageFormat};
   use std::path::Path;
   use std::time::Duration;
   use tokio::time::timeout;

   use crate::config::LimitsConfig;
   use crate::error::PipelineError;

   pub struct ImageDecoder {
       limits: LimitsConfig,
   }

   pub struct DecodedImage {
       pub image: DynamicImage,
       pub format: ImageFormat,
       pub width: u32,
       pub height: u32,
       pub file_size: u64,
   }

   impl ImageDecoder {
       pub fn new(limits: LimitsConfig) -> Self {
           Self { limits }
       }

       /// Decode an image with validation and timeout
       pub async fn decode(&self, path: &Path) -> Result<DecodedImage, PipelineError> {
           // Check file size first
           let metadata = std::fs::metadata(path).map_err(|e| {
               PipelineError::Decode {
                   path: path.to_path_buf(),
                   message: format!("Cannot read file: {}", e),
               }
           })?;

           let file_size = metadata.len();
           let max_size = self.limits.max_file_size_mb * 1024 * 1024;

           if file_size > max_size {
               return Err(PipelineError::FileTooLarge {
                   path: path.to_path_buf(),
                   size_mb: file_size / (1024 * 1024),
                   max_mb: self.limits.max_file_size_mb,
               });
           }

           // Decode with timeout
           let path_owned = path.to_path_buf();
           let timeout_duration = Duration::from_millis(self.limits.decode_timeout_ms);

           let decode_result = timeout(timeout_duration, async {
               tokio::task::spawn_blocking(move || {
                   Self::decode_sync(&path_owned)
               }).await
           }).await;

           match decode_result {
               Ok(Ok(Ok(decoded))) => {
                   // Validate dimensions
                   if decoded.width > self.limits.max_image_dimension
                       || decoded.height > self.limits.max_image_dimension
                   {
                       return Err(PipelineError::ImageTooLarge {
                           path: path.to_path_buf(),
                           width: decoded.width,
                           height: decoded.height,
                           max_dim: self.limits.max_image_dimension,
                       });
                   }
                   Ok(decoded)
               }
               Ok(Ok(Err(e))) => Err(e),
               Ok(Err(e)) => Err(PipelineError::Decode {
                   path: path.to_path_buf(),
                   message: format!("Task join error: {}", e),
               }),
               Err(_) => Err(PipelineError::Timeout {
                   path: path.to_path_buf(),
                   stage: "decode".to_string(),
                   timeout_ms: self.limits.decode_timeout_ms,
               }),
           }
       }

       fn decode_sync(path: &Path) -> Result<DecodedImage, PipelineError> {
           // Detect format
           let format = image::ImageFormat::from_path(path).map_err(|e| {
               PipelineError::Decode {
                   path: path.to_path_buf(),
                   message: format!("Unknown format: {}", e),
               }
           })?;

           // Handle special formats
           let image = match format {
               // HEIC needs special handling
               #[cfg(feature = "heic")]
               ImageFormat::Avif => Self::decode_heic(path)?,

               // Standard formats via image crate
               _ => image::open(path).map_err(|e| PipelineError::Decode {
                   path: path.to_path_buf(),
                   message: e.to_string(),
               })?,
           };

           let (width, height) = image.dimensions();
           let file_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);

           Ok(DecodedImage {
               image,
               format,
               width,
               height,
               file_size,
           })
       }

       #[cfg(feature = "heic")]
       fn decode_heic(path: &Path) -> Result<DynamicImage, PipelineError> {
           // HEIC decoding implementation
           todo!("Implement HEIC decoding with libheif-rs")
       }
   }
   ```

4. Add format detection helper:
   ```rust
   pub fn format_to_string(format: ImageFormat) -> String {
       match format {
           ImageFormat::Jpeg => "jpeg".to_string(),
           ImageFormat::Png => "png".to_string(),
           ImageFormat::WebP => "webp".to_string(),
           ImageFormat::Gif => "gif".to_string(),
           ImageFormat::Tiff => "tiff".to_string(),
           _ => "unknown".to_string(),
       }
   }
   ```

**Acceptance Criteria:**
- [ ] JPEG, PNG, WebP decode successfully
- [ ] HEIC decodes on macOS (with feature flag)
- [ ] File size limits are enforced
- [ ] Dimension limits are enforced
- [ ] Decode timeout works
- [ ] Corrupt images return clear errors

---

### 2.2 EXIF Metadata Extraction

**Goal:** Extract camera metadata, timestamps, GPS coordinates from images.

**Steps:**

1. Add dependency:
   ```toml
   kamadak-exif = "0.5"
   ```

2. Create `crates/photon-core/src/pipeline/metadata.rs`:
   ```rust
   use exif::{In, Reader, Tag, Value};
   use std::fs::File;
   use std::io::BufReader;
   use std::path::Path;

   use crate::types::ExifData;

   pub struct MetadataExtractor;

   impl MetadataExtractor {
       pub fn extract(path: &Path) -> Option<ExifData> {
           let file = File::open(path).ok()?;
           let reader = Reader::new().read_from_container(
               &mut BufReader::new(file)
           ).ok()?;

           Some(ExifData {
               captured_at: Self::get_datetime(&reader),
               camera_make: Self::get_string(&reader, Tag::Make),
               camera_model: Self::get_string(&reader, Tag::Model),
               gps_latitude: Self::get_gps_coord(&reader, Tag::GPSLatitude, Tag::GPSLatitudeRef),
               gps_longitude: Self::get_gps_coord(&reader, Tag::GPSLongitude, Tag::GPSLongitudeRef),
               iso: Self::get_u32(&reader, Tag::PhotographicSensitivity),
               aperture: Self::get_aperture(&reader),
           })
       }

       fn get_string(reader: &exif::Exif, tag: Tag) -> Option<String> {
           reader.get_field(tag, In::PRIMARY)
               .and_then(|f| f.display_value().to_string().into())
       }

       fn get_u32(reader: &exif::Exif, tag: Tag) -> Option<u32> {
           reader.get_field(tag, In::PRIMARY)
               .and_then(|f| match &f.value {
                   Value::Short(v) => v.first().map(|&x| x as u32),
                   Value::Long(v) => v.first().copied(),
                   _ => None,
               })
       }

       fn get_datetime(reader: &exif::Exif) -> Option<String> {
           reader.get_field(Tag::DateTimeOriginal, In::PRIMARY)
               .or_else(|| reader.get_field(Tag::DateTime, In::PRIMARY))
               .map(|f| f.display_value().to_string())
       }

       fn get_gps_coord(reader: &exif::Exif, coord_tag: Tag, ref_tag: Tag) -> Option<f64> {
           let coord = reader.get_field(coord_tag, In::PRIMARY)?;
           let reference = reader.get_field(ref_tag, In::PRIMARY)?;

           // Parse degrees, minutes, seconds from EXIF rational values
           let degrees = Self::parse_gps_rationals(&coord.value)?;
           let ref_str = reference.display_value().to_string();

           // Apply sign based on reference (N/S for lat, E/W for lon)
           let sign = if ref_str.contains('S') || ref_str.contains('W') {
               -1.0
           } else {
               1.0
           };

           Some(sign * degrees)
       }

       fn parse_gps_rationals(value: &Value) -> Option<f64> {
           match value {
               Value::Rational(rationals) if rationals.len() >= 3 => {
                   let degrees = rationals[0].to_f64();
                   let minutes = rationals[1].to_f64();
                   let seconds = rationals[2].to_f64();
                   Some(degrees + minutes / 60.0 + seconds / 3600.0)
               }
               _ => None,
           }
       }

       fn get_aperture(reader: &exif::Exif) -> Option<String> {
           reader.get_field(Tag::FNumber, In::PRIMARY)
               .map(|f| format!("f/{}", f.display_value()))
       }
   }
   ```

**Acceptance Criteria:**
- [ ] Extracts camera make/model
- [ ] Extracts capture date/time
- [ ] Extracts GPS coordinates correctly (with proper N/S/E/W handling)
- [ ] Extracts ISO and aperture
- [ ] Returns `None` gracefully for images without EXIF
- [ ] Handles corrupted EXIF data without panicking

---

### 2.3 Content and Perceptual Hashing

**Goal:** Generate blake3 content hash for deduplication and perceptual hash for similarity.

**Steps:**

1. Add dependencies:
   ```toml
   blake3 = "1"
   image_hasher = "2"
   ```

2. Create `crates/photon-core/src/pipeline/hash.rs`:
   ```rust
   use blake3::Hasher as Blake3Hasher;
   use image::DynamicImage;
   use image_hasher::{HashAlg, HasherConfig, ImageHash};
   use std::fs::File;
   use std::io::{BufReader, Read};
   use std::path::Path;

   pub struct Hasher;

   impl Hasher {
       /// Generate blake3 hash of file contents for exact deduplication
       pub fn content_hash(path: &Path) -> std::io::Result<String> {
           let file = File::open(path)?;
           let mut reader = BufReader::new(file);
           let mut hasher = Blake3Hasher::new();

           let mut buffer = [0u8; 65536]; // 64KB buffer
           loop {
               let bytes_read = reader.read(&mut buffer)?;
               if bytes_read == 0 {
                   break;
               }
               hasher.update(&buffer[..bytes_read]);
           }

           Ok(hasher.finalize().to_hex().to_string())
       }

       /// Generate perceptual hash for near-duplicate detection
       pub fn perceptual_hash(image: &DynamicImage) -> String {
           let hasher = HasherConfig::new()
               .hash_alg(HashAlg::DoubleGradient)
               .hash_size(16, 16)
               .to_hasher();

           let hash = hasher.hash_image(image);
           hash.to_base64()
       }

       /// Compare two perceptual hashes, returns distance (0 = identical)
       pub fn perceptual_distance(hash1: &str, hash2: &str) -> Option<u32> {
           let h1 = ImageHash::<Vec<u8>>::from_base64(hash1).ok()?;
           let h2 = ImageHash::<Vec<u8>>::from_base64(hash2).ok()?;
           Some(h1.dist(&h2))
       }
   }

   #[cfg(test)]
   mod tests {
       use super::*;

       #[test]
       fn test_perceptual_hash_consistency() {
           // Same image should produce same hash
           let img = DynamicImage::new_rgb8(100, 100);
           let hash1 = Hasher::perceptual_hash(&img);
           let hash2 = Hasher::perceptual_hash(&img);
           assert_eq!(hash1, hash2);
       }
   }
   ```

**Acceptance Criteria:**
- [ ] Content hash is deterministic (same file = same hash)
- [ ] Perceptual hash is stable for same image
- [ ] Perceptual hash similarity works (similar images have low distance)
- [ ] Large files hash efficiently (streaming, not loading entire file)

---

### 2.4 Thumbnail Generation

**Goal:** Generate WebP thumbnails at configurable size.

**Steps:**

1. Create `crates/photon-core/src/pipeline/thumbnail.rs`:
   ```rust
   use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
   use image::{DynamicImage, ImageFormat};
   use std::io::Cursor;

   use crate::config::ThumbnailConfig;

   pub struct ThumbnailGenerator {
       config: ThumbnailConfig,
   }

   impl ThumbnailGenerator {
       pub fn new(config: ThumbnailConfig) -> Self {
           Self { config }
       }

       /// Generate a thumbnail and return as base64-encoded WebP
       pub fn generate(&self, image: &DynamicImage) -> Option<String> {
           if !self.config.enabled {
               return None;
           }

           // Resize maintaining aspect ratio
           let thumbnail = image.thumbnail(
               self.config.size,
               self.config.size,
           );

           // Encode to WebP
           let mut buffer = Cursor::new(Vec::new());
           thumbnail.write_to(&mut buffer, ImageFormat::WebP).ok()?;

           // Return as base64
           Some(BASE64.encode(buffer.into_inner()))
       }

       /// Generate thumbnail and return raw bytes (for direct file output)
       pub fn generate_bytes(&self, image: &DynamicImage) -> Option<Vec<u8>> {
           if !self.config.enabled {
               return None;
           }

           let thumbnail = image.thumbnail(
               self.config.size,
               self.config.size,
           );

           let mut buffer = Cursor::new(Vec::new());
           thumbnail.write_to(&mut buffer, ImageFormat::WebP).ok()?;

           Some(buffer.into_inner())
       }
   }
   ```

2. Add base64 dependency:
   ```toml
   base64 = "0.22"
   ```

**Acceptance Criteria:**
- [ ] Thumbnails maintain aspect ratio
- [ ] Thumbnails are WebP format
- [ ] Configurable size works
- [ ] Returns `None` when disabled
- [ ] Base64 encoding works correctly

---

### 2.5 Batch File Discovery

**Goal:** Discover images in directories with format filtering and validation.

**Steps:**

1. Create `crates/photon-core/src/pipeline/discovery.rs`:
   ```rust
   use std::path::{Path, PathBuf};
   use walkdir::WalkDir;

   use crate::config::ProcessingConfig;

   pub struct FileDiscovery {
       config: ProcessingConfig,
   }

   pub struct DiscoveredFile {
       pub path: PathBuf,
       pub size: u64,
   }

   impl FileDiscovery {
       pub fn new(config: ProcessingConfig) -> Self {
           Self { config }
       }

       /// Discover all supported image files in a path (file or directory)
       pub fn discover(&self, path: &Path) -> Vec<DiscoveredFile> {
           if path.is_file() {
               if self.is_supported(path) {
                   if let Ok(meta) = std::fs::metadata(path) {
                       return vec![DiscoveredFile {
                           path: path.to_path_buf(),
                           size: meta.len(),
                       }];
                   }
               }
               return vec![];
           }

           let mut files = Vec::new();

           for entry in WalkDir::new(path)
               .follow_links(true)
               .into_iter()
               .filter_map(|e| e.ok())
           {
               let path = entry.path();
               if path.is_file() && self.is_supported(path) {
                   if let Ok(meta) = entry.metadata() {
                       files.push(DiscoveredFile {
                           path: path.to_path_buf(),
                           size: meta.len(),
                       });
                   }
               }
           }

           // Sort by path for deterministic ordering
           files.sort_by(|a, b| a.path.cmp(&b.path));
           files
       }

       fn is_supported(&self, path: &Path) -> bool {
           path.extension()
               .and_then(|ext| ext.to_str())
               .map(|ext| {
                   let ext_lower = ext.to_lowercase();
                   self.config.supported_formats.iter()
                       .any(|fmt| fmt.to_lowercase() == ext_lower)
               })
               .unwrap_or(false)
       }
   }
   ```

2. Add walkdir dependency:
   ```toml
   walkdir = "2"
   ```

**Acceptance Criteria:**
- [ ] Discovers files in nested directories
- [ ] Filters by supported formats
- [ ] Works with single file input
- [ ] Follows symlinks
- [ ] Provides file size for pre-filtering

---

### 2.6 Input Validation

**Goal:** Validate files before processing (size, format, corruption check).

**Steps:**

1. Create `crates/photon-core/src/pipeline/validate.rs`:
   ```rust
   use std::path::Path;

   use crate::config::LimitsConfig;
   use crate::error::PipelineError;

   pub struct Validator {
       limits: LimitsConfig,
   }

   impl Validator {
       pub fn new(limits: LimitsConfig) -> Self {
           Self { limits }
       }

       /// Quick validation before full decode
       pub fn validate(&self, path: &Path) -> Result<(), PipelineError> {
           // Check file exists and is readable
           if !path.exists() {
               return Err(PipelineError::Decode {
                   path: path.to_path_buf(),
                   message: "File does not exist".to_string(),
               });
           }

           // Check file size
           let metadata = std::fs::metadata(path).map_err(|e| {
               PipelineError::Decode {
                   path: path.to_path_buf(),
                   message: format!("Cannot read metadata: {}", e),
               }
           })?;

           let size_mb = metadata.len() / (1024 * 1024);
           if size_mb > self.limits.max_file_size_mb {
               return Err(PipelineError::FileTooLarge {
                   path: path.to_path_buf(),
                   size_mb,
                   max_mb: self.limits.max_file_size_mb,
               });
           }

           // Quick format check (read first few bytes)
           self.check_magic_bytes(path)?;

           Ok(())
       }

       fn check_magic_bytes(&self, path: &Path) -> Result<(), PipelineError> {
           use std::io::Read;

           let mut file = std::fs::File::open(path).map_err(|e| {
               PipelineError::Decode {
                   path: path.to_path_buf(),
                   message: format!("Cannot open file: {}", e),
               }
           })?;

           let mut header = [0u8; 12];
           let bytes_read = file.read(&mut header).unwrap_or(0);

           if bytes_read < 4 {
               return Err(PipelineError::Decode {
                   path: path.to_path_buf(),
                   message: "File too small".to_string(),
               });
           }

           // Check common format signatures
           let is_valid = matches!(
               &header[..4],
               // JPEG
               [0xFF, 0xD8, 0xFF, _] |
               // PNG
               [0x89, b'P', b'N', b'G'] |
               // GIF
               [b'G', b'I', b'F', b'8'] |
               // WebP (RIFF....WEBP)
               [b'R', b'I', b'F', b'F']
           ) || (
               // HEIC/HEIF (ftyp box)
               bytes_read >= 12 && &header[4..8] == b"ftyp"
           );

           if !is_valid {
               return Err(PipelineError::Decode {
                   path: path.to_path_buf(),
                   message: "Unrecognized file format".to_string(),
               });
           }

           Ok(())
       }
   }
   ```

**Acceptance Criteria:**
- [ ] Validates file existence
- [ ] Validates file size before loading
- [ ] Validates file format via magic bytes
- [ ] Returns specific error messages

---

### 2.7 Pipeline Orchestration (Single Image)

**Goal:** Wire together all components into a complete single-image pipeline.

**Steps:**

1. Create `crates/photon-core/src/pipeline/processor.rs`:
   ```rust
   use std::path::Path;

   use crate::config::Config;
   use crate::error::Result;
   use crate::types::ProcessedImage;

   use super::{
       decode::ImageDecoder,
       metadata::MetadataExtractor,
       hash::Hasher,
       thumbnail::ThumbnailGenerator,
       validate::Validator,
   };

   pub struct ImageProcessor {
       decoder: ImageDecoder,
       thumbnail_gen: ThumbnailGenerator,
       validator: Validator,
   }

   impl ImageProcessor {
       pub fn new(config: &Config) -> Self {
           Self {
               decoder: ImageDecoder::new(config.limits.clone()),
               thumbnail_gen: ThumbnailGenerator::new(config.thumbnail.clone()),
               validator: Validator::new(config.limits.clone()),
           }
       }

       /// Process a single image through the full pipeline
       pub async fn process(&self, path: &Path) -> Result<ProcessedImage> {
           tracing::debug!("Processing: {:?}", path);

           // Validate
           self.validator.validate(path)?;

           // Decode
           let decoded = self.decoder.decode(path).await?;

           // Extract metadata
           let exif = MetadataExtractor::extract(path);

           // Generate hashes
           let content_hash = Hasher::content_hash(path)
               .map_err(|e| crate::error::PipelineError::Decode {
                   path: path.to_path_buf(),
                   message: format!("Hash error: {}", e),
               })?;
           let perceptual_hash = Some(Hasher::perceptual_hash(&decoded.image));

           // Generate thumbnail
           let thumbnail = self.thumbnail_gen.generate(&decoded.image);

           let file_name = path.file_name()
               .and_then(|n| n.to_str())
               .unwrap_or("unknown")
               .to_string();

           Ok(ProcessedImage {
               file_path: path.to_path_buf(),
               file_name,
               content_hash,
               width: decoded.width,
               height: decoded.height,
               format: super::decode::format_to_string(decoded.format),
               file_size: decoded.file_size,
               embedding: vec![], // Placeholder - Phase 3
               exif,
               tags: vec![], // Placeholder - Phase 4
               description: None, // Placeholder - Phase 5
               thumbnail,
               perceptual_hash,
           })
       }
   }
   ```

2. Update CLI to use processor:
   ```rust
   // In cli/process.rs
   pub async fn execute(args: ProcessArgs) -> anyhow::Result<()> {
       let config = Config::load()?;
       let processor = ImageProcessor::new(&config);

       if args.input.is_file() {
           let result = processor.process(&args.input).await?;
           // Output result
           let output = serde_json::to_string_pretty(&result)?;
           println!("{}", output);
       } else {
           // Batch processing - Phase 6
           todo!()
       }

       Ok(())
   }
   ```

**Acceptance Criteria:**
- [ ] Single image processes end-to-end
- [ ] All fields populated (except embedding, tags, description)
- [ ] JSON output is valid and complete
- [ ] Errors are properly propagated and logged

---

### 2.8 Bounded Channels (Backpressure)

**Goal:** Implement async channels with backpressure for batch processing.

**Steps:**

1. Create `crates/photon-core/src/pipeline/channel.rs`:
   ```rust
   use tokio::sync::mpsc;

   use crate::config::PipelineConfig;

   /// Create a bounded channel pair with configured buffer size
   pub fn bounded_channel<T>(config: &PipelineConfig) -> (mpsc::Sender<T>, mpsc::Receiver<T>) {
       mpsc::channel(config.buffer_size)
   }

   /// Pipeline stage that processes items with backpressure
   pub struct PipelineStage<I, O> {
       input: mpsc::Receiver<I>,
       output: mpsc::Sender<O>,
   }

   impl<I, O> PipelineStage<I, O> {
       pub fn new(input: mpsc::Receiver<I>, output: mpsc::Sender<O>) -> Self {
           Self { input, output }
       }

       /// Run the stage with a processing function
       pub async fn run<F, Fut>(mut self, f: F)
       where
           F: Fn(I) -> Fut,
           Fut: std::future::Future<Output = Option<O>>,
       {
           while let Some(item) = self.input.recv().await {
               if let Some(result) = f(item).await {
                   if self.output.send(result).await.is_err() {
                       break; // Downstream closed
                   }
               }
           }
       }
   }
   ```

2. This will be fully utilized in Phase 6 for parallel batch processing.

**Acceptance Criteria:**
- [ ] Channel buffer size is configurable
- [ ] Sender blocks when buffer is full (backpressure)
- [ ] Clean shutdown when receiver is dropped

---

## Integration Test

Create `tests/integration/pipeline_test.rs`:

```rust
use photon_core::{Config, ImageProcessor};
use std::path::Path;

#[tokio::test]
async fn test_single_image_processing() {
    let config = Config::default();
    let processor = ImageProcessor::new(&config);

    let result = processor.process(Path::new("tests/fixtures/images/test.jpg")).await;

    assert!(result.is_ok());
    let image = result.unwrap();

    assert!(!image.content_hash.is_empty());
    assert!(image.width > 0);
    assert!(image.height > 0);
    assert_eq!(image.format, "jpeg");
}

#[tokio::test]
async fn test_corrupt_image_handling() {
    let config = Config::default();
    let processor = ImageProcessor::new(&config);

    let result = processor.process(Path::new("tests/fixtures/images/corrupt.jpg")).await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_exif_extraction() {
    let config = Config::default();
    let processor = ImageProcessor::new(&config);

    let result = processor.process(Path::new("tests/fixtures/images/with_exif.jpg")).await;

    let image = result.unwrap();
    assert!(image.exif.is_some());
    let exif = image.exif.unwrap();
    assert!(exif.camera_make.is_some());
}
```

---

## Verification Checklist

Before moving to Phase 3, verify:

- [ ] `photon process image.jpg` outputs JSON with all metadata fields
- [ ] JPEG, PNG, WebP formats decode correctly
- [ ] EXIF extraction works on photos with metadata
- [ ] Content hash is 64 hex characters (blake3)
- [ ] Perceptual hash is base64 encoded
- [ ] Thumbnails are base64-encoded WebP
- [ ] File size limits trigger appropriate errors
- [ ] Corrupt images produce clear error messages
- [ ] `--no-thumbnail` flag works
- [ ] All integration tests pass
- [ ] Memory usage is reasonable for large images

---

## Files Created/Modified

```
crates/photon-core/src/
├── pipeline/
│   ├── mod.rs           # Module exports
│   ├── decode.rs        # Image decoding
│   ├── metadata.rs      # EXIF extraction
│   ├── hash.rs          # Content + perceptual hashing
│   ├── thumbnail.rs     # Thumbnail generation
│   ├── discovery.rs     # File discovery
│   ├── validate.rs      # Input validation
│   ├── processor.rs     # Pipeline orchestration
│   └── channel.rs       # Bounded channels

tests/
├── integration/
│   └── pipeline_test.rs
└── fixtures/
    └── images/
        ├── test.jpg
        ├── test.png
        ├── with_exif.jpg
        └── corrupt.jpg
```

---

## Notes

- Keep decode operations in `spawn_blocking` to avoid blocking the async runtime
- Use streaming for hash computation on large files
- Consider memory usage: don't hold multiple full images in memory simultaneously
- HEIC support may require system libraries on Linux (libheif)
- Test with real camera photos for EXIF parsing edge cases
