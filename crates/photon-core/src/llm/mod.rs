//! LLM integration for image description generation.
//!
//! Provides a provider abstraction over multiple LLM backends (Ollama, Anthropic,
//! OpenAI, Hyperbolic) and a concurrent enrichment engine that generates descriptions
//! for batches of already-processed images.

pub(crate) mod anthropic;
pub(crate) mod enricher;
pub(crate) mod hyperbolic;
pub(crate) mod ollama;
pub(crate) mod openai;
pub(crate) mod provider;
pub(crate) mod retry;

pub use enricher::{EnrichOptions, EnrichResult, Enricher};
pub use provider::LlmProviderFactory;
