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
    use photon_core::llm::EnrichResult;
    use photon_core::output::OutputFormat as CoreOutputFormat;
    use photon_core::types::OutputRecord;
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

    // Determine if LLM enrichment is enabled
    let llm_enabled = args.llm.is_some() && !args.no_description;

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

    // ── Phase 1: Core pipeline (fast, unchanged) ──

    if args.input.is_file() {
        // Single file processing
        let result = processor
            .process_with_options(&args.input, &options)
            .await?;

        if llm_enabled {
            // Dual-stream: emit core record, then enrich
            let core_record = OutputRecord::Core(Box::new(result.clone()));

            if let Some(ref output_path) = args.output {
                let file = File::create(output_path)?;
                let mut writer = OutputWriter::new(BufWriter::new(file), output_format, false);
                writer.write(&core_record)?;

                // Run enrichment for single file
                if let Some(enricher) = create_enricher(&args, &config)? {
                    let results_vec = vec![result];
                    let (file_writer_tx, file_writer_rx) =
                        std::sync::mpsc::channel::<OutputRecord>();

                    let enricher_handle = {
                        let tx = file_writer_tx;
                        tokio::spawn(async move {
                            enricher
                                .enrich_batch(&results_vec, move |enrich_result| {
                                    if let EnrichResult::Success(patch) = enrich_result {
                                        let _ = tx.send(OutputRecord::Enrichment(patch));
                                    }
                                })
                                .await
                        })
                    };

                    let (enriched, enrich_failed) = enricher_handle.await?;
                    for record in file_writer_rx.try_iter() {
                        writer.write(&record)?;
                    }
                    writer.flush()?;
                    tracing::info!("Output written to {:?}", output_path);
                    log_enrichment_stats(enriched, enrich_failed);
                } else {
                    writer.flush()?;
                    tracing::info!("Output written to {:?}", output_path);
                }
            } else {
                // Stdout
                println!("{}", serde_json::to_string_pretty(&core_record)?);

                if let Some(enricher) = create_enricher(&args, &config)? {
                    let results_vec = vec![result];
                    enricher
                        .enrich_batch(&results_vec, |enrich_result| match enrich_result {
                            EnrichResult::Success(patch) => {
                                let record = OutputRecord::Enrichment(patch);
                                if let Ok(json) = serde_json::to_string_pretty(&record) {
                                    println!("{json}");
                                }
                            }
                            EnrichResult::Failure(path, msg) => {
                                tracing::error!("Enrichment failed: {path:?} - {msg}");
                            }
                        })
                        .await;
                }
            }
        } else {
            // No LLM — backward compatible plain ProcessedImage output
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
        }
        // Save relevance tracking data for single-file mode (if enabled)
        if let Err(e) = processor.save_relevance(&config) {
            tracing::warn!("Failed to save relevance data: {e}");
        }
    } else {
        // Batch processing
        let mut succeeded = 0;
        let mut failed = 0;
        let start_time = std::time::Instant::now();
        let mut results = Vec::new();

        for file in &files {
            match processor.process_with_options(&file.path, &options).await {
                Ok(result) => {
                    succeeded += 1;
                    if matches!(args.format, OutputFormat::Jsonl) && args.output.is_none() {
                        // Stream to stdout immediately
                        if llm_enabled {
                            let record = OutputRecord::Core(Box::new(result.clone()));
                            println!("{}", serde_json::to_string(&record)?);
                        } else {
                            println!("{}", serde_json::to_string(&result)?);
                        }
                    }
                    // Always collect for LLM enrichment; also for file/JSON output
                    if llm_enabled
                        || args.output.is_some()
                        || matches!(args.format, OutputFormat::Json)
                    {
                        results.push(result);
                    }
                }
                Err(e) => {
                    failed += 1;
                    tracing::error!("Failed: {:?} - {}", file.path, e);
                }
            }
        }

        // Write batch results to file (core records)
        if !results.is_empty() && args.output.is_some() {
            let output_path = args.output.as_ref().unwrap();
            let file = File::create(output_path)?;
            let mut writer = OutputWriter::new(BufWriter::new(file), output_format, false);

            if llm_enabled {
                // ── Phase 2: LLM enrichment to file (only if --llm) ──
                let mut all_records: Vec<OutputRecord> = results
                    .iter()
                    .map(|r| OutputRecord::Core(Box::new(r.clone())))
                    .collect();

                if let Some(enricher) = create_enricher(&args, &config)? {
                    tracing::info!("Starting LLM enrichment for {} images...", results.len());

                    let (file_writer_tx, file_writer_rx) =
                        std::sync::mpsc::channel::<OutputRecord>();

                    let enricher_handle = {
                        let tx = file_writer_tx;
                        let results_clone = results.clone();
                        tokio::spawn(async move {
                            enricher
                                .enrich_batch(&results_clone, move |enrich_result| match enrich_result {
                                    EnrichResult::Success(patch) => {
                                        let _ = tx.send(OutputRecord::Enrichment(patch));
                                    }
                                    EnrichResult::Failure(path, msg) => {
                                        tracing::error!("Enrichment failed: {path:?} - {msg}");
                                    }
                                })
                                .await
                        })
                    };

                    let (enriched, enrich_failed) = enricher_handle.await?;
                    all_records.extend(file_writer_rx.try_iter());
                    log_enrichment_stats(enriched, enrich_failed);
                }

                // write_all produces a valid JSON array for JSON format,
                // or one record per line for JSONL — correct in both cases.
                writer.write_all(&all_records)?;
            } else if matches!(args.format, OutputFormat::Json) {
                writer.write_all(&results)?;
            } else {
                for result in &results {
                    writer.write(result)?;
                }
            }

            writer.flush()?;
            tracing::info!("Output written to {:?}", output_path);
        } else if !results.is_empty()
            && args.output.is_none()
            && matches!(args.format, OutputFormat::Json)
        {
            if llm_enabled {
                // JSON array to stdout with OutputRecord::Core wrappers
                let core_records: Vec<OutputRecord> = results
                    .iter()
                    .map(|r| OutputRecord::Core(Box::new(r.clone())))
                    .collect();
                println!("{}", serde_json::to_string_pretty(&core_records)?);
            } else {
                // JSON array to stdout (non-LLM batch)
                println!("{}", serde_json::to_string_pretty(&results)?);
            }
        }

        // LLM enrichment for stdout streaming (JSON and JSONL)
        if llm_enabled && args.output.is_none() {
            if let Some(enricher) = create_enricher(&args, &config)? {
                tracing::info!("Starting LLM enrichment for {} images...", results.len());

                let (enriched, enrich_failed) = enricher
                    .enrich_batch(&results, |enrich_result| match enrich_result {
                        EnrichResult::Success(patch) => {
                            let record = OutputRecord::Enrichment(patch);
                            if let Ok(json) = serde_json::to_string(&record) {
                                println!("{json}");
                            }
                        }
                        EnrichResult::Failure(path, msg) => {
                            tracing::error!("Enrichment failed: {path:?} - {msg}");
                        }
                    })
                    .await;
                log_enrichment_stats(enriched, enrich_failed);
            }
        }

        // Save relevance tracking data (if enabled)
        if let Err(e) = processor.save_relevance(&config) {
            tracing::warn!("Failed to save relevance data: {e}");
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

/// Create an LLM enricher from CLI args and config, if --llm was specified.
fn create_enricher(
    args: &ProcessArgs,
    config: &photon_core::Config,
) -> anyhow::Result<Option<photon_core::llm::Enricher>> {
    use photon_core::llm::{EnrichOptions, Enricher, LlmProviderFactory};

    let provider_name = match &args.llm {
        Some(p) => p.to_string(),
        None => return Ok(None),
    };

    let provider = LlmProviderFactory::create(
        &provider_name,
        &config.llm,
        args.llm_model.as_deref(),
    )?;

    let options = EnrichOptions {
        parallel: args.parallel.min(8), // Cap LLM concurrency
        timeout_ms: config.limits.llm_timeout_ms,
        retry_attempts: config.pipeline.retry_attempts,
        retry_delay_ms: config.pipeline.retry_delay_ms,
    };

    Ok(Some(Enricher::new(provider, options)))
}

fn log_enrichment_stats(succeeded: usize, failed: usize) {
    if failed > 0 {
        tracing::warn!(
            "LLM enrichment: {} succeeded, {} failed",
            succeeded,
            failed
        );
    } else {
        tracing::info!("LLM enrichment: {} succeeded", succeeded);
    }
}
