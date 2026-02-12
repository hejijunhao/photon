//! Interactive CLI mode — guided experience for bare `photon` invocation.
//!
//! When `photon` is invoked with no subcommand on a TTY, this module provides
//! a menu-driven interface that delegates to the same processing logic as the
//! flag-based CLI.

pub mod models;
pub mod process;
pub mod setup;
pub mod theme;

use console::Style;
use dialoguer::Select;
use photon_core::Config;

/// Convert a dialoguer result into `Ok(Some(value))` on success, `Ok(None)` on
/// interrupt (Ctrl+C / terminal disconnect), and `Err` for other I/O failures.
///
/// Use this to wrap `interact_text()` / `interact()` calls that lack an `_opt`
/// variant, so interrupts exit the current flow cleanly instead of panicking.
fn handle_interrupt<T>(result: dialoguer::Result<T>) -> anyhow::Result<Option<T>> {
    match result {
        Ok(v) => Ok(Some(v)),
        Err(dialoguer::Error::IO(e)) if e.kind() == std::io::ErrorKind::Interrupted => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Main menu options presented to the user.
const MENU_ITEMS: &[&str] = &[
    "Process images",
    "Download / manage models",
    "Configure settings",
    "Exit",
];

/// Entry point for interactive mode. Called when `photon` is invoked with no subcommand on a TTY.
pub async fn run(config: &Config) -> anyhow::Result<()> {
    theme::print_banner();

    let theme = theme::photon_theme();

    loop {
        let selection = Select::with_theme(&theme)
            .with_prompt("What would you like to do?")
            .items(MENU_ITEMS)
            .default(0)
            .interact_opt()?;

        match selection {
            Some(0) => process::guided_process(config).await?,
            Some(1) => models::guided_models(config).await?,
            Some(2) => show_config(config)?,
            Some(3) | None => break, // Exit or Ctrl+C / Esc
            _ => unreachable!(),
        }
    }

    Ok(())
}

/// Interactive config viewer — shows a summary of current settings and offers
/// to display the full TOML or the config file path.
fn show_config(config: &Config) -> anyhow::Result<()> {
    let theme = theme::photon_theme();
    let dim = Style::new().for_stderr().dim();
    let cyan = Style::new().for_stderr().cyan();
    let label = Style::new().for_stderr().bold();

    loop {
        // Config summary
        eprintln!();
        eprintln!("  {}", cyan.apply_to("Current configuration:"));
        eprintln!();

        let config_path = Config::default_path();
        let path_note = if config_path.exists() {
            "(exists)"
        } else {
            "(using defaults)"
        };

        eprintln!(
            "    {:<20} {} {}",
            label.apply_to("Config file:"),
            config_path.display(),
            dim.apply_to(path_note)
        );
        eprintln!(
            "    {:<20} {}",
            label.apply_to("Model dir:"),
            config.model_dir().display()
        );
        eprintln!(
            "    {:<20} {} workers",
            label.apply_to("Parallel:"),
            config.processing.parallel_workers
        );
        eprintln!(
            "    {:<20} {}px {}",
            label.apply_to("Thumbnail:"),
            config.thumbnail.size,
            config.thumbnail.format
        );
        eprintln!(
            "    {:<20} {}",
            label.apply_to("Embedding model:"),
            config.embedding.model
        );
        eprintln!(
            "    {:<20} {} (min confidence: {})",
            label.apply_to("Tagging:"),
            if config.tagging.enabled {
                format!("up to {} tags", config.tagging.max_tags)
            } else {
                "disabled".to_string()
            },
            config.tagging.min_confidence
        );
        eprintln!(
            "    {:<20} {}",
            label.apply_to("Log level:"),
            config.logging.level
        );
        eprintln!(
            "    {:<20} {}",
            label.apply_to("LLM providers:"),
            llm_summary(config)
        );
        eprintln!();

        // Action menu
        let items = &["View full config (TOML)", "Show config file path", "Back"];

        let selection = Select::with_theme(&theme)
            .with_prompt("Configuration")
            .items(items)
            .default(0)
            .interact_opt()?;

        match selection {
            Some(0) => match config.to_toml() {
                Ok(toml) => {
                    eprintln!();
                    eprintln!("{}", dim.apply_to("─".repeat(50)));
                    eprintln!("{toml}");
                    eprintln!("{}", dim.apply_to("─".repeat(50)));
                    eprintln!();
                }
                Err(e) => {
                    let err = Style::new().for_stderr().red();
                    eprintln!("  {} Failed to serialize config: {e}", err.apply_to("✗"));
                    eprintln!();
                }
            },
            Some(1) => {
                eprintln!();
                eprintln!("  {}", Config::default_path().display());
                eprintln!();
            }
            Some(2) | None => break, // Back or Esc / Ctrl+C
            _ => unreachable!(),
        }
    }

    Ok(())
}

/// Summarise which LLM providers are enabled in the config.
fn llm_summary(config: &Config) -> String {
    let mut providers = Vec::new();

    if let Some(ref c) = config.llm.ollama {
        if c.enabled {
            providers.push("Ollama");
        }
    }
    if let Some(ref c) = config.llm.anthropic {
        if c.enabled {
            providers.push("Anthropic");
        }
    }
    if let Some(ref c) = config.llm.openai {
        if c.enabled {
            providers.push("OpenAI");
        }
    }
    if let Some(ref c) = config.llm.hyperbolic {
        if c.enabled {
            providers.push("Hyperbolic");
        }
    }

    if providers.is_empty() {
        "none configured".to_string()
    } else {
        providers.join(", ")
    }
}
