# Phase 6: Polish & Release

> **Duration:** 1 week
> **Milestone:** v0.1.0 release with polished UX and documentation

---

## Overview

This final phase focuses on production readiness: parallel batch processing with progress indication, the `--skip-existing` flag for incremental processing, comprehensive error handling, performance optimization, documentation, and release preparation.

---

## Prerequisites

- Phases 1-5 completed (full pipeline working)
- All integration tests passing
- Basic performance profiling done

---

## Implementation Tasks

### 6.1 Parallel Batch Processing with Progress Bar

**Goal:** Process directories of images efficiently with visual progress feedback.

**Steps:**

1. Add progress bar dependency:
   ```toml
   indicatif = "0.17"
   console = "0.15"
   ```

2. Create progress tracking:
   ```rust
   // crates/photon-core/src/pipeline/progress.rs

   use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
   use std::sync::Arc;
   use std::time::{Duration, Instant};

   pub struct BatchProgress {
       multi: MultiProgress,
       main_bar: ProgressBar,
       start_time: Instant,
       total: u64,
   }

   impl BatchProgress {
       pub fn new(total: u64) -> Self {
           let multi = MultiProgress::new();

           let main_bar = multi.add(ProgressBar::new(total));
           main_bar.set_style(
               ProgressStyle::default_bar()
                   .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({percent}%) {msg}")
                   .unwrap()
                   .progress_chars("█▓░")
           );
           main_bar.set_message("Processing images...");

           Self {
               multi,
               main_bar,
               start_time: Instant::now(),
               total,
           }
       }

       pub fn inc(&self, delta: u64) {
           self.main_bar.inc(delta);
           self.update_rate();
       }

       pub fn set_message(&self, msg: &str) {
           self.main_bar.set_message(msg.to_string());
       }

       fn update_rate(&self) {
           let elapsed = self.start_time.elapsed().as_secs_f64();
           let completed = self.main_bar.position();
           if elapsed > 0.0 {
               let rate = completed as f64 / elapsed;
               self.main_bar.set_message(format!("{:.1} img/sec", rate));
           }
       }

       pub fn finish(&self, succeeded: u64, failed: u64) {
           let elapsed = self.start_time.elapsed();
           let rate = if elapsed.as_secs_f64() > 0.0 {
               self.total as f64 / elapsed.as_secs_f64()
           } else {
               0.0
           };

           self.main_bar.finish_with_message(format!(
               "Completed: {} succeeded, {} failed ({:.1} img/sec)",
               succeeded, failed, rate
           ));
       }

       pub fn abandon(&self) {
           self.main_bar.abandon();
       }
   }
   ```

3. Create full batch processor:
   ```rust
   // crates/photon-core/src/pipeline/batch.rs

   use std::fs::File;
   use std::io::{BufWriter, Write};
   use std::path::{Path, PathBuf};
   use std::sync::atomic::{AtomicU64, Ordering};
   use std::sync::Arc;
   use tokio::sync::mpsc;

   use crate::config::Config;
   use crate::error::Result;
   use crate::output::{OutputFormat, OutputWriter};
   use crate::types::ProcessedImage;

   use super::discovery::FileDiscovery;
   use super::processor::ImageProcessor;
   use super::progress::BatchProgress;

   pub struct BatchProcessor {
       config: Config,
   }

   pub struct BatchOptions {
       pub parallel_workers: usize,
       pub output_path: Option<PathBuf>,
       pub output_format: OutputFormat,
       pub skip_existing: bool,
       pub verbose: bool,
   }

   pub struct BatchResult {
       pub succeeded: u64,
       pub failed: u64,
       pub skipped: u64,
       pub total: u64,
       pub elapsed_secs: f64,
   }

   impl BatchResult {
       pub fn rate(&self) -> f64 {
           if self.elapsed_secs > 0.0 {
               (self.succeeded + self.failed) as f64 / self.elapsed_secs
           } else {
               0.0
           }
       }
   }

   impl BatchProcessor {
       pub fn new(config: Config) -> Self {
           Self { config }
       }

       pub async fn process(
           &self,
           input_path: &Path,
           options: BatchOptions,
       ) -> Result<BatchResult> {
           let start = std::time::Instant::now();

           // Discover files
           let discovery = FileDiscovery::new(self.config.processing.clone());
           let mut files = discovery.discover(input_path);
           let total = files.len() as u64;

           tracing::info!("Discovered {} images", total);

           // Load existing hashes if skip_existing
           let existing_hashes = if options.skip_existing {
               self.load_existing_hashes(&options.output_path)?
           } else {
               std::collections::HashSet::new()
           };

           // Filter already-processed files
           let skipped = if !existing_hashes.is_empty() {
               let original_len = files.len();
               files.retain(|f| {
                   // Quick hash check would require reading file
                   // For now, we'll check during processing
                   true
               });
               (original_len - files.len()) as u64
           } else {
               0
           };

           let to_process = files.len() as u64;

           if to_process == 0 {
               tracing::info!("No images to process");
               return Ok(BatchResult {
                   succeeded: 0,
                   failed: 0,
                   skipped,
                   total,
                   elapsed_secs: start.elapsed().as_secs_f64(),
               });
           }

           // Initialize processor
           let mut processor = ImageProcessor::new(&self.config);
           // Load optional capabilities (follows opt-in pattern from Phases 3-4)
           let _ = processor.load_embedding(&self.config);
           let _ = processor.load_tagging(&self.config);
           let processor = Arc::new(processor);

           // Progress bar
           let progress = Arc::new(BatchProgress::new(to_process));

           // Set up output
           let (result_tx, mut result_rx) = mpsc::channel::<ProcessedImage>(
               self.config.pipeline.buffer_size
           );

           // Spawn output writer task
           let output_path = options.output_path.clone();
           let output_format = options.output_format;
           let output_handle = tokio::spawn(async move {
               let writer: Box<dyn Write + Send> = match output_path {
                   Some(path) => Box::new(BufWriter::new(File::create(path).unwrap())),
                   None => Box::new(std::io::stdout()),
               };

               let mut output = OutputWriter::new(writer, output_format, false);

               while let Some(image) = result_rx.recv().await {
                   if let Err(e) = output.write(&image) {
                       tracing::error!("Failed to write output: {}", e);
                   }
               }
           });

           // Process files with worker pool
           let succeeded = Arc::new(AtomicU64::new(0));
           let failed = Arc::new(AtomicU64::new(0));

           // Create semaphore for concurrency control
           let semaphore = Arc::new(tokio::sync::Semaphore::new(options.parallel_workers));

           let mut handles = Vec::new();

           for file in files {
               let processor = processor.clone();
               let result_tx = result_tx.clone();
               let progress = progress.clone();
               let succeeded = succeeded.clone();
               let failed = failed.clone();
               let semaphore = semaphore.clone();
               let existing_hashes = existing_hashes.clone();

               handles.push(tokio::spawn(async move {
                   // Acquire permit
                   let _permit = semaphore.acquire().await.unwrap();

                   let path = file.path;

                   // Check if already processed (by hash)
                   // This requires a quick hash check
                   if !existing_hashes.is_empty() {
                       if let Ok(hash) = crate::pipeline::hash::Hasher::content_hash(&path) {
                           if existing_hashes.contains(&hash) {
                               progress.inc(1);
                               return;
                           }
                       }
                   }

                   match processor.process(&path).await {
                       Ok(result) => {
                           succeeded.fetch_add(1, Ordering::SeqCst);
                           let _ = result_tx.send(result).await;
                       }
                       Err(e) => {
                           failed.fetch_add(1, Ordering::SeqCst);
                           tracing::error!("Failed: {:?} - {}", path, e);
                       }
                   }

                   progress.inc(1);
               }));
           }

           // Wait for all processing to complete
           for handle in handles {
               let _ = handle.await;
           }

           // Close result channel
           drop(result_tx);

           // Wait for output to finish
           let _ = output_handle.await;

           let succeeded_count = succeeded.load(Ordering::SeqCst);
           let failed_count = failed.load(Ordering::SeqCst);

           progress.finish(succeeded_count, failed_count);

           Ok(BatchResult {
               succeeded: succeeded_count,
               failed: failed_count,
               skipped,
               total,
               elapsed_secs: start.elapsed().as_secs_f64(),
           })
       }

       fn load_existing_hashes(
           &self,
           output_path: &Option<PathBuf>,
       ) -> Result<std::collections::HashSet<String>> {
           let mut hashes = std::collections::HashSet::new();

           if let Some(path) = output_path {
               if path.exists() {
                   let content = std::fs::read_to_string(path)?;
                   for line in content.lines() {
                       if let Ok(image) = serde_json::from_str::<ProcessedImage>(line) {
                           hashes.insert(image.content_hash);
                       }
                   }
                   tracing::info!("Loaded {} existing hashes", hashes.len());
               }
           }

           Ok(hashes)
       }
   }
   ```

4. Update CLI:
   ```rust
   // In cli/process.rs

   pub async fn execute(args: ProcessArgs) -> anyhow::Result<()> {
       let config = Config::load()?;

       if args.input.is_file() {
           // Single file processing
           let mut processor = ImageProcessor::new(&config);
           let _ = processor.load_embedding(&config);
           let _ = processor.load_tagging(&config);
           let result = processor.process(&args.input).await?;

           let output = if args.format == OutputFormat::Json {
               serde_json::to_string_pretty(&result)?
           } else {
               serde_json::to_string(&result)?
           };
           println!("{}", output);
       } else {
           // Batch processing
           let batch_processor = BatchProcessor::new(config);

           let options = BatchOptions {
               parallel_workers: args.parallel,
               output_path: args.output.clone(),
               output_format: match args.format {
                   OutputFormat::Json => crate::output::OutputFormat::Json,
                   OutputFormat::Jsonl => crate::output::OutputFormat::JsonLines,
               },
               skip_existing: args.skip_existing,
               verbose: args.verbose,
           };

           let result = batch_processor.process(&args.input, options).await?;

           tracing::info!(
               "Completed: {} succeeded, {} failed, {} skipped ({:.1} img/sec)",
               result.succeeded,
               result.failed,
               result.skipped,
               result.rate()
           );
       }

       Ok(())
   }
   ```

**Acceptance Criteria:**
- [ ] Progress bar shows during batch processing
- [ ] Rate (img/sec) updates in real-time
- [ ] Summary shows at completion
- [ ] Ctrl+C gracefully stops processing
- [ ] Output writes incrementally (not at end)

---

### 6.2 Skip Already-Processed Images

**Goal:** Implement `--skip-existing` to avoid reprocessing.

**Steps:**

1. Already implemented in 6.1, but enhance:
   ```rust
   impl BatchProcessor {
       /// More efficient skip check using file path index
       fn load_existing_paths(
           &self,
           output_path: &Option<PathBuf>,
       ) -> Result<std::collections::HashSet<PathBuf>> {
           let mut paths = std::collections::HashSet::new();

           if let Some(path) = output_path {
               if path.exists() {
                   let content = std::fs::read_to_string(path)?;
                   for line in content.lines() {
                       if let Ok(image) = serde_json::from_str::<ProcessedImage>(line) {
                           paths.insert(image.file_path);
                       }
                   }
               }
           }

           Ok(paths)
       }
   }
   ```

2. Add CLI documentation:
   ```rust
   /// Skip already-processed images (checks output file for existing hashes)
   #[arg(long)]
   pub skip_existing: bool,
   ```

**Acceptance Criteria:**
- [ ] `--skip-existing` reads output file
- [ ] Previously processed images are skipped
- [ ] New images are processed and appended
- [ ] Works with both JSON and JSONL output

---

### 6.3 Summary Statistics

**Goal:** Display comprehensive statistics at the end of processing.

**Steps:**

1. Create stats collector:
   ```rust
   // crates/photon-core/src/pipeline/stats.rs

   use std::collections::HashMap;
   use std::time::Duration;

   #[derive(Default)]
   pub struct ProcessingStats {
       pub succeeded: u64,
       pub failed: u64,
       pub skipped: u64,
       pub total_bytes: u64,
       pub total_duration: Duration,
       pub stage_durations: HashMap<String, Duration>,
       pub failures_by_stage: HashMap<String, u64>,
   }

   impl ProcessingStats {
       pub fn images_per_second(&self) -> f64 {
           let secs = self.total_duration.as_secs_f64();
           if secs > 0.0 {
               self.succeeded as f64 / secs
           } else {
               0.0
           }
       }

       pub fn bytes_per_second(&self) -> f64 {
           let secs = self.total_duration.as_secs_f64();
           if secs > 0.0 {
               self.total_bytes as f64 / secs
           } else {
               0.0
           }
       }

       pub fn format_summary(&self) -> String {
           let mut lines = Vec::new();

           lines.push(format!(""));
           lines.push(format!("═══════════════════════════════════════════"));
           lines.push(format!("                 Summary                   "));
           lines.push(format!("═══════════════════════════════════════════"));
           lines.push(format!("  Succeeded:    {:>8}", self.succeeded));
           lines.push(format!("  Failed:       {:>8}", self.failed));
           lines.push(format!("  Skipped:      {:>8}", self.skipped));
           lines.push(format!("───────────────────────────────────────────"));
           lines.push(format!("  Total:        {:>8}", self.succeeded + self.failed + self.skipped));
           lines.push(format!("  Duration:     {:>8.1}s", self.total_duration.as_secs_f64()));
           lines.push(format!("  Rate:         {:>8.1} img/sec", self.images_per_second()));
           lines.push(format!("  Throughput:   {:>8.1} MB/sec", self.bytes_per_second() / 1_000_000.0));
           lines.push(format!("═══════════════════════════════════════════"));

           if !self.failures_by_stage.is_empty() {
               lines.push(format!(""));
               lines.push(format!("Failures by stage:"));
               for (stage, count) in &self.failures_by_stage {
                   lines.push(format!("  {}: {}", stage, count));
               }
           }

           lines.join("\n")
       }
   }
   ```

2. Integrate into batch processor output.

**Acceptance Criteria:**
- [ ] Summary shows succeeded/failed/skipped counts
- [ ] Duration and rate are displayed
- [ ] Failure breakdown by stage (if any)
- [ ] Human-readable formatting

---

### 6.4 Comprehensive Error Messages

**Goal:** Ensure all errors are user-friendly and actionable.

**Steps:**

1. Review and enhance error messages:
   ```rust
   // Update error.rs with more context

   #[derive(Error, Debug)]
   pub enum PhotonError {
       #[error("Configuration error: {0}\n\nHint: Run 'photon config show' to see current configuration.")]
       Config(#[from] ConfigError),

       #[error("Model not found: {model}\n\nHint: Run 'photon models download' to download required models.")]
       ModelNotFound { model: String },

       // ...
   }

   impl PipelineError {
       pub fn user_message(&self) -> String {
           match self {
               PipelineError::Decode { path, message } => {
                   format!(
                       "Failed to decode image: {}\n  File: {}\n  Error: {}\n\n  \
                        Hint: The file may be corrupted or in an unsupported format.",
                       path.file_name().unwrap_or_default().to_string_lossy(),
                       path.display(),
                       message
                   )
               }
               PipelineError::FileTooLarge { path, size_mb, max_mb } => {
                   format!(
                       "File too large: {}\n  Size: {} MB (max: {} MB)\n\n  \
                        Hint: Increase 'limits.max_file_size_mb' in config, or resize the image.",
                       path.display(), size_mb, max_mb
                   )
               }
               // ... other errors
               _ => self.to_string(),
           }
       }
   }
   ```

2. Add suggestions where helpful:
   - Missing API key → "Set ANTHROPIC_API_KEY environment variable"
   - Model not found → "Run 'photon models download'"
   - Permission denied → "Check file permissions"
   - Network error → "Check your internet connection"

**Acceptance Criteria:**
- [ ] Error messages include file path
- [ ] Error messages include suggested fix
- [ ] No raw panic messages reach user
- [ ] Exit codes are meaningful (0 = success, 1 = error)

---

### 6.5 Performance Optimization

**Goal:** Ensure pipeline meets performance targets.

**Steps:**

1. Profile and identify bottlenecks:
   ```bash
   # Build with profiling
   cargo build --release

   # Profile with samply (macOS)
   samply record ./target/release/photon process ./test-images/

   # Or with perf (Linux)
   perf record ./target/release/photon process ./test-images/
   ```

2. Common optimizations:
   ```rust
   // Reduce allocations in hot paths
   // Use pre-allocated buffers for image data
   // Batch ONNX inference where possible
   // Use memory-mapped files for large images
   // Parallelize thumbnail generation with embedding
   ```

3. Add benchmarks:
   ```rust
   // benches/pipeline.rs

   use criterion::{black_box, criterion_group, criterion_main, Criterion};

   fn benchmark_decode(c: &mut Criterion) {
       let image_path = "tests/fixtures/images/test.jpg";

       c.bench_function("decode_jpeg", |b| {
           b.iter(|| {
               let _ = image::open(black_box(image_path));
           })
       });
   }

   fn benchmark_embedding(c: &mut Criterion) {
       // Benchmark embedding generation
   }

   criterion_group!(benches, benchmark_decode, benchmark_embedding);
   criterion_main!(benches);
   ```

4. Verify targets:
   | Operation | Target | Actual |
   |-----------|--------|--------|
   | Decode + metadata | 200 img/sec | ? |
   | SigLIP embedding | 50-100 img/min | ? |
   | Tag scoring (80K vocab) | < 10ms per image | ? |
   | Full pipeline (no LLM) | 50 img/min | ? |
   | First-run vocab encoding | < 3 min (4a) / < 3 sec (4b) | ? |

**Acceptance Criteria:**
- [ ] Meets performance targets on M1 Mac
- [ ] Memory usage stays bounded
- [ ] No memory leaks in long runs
- [ ] CPU utilization is efficient

---

### 6.6 Documentation

**Goal:** Comprehensive documentation for users and contributors.

**Steps:**

1. Create README.md:
   ```markdown
   # Photon

   Pure image processing pipeline for AI-powered tagging and embeddings.

   ## Features

   - Adaptive zero-shot tagging: 80K WordNet vocabulary, self-organizing to your library
   - SigLIP embeddings: 768-dim vectors for image similarity and search
   - Fast: ~50 images/minute on CPU (tagging adds < 10ms per image)
   - Single binary, no Python required
   - BYOK: Use Ollama, Anthropic, OpenAI, or Hyperbolic for optional LLM descriptions

   ## Installation

   ### From Binary (Recommended)

   ```bash
   # macOS (Apple Silicon)
   curl -L https://github.com/yourname/photon/releases/latest/download/photon-aarch64-apple-darwin.tar.gz | tar xz
   sudo mv photon /usr/local/bin/

   # macOS (Intel)
   curl -L https://github.com/yourname/photon/releases/latest/download/photon-x86_64-apple-darwin.tar.gz | tar xz
   sudo mv photon /usr/local/bin/
   ```

   ### From Source

   ```bash
   cargo install photon
   ```

   ## Quick Start

   ```bash
   # Process a single image
   photon process image.jpg

   # Process a directory
   photon process ./photos/ --output results.jsonl

   # Enrich with LLM descriptions (separate step)
   photon enrich results.jsonl --llm anthropic --output enriched.jsonl
   ```

   ## Configuration

   ```bash
   # Show config
   photon config show

   # Config file location
   photon config path  # ~/.photon/config.toml
   ```

   ## Output Format

   ```json
   {
     "file_path": "/photos/beach.jpg",
     "content_hash": "a7f3b2c1...",
     "embedding": [0.023, -0.156, ...],
     "tags": [
       {"name": "sandy beach", "confidence": 0.91},
       {"name": "ocean", "confidence": 0.87},
       {"name": "tropical", "confidence": 0.74, "category": "scene"},
       {"name": "palm tree", "confidence": 0.68, "path": "plant > tree > palm tree"}
     ],
     "description": "A sandy tropical beach..."
   }
   ```

   ## License

   MIT OR Apache-2.0
   ```

2. Create CONTRIBUTING.md:
   ```markdown
   # Contributing to Photon

   ## Development Setup

   ```bash
   git clone https://github.com/yourname/photon
   cd photon
   cargo build
   cargo test
   ```

   ## Running Tests

   ```bash
   # Unit tests
   cargo test

   # Integration tests
   cargo test --test integration

   # With coverage
   cargo llvm-cov
   ```

   ## Code Style

   ```bash
   cargo fmt
   cargo clippy
   ```
   ```

3. Add doc comments to public API:
   ```rust
   /// Photon image processor.
   ///
   /// # Example
   ///
   /// ```rust
   /// use photon_core::{Photon, Config};
   ///
   /// let config = Config::default();
   /// let photon = Photon::new(config).await?;
   /// let result = photon.process("image.jpg").await?;
   /// println!("{:?}", result.tags);
   /// ```
   pub struct Photon { ... }
   ```

**Acceptance Criteria:**
- [ ] README covers installation, quick start, configuration
- [ ] CONTRIBUTING covers development setup
- [ ] Public API has doc comments
- [ ] Examples compile and work

---

### 6.7 Release Preparation

**Goal:** Prepare for v0.1.0 release with binaries and changelog.

**Steps:**

1. Create GitHub Actions workflow for releases:
   ```yaml
   # .github/workflows/release.yml

   name: Release

   on:
     push:
       tags:
         - 'v*'

   jobs:
     build:
       strategy:
         matrix:
           include:
             - os: macos-14
               target: aarch64-apple-darwin
             - os: macos-13
               target: x86_64-apple-darwin
             - os: ubuntu-latest
               target: x86_64-unknown-linux-gnu

       runs-on: ${{ matrix.os }}

       steps:
         - uses: actions/checkout@v4

         - name: Install Rust
           uses: dtolnay/rust-action@stable
           with:
             targets: ${{ matrix.target }}

         - name: Build
           run: cargo build --release --target ${{ matrix.target }}

         - name: Package
           run: |
             mkdir -p dist
             cp target/${{ matrix.target }}/release/photon dist/
             tar -czf photon-${{ matrix.target }}.tar.gz -C dist photon

         - name: Upload
           uses: actions/upload-artifact@v4
           with:
             name: photon-${{ matrix.target }}
             path: photon-${{ matrix.target }}.tar.gz

     release:
       needs: build
       runs-on: ubuntu-latest
       steps:
         - uses: actions/download-artifact@v4

         - name: Create Release
           uses: softprops/action-gh-release@v1
           with:
             files: |
               photon-*/photon-*.tar.gz
             draft: false
             prerelease: false
           env:
             GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
   ```

2. Create CHANGELOG.md:
   ```markdown
   # Changelog

   All notable changes to this project will be documented in this file.

   ## [0.1.0] - 2026-XX-XX

   ### Added
   - Initial release
   - Image processing pipeline (JPEG, PNG, WebP, HEIC)
   - SigLIP embeddings (768-dim vectors)
   - Adaptive zero-shot tagging with WordNet vocabulary (~80K terms)
   - Self-organizing vocabulary (three-pool relevance system)
   - WordNet hierarchy deduplication (ancestor suppression)
   - Progressive vocabulary encoding (fast startup)
   - EXIF metadata extraction
   - Content and perceptual hashing
   - Thumbnail generation (WebP)
   - Optional LLM descriptions (Ollama, Anthropic, OpenAI, Hyperbolic)
   - Tags passed as context to LLM for improved descriptions
   - Batch processing with progress bar
   - Skip already-processed images
   - TOML configuration
   ```

3. Version bump:
   ```bash
   # Update version in Cargo.toml
   cargo set-version 0.1.0
   ```

4. Create release:
   ```bash
   git tag -a v0.1.0 -m "Release v0.1.0"
   git push origin v0.1.0
   ```

**Acceptance Criteria:**
- [ ] Binaries build for macOS (ARM64, x86_64)
- [ ] Binaries build for Linux (x86_64)
- [ ] GitHub release has all binaries
- [ ] CHANGELOG documents all features
- [ ] Version is consistent across crates

---

### 6.8 Final Testing

**Goal:** Comprehensive testing before release.

**Steps:**

1. Run full test suite:
   ```bash
   cargo test --all
   cargo test --all --release
   ```

2. Test with real-world data:
   ```bash
   # Large batch
   photon process ~/Photos --output test.jsonl --parallel 8

   # Various formats
   photon process test.jpg
   photon process test.png
   photon process test.heic
   photon process test.webp

   # LLM enrichment (separate from process)
   photon enrich results.jsonl --llm ollama --output enriched.jsonl
   photon enrich results.jsonl --llm anthropic --output enriched.jsonl
   photon enrich results.jsonl --llm openai --output enriched.jsonl

   # Edge cases
   photon process empty.jpg
   photon process corrupt.jpg
   photon process huge.jpg  # 100MB+
   ```

3. Test on clean system:
   ```bash
   # Remove all Photon data
   rm -rf ~/.photon

   # Fresh install and run
   photon process test.jpg  # Should download models automatically
   ```

4. Performance test:
   ```bash
   # 1000 images
   time photon process ./large-batch --output results.jsonl --parallel 8
   ```

**Acceptance Criteria:**
- [ ] All tests pass
- [ ] Works on fresh system
- [ ] Performance meets targets
- [ ] No crashes on edge cases
- [ ] Clean error messages

---

## Final Verification Checklist

Before v0.1.0 release:

**Core Functionality:**
- [ ] `photon process image.jpg` works
- [ ] `photon process ./dir/` works with batch processing
- [ ] Progress bar displays correctly
- [ ] Output is valid JSON/JSONL
- [ ] Embeddings are 768 dimensions
- [ ] Tags have confidence (and optional category/path fields)
- [ ] Tags are from WordNet vocabulary (not hardcoded list)
- [ ] `photon enrich` produces LLM descriptions with all providers

**User Experience:**
- [ ] `photon --help` is clear and complete
- [ ] Error messages are helpful
- [ ] `--skip-existing` works correctly
- [ ] Summary statistics display at end
- [ ] Ctrl+C exits gracefully

**Configuration:**
- [ ] Default config works out of box
- [ ] Custom config loads correctly
- [ ] Environment variables work for API keys
- [ ] `photon config show` displays config

**Models & Vocabulary:**
- [ ] `photon models download` works
- [ ] Auto-download on first use works
- [ ] `photon models list` shows status
- [ ] Vocabulary files are present or auto-downloaded
- [ ] Label bank caches correctly on first run

**Performance:**
- [ ] Meets 50 img/min target (no LLM)
- [ ] Memory usage is stable
- [ ] Parallel processing works

**Release:**
- [ ] README is complete
- [ ] CHANGELOG is complete
- [ ] Binaries build successfully
- [ ] Version numbers are consistent
- [ ] License files present

---

## Files Created/Modified

```
photon/
├── .github/
│   └── workflows/
│       └── release.yml      # Release automation
├── README.md                # User documentation
├── CONTRIBUTING.md          # Contributor guide
├── CHANGELOG.md             # Version history
├── LICENSE-MIT              # MIT license
├── LICENSE-APACHE           # Apache 2.0 license
├── benches/
│   └── pipeline.rs          # Performance benchmarks
└── crates/
    ├── photon/src/
    │   └── cli/
    │       └── process.rs   # Updated with batch processing
    └── photon-core/src/
        └── pipeline/
            ├── progress.rs  # Progress bar
            ├── batch.rs     # Batch processing
            └── stats.rs     # Statistics
```

---

## Post-Release

After v0.1.0:

1. Monitor GitHub issues
2. Gather user feedback
3. Plan v0.2.0 features:
   - CUDA support
   - Video processing
   - Additional models
   - WASM build
4. Consider:
   - Homebrew formula
   - Docker image
   - Package managers (apt, dnf)
