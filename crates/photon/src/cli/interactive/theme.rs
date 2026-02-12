//! Custom dialoguer theme and banner for Photon interactive mode.
//!
//! Provides a pre-configured `ColorfulTheme` with Photon's visual identity
//! and a version banner for the interactive entry screen.

use console::{style, Style};
use dialoguer::theme::ColorfulTheme;

/// Returns a `ColorfulTheme` configured with Photon's visual identity.
///
/// - Prompt prefix: cyan `?`
/// - Active item indicator: cyan `▸`
/// - Success prefix: green `✓`
/// - Error prefix: red `✗`
pub fn photon_theme() -> ColorfulTheme {
    ColorfulTheme {
        prompt_prefix: style("?".to_string()).for_stderr().cyan(),
        prompt_style: Style::new().for_stderr().bold(),
        prompt_suffix: style("›".to_string()).for_stderr().bright().black(),
        active_item_prefix: style("▸".to_string()).for_stderr().cyan(),
        active_item_style: Style::new().for_stderr().cyan(),
        success_prefix: style("✓".to_string()).for_stderr().green(),
        success_suffix: style("·".to_string()).for_stderr().bright().black(),
        error_prefix: style("✗".to_string()).for_stderr().red(),
        error_style: Style::new().for_stderr().red(),
        values_style: Style::new().for_stderr().green(),
        ..ColorfulTheme::default()
    }
}

/// Prints the Photon banner to stderr.
///
/// Uses box-drawing characters with the version from `photon_core::VERSION`.
/// All output goes to stderr so stdout remains clean for piped data.
pub fn print_banner() {
    let version_line = format!("Photon v{}", photon_core::VERSION);
    let tagline = "AI-powered image processing pipeline";

    // Inner width: enough for the tagline + 4 chars padding (2 each side)
    let inner_width = tagline.len() + 4;

    let top = format!("  ╔{:═<width$}╗", "", width = inner_width);
    let mid1 = format!("  ║{:^width$}║", version_line, width = inner_width);
    let mid2 = format!("  ║{:^width$}║", tagline, width = inner_width);
    let bot = format!("  ╚{:═<width$}╝", "", width = inner_width);

    let cyan = Style::new().for_stderr().cyan();

    eprintln!();
    eprintln!("{}", cyan.apply_to(&top));
    eprintln!("{}", cyan.apply_to(&mid1));
    eprintln!("{}", cyan.apply_to(&mid2));
    eprintln!("{}", cyan.apply_to(&bot));
    eprintln!();
}
