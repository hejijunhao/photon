# Phase 1: Foundation

> **Duration:** 1 week
> **Milestone:** `photon --help` works, `photon config show` displays config

---

## Overview

This phase establishes the project scaffolding: Cargo workspace, CLI structure, configuration system, and foundational infrastructure like logging and error handling. No image processing yet—just the skeleton that everything else builds on.

---

## Prerequisites

- Rust toolchain (1.75+)
- Cargo installed
- Git repository initialized

---

## Implementation Tasks

### 1.1 Initialize Cargo Workspace

**Goal:** Set up a multi-crate workspace with `photon` (CLI binary) and `photon-core` (library).

**Steps:**

1. Create workspace `Cargo.toml` at project root:
   ```toml
   [workspace]
   resolver = "2"
   members = ["crates/photon", "crates/photon-core"]

   [workspace.package]
   version = "0.1.0"
   edition = "2021"
   authors = ["Your Name <you@example.com>"]
   license = "MIT OR Apache-2.0"
   repository = "https://github.com/yourname/photon"

   [workspace.dependencies]
   # Shared dependencies go here
   tokio = { version = "1", features = ["full"] }
   serde = { version = "1", features = ["derive"] }
   serde_json = "1"
   thiserror = "2"
   tracing = "0.1"
   tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
   ```

2. Create directory structure:
   ```
   crates/
   ├── photon/
   │   ├── Cargo.toml
   │   └── src/
   │       └── main.rs
   └── photon-core/
       ├── Cargo.toml
       └── src/
           └── lib.rs
   ```

3. Create `crates/photon/Cargo.toml`:
   ```toml
   [package]
   name = "photon"
   version.workspace = true
   edition.workspace = true

   [[bin]]
   name = "photon"
   path = "src/main.rs"

   [dependencies]
   photon-core = { path = "../photon-core" }
   clap = { version = "4", features = ["derive", "env"] }
   tokio.workspace = true
   serde.workspace = true
   serde_json.workspace = true
   tracing.workspace = true
   tracing-subscriber.workspace = true
   ```

4. Create `crates/photon-core/Cargo.toml`:
   ```toml
   [package]
   name = "photon-core"
   version.workspace = true
   edition.workspace = true

   [dependencies]
   tokio.workspace = true
   serde.workspace = true
   serde_json.workspace = true
   thiserror.workspace = true
   tracing.workspace = true
   ```

5. Verify workspace builds:
   ```bash
   cargo build
   cargo test
   ```

**Acceptance Criteria:**
- [ ] `cargo build` succeeds
- [ ] Both crates are recognized in workspace
- [ ] `cargo run -p photon` executes without error

---

### 1.2 CLI Skeleton with Clap

**Goal:** Create the command structure: `photon process`, `photon models`, `photon config`.

**Steps:**

1. Create CLI module structure in `crates/photon/src/`:
   ```
   src/
   ├── main.rs
   └── cli/
       ├── mod.rs
       ├── process.rs
       ├── models.rs
       └── config.rs
   ```

2. Define root CLI in `main.rs`:
   ```rust
   use clap::{Parser, Subcommand};

   mod cli;

   #[derive(Parser)]
   #[command(name = "photon")]
   #[command(author, version, about = "Pure image processing pipeline for AI-powered tagging and embeddings")]
   struct Cli {
       /// Enable verbose logging
       #[arg(short, long, global = true)]
       verbose: bool,

       #[command(subcommand)]
       command: Commands,
   }

   #[derive(Subcommand)]
   enum Commands {
       /// Process images and generate embeddings, tags, and metadata
       Process(cli::process::ProcessArgs),
       /// Manage AI models
       Models(cli::models::ModelsArgs),
       /// View and manage configuration
       Config(cli::config::ConfigArgs),
   }
   ```

3. Implement `cli/process.rs`:
   ```rust
   use clap::{Args, ValueEnum};
   use std::path::PathBuf;

   #[derive(Args)]
   pub struct ProcessArgs {
       /// Image file or directory to process
       #[arg(required = true)]
       pub input: PathBuf,

       /// Output file (defaults to stdout)
       #[arg(short, long)]
       pub output: Option<PathBuf>,

       /// Output format
       #[arg(short, long, default_value = "json")]
       pub format: OutputFormat,

       /// Number of parallel workers
       #[arg(short, long, default_value = "4")]
       pub parallel: usize,

       /// Skip already-processed images (checks output file)
       #[arg(long)]
       pub skip_existing: bool,

       /// Disable thumbnail generation
       #[arg(long)]
       pub no_thumbnail: bool,

       /// Disable LLM descriptions
       #[arg(long)]
       pub no_description: bool,

       /// Thumbnail size in pixels
       #[arg(long, default_value = "256")]
       pub thumbnail_size: u32,

       /// LLM provider for descriptions
       #[arg(long)]
       pub llm: Option<LlmProvider>,

       /// LLM model name
       #[arg(long)]
       pub llm_model: Option<String>,
   }

   #[derive(Clone, ValueEnum)]
   pub enum OutputFormat {
       Json,
       Jsonl,
   }

   #[derive(Clone, ValueEnum)]
   pub enum LlmProvider {
       Ollama,
       Hyperbolic,
       Anthropic,
       Openai,
   }

   pub async fn execute(args: ProcessArgs) -> anyhow::Result<()> {
       tracing::info!("Processing: {:?}", args.input);
       // Stub - will be implemented in Phase 2
       Ok(())
   }
   ```

4. Implement `cli/models.rs`:
   ```rust
   use clap::{Args, Subcommand};

   #[derive(Args)]
   pub struct ModelsArgs {
       #[command(subcommand)]
       pub command: ModelsCommand,
   }

   #[derive(Subcommand)]
   pub enum ModelsCommand {
       /// Download required models
       Download,
       /// List installed models
       List,
       /// Show model directory path
       Path,
   }

   pub async fn execute(args: ModelsArgs) -> anyhow::Result<()> {
       match args.command {
           ModelsCommand::Download => {
               tracing::info!("Downloading models...");
               // Stub
           }
           ModelsCommand::List => {
               tracing::info!("Installed models:");
               // Stub
           }
           ModelsCommand::Path => {
               println!("~/.photon/models/");
           }
       }
       Ok(())
   }
   ```

5. Implement `cli/config.rs`:
   ```rust
   use clap::{Args, Subcommand};

   #[derive(Args)]
   pub struct ConfigArgs {
       #[command(subcommand)]
       pub command: ConfigCommand,
   }

   #[derive(Subcommand)]
   pub enum ConfigCommand {
       /// Display current configuration
       Show,
       /// Show config file path
       Path,
   }

   pub async fn execute(args: ConfigArgs) -> anyhow::Result<()> {
       match args.command {
           ConfigCommand::Show => {
               // Will use photon_core::Config
               tracing::info!("Current configuration:");
           }
           ConfigCommand::Path => {
               println!("~/.photon/config.toml");
           }
       }
       Ok(())
   }
   ```

**Acceptance Criteria:**
- [ ] `photon --help` displays all commands
- [ ] `photon process --help` shows all process options
- [ ] `photon models path` outputs the model directory
- [ ] `photon config path` outputs the config path

---

### 1.3 Configuration System

**Goal:** TOML-based configuration with defaults, file loading, and environment variable overrides.

**Steps:**

1. Add dependencies to `photon-core/Cargo.toml`:
   ```toml
   toml = "0.8"
   directories = "5"
   ```

2. Create `crates/photon-core/src/config.rs`:
   ```rust
   use serde::{Deserialize, Serialize};
   use std::path::PathBuf;

   #[derive(Debug, Clone, Serialize, Deserialize)]
   #[serde(default)]
   pub struct Config {
       pub general: GeneralConfig,
       pub processing: ProcessingConfig,
       pub pipeline: PipelineConfig,
       pub limits: LimitsConfig,
       pub embedding: EmbeddingConfig,
       pub thumbnail: ThumbnailConfig,
       pub tagging: TaggingConfig,
       pub output: OutputConfig,
       pub logging: LoggingConfig,
       pub llm: LlmConfig,
   }

   #[derive(Debug, Clone, Serialize, Deserialize)]
   #[serde(default)]
   pub struct GeneralConfig {
       pub model_dir: PathBuf,
   }

   #[derive(Debug, Clone, Serialize, Deserialize)]
   #[serde(default)]
   pub struct ProcessingConfig {
       pub parallel_workers: usize,
       pub supported_formats: Vec<String>,
   }

   #[derive(Debug, Clone, Serialize, Deserialize)]
   #[serde(default)]
   pub struct PipelineConfig {
       pub buffer_size: usize,
       pub retry_attempts: u32,
       pub retry_delay_ms: u64,
   }

   #[derive(Debug, Clone, Serialize, Deserialize)]
   #[serde(default)]
   pub struct LimitsConfig {
       pub max_file_size_mb: u64,
       pub max_image_dimension: u32,
       pub decode_timeout_ms: u64,
       pub embed_timeout_ms: u64,
       pub llm_timeout_ms: u64,
   }

   #[derive(Debug, Clone, Serialize, Deserialize)]
   #[serde(default)]
   pub struct EmbeddingConfig {
       pub model: String,
       pub device: String,
   }

   #[derive(Debug, Clone, Serialize, Deserialize)]
   #[serde(default)]
   pub struct ThumbnailConfig {
       pub enabled: bool,
       pub size: u32,
       pub format: String,
       pub quality: u8,
   }

   #[derive(Debug, Clone, Serialize, Deserialize)]
   #[serde(default)]
   pub struct TaggingConfig {
       pub min_confidence: f32,
       pub max_tags: usize,
       pub zero_shot_enabled: bool,
   }

   #[derive(Debug, Clone, Serialize, Deserialize)]
   #[serde(default)]
   pub struct OutputConfig {
       pub format: String,
       pub pretty: bool,
       pub include_embedding: bool,
   }

   #[derive(Debug, Clone, Serialize, Deserialize)]
   #[serde(default)]
   pub struct LoggingConfig {
       pub level: String,
       pub format: String,
   }

   #[derive(Debug, Clone, Serialize, Deserialize, Default)]
   pub struct LlmConfig {
       pub ollama: Option<OllamaConfig>,
       pub hyperbolic: Option<HyperbolicConfig>,
       pub anthropic: Option<AnthropicConfig>,
       pub openai: Option<OpenAiConfig>,
   }

   // Implement Default for all config structs with sensible defaults
   // ... (implement Default trait for each)

   impl Config {
       /// Load config from default location (~/.photon/config.toml)
       pub fn load() -> Result<Self, ConfigError> {
           let path = Self::default_path();
           if path.exists() {
               Self::load_from(&path)
           } else {
               Ok(Self::default())
           }
       }

       /// Load config from a specific path
       pub fn load_from(path: &PathBuf) -> Result<Self, ConfigError> {
           let content = std::fs::read_to_string(path)?;
           let config: Config = toml::from_str(&content)?;
           Ok(config)
       }

       /// Get default config file path
       pub fn default_path() -> PathBuf {
           directories::ProjectDirs::from("com", "photon", "photon")
               .map(|dirs| dirs.config_dir().join("config.toml"))
               .unwrap_or_else(|| PathBuf::from("~/.photon/config.toml"))
       }

       /// Get model directory path
       pub fn model_dir(&self) -> PathBuf {
           shellexpand::tilde(&self.general.model_dir.to_string_lossy())
               .into_owned()
               .into()
       }
   }
   ```

3. Implement `Default` for all config structs with values from blueprint.

4. Add config display functionality for `photon config show`.

**Acceptance Criteria:**
- [ ] `Config::default()` returns sensible defaults
- [ ] `Config::load()` reads from `~/.photon/config.toml` if exists
- [ ] Missing config file falls back to defaults
- [ ] `photon config show` displays current configuration

---

### 1.4 Output Formatting

**Goal:** JSON and JSONL output with pretty-print option.

**Steps:**

1. Create `crates/photon-core/src/output.rs`:
   ```rust
   use serde::Serialize;
   use std::io::Write;

   pub enum OutputFormat {
       Json,
       JsonLines,
   }

   pub struct OutputWriter<W: Write> {
       writer: W,
       format: OutputFormat,
       pretty: bool,
   }

   impl<W: Write> OutputWriter<W> {
       pub fn new(writer: W, format: OutputFormat, pretty: bool) -> Self {
           Self { writer, format, pretty }
       }

       /// Write a single item
       pub fn write<T: Serialize>(&mut self, item: &T) -> std::io::Result<()> {
           match self.format {
               OutputFormat::Json => {
                   if self.pretty {
                       serde_json::to_writer_pretty(&mut self.writer, item)?;
                   } else {
                       serde_json::to_writer(&mut self.writer, item)?;
                   }
                   writeln!(self.writer)?;
               }
               OutputFormat::JsonLines => {
                   serde_json::to_writer(&mut self.writer, item)?;
                   writeln!(self.writer)?;
               }
           }
           Ok(())
       }

       /// Write multiple items (for JSON format, writes as array)
       pub fn write_all<T: Serialize>(&mut self, items: &[T]) -> std::io::Result<()> {
           match self.format {
               OutputFormat::Json => {
                   if self.pretty {
                       serde_json::to_writer_pretty(&mut self.writer, items)?;
                   } else {
                       serde_json::to_writer(&mut self.writer, items)?;
                   }
                   writeln!(self.writer)?;
               }
               OutputFormat::JsonLines => {
                   for item in items {
                       self.write(item)?;
                   }
               }
           }
           Ok(())
       }
   }
   ```

2. Create stub output types in `crates/photon-core/src/types.rs`:
   ```rust
   use serde::{Deserialize, Serialize};
   use std::path::PathBuf;

   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct ProcessedImage {
       pub file_path: PathBuf,
       pub file_name: String,
       pub content_hash: String,
       pub width: u32,
       pub height: u32,
       pub format: String,
       pub file_size: u64,
       pub embedding: Vec<f32>,
       pub exif: Option<ExifData>,
       pub tags: Vec<Tag>,
       pub description: Option<String>,
       pub thumbnail: Option<String>,  // base64 encoded
       pub perceptual_hash: Option<String>,
   }

   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct ExifData {
       pub captured_at: Option<String>,
       pub camera_make: Option<String>,
       pub camera_model: Option<String>,
       pub gps_latitude: Option<f64>,
       pub gps_longitude: Option<f64>,
       pub iso: Option<u32>,
       pub aperture: Option<String>,
   }

   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct Tag {
       pub name: String,
       pub confidence: f32,
       pub category: Option<String>,
   }
   ```

**Acceptance Criteria:**
- [ ] `OutputWriter` can write single items in JSON and JSONL
- [ ] `OutputWriter` can write multiple items
- [ ] Pretty-print option works for JSON format
- [ ] JSONL outputs one item per line

---

### 1.5 Structured Logging with Tracing

**Goal:** Configure tracing for human-readable and JSON log output.

**Steps:**

1. Create logging initialization in `crates/photon/src/logging.rs`:
   ```rust
   use tracing_subscriber::{fmt, prelude::*, EnvFilter};

   pub fn init(verbose: bool, json_format: bool) {
       let filter = if verbose {
           EnvFilter::new("debug")
       } else {
           EnvFilter::new("info")
       };

       if json_format {
           tracing_subscriber::registry()
               .with(filter)
               .with(fmt::layer().json())
               .init();
       } else {
           tracing_subscriber::registry()
               .with(filter)
               .with(fmt::layer().with_target(false))
               .init();
       }
   }
   ```

2. Add tracing macros throughout the codebase:
   ```rust
   tracing::info!("Processing {} images with {} workers", count, workers);
   tracing::debug!("Loading config from {:?}", path);
   tracing::error!("Failed: {:?} - {} - {}", path, stage, error);
   ```

3. Add progress logging for batch operations (placeholder):
   ```rust
   tracing::info!("Progress: {}/{} ({}%) - {:.1} img/sec",
       completed, total, percent, rate);
   ```

**Acceptance Criteria:**
- [ ] Default logging shows INFO level messages
- [ ] `--verbose` flag enables DEBUG level
- [ ] JSON logging format works when configured
- [ ] Log output goes to stderr (stdout reserved for data)

---

### 1.6 Error Types with Thiserror

**Goal:** Define granular, per-stage error types for clear error reporting.

**Steps:**

1. Create `crates/photon-core/src/error.rs`:
   ```rust
   use std::path::PathBuf;
   use thiserror::Error;

   #[derive(Error, Debug)]
   pub enum PhotonError {
       #[error("Configuration error: {0}")]
       Config(#[from] ConfigError),

       #[error("Pipeline error: {0}")]
       Pipeline(#[from] PipelineError),

       #[error("IO error: {0}")]
       Io(#[from] std::io::Error),
   }

   #[derive(Error, Debug)]
   pub enum ConfigError {
       #[error("Failed to read config file: {0}")]
       ReadError(#[from] std::io::Error),

       #[error("Failed to parse config: {0}")]
       ParseError(#[from] toml::de::Error),

       #[error("Invalid configuration: {0}")]
       ValidationError(String),
   }

   #[derive(Error, Debug)]
   pub enum PipelineError {
       #[error("Decode error for {path}: {message}")]
       Decode { path: PathBuf, message: String },

       #[error("Metadata extraction failed for {path}: {message}")]
       Metadata { path: PathBuf, message: String },

       #[error("Embedding failed for {path}: {message}")]
       Embedding { path: PathBuf, message: String },

       #[error("Tagging failed for {path}: {message}")]
       Tagging { path: PathBuf, message: String },

       #[error("LLM error for {path}: {message}")]
       Llm { path: PathBuf, message: String },

       #[error("Timeout in {stage} stage for {path} after {timeout_ms}ms")]
       Timeout {
           path: PathBuf,
           stage: String,
           timeout_ms: u64,
       },

       #[error("File too large: {path} ({size_mb}MB > {max_mb}MB)")]
       FileTooLarge {
           path: PathBuf,
           size_mb: u64,
           max_mb: u64,
       },

       #[error("Image too large: {path} ({width}x{height} > {max_dim})")]
       ImageTooLarge {
           path: PathBuf,
           width: u32,
           height: u32,
           max_dim: u32,
       },
   }

   /// Result type alias for Photon operations
   pub type Result<T> = std::result::Result<T, PhotonError>;
   ```

2. Export errors from `lib.rs`:
   ```rust
   pub mod error;
   pub use error::{PhotonError, PipelineError, ConfigError, Result};
   ```

**Acceptance Criteria:**
- [ ] Error types cover all pipeline stages
- [ ] Errors include relevant context (file path, stage, details)
- [ ] Errors implement `std::error::Error` via thiserror
- [ ] Error messages are user-friendly

---

## Verification Checklist

Before moving to Phase 2, verify:

- [ ] `cargo build --release` succeeds without warnings
- [ ] `photon --help` displays usage information
- [ ] `photon --version` displays version
- [ ] `photon process --help` shows all options from blueprint
- [ ] `photon models path` outputs model directory
- [ ] `photon config path` outputs config path
- [ ] `photon config show` displays default configuration
- [ ] Verbose logging works with `-v` flag
- [ ] Unit tests pass: `cargo test`
- [ ] Code is formatted: `cargo fmt --check`
- [ ] Lints pass: `cargo clippy`

---

## Files Created

After completing this phase, you should have:

```
photon/
├── Cargo.toml                    # Workspace manifest
├── crates/
│   ├── photon/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs           # CLI entry point
│   │       ├── logging.rs        # Logging setup
│   │       └── cli/
│   │           ├── mod.rs
│   │           ├── process.rs    # Process command
│   │           ├── models.rs     # Models command
│   │           └── config.rs     # Config command
│   │
│   └── photon-core/
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs            # Library exports
│           ├── config.rs         # Configuration types
│           ├── error.rs          # Error types
│           ├── types.rs          # Data types (ProcessedImage, etc.)
│           └── output.rs         # Output formatting
```

---

## Notes

- Keep the CLI thin; business logic belongs in `photon-core`
- Use `anyhow` in the CLI binary for ergonomic error handling
- Use `thiserror` in the library for typed errors
- All async runtime setup happens in `main.rs`
- Configuration supports environment variable expansion (e.g., `${ANTHROPIC_API_KEY}`)
