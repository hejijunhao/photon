//! Guided image processing flow.
//!
//! Walks the user through: input path → file discovery → model check →
//! quality preset → LLM provider → output format → confirmation → processing.
//! Builds a `ProcessArgs` and delegates to `cli::process::execute()`.

use crate::cli::models::check_installed;
use crate::cli::process::{OutputFormat, ProcessArgs, Quality};
use console::Style;
use dialoguer::{Confirm, Input, Select};
use photon_core::pipeline::FileDiscovery;
use photon_core::Config;
use std::path::PathBuf;

use super::theme::photon_theme;

/// Walk the user through the full processing flow.
pub async fn guided_process(config: &Config) -> anyhow::Result<()> {
    let theme = photon_theme();

    // ── Steps 1+2: Input path with file discovery ─────────────────────────
    // Combined loop: re-prompts on both "path not found" and "no images found".

    let (input, files) = loop {
        let Some(raw_path) = super::handle_interrupt(
            Input::<String>::with_theme(&theme)
                .with_prompt("Path to image or folder")
                .interact_text(),
        )?
        else {
            return Ok(());
        };

        let path = PathBuf::from(shellexpand::tilde(&raw_path).into_owned());

        if !path.exists() {
            let warn = Style::new().for_stderr().yellow();
            eprintln!(
                "  {}",
                warn.apply_to(format!("Path not found: {}", path.display()))
            );
            continue;
        }

        let discovery = FileDiscovery::new(config.processing.clone());
        let found = discovery.discover(&path);

        if found.is_empty() {
            let warn = Style::new().for_stderr().yellow();
            eprintln!(
                "  {}",
                warn.apply_to("No supported images found at that path.")
            );
            continue;
        }

        break (path, found);
    };

    let total_size = FileDiscovery::total_size(&files);
    let dim = Style::new().for_stderr().dim();
    eprintln!(
        "  {}",
        dim.apply_to(format!(
            "Found {} image(s) ({:.1} MB)",
            files.len(),
            total_size as f64 / 1_000_000.0
        ))
    );

    // ── Step 3: Model check ─────────────────────────────────────────────────

    let status = check_installed(config);
    if !status.can_process() {
        let warn = Style::new().for_stderr().yellow();
        eprintln!("  {}", warn.apply_to("Required models not installed."));

        let install = Confirm::with_theme(&theme)
            .with_prompt("Download models now?")
            .default(true)
            .interact_opt()?;

        match install {
            Some(true) => {
                super::models::guided_models(config).await?;
                // Re-check after download
                let status = check_installed(config);
                if !status.can_process() {
                    eprintln!("  Models still missing. Returning to menu.");
                    return Ok(());
                }
            }
            _ => return Ok(()),
        }
    }

    // ── Step 4: Quality preset ──────────────────────────────────────────────

    let quality_items = &["Fast (default) — 224px model", "High detail — 384px model"];
    let quality_choice = Select::with_theme(&theme)
        .with_prompt("Quality preset")
        .items(quality_items)
        .default(0)
        .interact_opt()?;

    let quality = match quality_choice {
        Some(1) => Quality::High,
        Some(0) => Quality::Fast,
        None => return Ok(()), // Esc
        _ => Quality::Fast,
    };

    // ── Step 5: LLM provider ───────────────────────────────────────────────

    let llm_selection = super::setup::select_llm_provider(config)?;
    let (llm, llm_model) = match llm_selection {
        Some(sel) => (Some(sel.provider), Some(sel.model)),
        None => (None, None),
    };

    // ── Step 6: Output format ───────────────────────────────────────────────

    let is_batch = files.len() > 1;
    let output_items = if is_batch {
        vec![
            "JSONL file (recommended for batches)",
            "JSON array file",
            "Stream to stdout",
        ]
    } else {
        vec!["JSON to stdout", "JSON to file", "JSONL to file"]
    };

    let output_choice = Select::with_theme(&theme)
        .with_prompt("Output format")
        .items(&output_items)
        .default(0)
        .interact_opt()?;

    let Some(output_choice) = output_choice else {
        return Ok(()); // Esc
    };

    let (output, format) = if is_batch {
        match output_choice {
            0 => {
                // JSONL file
                let path = prompt_output_path(&theme, "results.jsonl")?;
                (path, OutputFormat::Jsonl)
            }
            1 => {
                // JSON array file
                let path = prompt_output_path(&theme, "results.json")?;
                (path, OutputFormat::Json)
            }
            _ => (None, OutputFormat::Jsonl), // stdout
        }
    } else {
        match output_choice {
            0 => (None, OutputFormat::Json), // stdout
            1 => {
                let path = prompt_output_path(&theme, "result.json")?;
                (path, OutputFormat::Json)
            }
            2 => {
                let path = prompt_output_path(&theme, "result.jsonl")?;
                (path, OutputFormat::Jsonl)
            }
            _ => (None, OutputFormat::Json),
        }
    };

    // ── Step 7: Confirmation ────────────────────────────────────────────────

    eprintln!();
    let bold = Style::new().for_stderr().bold();
    let dim = Style::new().for_stderr().dim();
    eprintln!(
        "  {}",
        bold.apply_to(format!("Ready to process {} image(s)", files.len()))
    );
    let llm_label = match &llm {
        Some(p) => format!("{p}"),
        None => "off".to_string(),
    };
    let output_label = match &output {
        Some(p) => p.display().to_string(),
        None => "stdout".to_string(),
    };
    eprintln!(
        "  {}",
        dim.apply_to(format!(
            "Quality: {quality:?} | LLM: {llm_label} | Output: {output_label}"
        ))
    );
    eprintln!();

    let confirm = Confirm::with_theme(&theme)
        .with_prompt("Start processing?")
        .default(true)
        .interact_opt()?;

    if !matches!(confirm, Some(true)) {
        return Ok(());
    }

    // ── Step 8: Build ProcessArgs and delegate ──────────────────────────────

    let args = ProcessArgs {
        input,
        output,
        format,
        quality,
        llm,
        llm_model,
        ..ProcessArgs::default()
    };

    crate::cli::process::execute(args).await?;

    // ── Post-processing menu ────────────────────────────────────────────────

    eprintln!();
    let post_items = &["Process more images", "Back to main menu"];
    let post_choice = Select::with_theme(&theme)
        .with_prompt("What next?")
        .items(post_items)
        .default(0)
        .interact_opt()?;

    if matches!(post_choice, Some(0)) {
        // Recurse into another guided_process
        Box::pin(guided_process(config)).await?;
    }

    Ok(())
}

/// Prompt for an output file path with a default.
/// Returns `Ok(None)` if the user interrupts (Ctrl+C).
fn prompt_output_path(
    theme: &dialoguer::theme::ColorfulTheme,
    default: &str,
) -> anyhow::Result<Option<PathBuf>> {
    let Some(path) = super::handle_interrupt(
        Input::<String>::with_theme(theme)
            .with_prompt("Output file path")
            .default(format!("./{default}"))
            .interact_text(),
    )?
    else {
        return Ok(None);
    };
    Ok(Some(PathBuf::from(shellexpand::tilde(&path).into_owned())))
}
