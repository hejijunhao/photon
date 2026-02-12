//! The `photon models` command for managing AI models.

use clap::{Args, Subcommand};
use photon_core::pipeline::Hasher;
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
    blake3: &'static str,
}

const VISION_VARIANTS: &[ModelVariant] = &[
    ModelVariant {
        name: "siglip-base-patch16",
        label: "Base (224)",
        repo: "Xenova/siglip-base-patch16-224",
        remote_path: "onnx/vision_model.onnx",
        blake3: "05cd313b67db70acd8e800cd4c16105c3ebc4c385fe6002108d24ea806a248be",
    },
    ModelVariant {
        name: "siglip-base-patch16-384",
        label: "Base (384)",
        repo: "Xenova/siglip-base-patch16-384",
        remote_path: "onnx/vision_model.onnx",
        blake3: "9a4dcfd0c21b8e4d143652d1e566da52222605b564979723383f6012b53dd0df",
    },
];

/// Shared models (always downloaded alongside vision models).
const TEXT_ENCODER_REPO: &str = "Xenova/siglip-base-patch16-224";
const TEXT_ENCODER_REMOTE: &str = "onnx/text_model.onnx";
const TOKENIZER_REMOTE: &str = "tokenizer.json";

/// Expected BLAKE3 checksums for shared model files.
const TEXT_ENCODER_BLAKE3: &str =
    "fe62b4096a9e5c3ce735b771472c9e3faac6ddeceebab5794a0a5ce17ee171dd";
const TOKENIZER_BLAKE3: &str = "cf171f3552992f467891b9d59be5bde1256ffe1344c62030d4bf0f87df583906";

/// Local filenames.
const VISUAL_MODEL_LOCAL_NAME: &str = "visual.onnx";
const TEXT_MODEL_LOCAL_NAME: &str = "text_model.onnx";
const TOKENIZER_LOCAL_NAME: &str = "tokenizer.json";

// ── Reusable public API (used by both flag-based CLI and interactive module) ──

/// Status of each model file on disk.
pub struct InstalledModels {
    pub vision_224: bool,
    pub vision_384: bool,
    pub text_encoder: bool,
    pub tokenizer: bool,
    pub vocabulary: bool,
}

impl InstalledModels {
    /// Returns true if the minimum required models are present for processing.
    pub fn can_process(&self) -> bool {
        (self.vision_224 || self.vision_384) && self.text_encoder && self.tokenizer
    }
}

/// Check which models are currently installed.
pub fn check_installed(config: &Config) -> InstalledModels {
    let model_dir = config.model_dir();
    let vocab_dir = config.vocabulary_dir();

    InstalledModels {
        vision_224: model_dir
            .join(VISION_VARIANTS[0].name)
            .join(VISUAL_MODEL_LOCAL_NAME)
            .exists(),
        vision_384: model_dir
            .join(VISION_VARIANTS[1].name)
            .join(VISUAL_MODEL_LOCAL_NAME)
            .exists(),
        text_encoder: model_dir.join(TEXT_MODEL_LOCAL_NAME).exists(),
        tokenizer: model_dir.join(TOKENIZER_LOCAL_NAME).exists(),
        vocabulary: vocab_dir.join("wordnet_nouns.txt").exists()
            && vocab_dir.join("supplemental.txt").exists(),
    }
}

/// Download vision model variant(s) by index (0 = Base 224, 1 = Base 384).
///
/// Skips already-downloaded files.
pub async fn download_vision(
    variant_indices: &[usize],
    config: &Config,
    client: &reqwest::Client,
) -> anyhow::Result<()> {
    let model_dir = config.model_dir();

    for &idx in variant_indices {
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

        download_file(client, &url, &dest, Some(variant.blake3)).await?;

        let file_size = std::fs::metadata(&dest)?.len();
        tracing::info!(
            "  {} complete ({:.1} MB)",
            variant.label,
            file_size as f64 / (1024.0 * 1024.0)
        );
    }

    Ok(())
}

/// Download shared text encoder and tokenizer. Skips if already present.
pub async fn download_shared(config: &Config, client: &reqwest::Client) -> anyhow::Result<()> {
    let model_dir = config.model_dir();

    // Text encoder
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
        download_file(client, &url, &text_dest, Some(TEXT_ENCODER_BLAKE3)).await?;
        let file_size = std::fs::metadata(&text_dest)?.len();
        tracing::info!(
            "  Text encoder complete ({:.1} MB)",
            file_size as f64 / (1024.0 * 1024.0)
        );
    }

    // Tokenizer
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
        download_file(client, &url, &tok_dest, Some(TOKENIZER_BLAKE3)).await?;
        tracing::info!("  Tokenizer complete");
    }

    Ok(())
}

/// Public variant labels for display in interactive mode.
pub const VARIANT_LABELS: &[&str] = &["Base (224)", "Base (384)"];

/// Execute the models command.
pub async fn execute(args: ModelsArgs) -> anyhow::Result<()> {
    let config = Config::load()?;

    match args.command {
        ModelsCommand::Download => {
            // Show available variants
            println!("Select SigLIP vision model(s) to download:\n");
            println!("  1) Base (224)    ~350MB  — fast, good for most use cases");
            println!("  2) Base (384)    ~350MB  — higher detail, 3-4x slower");
            println!("  3) Both          ~700MB  — switch per-run with --quality");
            println!("\n  Text encoder + tokenizer (~443MB, fp32) will also be downloaded.\n");

            // Default to option 1 (non-interactive for CI/automation)
            tracing::info!("Downloading Base (224) variant (default)...");

            let client = reqwest::Client::new();

            download_vision(&[0], &config, &client).await?;
            download_shared(&config, &client).await?;
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
pub fn install_vocabulary(config: &Config) -> anyhow::Result<()> {
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
///
/// If `expected_blake3` is provided, the file is verified after download.
/// On checksum mismatch the corrupt file is removed and an error is returned.
async fn download_file(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    expected_blake3: Option<&str>,
) -> anyhow::Result<()> {
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

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

    // Verify checksum if expected hash is provided
    if let Some(expected) = expected_blake3 {
        verify_blake3(dest, expected)?;
    }

    Ok(())
}

/// Verify a downloaded file's BLAKE3 checksum.
///
/// On mismatch, removes the corrupt file so the next run re-downloads.
fn verify_blake3(path: &Path, expected: &str) -> anyhow::Result<()> {
    let actual = Hasher::content_hash(path)
        .map_err(|e| anyhow::anyhow!("Checksum computation failed for {}: {e}", path.display()))?;

    if actual != expected {
        let _ = std::fs::remove_file(path);
        anyhow::bail!(
            "Checksum mismatch for {}:\n  expected: {}\n  actual:   {}\n\
             Corrupt file removed — try downloading again.",
            path.display(),
            expected,
            actual
        );
    }

    tracing::debug!("  Checksum verified: {}…", &actual[..16]);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_file(name: &str, content: &[u8]) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("photon_test_{name}"));
        std::fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn verify_blake3_correct_hash() {
        let path = test_file("verify_ok", b"hello photon");
        let expected = Hasher::content_hash(&path).unwrap();

        assert!(verify_blake3(&path, &expected).is_ok());
        assert!(
            path.exists(),
            "file should still exist after successful verify"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn verify_blake3_wrong_hash_removes_file() {
        let path = test_file("verify_bad", b"hello photon");
        let wrong_hash = "0000000000000000000000000000000000000000000000000000000000000000";

        let result = verify_blake3(&path, wrong_hash);

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Checksum mismatch"),
            "error should mention mismatch: {err_msg}"
        );
        assert!(
            err_msg.contains("Corrupt file removed"),
            "error should mention removal: {err_msg}"
        );
        assert!(!path.exists(), "corrupt file should be deleted");
    }

    #[test]
    fn verify_blake3_missing_file() {
        let result = verify_blake3(
            Path::new("/nonexistent/file.onnx"),
            "0000000000000000000000000000000000000000000000000000000000000000",
        );
        assert!(result.is_err());
    }
}
