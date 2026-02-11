//! Zero-shot image tagging via SigLIP text encoder.
//!
//! Scores images against a vocabulary of terms by computing dot products
//! between image embeddings and pre-computed text embeddings.

pub mod label_bank;
pub mod progressive;
pub mod scorer;
pub mod seed;
pub mod text_encoder;
pub mod vocabulary;

pub use scorer::TagScorer;
pub use vocabulary::Vocabulary;
