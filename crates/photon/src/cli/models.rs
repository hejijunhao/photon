//! The `photon models` command for managing AI models.

use clap::{Args, Subcommand};
use photon_core::Config;
use std::path::Path;

/// Arguments for the `models` command.
#[derive(Args, Debug)]
pub struct ModelsArgs {
    #[command(subcommand)]
    pub command: ModelsCommand,
}

/// Subcommands for model management.
#[derive(Subcommand, Debug)]
pub enum ModelsCommand {
    /// Download required models (SigLIP vision + text encoder + tokenizer)
    Download,

    /// List installed models
    List,

    /// Show model directory path
    Path,
}

/// Available SigLIP vision model variants.
struct ModelVariant {
    name: &'static str,
    label: &'static str,
    repo: &'static str,
    remote_path: &'static str,
}

const VISION_VARIANTS: &[ModelVariant] = &[
    ModelVariant {
        name: "siglip-base-patch16",
        label: "Base (224)",
        repo: "Xenova/siglip-base-patch16-224",
        remote_path: "onnx/vision_model.onnx",
    },
    ModelVariant {
        name: "siglip-base-patch16-384",
        label: "Base (384)",
        repo: "Xenova/siglip-base-patch16-384",
        remote_path: "onnx/vision_model.onnx",
    },
];

/// Shared models (always downloaded alongside vision models).
const TEXT_ENCODER_REPO: &str = "Xenova/siglip-base-patch16-224";
const TEXT_ENCODER_REMOTE: &str = "onnx/text_model.onnx";
const TOKENIZER_REMOTE: &str = "tokenizer.json";

/// Local filenames.
const VISUAL_MODEL_LOCAL_NAME: &str = "visual.onnx";
const TEXT_MODEL_LOCAL_NAME: &str = "text_model.onnx";
const TOKENIZER_LOCAL_NAME: &str = "tokenizer.json";

/// Execute the models command.
pub async fn execute(args: ModelsArgs) -> anyhow::Result<()> {
    let config = Config::load()?;

    match args.command {
        ModelsCommand::Download => {
            let model_dir = config.model_dir();

            // Show available variants
            println!("Select SigLIP vision model(s) to download:\n");
            println!("  1) Base (224)    ~350MB  — fast, good for most use cases");
            println!("  2) Base (384)    ~350MB  — higher detail, 3-4x slower");
            println!("  3) Both          ~700MB  — switch per-run with --quality");
            println!("\n  Text encoder + tokenizer (~443MB, fp32) will also be downloaded.\n");

            // Default to option 1 (non-interactive for CI/automation)
            let selection = 1;
            tracing::info!("Downloading Base (224) variant (default)...");

            let variants_to_download: Vec<usize> = match selection {
                1 => vec![0],
                2 => vec![1],
                3 => vec![0, 1],
                _ => vec![0],
            };

            // Download selected vision model(s)
            for &idx in &variants_to_download {
                let variant = &VISION_VARIANTS[idx];
                let variant_dir = model_dir.join(variant.name);
                let dest = variant_dir.join(VISUAL_MODEL_LOCAL_NAME);

                if dest.exists() {
                    tracing::info!("{} already exists at {:?}", variant.label, dest);
                    continue;
                }

                std::fs::create_dir_all(&variant_dir)?;

                let url = format!(
                    "https://huggingface.co/{}/resolve/main/{}",
                    variant.repo, variant.remote_path
                );

                tracing::info!("Downloading {} vision encoder...", variant.label);
                tracing::info!("  Source: {}", url);
                tracing::info!("  Destination: {:?}", dest);

                download_file(&url, &dest).await?;

                let file_size = std::fs::metadata(&dest)?.len();
                tracing::info!(
                    "  {} complete ({:.1} MB)",
                    variant.label,
                    file_size as f64 / (1024.0 * 1024.0)
                );
            }

            // Download text encoder (shared, goes to models/ root)
            let text_dest = model_dir.join(TEXT_MODEL_LOCAL_NAME);
            if text_dest.exists() {
                tracing::info!("Text encoder already exists at {:?}", text_dest);
            } else {
                std::fs::create_dir_all(&model_dir)?;
                let url = format!(
                    "https://huggingface.co/{}/resolve/main/{}",
                    TEXT_ENCODER_REPO, TEXT_ENCODER_REMOTE
                );
                tracing::info!("Downloading text encoder (fp32)...");
                tracing::info!("  Source: {}", url);
                tracing::info!("  Destination: {:?}", text_dest);
                download_file(&url, &text_dest).await?;
                let file_size = std::fs::metadata(&text_dest)?.len();
                tracing::info!(
                    "  Text encoder complete ({:.1} MB)",
                    file_size as f64 / (1024.0 * 1024.0)
                );
            }

            // Download tokenizer (shared, goes to models/ root)
            let tok_dest = model_dir.join(TOKENIZER_LOCAL_NAME);
            if tok_dest.exists() {
                tracing::info!("Tokenizer already exists at {:?}", tok_dest);
            } else {
                let url = format!(
                    "https://huggingface.co/{}/resolve/main/{}",
                    TEXT_ENCODER_REPO, TOKENIZER_REMOTE
                );
                tracing::info!("Downloading tokenizer...");
                tracing::info!("  Source: {}", url);
                tracing::info!("  Destination: {:?}", tok_dest);
                download_file(&url, &tok_dest).await?;
                tracing::info!("  Tokenizer complete");
            }

            // Install vocabulary files
            install_vocabulary(&config)?;

            tracing::info!("All downloads complete.");
        }

        ModelsCommand::List => {
            let model_dir = config.model_dir();

            if !model_dir.exists() {
                println!("No models installed.");
                println!("Run `photon models download` to download required models.");
                return Ok(());
            }

            println!("Installed models:");
            println!("  Directory: {}\n", model_dir.display());

            // Vision encoders
            println!("  Vision encoders:");
            for variant in VISION_VARIANTS {
                let variant_dir = model_dir.join(variant.name);
                let visual_path = variant_dir.join(VISUAL_MODEL_LOCAL_NAME);
                let status = if visual_path.exists() {
                    "ready"
                } else {
                    "not installed"
                };
                let default_marker = if variant.name == config.embedding.model {
                    "  (default)"
                } else {
                    ""
                };
                println!("    - {:30} {:14}{}", variant.name, status, default_marker);
            }

            // Shared models
            println!("\n  Shared:");
            let text_path = model_dir.join(TEXT_MODEL_LOCAL_NAME);
            let text_status = if text_path.exists() {
                "ready"
            } else {
                "not installed"
            };
            println!("    - {:30} {}", TEXT_MODEL_LOCAL_NAME, text_status);

            let tok_path = model_dir.join(TOKENIZER_LOCAL_NAME);
            let tok_status = if tok_path.exists() {
                "ready"
            } else {
                "not installed"
            };
            println!("    - {:30} {}", TOKENIZER_LOCAL_NAME, tok_status);

            // Vocabulary
            let vocab_dir = config.vocabulary_dir();
            println!("\n  Vocabulary:");
            let nouns_path = vocab_dir.join("wordnet_nouns.txt");
            let nouns_status = if nouns_path.exists() {
                "ready"
            } else {
                "not installed"
            };
            println!("    - {:30} {}", "wordnet_nouns.txt", nouns_status);

            let supp_path = vocab_dir.join("supplemental.txt");
            let supp_status = if supp_path.exists() {
                "ready"
            } else {
                "not installed"
            };
            println!("    - {:30} {}", "supplemental.txt", supp_status);
        }

        ModelsCommand::Path => {
            let model_dir = config.model_dir();
            println!("{}", model_dir.display());
        }
    }

    Ok(())
}

/// Install embedded vocabulary files to the vocabulary directory.
fn install_vocabulary(config: &Config) -> anyhow::Result<()> {
    let vocab_dir = config.vocabulary_dir();

    let nouns_path = vocab_dir.join("wordnet_nouns.txt");
    let supp_path = vocab_dir.join("supplemental.txt");

    if nouns_path.exists() && supp_path.exists() {
        tracing::info!("Vocabulary files already installed at {:?}", vocab_dir);
        return Ok(());
    }

    std::fs::create_dir_all(&vocab_dir)?;

    if !nouns_path.exists() {
        let nouns_data = include_str!("../../../../data/vocabulary/wordnet_nouns.txt");
        std::fs::write(&nouns_path, nouns_data)?;
        tracing::info!("Installed wordnet_nouns.txt to {:?}", nouns_path);
    }

    if !supp_path.exists() {
        let supp_data = include_str!("../../../../data/vocabulary/supplemental.txt");
        std::fs::write(&supp_path, supp_data)?;
        tracing::info!("Installed supplemental.txt to {:?}", supp_path);
    }

    Ok(())
}

/// Download a file from a URL to a local path, streaming to disk.
async fn download_file(url: &str, dest: &Path) -> anyhow::Result<()> {
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .send()
        .await?
        .error_for_status()
        .map_err(|e| anyhow::anyhow!("Download failed: {e}"))?;

    let total_size = response.content_length();
    if let Some(size) = total_size {
        tracing::info!("  Size: {:.1} MB", size as f64 / (1024.0 * 1024.0));
    }

    let mut file = tokio::fs::File::create(dest).await?;
    let mut stream = response.bytes_stream();
    let mut downloaded: u64 = 0;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;

        if let Some(total) = total_size {
            if downloaded % (50 * 1024 * 1024) < chunk.len() as u64 {
                tracing::info!(
                    "  Progress: {:.0}%",
                    downloaded as f64 / total as f64 * 100.0
                );
            }
        }
    }

    file.flush().await?;
    Ok(())
}
