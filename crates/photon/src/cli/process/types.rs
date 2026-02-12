//! CLI enum types for the process command: output format, quality, LLM provider.

use clap::ValueEnum;

/// Supported output formats.
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum OutputFormat {
    /// Single JSON object or array
    Json,
    /// One JSON object per line (newline-delimited)
    Jsonl,
}

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OutputFormat::Json => write!(f, "json"),
            OutputFormat::Jsonl => write!(f, "jsonl"),
        }
    }
}

/// Quality preset for SigLIP vision model resolution.
#[derive(Clone, Copy, Debug, ValueEnum, Default)]
pub enum Quality {
    /// Fast processing with base 224 model (default)
    #[default]
    Fast,
    /// Higher detail with base 384 model (~3-4x slower)
    High,
}

/// Supported LLM providers.
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum LlmProvider {
    /// Local Ollama instance
    Ollama,
    /// Hyperbolic API
    Hyperbolic,
    /// Anthropic API
    Anthropic,
    /// OpenAI API
    Openai,
}

impl std::fmt::Display for LlmProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LlmProvider::Ollama => write!(f, "ollama"),
            LlmProvider::Hyperbolic => write!(f, "hyperbolic"),
            LlmProvider::Anthropic => write!(f, "anthropic"),
            LlmProvider::Openai => write!(f, "openai"),
        }
    }
}
