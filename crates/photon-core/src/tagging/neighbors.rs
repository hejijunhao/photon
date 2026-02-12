//! Neighbor expansion for vocabulary coverage (Phase 4d).
//!
//! When a term is promoted to active, its WordNet siblings (terms sharing
//! the same immediate parent) are promoted to warm for evaluation.

use std::collections::{HashMap, HashSet};

use super::vocabulary::Vocabulary;

/// Finds WordNet neighbors of terms for vocabulary expansion.
pub struct NeighborExpander;

impl NeighborExpander {
    /// Find sibling terms that share the same immediate parent (first hypernym).
    ///
    /// Returns indices of sibling terms in the vocabulary, excluding the input term.
    /// Supplemental terms (no hypernyms) return an empty vec.
    #[cfg(test)]
    pub fn find_siblings(vocabulary: &Vocabulary, term_index: usize) -> Vec<usize> {
        let parent_index = vocabulary.build_parent_index();
        Self::find_siblings_indexed(vocabulary, term_index, &parent_index)
    }

    /// Find siblings using a precomputed parent → children index.
    ///
    /// Use this when looking up siblings for multiple terms to avoid rebuilding
    /// the index each time (O(K) lookups instead of O(N*K) linear scans).
    fn find_siblings_indexed(
        vocabulary: &Vocabulary,
        term_index: usize,
        parent_index: &HashMap<String, Vec<usize>>,
    ) -> Vec<usize> {
        let parent = match vocabulary.parent_of(term_index) {
            Some(p) => p,
            None => return vec![],
        };

        parent_index
            .get(parent)
            .map(|children| {
                children
                    .iter()
                    .filter(|&&i| i != term_index)
                    .copied()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Batch expansion: find siblings for multiple promoted terms.
    ///
    /// Builds the parent index once and uses it for all lookups (O(N + K)
    /// instead of O(N*K) where N = vocabulary size, K = promoted terms).
    /// Deduplicates results and excludes the promoted terms themselves.
    pub fn expand_all(vocabulary: &Vocabulary, promoted_indices: &[usize]) -> Vec<usize> {
        let parent_index = vocabulary.build_parent_index();
        let mut siblings = HashSet::new();
        for &idx in promoted_indices {
            for sib in Self::find_siblings_indexed(vocabulary, idx, &parent_index) {
                siblings.insert(sib);
            }
        }
        // Don't include the promoted terms themselves
        for &idx in promoted_indices {
            siblings.remove(&idx);
        }
        siblings.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Returns TempDir alongside Vocabulary so the directory stays alive
    /// for the test's duration and is cleaned up automatically.
    fn test_vocab() -> (Vocabulary, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let nouns_path = dir.path().join("wordnet_nouns.txt");
        let mut f = std::fs::File::create(&nouns_path).unwrap();
        // Three dogs sharing parent "retriever", one cat with parent "feline"
        writeln!(f, "labrador_retriever\t00000001\tretriever|dog|animal").unwrap();
        writeln!(f, "golden_retriever\t00000002\tretriever|dog|animal").unwrap();
        writeln!(f, "curly_coated_retriever\t00000003\tretriever|dog|animal").unwrap();
        writeln!(f, "persian_cat\t00000004\tfeline|cat|animal").unwrap();
        writeln!(f, "siamese_cat\t00000005\tfeline|cat|animal").unwrap();

        let supp_path = dir.path().join("supplemental.txt");
        let mut f = std::fs::File::create(&supp_path).unwrap();
        writeln!(f, "sunset\tscene").unwrap();

        let vocab = Vocabulary::load(dir.path()).unwrap();
        (vocab, dir)
    }

    #[test]
    fn test_find_siblings_shared_parent() {
        let (vocab, _dir) = test_vocab();
        // labrador_retriever (0) → siblings: golden (1), curly_coated (2)
        let mut siblings = NeighborExpander::find_siblings(&vocab, 0);
        siblings.sort();
        assert_eq!(siblings, vec![1, 2]);
    }

    #[test]
    fn test_find_siblings_excludes_self() {
        let (vocab, _dir) = test_vocab();
        let siblings = NeighborExpander::find_siblings(&vocab, 0);
        assert!(!siblings.contains(&0));
    }

    #[test]
    fn test_find_siblings_no_hypernyms() {
        let (vocab, _dir) = test_vocab();
        // sunset (5) is supplemental — no hypernyms
        let siblings = NeighborExpander::find_siblings(&vocab, 5);
        assert!(siblings.is_empty());
    }

    #[test]
    fn test_find_siblings_different_parent() {
        let (vocab, _dir) = test_vocab();
        // persian_cat (3) → sibling: siamese_cat (4), not any retrievers
        let siblings = NeighborExpander::find_siblings(&vocab, 3);
        assert_eq!(siblings, vec![4]);
    }

    #[test]
    fn test_expand_all_deduplicates() {
        let (vocab, _dir) = test_vocab();
        // Promote both labrador (0) and golden (1) — curly_coated (2) is sibling of both
        let mut expanded = NeighborExpander::expand_all(&vocab, &[0, 1]);
        expanded.sort();
        // Should contain curly_coated (2) only once
        assert_eq!(expanded, vec![2]);
    }

    #[test]
    fn test_expand_all_excludes_promoted() {
        let (vocab, _dir) = test_vocab();
        let expanded = NeighborExpander::expand_all(&vocab, &[0]);
        // golden (1) and curly_coated (2) but not labrador (0)
        assert!(!expanded.contains(&0));
        assert!(expanded.contains(&1));
        assert!(expanded.contains(&2));
    }
}
