//! Processor setup: config overrides, model loading, enricher creation.

use photon_core::{
    Config, EmbeddingEngine, ImageProcessor, OutputFormat as CoreOutputFormat, ProcessOptions,
};

use super::types::{LlmProvider, OutputFormat, Quality};
use super::{ProcessArgs, ProcessContext};

/// Validate input, load config/models, and assemble everything needed for processing.
pub fn setup_processor(args: &ProcessArgs) -> anyhow::Result<ProcessContext> {
    // Validate input path exists
    if !args.input.exists() {
        anyhow::bail!(
            "Input path does not exist: {:?}\n\n  Hint: Check the file path and try again.",
            args.input
        );
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

    // Apply hierarchy dedup CLI flags
    if args.show_tag_paths {
        config.tagging.show_paths = true;
    }
    if args.no_dedup_tags {
        config.tagging.deduplicate_ancestors = false;
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

    // Create enricher once (if LLM enabled) — avoids recreating HTTP client per branch
    let enricher = if llm_enabled {
        create_enricher(args, &config)?
    } else {
        None
    };

    // Set up output format
    let output_format = match args.format {
        OutputFormat::Json => CoreOutputFormat::Json,
        OutputFormat::Jsonl => CoreOutputFormat::JsonLines,
    };

    Ok(ProcessContext {
        processor,
        options,
        enricher,
        output_format,
        llm_enabled,
        config,
    })
}

/// Create an LLM enricher from CLI args and config, if --llm was specified.
fn create_enricher(
    args: &ProcessArgs,
    config: &Config,
) -> anyhow::Result<Option<photon_core::Enricher>> {
    use photon_core::{EnrichOptions, LlmProviderFactory};

    let provider_name = match &args.llm {
        Some(p) => p.to_string(),
        None => return Ok(None),
    };

    // If the interactive flow provided a session API key, inject it into
    // the config so the factory picks it up without needing env vars.
    let mut llm_config = config.llm.clone();
    if let Some(ref key) = args.api_key {
        inject_api_key(&mut llm_config, args.llm.as_ref().unwrap(), key);
    }

    let provider =
        LlmProviderFactory::create(&provider_name, &llm_config, args.llm_model.as_deref())?;

    let options = EnrichOptions {
        parallel: args.parallel.min(8), // Cap LLM concurrency
        timeout_ms: config.limits.llm_timeout_ms,
        retry_attempts: config.pipeline.retry_attempts,
        retry_delay_ms: config.pipeline.retry_delay_ms,
        max_file_size_mb: config.limits.max_file_size_mb,
    };

    Ok(Some(photon_core::Enricher::new(provider, options)))
}

/// Inject a session API key into the LLM config for the specified provider.
pub fn inject_api_key(
    llm_config: &mut photon_core::config::LlmConfig,
    provider: &LlmProvider,
    key: &str,
) {
    match provider {
        LlmProvider::Anthropic => {
            let cfg = llm_config.anthropic.get_or_insert_with(Default::default);
            cfg.api_key = key.to_string();
        }
        LlmProvider::Openai => {
            let cfg = llm_config.openai.get_or_insert_with(Default::default);
            cfg.api_key = key.to_string();
        }
        LlmProvider::Hyperbolic => {
            let cfg = llm_config.hyperbolic.get_or_insert_with(Default::default);
            cfg.api_key = key.to_string();
        }
        LlmProvider::Ollama => {} // Ollama doesn't use API keys
    }
}
