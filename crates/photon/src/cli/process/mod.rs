//! The `photon process` command for processing images.

mod batch;
mod enrichment;
mod setup;
pub mod types;

pub use types::{LlmProvider, OutputFormat, Quality};

use clap::Args;
use photon_core::{
    Config, Enricher, ImageProcessor, OutputFormat as CoreOutputFormat, OutputRecord,
    ProcessOptions,
};
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;

use batch::process_batch;
use enrichment::{run_enrichment_collect, run_enrichment_stdout};
use setup::setup_processor;

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

    /// Show hierarchy paths in tag output (e.g., "animal > dog > labrador retriever")
    #[arg(long)]
    pub show_tag_paths: bool,

    /// Disable ancestor deduplication in tags
    #[arg(long)]
    pub no_dedup_tags: bool,

    /// API key for the selected LLM provider (session-only, set by interactive mode).
    #[arg(skip)]
    pub api_key: Option<String>,
}

/// Manual Default impl for constructing ProcessArgs outside of clap.
///
/// Values match the clap `#[arg(default_value = ...)]` annotations above.
/// Used by the interactive module to build ProcessArgs field-by-field.
impl Default for ProcessArgs {
    fn default() -> Self {
        Self {
            input: PathBuf::new(),
            output: None,
            format: OutputFormat::Json,
            parallel: 4,
            skip_existing: false,
            no_thumbnail: false,
            no_embedding: false,
            no_tagging: false,
            no_description: false,
            quality: Quality::Fast,
            thumbnail_size: 256,
            llm: None,
            llm_model: None,
            show_tag_paths: false,
            no_dedup_tags: false,
            api_key: None,
        }
    }
}

/// Processing context assembled by setup_processor().
pub(crate) struct ProcessContext {
    pub processor: ImageProcessor,
    pub options: ProcessOptions,
    pub enricher: Option<Enricher>,
    pub output_format: CoreOutputFormat,
    pub llm_enabled: bool,
    pub config: Config,
}

/// Execute the process command.
pub async fn execute(args: ProcessArgs) -> anyhow::Result<()> {
    let ctx = setup_processor(&args)?;

    let files = ctx.processor.discover(&args.input);
    if files.is_empty() {
        tracing::warn!("No supported image files found at {:?}", args.input);
        return Ok(());
    }
    tracing::info!("Found {} image(s) to process", files.len());

    if args.input.is_file() {
        process_single(ctx, &args).await
    } else {
        process_batch(ctx, &args, files).await
    }
}

// ── Single-file processing ─────────────────────────────────────────────────

/// Process a single image file with optional LLM enrichment.
async fn process_single(mut ctx: ProcessContext, args: &ProcessArgs) -> anyhow::Result<()> {
    let result = ctx
        .processor
        .process_with_options(&args.input, &ctx.options)
        .await?;

    if ctx.llm_enabled {
        // Dual-stream: emit core record, then enrich
        let core_record = OutputRecord::Core(Box::new(result.clone()));

        if let Some(ref output_path) = args.output {
            // File output — use write_all() for a proper JSON array (or JSONL lines)
            let file = File::create(output_path)?;
            let mut writer =
                photon_core::OutputWriter::new(BufWriter::new(file), ctx.output_format, false);

            let mut all_records: Vec<OutputRecord> = vec![core_record];
            if let Some(enricher) = ctx.enricher.take() {
                let patches = run_enrichment_collect(enricher, vec![result]).await?;
                all_records.extend(patches);
            }
            writer.write_all(&all_records)?;
            writer.flush()?;
            tracing::info!("Output written to {:?}", output_path);
        } else {
            // Stdout
            match args.format {
                OutputFormat::Json => {
                    // JSON: collect enrichment, emit combined array
                    let mut all_records: Vec<OutputRecord> = vec![core_record];
                    if let Some(enricher) = ctx.enricher.take() {
                        let patches = run_enrichment_collect(enricher, vec![result]).await?;
                        all_records.extend(patches);
                    }
                    println!("{}", serde_json::to_string_pretty(&all_records)?);
                }
                OutputFormat::Jsonl => {
                    // JSONL: stream core record, then enrichment patches
                    println!("{}", serde_json::to_string(&core_record)?);
                    if let Some(enricher) = ctx.enricher.take() {
                        run_enrichment_stdout(enricher, &[result], false).await?;
                    }
                }
            }
        }
    } else {
        // No LLM — backward compatible plain ProcessedImage output
        if let Some(ref output_path) = args.output {
            let file = File::create(output_path)?;
            let mut writer =
                photon_core::OutputWriter::new(BufWriter::new(file), ctx.output_format, true);
            writer.write(&result)?;
            writer.flush()?;
            tracing::info!("Output written to {:?}", output_path);
        } else {
            let json = serde_json::to_string_pretty(&result)?;
            println!("{}", json);
        }
    }

    // Save relevance tracking data for single-file mode (if enabled)
    if let Err(e) = ctx.processor.save_relevance(&ctx.config) {
        tracing::warn!("Failed to save relevance data: {e}");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn process_args_default_parallel() {
        let args = ProcessArgs::default();
        assert_eq!(args.parallel, 4);
    }

    #[test]
    fn process_args_default_format_is_json() {
        let args = ProcessArgs::default();
        assert!(matches!(args.format, OutputFormat::Json));
    }

    #[test]
    fn process_args_default_quality_is_fast() {
        let args = ProcessArgs::default();
        assert!(matches!(args.quality, Quality::Fast));
    }

    #[test]
    fn process_args_default_thumbnail_size() {
        let args = ProcessArgs::default();
        assert_eq!(args.thumbnail_size, 256);
    }

    #[test]
    fn process_args_default_bool_flags_are_false() {
        let args = ProcessArgs::default();
        assert!(!args.skip_existing);
        assert!(!args.no_thumbnail);
        assert!(!args.no_embedding);
        assert!(!args.no_tagging);
        assert!(!args.no_description);
        assert!(!args.show_tag_paths);
        assert!(!args.no_dedup_tags);
    }

    #[test]
    fn process_args_default_option_fields_are_none() {
        let args = ProcessArgs::default();
        assert!(args.output.is_none());
        assert!(args.llm.is_none());
        assert!(args.llm_model.is_none());
    }

    #[test]
    fn process_args_default_input_is_empty_path() {
        let args = ProcessArgs::default();
        assert_eq!(args.input, PathBuf::new());
    }
}
