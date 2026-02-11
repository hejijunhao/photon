//! Logging initialization and configuration.
//!
//! Uses the `tracing` ecosystem for structured logging with support for
//! both human-readable and JSON output formats.

use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Initialize the logging subsystem.
///
/// # Arguments
///
/// * `verbose` - If true, enables DEBUG level logging; otherwise INFO level.
/// * `json_format` - If true, outputs structured JSON logs; otherwise pretty-printed.
///
/// # Notes
///
/// - Log output goes to stderr (stdout is reserved for data output)
/// - The RUST_LOG environment variable can override the log level
pub fn init(verbose: bool, json_format: bool) {
    // Build the filter, respecting RUST_LOG if set
    let default_level = if verbose { "debug" } else { "info" };
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level));

    if json_format {
        // JSON format for machine parsing
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt::layer().json().with_writer(std::io::stderr))
            .init();
    } else {
        // Pretty format for humans
        tracing_subscriber::registry()
            .with(filter)
            .with(
                fmt::layer()
                    .with_target(false)
                    .with_writer(std::io::stderr)
                    .with_ansi(true),
            )
            .init();
    }
}

/// Initialize logging with configuration from Config.
///
/// This variant reads settings from the Photon configuration file.
pub fn init_from_config(
    config: &photon_core::Config,
    verbose_override: bool,
    json_logs_override: bool,
) {
    let verbose =
        verbose_override || config.logging.level == "debug" || config.logging.level == "trace";
    let json_format = json_logs_override || config.logging.format == "json";
    init(verbose, json_format);
}
