//! LLM integration for image description generation.
//!
//! Provides a provider abstraction over multiple LLM backends (Ollama, Anthropic,
//! OpenAI, Hyperbolic) and a concurrent enrichment engine that generates descriptions
//! for batches of already-processed images.

pub mod anthropic;
pub mod enricher;
pub mod hyperbolic;
pub mod ollama;
pub mod openai;
pub mod provider;
pub mod retry;

pub use enricher::{EnrichOptions, EnrichResult, Enricher};
pub use provider::{ImageInput, LlmProvider, LlmProviderFactory, LlmRequest, LlmResponse};
