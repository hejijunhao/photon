//! Photon CLI - Pure image processing pipeline for AI-powered tagging and embeddings.
//!
//! Photon takes images as input and outputs structured data: vector embeddings,
//! semantic tags, metadata, and descriptions. It's designed as a pure processing
//! pipeline with no database dependencies.
//!
//! # Usage
//!
//! ```bash
//! # Process a single image
//! photon process image.jpg
//!
//! # Process a directory
//! photon process ./photos/ --output results.jsonl
//!
//! # View configuration
//! photon config show
//!
//! # Manage models
//! photon models download
//! ```

use clap::{Parser, Subcommand};

mod cli;
mod logging;

/// Photon - Pure image processing pipeline for AI-powered tagging and embeddings.
#[derive(Parser, Debug)]
#[command(name = "photon")]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    /// Enable verbose (debug) logging
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Output logs in JSON format
    #[arg(long, global = true)]
    json_logs: bool,

    #[command(subcommand)]
    command: Commands,
}

/// Available commands.
#[derive(Subcommand, Debug)]
enum Commands {
    /// Process images and generate embeddings, tags, and metadata
    Process(cli::process::ProcessArgs),

    /// Manage AI models (download, list, etc.)
    Models(cli::models::ModelsArgs),

    /// View and manage configuration
    Config(cli::config::ConfigArgs),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Initialize logging from config, with CLI verbose override.
    // Note: logging isn't initialized yet, so use eprintln for config warnings.
    let config = match photon_core::Config::load() {
        Ok(config) => config,
        Err(e) => {
            eprintln!(
                "Warning: Failed to load config: {e}\n  \
                 Using default configuration. Check your config file with `photon config path`."
            );
            photon_core::Config::default()
        }
    };
    logging::init_from_config(&config, cli.verbose, cli.json_logs);

    tracing::debug!("Photon v{}", photon_core::VERSION);

    // Dispatch to the appropriate command handler
    match cli.command {
        Commands::Process(args) => cli::process::execute(args).await,
        Commands::Models(args) => cli::models::execute(args).await,
        Commands::Config(args) => cli::config::execute(args).await,
    }
}
