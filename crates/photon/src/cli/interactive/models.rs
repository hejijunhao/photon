//! Guided model management — check install status and offer downloads.

use crate::cli::models::{
    check_installed, download_shared, download_vision, install_vocabulary, InstalledModels,
    VARIANT_LABELS,
};
use console::Style;
use dialoguer::Select;
use photon_core::Config;

use super::theme::photon_theme;

/// Show installed model status and offer download options.
pub async fn guided_models(config: &Config) -> anyhow::Result<()> {
    let theme = photon_theme();

    loop {
        let status = check_installed(config);
        print_status(&status, config);

        let mut items: Vec<String> = Vec::new();
        let mut actions: Vec<ModelAction> = Vec::new();

        // Offer downloads for missing components
        if !status.vision_224 {
            items.push("Download Base (224) vision model".to_string());
            actions.push(ModelAction::DownloadVision(vec![0]));
        }
        if !status.vision_384 {
            items.push("Download Base (384) vision model".to_string());
            actions.push(ModelAction::DownloadVision(vec![1]));
        }
        if !status.vision_224 && !status.vision_384 {
            items.push("Download both vision models".to_string());
            actions.push(ModelAction::DownloadVision(vec![0, 1]));
        }
        if !status.text_encoder || !status.tokenizer {
            items.push("Download text encoder + tokenizer".to_string());
            actions.push(ModelAction::DownloadShared);
        }
        if !status.vocabulary {
            items.push("Install vocabulary files".to_string());
            actions.push(ModelAction::InstallVocabulary);
        }

        // Always available
        items.push("Show model directory".to_string());
        actions.push(ModelAction::ShowPath);
        items.push("Back".to_string());
        actions.push(ModelAction::Back);

        let selection = Select::with_theme(&theme)
            .with_prompt("Model management")
            .items(&items)
            .default(0)
            .interact_opt()?;

        match selection {
            Some(idx) => match &actions[idx] {
                ModelAction::DownloadVision(indices) => {
                    let client = reqwest::Client::new();
                    download_vision(indices, config, &client).await?;
                    download_shared(config, &client).await?;
                    install_vocabulary(config)?;
                    eprintln!();
                    let done = Style::new().for_stderr().green();
                    eprintln!("{}", done.apply_to("  Downloads complete."));
                    eprintln!();
                }
                ModelAction::DownloadShared => {
                    let client = reqwest::Client::new();
                    download_shared(config, &client).await?;
                    eprintln!();
                    let done = Style::new().for_stderr().green();
                    eprintln!("{}", done.apply_to("  Downloads complete."));
                    eprintln!();
                }
                ModelAction::InstallVocabulary => {
                    install_vocabulary(config)?;
                    let done = Style::new().for_stderr().green();
                    eprintln!("{}", done.apply_to("  Vocabulary installed."));
                    eprintln!();
                }
                ModelAction::ShowPath => {
                    eprintln!("  {}", config.model_dir().display());
                    eprintln!();
                }
                ModelAction::Back => break,
            },
            None => break, // Esc / Ctrl+C
        }
    }

    Ok(())
}

/// Print the current install status of all model components.
fn print_status(status: &InstalledModels, config: &Config) {
    let ok = Style::new().for_stderr().green();
    let missing = Style::new().for_stderr().red();
    let dim = Style::new().for_stderr().dim();

    let check = |installed: bool, label: &str, detail: &str| {
        if installed {
            eprintln!(
                "  {} {:<26} {}",
                ok.apply_to("✓"),
                label,
                dim.apply_to(detail)
            );
        } else {
            eprintln!(
                "  {} {:<26} {}",
                missing.apply_to("✗"),
                label,
                dim.apply_to("not installed")
            );
        }
    };

    eprintln!();
    eprintln!(
        "  {}",
        dim.apply_to(format!("Model directory: {}", config.model_dir().display()))
    );
    eprintln!();

    check(status.vision_224, VARIANT_LABELS[0], "visual.onnx ~348 MB");
    check(status.vision_384, VARIANT_LABELS[1], "visual.onnx ~348 MB");
    check(
        status.text_encoder,
        "Text encoder",
        "text_model.onnx ~443 MB",
    );
    check(status.tokenizer, "Tokenizer", "tokenizer.json ~1 MB");
    check(status.vocabulary, "Vocabulary", "wordnet + supplemental");
    eprintln!();
}

/// Internal action type for the model menu.
enum ModelAction {
    DownloadVision(Vec<usize>),
    DownloadShared,
    InstallVocabulary,
    ShowPath,
    Back,
}
