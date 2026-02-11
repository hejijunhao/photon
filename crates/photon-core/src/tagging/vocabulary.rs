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

    /// Create an empty vocabulary (placeholder for progressive encoding initialization).
    pub fn empty() -> Self {
        Self {
            terms: vec![],
            by_name: HashMap::new(),
        }
    }

    /// Create a sub-vocabulary containing only the terms at the given indices.
    ///
    /// Preserves term order as given in `indices`. Rebuilds the `by_name` index
    /// for the subset so lookups work correctly on the smaller vocabulary.
    pub fn subset(&self, indices: &[usize]) -> Self {
        let terms: Vec<VocabTerm> = indices
            .iter()
            .filter_map(|&i| self.terms.get(i).cloned())
            .collect();

        let by_name: HashMap<String, usize> = terms
            .iter()
            .enumerate()
            .map(|(i, t)| (t.name.clone(), i))
            .collect();

        Self { terms, by_name }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn vocab_from_terms(wordnet: &[(&str, &str)], supplemental: &[(&str, &str)]) -> Vocabulary {
        let dir = tempfile::tempdir().unwrap();

        let nouns_path = dir.path().join("wordnet_nouns.txt");
        let mut f = std::fs::File::create(&nouns_path).unwrap();
        writeln!(f, "# Test").unwrap();
        for (name, hyp) in wordnet {
            writeln!(f, "{}\t00000000\t{}", name, hyp).unwrap();
        }

        let supp_path = dir.path().join("supplemental.txt");
        let mut f = std::fs::File::create(&supp_path).unwrap();
        writeln!(f, "# Test").unwrap();
        for (name, cat) in supplemental {
            writeln!(f, "{}\t{}", name, cat).unwrap();
        }

        Vocabulary::load(dir.path()).unwrap()
    }

    #[test]
    fn test_empty_vocabulary() {
        let vocab = Vocabulary::empty();
        assert!(vocab.is_empty());
        assert_eq!(vocab.len(), 0);
        assert!(vocab.all_terms().is_empty());
        assert!(vocab.get("anything").is_none());
    }

    #[test]
    fn test_subset_preserves_terms() {
        let vocab = vocab_from_terms(
            &[("dog", "animal"), ("cat", "animal"), ("car", "vehicle"),
              ("tree", "plant"), ("fish", "animal")],
            &[],
        );

        let sub = vocab.subset(&[0, 2, 4]);
        assert_eq!(sub.len(), 3);
        assert_eq!(sub.all_terms()[0].name, "dog");
        assert_eq!(sub.all_terms()[1].name, "car");
        assert_eq!(sub.all_terms()[2].name, "fish");
    }

    #[test]
    fn test_subset_empty() {
        let vocab = vocab_from_terms(
            &[("dog", "animal"), ("cat", "animal")],
            &[],
        );

        let sub = vocab.subset(&[]);
        assert!(sub.is_empty());
        assert_eq!(sub.len(), 0);
    }

    #[test]
    fn test_subset_rebuilds_index() {
        let vocab = vocab_from_terms(
            &[("dog", "animal"), ("cat", "animal"), ("car", "vehicle")],
            &[],
        );

        let sub = vocab.subset(&[1, 2]); // cat, car
        assert!(sub.get("cat").is_some());
        assert!(sub.get("car").is_some());
        assert!(sub.get("dog").is_none()); // not in subset
        // Verify index points to correct position in the SUBSET
        assert_eq!(sub.get("cat").unwrap().name, "cat");
        assert_eq!(sub.get("car").unwrap().name, "car");
    }

    #[test]
    fn test_subset_preserves_hypernyms() {
        let vocab = vocab_from_terms(
            &[("dog", "animal|organism|entity")],
            &[],
        );

        let sub = vocab.subset(&[0]);
        assert_eq!(sub.all_terms()[0].hypernyms, vec!["animal", "organism", "entity"]);
    }
}
