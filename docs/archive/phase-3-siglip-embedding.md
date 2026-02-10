# Phase 3: SigLIP Embedding

> **Duration:** 2 weeks
> **Milestone:** `photon process image.jpg` outputs 768-dim embedding vector

---

## Overview

This phase integrates SigLIP (Sigmoid Loss for Language-Image Pre-training) for generating semantic embeddings from images. These embeddings power similarity search and zero-shot classification. The model runs locally via ONNX Runtime with Metal acceleration on Apple Silicon.

---

## Prerequisites

- Phase 2 completed (image pipeline working)
- Understanding of ONNX model inference
- SigLIP ONNX model files (will be downloaded)

---

## Background: SigLIP

**What is SigLIP?**
- Successor to CLIP, developed by Google Research
- Trained on image-text pairs to learn visual concepts
- Produces 768-dimensional embedding vectors
- Better zero-shot performance than CLIP

**Why ONNX?**
- Run models without Python/PyTorch dependency
- Native Rust bindings via `ort` crate
- Hardware acceleration (Metal, CUDA, CoreML)
- Single static binary distribution

---

## Implementation Tasks

### 3.1 ONNX Runtime Setup

**Goal:** Configure ONNX Runtime with Metal acceleration for Apple Silicon.

**Steps:**

1. Add dependencies to `photon-core/Cargo.toml`:
   ```toml
   ort = { version = "2", features = ["load-dynamic"] }
   ndarray = "0.16"
   ```

2. Create `crates/photon-core/src/embedding/mod.rs`:
   ```rust
   pub mod siglip;
   pub mod preprocess;

   pub use siglip::SigLipEmbedder;
   ```

3. Initialize ONNX Runtime in `crates/photon-core/src/embedding/siglip.rs`:
   ```rust
   use ort::{Environment, Session, SessionBuilder, Value};
   use ndarray::{Array, Array4};
   use std::path::Path;
   use std::sync::Arc;

   use crate::config::EmbeddingConfig;
   use crate::error::PipelineError;

   pub struct SigLipEmbedder {
       session: Session,
       config: EmbeddingConfig,
   }

   impl SigLipEmbedder {
       /// Initialize SigLIP with ONNX Runtime
       pub fn new(config: &EmbeddingConfig, model_path: &Path) -> Result<Self, PipelineError> {
           tracing::info!("Loading SigLIP model from {:?}", model_path);

           // Initialize ONNX environment
           let environment = Environment::builder()
               .with_name("photon")
               .with_execution_providers([
                   // Try Metal first (Apple Silicon)
                   #[cfg(target_os = "macos")]
                   ort::CoreMLExecutionProvider::default().build(),
                   // Fall back to CPU
                   ort::CPUExecutionProvider::default().build(),
               ])
               .build()
               .map_err(|e| PipelineError::Embedding {
                   path: model_path.to_path_buf(),
                   message: format!("Failed to create ONNX environment: {}", e),
               })?;

           // Load model
           let visual_model_path = model_path.join("visual.onnx");
           let session = SessionBuilder::new(&environment)?
               .with_optimization_level(ort::GraphOptimizationLevel::Level3)?
               .with_intra_threads(4)?
               .with_model_from_file(&visual_model_path)
               .map_err(|e| PipelineError::Embedding {
                   path: visual_model_path.clone(),
                   message: format!("Failed to load model: {}", e),
               })?;

           tracing::info!("SigLIP model loaded successfully");
           tracing::debug!("Execution providers: {:?}", session.execution_providers());

           Ok(Self {
               session,
               config: config.clone(),
           })
       }

       /// Check if using GPU acceleration
       pub fn is_accelerated(&self) -> bool {
           self.session
               .execution_providers()
               .iter()
               .any(|p| p != "CPUExecutionProvider")
       }
   }
   ```

**Acceptance Criteria:**
- [ ] ONNX Runtime initializes without errors
- [ ] Metal/CoreML is detected on Apple Silicon
- [ ] Falls back to CPU gracefully on other hardware
- [ ] Model file loading works

---

### 3.2 Model Download System

**Goal:** Download SigLIP ONNX model on first run, with progress indication.

**Steps:**

1. Add download dependencies:
   ```toml
   reqwest = { version = "0.12", features = ["stream"] }
   futures-util = "0.3"
   indicatif = "0.17"
   sha2 = "0.10"
   ```

2. Create `crates/photon-core/src/models/mod.rs`:
   ```rust
   pub mod download;
   pub mod registry;

   pub use download::ModelDownloader;
   pub use registry::ModelRegistry;
   ```

3. Create model registry:
   ```rust
   // crates/photon-core/src/models/registry.rs

   use std::collections::HashMap;

   pub struct ModelInfo {
       pub name: String,
       pub version: String,
       pub files: Vec<ModelFile>,
       pub total_size: u64,
   }

   pub struct ModelFile {
       pub name: String,
       pub url: String,
       pub sha256: String,
       pub size: u64,
   }

   pub struct ModelRegistry {
       models: HashMap<String, ModelInfo>,
   }

   impl ModelRegistry {
       pub fn new() -> Self {
           let mut models = HashMap::new();

           // SigLIP base model
           models.insert(
               "siglip-base-patch16".to_string(),
               ModelInfo {
                   name: "siglip-base-patch16".to_string(),
                   version: "1.0.0".to_string(),
                   files: vec![
                       ModelFile {
                           name: "visual.onnx".to_string(),
                           url: "https://huggingface.co/photon-models/siglip-base-patch16-onnx/resolve/main/visual.onnx".to_string(),
                           sha256: "abc123...".to_string(), // Real checksum
                           size: 350_000_000, // ~350MB
                       },
                       ModelFile {
                           name: "textual.onnx".to_string(),
                           url: "https://huggingface.co/photon-models/siglip-base-patch16-onnx/resolve/main/textual.onnx".to_string(),
                           sha256: "def456...".to_string(),
                           size: 150_000_000, // ~150MB
                       },
                   ],
                   total_size: 500_000_000,
               },
           );

           Self { models }
       }

       pub fn get(&self, name: &str) -> Option<&ModelInfo> {
           self.models.get(name)
       }

       pub fn list(&self) -> Vec<&ModelInfo> {
           self.models.values().collect()
       }
   }
   ```

4. Create downloader:
   ```rust
   // crates/photon-core/src/models/download.rs

   use futures_util::StreamExt;
   use indicatif::{ProgressBar, ProgressStyle};
   use sha2::{Sha256, Digest};
   use std::io::Write;
   use std::path::Path;
   use tokio::fs::File;
   use tokio::io::AsyncWriteExt;

   use super::registry::{ModelFile, ModelInfo};

   pub struct ModelDownloader {
       client: reqwest::Client,
   }

   impl ModelDownloader {
       pub fn new() -> Self {
           Self {
               client: reqwest::Client::new(),
           }
       }

       /// Download a model to the specified directory
       pub async fn download(
           &self,
           model: &ModelInfo,
           target_dir: &Path,
       ) -> Result<(), DownloadError> {
           // Create target directory
           tokio::fs::create_dir_all(target_dir).await?;

           println!("Downloading {} ({} files, {:.1} MB total)",
               model.name,
               model.files.len(),
               model.total_size as f64 / 1_000_000.0
           );

           for file in &model.files {
               self.download_file(file, target_dir).await?;
           }

           println!("Download complete!");
           Ok(())
       }

       async fn download_file(
           &self,
           file: &ModelFile,
           target_dir: &Path,
       ) -> Result<(), DownloadError> {
           let target_path = target_dir.join(&file.name);

           // Check if already downloaded with correct checksum
           if target_path.exists() {
               if self.verify_checksum(&target_path, &file.sha256).await? {
                   println!("  {} already downloaded", file.name);
                   return Ok(());
               }
           }

           // Create progress bar
           let pb = ProgressBar::new(file.size);
           pb.set_style(ProgressStyle::default_bar()
               .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
               .unwrap()
               .progress_chars("#>-"));
           pb.set_message(file.name.clone());

           // Download with streaming
           let response = self.client.get(&file.url).send().await?;
           let mut stream = response.bytes_stream();

           let mut dest = File::create(&target_path).await?;
           let mut hasher = Sha256::new();
           let mut downloaded = 0u64;

           while let Some(chunk) = stream.next().await {
               let chunk = chunk?;
               dest.write_all(&chunk).await?;
               hasher.update(&chunk);
               downloaded += chunk.len() as u64;
               pb.set_position(downloaded);
           }

           pb.finish_with_message(format!("{} ✓", file.name));

           // Verify checksum
           let checksum = format!("{:x}", hasher.finalize());
           if checksum != file.sha256 {
               tokio::fs::remove_file(&target_path).await?;
               return Err(DownloadError::ChecksumMismatch {
                   expected: file.sha256.clone(),
                   actual: checksum,
               });
           }

           Ok(())
       }

       async fn verify_checksum(&self, path: &Path, expected: &str) -> Result<bool, DownloadError> {
           let data = tokio::fs::read(path).await?;
           let mut hasher = Sha256::new();
           hasher.update(&data);
           let checksum = format!("{:x}", hasher.finalize());
           Ok(checksum == expected)
       }
   }

   #[derive(Debug, thiserror::Error)]
   pub enum DownloadError {
       #[error("Network error: {0}")]
       Network(#[from] reqwest::Error),
       #[error("IO error: {0}")]
       Io(#[from] std::io::Error),
       #[error("Checksum mismatch: expected {expected}, got {actual}")]
       ChecksumMismatch { expected: String, actual: String },
   }
   ```

5. Implement CLI model commands:
   ```rust
   // In cli/models.rs

   pub async fn execute(args: ModelsArgs) -> anyhow::Result<()> {
       let config = Config::load()?;
       let registry = ModelRegistry::new();
       let downloader = ModelDownloader::new();

       match args.command {
           ModelsCommand::Download => {
               let model = registry.get(&config.embedding.model)
                   .ok_or_else(|| anyhow::anyhow!("Unknown model: {}", config.embedding.model))?;

               let model_dir = config.model_dir().join(&model.name);
               downloader.download(model, &model_dir).await?;
           }
           ModelsCommand::List => {
               println!("Installed models:");
               let model_dir = config.model_dir();
               for model in registry.list() {
                   let installed = model_dir.join(&model.name).exists();
                   let status = if installed { "✓" } else { "✗" };
                   println!("  {} {} (v{})", status, model.name, model.version);
               }
           }
           ModelsCommand::Path => {
               println!("{}", config.model_dir().display());
           }
       }
       Ok(())
   }
   ```

**Acceptance Criteria:**
- [ ] `photon models download` downloads SigLIP model
- [ ] Progress bar shows download status
- [ ] Checksum verification prevents corrupted downloads
- [ ] Already-downloaded files are skipped
- [ ] `photon models list` shows installed models

---

### 3.3 Image Preprocessing

**Goal:** Preprocess images for SigLIP input (resize, normalize, convert to tensor).

**Steps:**

1. Create `crates/photon-core/src/embedding/preprocess.rs`:
   ```rust
   use image::{DynamicImage, GenericImageView, Rgb};
   use ndarray::{Array, Array4};

   /// SigLIP preprocessing parameters
   const SIGLIP_IMAGE_SIZE: u32 = 224;  // Or 384 for large variant
   const SIGLIP_MEAN: [f32; 3] = [0.5, 0.5, 0.5];
   const SIGLIP_STD: [f32; 3] = [0.5, 0.5, 0.5];

   pub struct ImagePreprocessor {
       target_size: u32,
       mean: [f32; 3],
       std: [f32; 3],
   }

   impl ImagePreprocessor {
       pub fn siglip() -> Self {
           Self {
               target_size: SIGLIP_IMAGE_SIZE,
               mean: SIGLIP_MEAN,
               std: SIGLIP_STD,
           }
       }

       /// Preprocess image for SigLIP model input
       /// Returns tensor of shape [1, 3, H, W] in NCHW format
       pub fn preprocess(&self, image: &DynamicImage) -> Array4<f32> {
           // Resize with bicubic interpolation, maintaining aspect ratio
           // Then center crop to target size
           let resized = self.resize_and_crop(image);

           // Convert to RGB if needed
           let rgb = resized.to_rgb8();

           // Create NCHW tensor
           let (width, height) = rgb.dimensions();
           let mut tensor = Array4::<f32>::zeros((1, 3, height as usize, width as usize));

           for (x, y, pixel) in rgb.enumerate_pixels() {
               let x = x as usize;
               let y = y as usize;

               // Normalize: (pixel / 255.0 - mean) / std
               for c in 0..3 {
                   let value = pixel[c] as f32 / 255.0;
                   let normalized = (value - self.mean[c]) / self.std[c];
                   tensor[[0, c, y, x]] = normalized;
               }
           }

           tensor
       }

       /// Preprocess batch of images
       pub fn preprocess_batch(&self, images: &[DynamicImage]) -> Array4<f32> {
           let batch_size = images.len();
           let size = self.target_size as usize;
           let mut tensor = Array4::<f32>::zeros((batch_size, 3, size, size));

           for (i, image) in images.iter().enumerate() {
               let single = self.preprocess(image);
               tensor.slice_mut(s![i, .., .., ..]).assign(&single.slice(s![0, .., .., ..]));
           }

           tensor
       }

       fn resize_and_crop(&self, image: &DynamicImage) -> DynamicImage {
           let (width, height) = image.dimensions();
           let target = self.target_size;

           // Calculate resize dimensions (resize shortest side to target)
           let (new_width, new_height) = if width < height {
               let ratio = target as f32 / width as f32;
               (target, (height as f32 * ratio) as u32)
           } else {
               let ratio = target as f32 / height as f32;
               ((width as f32 * ratio) as u32, target)
           };

           // Resize
           let resized = image.resize_exact(
               new_width,
               new_height,
               image::imageops::FilterType::CatmullRom,
           );

           // Center crop
           let x = (new_width - target) / 2;
           let y = (new_height - target) / 2;
           resized.crop_imm(x, y, target, target)
       }
   }

   #[cfg(test)]
   mod tests {
       use super::*;

       #[test]
       fn test_preprocess_shape() {
           let image = DynamicImage::new_rgb8(640, 480);
           let preprocessor = ImagePreprocessor::siglip();
           let tensor = preprocessor.preprocess(&image);

           assert_eq!(tensor.shape(), &[1, 3, 224, 224]);
       }

       #[test]
       fn test_normalization_range() {
           let image = DynamicImage::new_rgb8(224, 224);
           let preprocessor = ImagePreprocessor::siglip();
           let tensor = preprocessor.preprocess(&image);

           // With mean=0.5 and std=0.5, black (0) -> -1.0, white (1) -> 1.0
           for val in tensor.iter() {
               assert!(*val >= -1.0 && *val <= 1.0);
           }
       }
   }
   ```

**Acceptance Criteria:**
- [ ] Output tensor is correct shape [1, 3, 224, 224]
- [ ] Values are normalized to expected range
- [ ] Aspect ratio handling is correct
- [ ] Batch preprocessing works

---

### 3.4 Embedding Generation

**Goal:** Run SigLIP inference to generate 768-dim embeddings.

**Steps:**

1. Complete the `SigLipEmbedder` implementation:
   ```rust
   // In crates/photon-core/src/embedding/siglip.rs

   impl SigLipEmbedder {
       // ... (previous initialization code)

       /// Generate embedding for a single image
       pub fn embed(&self, image: &DynamicImage) -> Result<Vec<f32>, PipelineError> {
           let preprocessor = ImagePreprocessor::siglip();
           let tensor = preprocessor.preprocess(image);

           self.run_inference(tensor)
       }

       /// Generate embeddings for a batch of images
       pub fn embed_batch(&self, images: &[DynamicImage]) -> Result<Vec<Vec<f32>>, PipelineError> {
           if images.is_empty() {
               return Ok(vec![]);
           }

           let preprocessor = ImagePreprocessor::siglip();
           let tensor = preprocessor.preprocess_batch(images);

           let embeddings = self.run_inference_batch(tensor)?;
           Ok(embeddings)
       }

       fn run_inference(&self, tensor: Array4<f32>) -> Result<Vec<f32>, PipelineError> {
           // Convert ndarray to ONNX input
           let input_data = tensor.as_slice().ok_or_else(|| PipelineError::Embedding {
               path: PathBuf::new(),
               message: "Failed to convert tensor to slice".to_string(),
           })?;

           let input_shape: Vec<i64> = tensor.shape().iter().map(|&x| x as i64).collect();

           let inputs = vec![
               Value::from_array(
                   self.session.allocator(),
                   &input_shape,
                   input_data,
               )?
           ];

           // Run inference
           let outputs = self.session.run(inputs)?;

           // Extract embedding from output
           let output = outputs.get(0).ok_or_else(|| PipelineError::Embedding {
               path: PathBuf::new(),
               message: "No output from model".to_string(),
           })?;

           let embedding: Vec<f32> = output.try_extract()?.view().iter().copied().collect();

           // L2 normalize the embedding
           Ok(Self::normalize(&embedding))
       }

       fn run_inference_batch(&self, tensor: Array4<f32>) -> Result<Vec<Vec<f32>>, PipelineError> {
           let batch_size = tensor.shape()[0];

           let input_data = tensor.as_slice().ok_or_else(|| PipelineError::Embedding {
               path: PathBuf::new(),
               message: "Failed to convert tensor to slice".to_string(),
           })?;

           let input_shape: Vec<i64> = tensor.shape().iter().map(|&x| x as i64).collect();

           let inputs = vec![
               Value::from_array(
                   self.session.allocator(),
                   &input_shape,
                   input_data,
               )?
           ];

           let outputs = self.session.run(inputs)?;

           let output = outputs.get(0).ok_or_else(|| PipelineError::Embedding {
               path: PathBuf::new(),
               message: "No output from model".to_string(),
           })?;

           let flat_embeddings: Vec<f32> = output.try_extract()?.view().iter().copied().collect();
           let embedding_dim = flat_embeddings.len() / batch_size;

           // Split into individual embeddings and normalize
           let embeddings: Vec<Vec<f32>> = flat_embeddings
               .chunks(embedding_dim)
               .map(|chunk| Self::normalize(&chunk.to_vec()))
               .collect();

           Ok(embeddings)
       }

       /// L2 normalize an embedding vector
       fn normalize(embedding: &[f32]) -> Vec<f32> {
           let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
           if norm > 0.0 {
               embedding.iter().map(|x| x / norm).collect()
           } else {
               embedding.to_vec()
           }
       }

       /// Get embedding dimension (768 for base, 1024 for large)
       pub fn embedding_dim(&self) -> usize {
           768 // SigLIP base
       }
   }
   ```

2. Add async wrapper with timeout:
   ```rust
   use std::time::Duration;
   use tokio::time::timeout;

   impl SigLipEmbedder {
       /// Generate embedding with timeout
       pub async fn embed_async(
           &self,
           image: DynamicImage,
           timeout_ms: u64,
       ) -> Result<Vec<f32>, PipelineError> {
           let timeout_duration = Duration::from_millis(timeout_ms);

           // Clone self for the blocking task (if needed, or use Arc)
           let result = timeout(timeout_duration, async {
               tokio::task::spawn_blocking(move || {
                   // This needs access to the embedder
                   // Consider using Arc<SigLipEmbedder> for sharing across tasks
                   todo!("Implement blocking embed")
               }).await
           }).await;

           match result {
               Ok(Ok(Ok(embedding))) => Ok(embedding),
               Ok(Ok(Err(e))) => Err(e),
               Ok(Err(e)) => Err(PipelineError::Embedding {
                   path: PathBuf::new(),
                   message: format!("Task error: {}", e),
               }),
               Err(_) => Err(PipelineError::Timeout {
                   path: PathBuf::new(),
                   stage: "embed".to_string(),
                   timeout_ms,
               }),
           }
       }
   }
   ```

**Acceptance Criteria:**
- [ ] Embedding output is 768 floats
- [ ] Embeddings are L2 normalized (unit length)
- [ ] Batch processing works correctly
- [ ] Timeout mechanism works
- [ ] Similar images produce similar embeddings

---

### 3.5 Integrate Embedding into Pipeline

**Goal:** Wire embedding generation into the image processing pipeline.

**Steps:**

1. Update `ImageProcessor` to include embedding:
   ```rust
   // In crates/photon-core/src/pipeline/processor.rs

   use crate::embedding::SigLipEmbedder;
   use std::sync::Arc;

   pub struct ImageProcessor {
       decoder: ImageDecoder,
       thumbnail_gen: ThumbnailGenerator,
       validator: Validator,
       embedder: Arc<SigLipEmbedder>,
   }

   impl ImageProcessor {
       pub async fn new(config: &Config) -> Result<Self> {
           let model_path = config.model_dir().join(&config.embedding.model);

           // Check if model exists, download if not
           if !model_path.exists() {
               tracing::info!("Model not found, downloading...");
               let registry = ModelRegistry::new();
               let downloader = ModelDownloader::new();
               let model = registry.get(&config.embedding.model)
                   .ok_or_else(|| anyhow::anyhow!("Unknown model"))?;
               downloader.download(model, &model_path).await?;
           }

           let embedder = SigLipEmbedder::new(&config.embedding, &model_path)?;

           Ok(Self {
               decoder: ImageDecoder::new(config.limits.clone()),
               thumbnail_gen: ThumbnailGenerator::new(config.thumbnail.clone()),
               validator: Validator::new(config.limits.clone()),
               embedder: Arc::new(embedder),
           })
       }

       pub async fn process(&self, path: &Path) -> Result<ProcessedImage> {
           tracing::debug!("Processing: {:?}", path);
           let start = std::time::Instant::now();

           // Validate
           self.validator.validate(path)?;
           let validate_time = start.elapsed();

           // Decode
           let decode_start = std::time::Instant::now();
           let decoded = self.decoder.decode(path).await?;
           let decode_time = decode_start.elapsed();

           // Extract metadata
           let metadata_start = std::time::Instant::now();
           let exif = MetadataExtractor::extract(path);
           let metadata_time = metadata_start.elapsed();

           // Generate hashes
           let content_hash = Hasher::content_hash(path)?;
           let perceptual_hash = Some(Hasher::perceptual_hash(&decoded.image));

           // Generate embedding
           let embed_start = std::time::Instant::now();
           let embedding = self.embedder.embed(&decoded.image)?;
           let embed_time = embed_start.elapsed();

           // Generate thumbnail
           let thumbnail = self.thumbnail_gen.generate(&decoded.image);

           tracing::debug!(
               "  Validate: {:?}, Decode: {:?}, Metadata: {:?}, Embed: {:?}",
               validate_time, decode_time, metadata_time, embed_time
           );

           // ... rest of ProcessedImage construction
       }
   }
   ```

2. Update CLI to use async processor:
   ```rust
   // In cli/process.rs

   pub async fn execute(args: ProcessArgs) -> anyhow::Result<()> {
       let config = Config::load()?;
       let processor = ImageProcessor::new(&config).await?;

       if args.input.is_file() {
           let result = processor.process(&args.input).await?;
           let output = serde_json::to_string_pretty(&result)?;
           println!("{}", output);
       } else {
           // Batch processing
           // ...
       }

       Ok(())
   }
   ```

**Acceptance Criteria:**
- [ ] `photon process image.jpg` outputs embedding in JSON
- [ ] Embedding is 768 floats
- [ ] Model auto-downloads on first run
- [ ] Performance logging shows timing breakdown

---

### 3.6 Batch Processing with Bounded Channels

**Goal:** Process multiple images efficiently with backpressure.

**Steps:**

1. Create batch processor:
   ```rust
   // In crates/photon-core/src/pipeline/batch.rs

   use tokio::sync::mpsc;
   use std::path::PathBuf;
   use std::sync::Arc;
   use std::sync::atomic::{AtomicUsize, Ordering};

   use crate::config::Config;
   use crate::types::ProcessedImage;
   use crate::error::PipelineError;

   pub struct BatchProcessor {
       processor: Arc<ImageProcessor>,
       config: Config,
   }

   pub struct BatchProgress {
       pub completed: usize,
       pub failed: usize,
       pub total: usize,
   }

   pub enum BatchResult {
       Success(ProcessedImage),
       Failure { path: PathBuf, error: String },
   }

   impl BatchProcessor {
       pub async fn new(config: Config) -> Result<Self, PipelineError> {
           let processor = Arc::new(ImageProcessor::new(&config).await?);
           Ok(Self { processor, config })
       }

       /// Process a batch of images with bounded concurrency
       pub async fn process_batch(
           &self,
           paths: Vec<PathBuf>,
           tx: mpsc::Sender<BatchResult>,
       ) -> BatchProgress {
           let total = paths.len();
           let completed = Arc::new(AtomicUsize::new(0));
           let failed = Arc::new(AtomicUsize::new(0));

           // Create bounded channel for input paths
           let (path_tx, mut path_rx) = mpsc::channel::<PathBuf>(
               self.config.pipeline.buffer_size
           );

           // Spawn workers
           let workers = self.config.processing.parallel_workers;
           let mut handles = Vec::with_capacity(workers);

           for _ in 0..workers {
               let processor = self.processor.clone();
               let tx = tx.clone();
               let completed = completed.clone();
               let failed = failed.clone();
               let mut rx = path_rx.clone();

               handles.push(tokio::spawn(async move {
                   while let Some(path) = rx.recv().await {
                       let result = processor.process(&path).await;

                       let batch_result = match result {
                           Ok(image) => {
                               completed.fetch_add(1, Ordering::SeqCst);
                               BatchResult::Success(image)
                           }
                           Err(e) => {
                               failed.fetch_add(1, Ordering::SeqCst);
                               tracing::error!("Failed: {:?} - {}", path, e);
                               BatchResult::Failure {
                                   path: path.clone(),
                                   error: e.to_string(),
                               }
                           }
                       };

                       if tx.send(batch_result).await.is_err() {
                           break; // Receiver closed
                       }
                   }
               }));
           }

           // Feed paths to workers
           for path in paths {
               if path_tx.send(path).await.is_err() {
                   break;
               }
           }
           drop(path_tx); // Signal no more paths

           // Wait for all workers to complete
           for handle in handles {
               let _ = handle.await;
           }

           BatchProgress {
               completed: completed.load(Ordering::SeqCst),
               failed: failed.load(Ordering::SeqCst),
               total,
           }
       }
   }
   ```

2. Note: Full batch CLI integration will be completed in Phase 6.

**Acceptance Criteria:**
- [ ] Multiple workers process images concurrently
- [ ] Bounded channel prevents memory exhaustion
- [ ] Progress tracking works
- [ ] Failed images don't stop the batch

---

### 3.7 Embedding Timeout

**Goal:** Implement timeout for embedding generation to handle slow/stuck inference.

**Steps:**

1. Add timeout wrapper (already included in 3.4)

2. Update config with timeout:
   ```toml
   [limits]
   embed_timeout_ms = 30000  # 30 seconds per image
   ```

3. Test timeout behavior with large images

**Acceptance Criteria:**
- [ ] Timeout triggers after configured duration
- [ ] Timeout produces clear error message
- [ ] Batch processing continues after timeout

---

## Integration Tests

```rust
#[tokio::test]
async fn test_embedding_generation() {
    let config = Config::default();
    let processor = ImageProcessor::new(&config).await.unwrap();

    let result = processor.process(Path::new("tests/fixtures/images/test.jpg")).await;

    let image = result.unwrap();
    assert_eq!(image.embedding.len(), 768);

    // Check normalization (unit length)
    let norm: f32 = image.embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
    assert!((norm - 1.0).abs() < 0.001);
}

#[tokio::test]
async fn test_similar_images_have_similar_embeddings() {
    let config = Config::default();
    let processor = ImageProcessor::new(&config).await.unwrap();

    // Process same image twice (should be identical)
    let result1 = processor.process(Path::new("tests/fixtures/images/test.jpg")).await.unwrap();
    let result2 = processor.process(Path::new("tests/fixtures/images/test.jpg")).await.unwrap();

    // Cosine similarity should be 1.0
    let similarity: f32 = result1.embedding.iter()
        .zip(result2.embedding.iter())
        .map(|(a, b)| a * b)
        .sum();

    assert!((similarity - 1.0).abs() < 0.001);
}

#[tokio::test]
async fn test_batch_processing() {
    let config = Config::default();
    let processor = BatchProcessor::new(config).await.unwrap();

    let paths = vec![
        PathBuf::from("tests/fixtures/images/test1.jpg"),
        PathBuf::from("tests/fixtures/images/test2.jpg"),
        PathBuf::from("tests/fixtures/images/test3.jpg"),
    ];

    let (tx, mut rx) = mpsc::channel(10);
    let progress = processor.process_batch(paths, tx).await;

    assert_eq!(progress.total, 3);
    assert_eq!(progress.completed + progress.failed, 3);
}
```

---

## Verification Checklist

Before moving to Phase 4:

- [ ] `photon models download` downloads SigLIP successfully
- [ ] `photon models list` shows installed model
- [ ] `photon process image.jpg` outputs 768-dim embedding
- [ ] Embedding is normalized (L2 norm ≈ 1.0)
- [ ] Metal/CoreML acceleration is used on Apple Silicon
- [ ] Model auto-downloads on first `process` command
- [ ] Timeout works for slow inference
- [ ] Batch processing works with multiple images
- [ ] Memory usage is stable during batch processing
- [ ] Performance meets targets (~50-100 img/min on M1)

---

## Files Created/Modified

```
crates/photon-core/src/
├── embedding/
│   ├── mod.rs           # Module exports
│   ├── siglip.rs        # SigLIP embedder
│   └── preprocess.rs    # Image preprocessing
├── models/
│   ├── mod.rs           # Module exports
│   ├── registry.rs      # Model metadata
│   └── download.rs      # Download with progress
└── pipeline/
    ├── processor.rs     # Updated with embedding
    └── batch.rs         # Batch processing

crates/photon/src/cli/
└── models.rs            # Updated with download command
```

---

## Notes

- ONNX Runtime may need `ORT_DYLIB_PATH` environment variable on some systems
- Consider model quantization (INT8) for faster inference in future
- The text encoder (textual.onnx) will be used in Phase 4 for zero-shot tagging
- For production, consider model caching and warmup runs
- Monitor Metal GPU usage with `sudo powermetrics --samplers gpu_power`
