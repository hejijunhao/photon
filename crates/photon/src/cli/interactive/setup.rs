//! LLM provider setup — API key detection, input, and optional persistence.

use crate::cli::process::LlmProvider;
use console::Style;
use dialoguer::{Input, Password, Select};
use photon_core::Config;

use super::theme::photon_theme;

/// Result of the LLM provider selection flow.
pub struct LlmSelection {
    pub provider: LlmProvider,
    pub model: String,
    /// API key entered during this session (not from env/config).
    pub api_key: Option<String>,
}

/// Guide the user through selecting an LLM provider, API key, and model.
///
/// Returns `None` if the user skips LLM or cancels.
pub fn select_llm_provider(config: &Config) -> anyhow::Result<Option<LlmSelection>> {
    let theme = photon_theme();
    let dim = Style::new().for_stderr().dim();
    let warn = Style::new().for_stderr().yellow();

    // Step 1: Choose provider
    let providers = &[
        "Skip (no LLM descriptions)",
        "Anthropic (Claude)",
        "OpenAI",
        "Ollama (local)",
        "Hyperbolic",
    ];

    let selection = Select::with_theme(&theme)
        .with_prompt("LLM provider for image descriptions")
        .items(providers)
        .default(0)
        .interact_opt()?;

    let provider = match selection {
        Some(1) => LlmProvider::Anthropic,
        Some(2) => LlmProvider::Openai,
        Some(3) => LlmProvider::Ollama,
        Some(4) => LlmProvider::Hyperbolic,
        _ => return Ok(None), // Skip, Esc, or Ctrl+C
    };

    // Step 2: API key handling (Ollama doesn't need one)
    let mut session_api_key: Option<String> = None;

    if !matches!(provider, LlmProvider::Ollama) {
        let env_var = env_var_for(&provider);
        let has_key = std::env::var(env_var).is_ok() || config_has_key(config, &provider);

        if has_key {
            eprintln!(
                "  {}",
                dim.apply_to(format!("Using existing API key from {env_var} / config"))
            );
        } else {
            eprintln!("  {}", warn.apply_to(format!("{env_var} not set.")));

            let key: String = match Password::with_theme(&theme)
                .with_prompt(format!(
                    "Enter your {} API key (Esc to skip)",
                    provider_label(&provider)
                ))
                .allow_empty_password(true)
                .interact()
            {
                Ok(k) if !k.is_empty() => k,
                _ => return Ok(None), // Empty or error → skip
            };

            // Step 2b: Save or use session-only
            let save_options = &["Yes, save to config file", "No, use for this session only"];
            let save_choice = Select::with_theme(&theme)
                .with_prompt("Save this key for future sessions?")
                .items(save_options)
                .default(0)
                .interact_opt()?;

            match save_choice {
                Some(0) => {
                    // Persist to config TOML and also keep for this session
                    if let Err(e) = save_key_to_config(&provider, &key) {
                        eprintln!(
                            "  {}",
                            warn.apply_to(format!("Could not save to config: {e}"))
                        );
                        eprintln!("  Using key for this session only.");
                    }
                    session_api_key = Some(key);
                }
                Some(1) => {
                    session_api_key = Some(key);
                }
                _ => return Ok(None), // Cancelled / Esc
            }
        }
    }

    // Step 3: Model selection
    let model = select_model(&provider, &theme)?;
    let Some(model) = model else {
        return Ok(None);
    };

    Ok(Some(LlmSelection {
        provider,
        model,
        api_key: session_api_key,
    }))
}

/// Prompt for model name based on provider.
fn select_model(
    provider: &LlmProvider,
    theme: &dialoguer::theme::ColorfulTheme,
) -> anyhow::Result<Option<String>> {
    match provider {
        LlmProvider::Anthropic => {
            let models = &[
                "claude-sonnet-4-20250514 (recommended)",
                "claude-haiku-4-5-20251001 (faster, cheaper)",
                "Custom model name...",
            ];
            let choice = Select::with_theme(theme)
                .with_prompt("Anthropic model")
                .items(models)
                .default(0)
                .interact_opt()?;

            match choice {
                Some(0) => Ok(Some("claude-sonnet-4-20250514".to_string())),
                Some(1) => Ok(Some("claude-haiku-4-5-20251001".to_string())),
                Some(2) => prompt_custom_model(theme),
                _ => Ok(None),
            }
        }
        LlmProvider::Openai => {
            let models = &[
                "gpt-4o (recommended)",
                "gpt-4o-mini (faster, cheaper)",
                "Custom model name...",
            ];
            let choice = Select::with_theme(theme)
                .with_prompt("OpenAI model")
                .items(models)
                .default(0)
                .interact_opt()?;

            match choice {
                Some(0) => Ok(Some("gpt-4o".to_string())),
                Some(1) => Ok(Some("gpt-4o-mini".to_string())),
                Some(2) => prompt_custom_model(theme),
                _ => Ok(None),
            }
        }
        LlmProvider::Ollama => {
            let model = super::handle_interrupt(
                Input::<String>::with_theme(theme)
                    .with_prompt("Ollama model name")
                    .default("llama3.2-vision".to_string())
                    .interact_text(),
            )?;
            match model {
                Some(m) if !m.trim().is_empty() => Ok(Some(m)),
                _ => Ok(None),
            }
        }
        LlmProvider::Hyperbolic => {
            let model = super::handle_interrupt(
                Input::<String>::with_theme(theme)
                    .with_prompt("Hyperbolic model name")
                    .default("meta-llama/Llama-3.2-11B-Vision-Instruct".to_string())
                    .interact_text(),
            )?;
            match model {
                Some(m) if !m.trim().is_empty() => Ok(Some(m)),
                _ => Ok(None),
            }
        }
    }
}

/// Prompt for a custom model name.
fn prompt_custom_model(theme: &dialoguer::theme::ColorfulTheme) -> anyhow::Result<Option<String>> {
    let Some(model) = super::handle_interrupt(
        Input::<String>::with_theme(theme)
            .with_prompt("Model name")
            .interact_text(),
    )?
    else {
        return Ok(None);
    };
    if model.is_empty() {
        Ok(None)
    } else {
        Ok(Some(model))
    }
}

/// Get the environment variable name for a provider's API key.
pub(crate) fn env_var_for(provider: &LlmProvider) -> &'static str {
    match provider {
        LlmProvider::Anthropic => "ANTHROPIC_API_KEY",
        LlmProvider::Openai => "OPENAI_API_KEY",
        LlmProvider::Hyperbolic => "HYPERBOLIC_API_KEY",
        LlmProvider::Ollama => "OLLAMA_HOST", // not really used, but consistent
    }
}

/// Human-readable label for a provider.
pub(crate) fn provider_label(provider: &LlmProvider) -> &'static str {
    match provider {
        LlmProvider::Anthropic => "Anthropic",
        LlmProvider::Openai => "OpenAI",
        LlmProvider::Hyperbolic => "Hyperbolic",
        LlmProvider::Ollama => "Ollama",
    }
}

/// Check if the config already has an API key set for the provider.
pub(crate) fn config_has_key(config: &Config, provider: &LlmProvider) -> bool {
    match provider {
        LlmProvider::Anthropic => config
            .llm
            .anthropic
            .as_ref()
            .is_some_and(|c| !c.api_key.is_empty() && !c.api_key.starts_with("${")),
        LlmProvider::Openai => config
            .llm
            .openai
            .as_ref()
            .is_some_and(|c| !c.api_key.is_empty() && !c.api_key.starts_with("${")),
        LlmProvider::Hyperbolic => config
            .llm
            .hyperbolic
            .as_ref()
            .is_some_and(|c| !c.api_key.is_empty() && !c.api_key.starts_with("${")),
        LlmProvider::Ollama => true, // no key needed
    }
}

/// Save an API key to the Photon config file, preserving existing comments.
fn save_key_to_config(provider: &LlmProvider, key: &str) -> anyhow::Result<()> {
    let config_path = Config::default_path();

    let content = if config_path.exists() {
        std::fs::read_to_string(&config_path)?
    } else {
        String::new()
    };

    let mut doc: toml_edit::DocumentMut = content.parse().unwrap_or_default();

    let section_name = match provider {
        LlmProvider::Anthropic => "anthropic",
        LlmProvider::Openai => "openai",
        LlmProvider::Hyperbolic => "hyperbolic",
        LlmProvider::Ollama => return Ok(()), // no key to save
    };

    // Ensure [llm] table exists
    if !doc.contains_key("llm") {
        doc["llm"] = toml_edit::Item::Table(toml_edit::Table::new());
    }

    // Ensure [llm.<provider>] table exists
    if !doc["llm"]
        .as_table()
        .is_some_and(|t| t.contains_key(section_name))
    {
        doc["llm"][section_name] = toml_edit::Item::Table(toml_edit::Table::new());
    }

    doc["llm"][section_name]["api_key"] = toml_edit::value(key);

    // Only set enabled if not already present
    if doc["llm"][section_name].get("enabled").is_none() {
        doc["llm"][section_name]["enabled"] = toml_edit::value(true);
    }

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&config_path, doc.to_string())?;

    let dim = Style::new().for_stderr().dim();
    eprintln!(
        "  {}",
        dim.apply_to(format!("Key saved to {}", config_path.display()))
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use photon_core::config::AnthropicConfig;

    // ── config_has_key tests ────────────────────────────────────────────

    #[test]
    fn config_has_key_anthropic_with_real_key() {
        let mut config = Config::default();
        config.llm.anthropic = Some(AnthropicConfig {
            enabled: true,
            api_key: "sk-ant-real-key-123".to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
        });
        assert!(config_has_key(&config, &LlmProvider::Anthropic));
    }

    #[test]
    fn config_has_key_anthropic_empty_key() {
        let mut config = Config::default();
        config.llm.anthropic = Some(AnthropicConfig {
            enabled: true,
            api_key: String::new(),
            model: "claude-sonnet-4-20250514".to_string(),
        });
        assert!(!config_has_key(&config, &LlmProvider::Anthropic));
    }

    #[test]
    fn config_has_key_anthropic_template_key() {
        let mut config = Config::default();
        config.llm.anthropic = Some(AnthropicConfig {
            enabled: true,
            api_key: "${ANTHROPIC_API_KEY}".to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
        });
        assert!(!config_has_key(&config, &LlmProvider::Anthropic));
    }

    #[test]
    fn config_has_key_anthropic_section_none() {
        let config = Config::default();
        // Default LlmConfig has all providers as None
        assert!(!config_has_key(&config, &LlmProvider::Anthropic));
    }

    #[test]
    fn config_has_key_ollama_always_true() {
        let config = Config::default();
        assert!(config_has_key(&config, &LlmProvider::Ollama));
    }

    // ── env_var_for tests ───────────────────────────────────────────────

    #[test]
    fn env_var_for_all_providers() {
        assert_eq!(env_var_for(&LlmProvider::Anthropic), "ANTHROPIC_API_KEY");
        assert_eq!(env_var_for(&LlmProvider::Openai), "OPENAI_API_KEY");
        assert_eq!(env_var_for(&LlmProvider::Hyperbolic), "HYPERBOLIC_API_KEY");
        assert_eq!(env_var_for(&LlmProvider::Ollama), "OLLAMA_HOST");
    }

    // ── provider_label tests ────────────────────────────────────────────

    #[test]
    fn provider_label_all_providers() {
        assert_eq!(provider_label(&LlmProvider::Anthropic), "Anthropic");
        assert_eq!(provider_label(&LlmProvider::Openai), "OpenAI");
        assert_eq!(provider_label(&LlmProvider::Hyperbolic), "Hyperbolic");
        assert_eq!(provider_label(&LlmProvider::Ollama), "Ollama");
    }
}
