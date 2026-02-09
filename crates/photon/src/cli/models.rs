//! The `photon models` command for managing AI models.

use clap::{Args, Subcommand};
use photon_core::Config;

/// Arguments for the `models` command.
#[derive(Args, Debug)]
pub struct ModelsArgs {
    #[command(subcommand)]
    pub command: ModelsCommand,
}

/// Subcommands for model management.
#[derive(Subcommand, Debug)]
pub enum ModelsCommand {
    /// Download required models (SigLIP)
    Download,

    /// List installed models
    List,

    /// Show model directory path
    Path,
}

/// Execute the models command.
pub async fn execute(args: ModelsArgs) -> anyhow::Result<()> {
    let config = Config::load()?;

    match args.command {
        ModelsCommand::Download => {
            tracing::info!("Downloading models...");
            let model_dir = config.model_dir();
            tracing::info!("Model directory: {:?}", model_dir);

            // Placeholder: actual download will be implemented in Phase 3
            tracing::warn!("Model download not yet implemented (Phase 3)");
            tracing::info!("Required model: {}", config.embedding.model);
        }

        ModelsCommand::List => {
            let model_dir = config.model_dir();

            if !model_dir.exists() {
                println!("No models installed.");
                println!("Run `photon models download` to download required models.");
                return Ok(());
            }

            println!("Installed models:");
            println!("  Directory: {}", model_dir.display());

            // List subdirectories as installed models
            if let Ok(entries) = std::fs::read_dir(&model_dir) {
                let mut found_any = false;
                for entry in entries.flatten() {
                    if entry.path().is_dir() {
                        println!("  - {}", entry.file_name().to_string_lossy());
                        found_any = true;
                    }
                }
                if !found_any {
                    println!("  (no models found)");
                }
            }
        }

        ModelsCommand::Path => {
            let model_dir = config.model_dir();
            println!("{}", model_dir.display());
        }
    }

    Ok(())
}
