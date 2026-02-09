//! The `photon config` command for configuration management.

use clap::{Args, Subcommand};
use photon_core::Config;

/// Arguments for the `config` command.
#[derive(Args, Debug)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub command: ConfigCommand,
}

/// Subcommands for configuration management.
#[derive(Subcommand, Debug)]
pub enum ConfigCommand {
    /// Display current configuration
    Show,

    /// Show config file path
    Path,

    /// Initialize a new config file with defaults
    Init {
        /// Overwrite existing config file
        #[arg(long)]
        force: bool,
    },
}

/// Execute the config command.
pub async fn execute(args: ConfigArgs) -> anyhow::Result<()> {
    match args.command {
        ConfigCommand::Show => {
            let config = Config::load()?;
            let toml = config.to_toml()?;
            println!("{}", toml);
        }

        ConfigCommand::Path => {
            let path = Config::default_path();
            println!("{}", path.display());
        }

        ConfigCommand::Init { force } => {
            let path = Config::default_path();

            if path.exists() && !force {
                anyhow::bail!(
                    "Config file already exists at: {}\nUse --force to overwrite.",
                    path.display()
                );
            }

            // Ensure parent directory exists
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            // Write default config
            let config = Config::default();
            let toml = config.to_toml()?;
            std::fs::write(&path, toml)?;

            tracing::info!("Config file created at: {}", path.display());
            println!("Configuration initialized at: {}", path.display());
        }
    }

    Ok(())
}
