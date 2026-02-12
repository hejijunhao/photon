//! Zero-shot image tagging via SigLIP text encoder.
//!
//! Scores images against a vocabulary of terms by computing dot products
//! between image embeddings and pre-computed text embeddings.

pub(crate) mod hierarchy;
pub(crate) mod label_bank;
pub(crate) mod neighbors;
pub(crate) mod progressive;
pub(crate) mod relevance;
pub(crate) mod scorer;
pub(crate) mod seed;
pub(crate) mod text_encoder;
pub(crate) mod vocabulary;

pub use scorer::TagScorer;
pub use vocabulary::Vocabulary;
