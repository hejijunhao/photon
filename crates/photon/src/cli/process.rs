//! The `photon process` command for processing images.

use clap::{Args, ValueEnum};
use std::path::PathBuf;

/// Arguments for the `process` command.
#[derive(Args, Debug)]
pub struct ProcessArgs {
    /// Image file or directory to process
    #[arg(required = true)]
    pub input: PathBuf,

    /// Output file (defaults to stdout)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Output format
    #[arg(short, long, value_enum, default_value = "json")]
    pub format: OutputFormat,

    /// Number of parallel workers
    #[arg(short, long, default_value = "4")]
    pub parallel: usize,

    /// Skip already-processed images (checks output file for existing hashes)
    #[arg(long)]
    pub skip_existing: bool,

    /// Disable thumbnail generation
    #[arg(long)]
    pub no_thumbnail: bool,

    /// Disable embedding generation
    #[arg(long)]
    pub no_embedding: bool,

    /// Disable zero-shot tagging
    #[arg(long)]
    pub no_tagging: bool,

    /// Disable LLM descriptions
    #[arg(long)]
    pub no_description: bool,

    /// Quality preset: fast (224 model) or high (384 model)
    #[arg(long, value_enum, default_value = "fast")]
    pub quality: Quality,

    /// Thumbnail size in pixels
    #[arg(long, default_value = "256")]
    pub thumbnail_size: u32,

    /// LLM provider for descriptions
    #[arg(long, value_enum)]
    pub llm: Option<LlmProvider>,

    /// LLM model name (provider-specific)
    #[arg(long)]
    pub llm_model: Option<String>,
}

/// Supported output formats.
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum OutputFormat {
    /// Single JSON object or array
    Json,
    /// One JSON object per line (newline-delimited)
    Jsonl,
}

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OutputFormat::Json => write!(f, "json"),
            OutputFormat::Jsonl => write!(f, "jsonl"),
        }
    }
}

/// Quality preset for SigLIP vision model resolution.
#[derive(Clone, Copy, Debug, ValueEnum, Default)]
pub enum Quality {
    /// Fast processing with base 224 model (default)
    #[default]
    Fast,
    /// Higher detail with base 384 model (~3-4× slower)
    High,
}

/// Supported LLM providers.
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum LlmProvider {
    /// Local Ollama instance
    Ollama,
    /// Hyperbolic API
    Hyperbolic,
    /// Anthropic API
    Anthropic,
    /// OpenAI API
    Openai,
}

impl std::fmt::Display for LlmProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LlmProvider::Ollama => write!(f, "ollama"),
            LlmProvider::Hyperbolic => write!(f, "hyperbolic"),
            LlmProvider::Anthropic => write!(f, "anthropic"),
            LlmProvider::Openai => write!(f, "openai"),
        }
    }
}

/// Execute the process command.
pub async fn execute(args: ProcessArgs) -> anyhow::Result<()> {
    use photon_core::embedding::EmbeddingEngine;
    use photon_core::output::OutputFormat as CoreOutputFormat;
    use photon_core::{Config, ImageProcessor, OutputWriter, ProcessOptions};
    use std::fs::File;
    use std::io::BufWriter;

    // Validate input path exists
    if !args.input.exists() {
        anyhow::bail!("Input path does not exist: {:?}", args.input);
    }

    // Load configuration
    let mut config = Config::load()?;

    // Override thumbnail size if specified
    config.thumbnail.size = args.thumbnail_size;

    // Disable thumbnail if requested
    if args.no_thumbnail {
        config.thumbnail.enabled = false;
    }

    // Apply quality preset — select model variant and image size
    match args.quality {
        Quality::High => {
            let high_model = "siglip-base-patch16-384".to_string();
            if EmbeddingEngine::model_exists(
                &photon_core::config::EmbeddingConfig {
                    model: high_model.clone(),
                    image_size: 384,
                    device: config.embedding.device.clone(),
                },
                &config.model_dir(),
            ) {
                config.embedding.model = high_model;
                config.embedding.image_size = 384;
            } else {
                tracing::warn!(
                    "Base 384 model not found. Falling back to 224. \
                     Run `photon models download` to install additional models."
                );
            }
        }
        Quality::Fast => {
            // Default — 224 model, no changes needed
        }
    }

    // Create processor
    let mut processor = ImageProcessor::new(&config);

    // Load embedding model unless disabled
    if !args.no_embedding {
        if EmbeddingEngine::model_exists(&config.embedding, &config.model_dir()) {
            match processor.load_embedding(&config) {
                Ok(()) => tracing::info!("Embedding model loaded"),
                Err(e) => tracing::warn!("Failed to load embedding model: {e}"),
            }
        } else {
            tracing::warn!(
                "Embedding model not found. Run `photon models download` to enable embeddings."
            );
        }
    }

    // Load tagging system unless disabled
    if !args.no_tagging && config.tagging.enabled && processor.has_embedding() {
        match processor.load_tagging(&config) {
            Ok(()) => tracing::info!("Tagging system loaded"),
            Err(e) => tracing::warn!("Failed to load tagging system: {e}"),
        }
    }

    // Create process options
    let options = ProcessOptions {
        skip_thumbnail: args.no_thumbnail,
        skip_perceptual_hash: false,
        skip_embedding: args.no_embedding || !processor.has_embedding(),
        skip_tagging: args.no_tagging || !processor.has_tagging(),
    };

    // Discover files
    let files = processor.discover(&args.input);

    if files.is_empty() {
        tracing::warn!("No supported image files found at {:?}", args.input);
        return Ok(());
    }

    tracing::info!("Found {} image(s) to process", files.len());

    // Set up output writer
    let output_format = match args.format {
        OutputFormat::Json => CoreOutputFormat::Json,
        OutputFormat::Jsonl => CoreOutputFormat::JsonLines,
    };

    // Process and output
    if args.input.is_file() {
        // Single file - output to stdout or file
        let result = processor
            .process_with_options(&args.input, &options)
            .await?;

        if let Some(ref output_path) = args.output {
            let file = File::create(output_path)?;
            let mut writer = OutputWriter::new(BufWriter::new(file), output_format, true);
            writer.write(&result)?;
            writer.flush()?;
            tracing::info!("Output written to {:?}", output_path);
        } else {
            let json = serde_json::to_string_pretty(&result)?;
            println!("{}", json);
        }
    } else {
        // Batch processing
        let mut succeeded = 0;
        let mut failed = 0;
        let start_time = std::time::Instant::now();

        // Prepare output
        let mut results = Vec::new();

        for file in &files {
            match processor.process_with_options(&file.path, &options).await {
                Ok(result) => {
                    succeeded += 1;
                    if matches!(args.format, OutputFormat::Jsonl) && args.output.is_none() {
                        // Stream JSONL to stdout immediately
                        println!("{}", serde_json::to_string(&result)?);
                    } else {
                        results.push(result);
                    }
                }
                Err(e) => {
                    failed += 1;
                    tracing::error!("Failed: {:?} - {}", file.path, e);
                }
            }
        }

        // Output batch results
        if !results.is_empty() {
            if let Some(ref output_path) = args.output {
                let file = File::create(output_path)?;
                let mut writer = OutputWriter::new(BufWriter::new(file), output_format, false);
                if matches!(args.format, OutputFormat::Json) {
                    writer.write_all(&results)?;
                } else {
                    for result in &results {
                        writer.write(result)?;
                    }
                }
                writer.flush()?;
                tracing::info!("Output written to {:?}", output_path);
            } else if matches!(args.format, OutputFormat::Json) {
                println!("{}", serde_json::to_string_pretty(&results)?);
            }
        }

        let elapsed = start_time.elapsed();
        let rate = if elapsed.as_secs_f64() > 0.0 {
            succeeded as f64 / elapsed.as_secs_f64()
        } else {
            0.0
        };

        tracing::info!(
            "Completed: {} succeeded, {} failed ({:.1} img/sec)",
            succeeded,
            failed,
            rate
        );
    }

    Ok(())
}
