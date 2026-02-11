//! Vocabulary loading for zero-shot tagging.
//!
//! Loads WordNet-derived nouns and supplemental visual terms from disk.
//! Each term includes an optional synset ID and hypernym chain for hierarchy.

use std::collections::HashMap;
use std::path::Path;

use crate::error::PipelineError;

/// A single vocabulary term with optional WordNet hierarchy.
#[derive(Debug, Clone)]
pub struct VocabTerm {
    /// Raw name (may contain underscores)
    pub name: String,
    /// Display name (underscores replaced with spaces)
    pub display_name: String,
    /// WordNet synset ID (if from WordNet)
    pub synset_id: Option<String>,
    /// Hypernym chain, most specific ancestor first
    pub hypernyms: Vec<String>,
    /// Category for supplemental terms (scene, mood, style, weather, time)
    pub category: Option<String>,
}

/// A loaded vocabulary ready for encoding and scoring.
pub struct Vocabulary {
    terms: Vec<VocabTerm>,
    by_name: HashMap<String, usize>,
}

impl Vocabulary {
    /// Load vocabulary from the vocabulary directory.
    ///
    /// Reads `wordnet_nouns.txt` and `supplemental.txt` if present.
    /// Returns an error only on I/O failures, not on missing files.
    pub fn load(vocab_dir: &Path) -> Result<Self, PipelineError> {
        let mut terms = Vec::new();

        // Load WordNet nouns
        let nouns_path = vocab_dir.join("wordnet_nouns.txt");
        if nouns_path.exists() {
            let content =
                std::fs::read_to_string(&nouns_path).map_err(|e| PipelineError::Model {
                    message: format!("Failed to read {:?}: {}", nouns_path, e),
                })?;
            for line in content.lines() {
                if line.starts_with('#') || line.trim().is_empty() {
                    continue;
                }
                let parts: Vec<&str> = line.split('\t').collect();
                if parts.len() >= 3 {
                    let name = parts[0].to_string();
                    let display_name = name.replace('_', " ");
                    let synset_id = Some(parts[1].to_string());
                    let hypernyms: Vec<String> =
                        parts[2].split('|').map(|s| s.replace('_', " ")).collect();

                    terms.push(VocabTerm {
                        name: name.clone(),
                        display_name,
                        synset_id,
                        hypernyms,
                        category: None,
                    });
                }
            }
        }

        // Load supplemental terms
        let supp_path = vocab_dir.join("supplemental.txt");
        if supp_path.exists() {
            let content =
                std::fs::read_to_string(&supp_path).map_err(|e| PipelineError::Model {
                    message: format!("Failed to read {:?}: {}", supp_path, e),
                })?;
            for line in content.lines() {
                if line.starts_with('#') || line.trim().is_empty() {
                    continue;
                }
                let parts: Vec<&str> = line.split('\t').collect();
                if parts.len() >= 2 {
                    terms.push(VocabTerm {
                        name: parts[0].to_string(),
                        display_name: parts[0].to_string(),
                        synset_id: None,
                        hypernyms: vec![],
                        category: Some(parts[1].to_string()),
                    });
                }
            }
        }

        // Build lookup index
        let by_name: HashMap<String, usize> = terms
            .iter()
            .enumerate()
            .map(|(i, t)| (t.name.clone(), i))
            .collect();

        let wordnet_count = terms.iter().filter(|t| t.synset_id.is_some()).count();
        let supp_count = terms.iter().filter(|t| t.category.is_some()).count();

        tracing::info!(
            "Loaded vocabulary: {} terms ({} WordNet, {} supplemental)",
            terms.len(),
            wordnet_count,
            supp_count,
        );

        Ok(Self { terms, by_name })
    }

    /// Get all terms in the vocabulary.
    pub fn all_terms(&self) -> &[VocabTerm] {
        &self.terms
    }

    /// Number of terms in the vocabulary.
    pub fn len(&self) -> usize {
        self.terms.len()
    }

    /// Whether the vocabulary is empty.
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }

    /// Look up a term by its raw name.
    pub fn get(&self, name: &str) -> Option<&VocabTerm> {
        self.by_name.get(name).map(|&i| &self.terms[i])
    }

    /// Compute a BLAKE3 hash of all term names in order.
    ///
    /// Used for label bank cache invalidation — if the vocabulary changes,
    /// the hash changes and the cached label bank is rebuilt.
    pub fn content_hash(&self) -> String {
        let mut hasher = blake3::Hasher::new();
        for term in &self.terms {
            hasher.update(term.name.as_bytes());
            hasher.update(b"\n");
        }
        hasher.finalize().to_hex().to_string()
    }
}
