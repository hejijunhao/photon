//! Hierarchy deduplication for zero-shot tags.
//!
//! Post-processes scored tags to remove redundant ancestor terms and
//! optionally annotate surviving tags with abbreviated hierarchy paths.

use std::collections::HashSet;

use crate::types::Tag;

use super::vocabulary::Vocabulary;

/// Terms too generic to be useful in abbreviated hierarchy paths.
const SKIP_TERMS: &[&str] = &[
    "entity",
    "physical entity",
    "object",
    "whole",
    "thing",
    "organism",
    "living thing",
    "abstraction",
    "matter",
    "substance",
    "body",
    "unit",
];

/// Post-processes scored tags to remove ancestor redundancy and add paths.
pub struct HierarchyDedup;

impl HierarchyDedup {
    /// Check if `ancestor_name` appears in `term_name`'s hypernym chain.
    ///
    /// Tag names are display names (spaces), but vocabulary lookup uses raw
    /// names (underscores). This helper normalizes between the two.
    fn is_ancestor(vocabulary: &Vocabulary, term_name: &str, ancestor_name: &str) -> bool {
        let raw_name = term_name.replace(' ', "_");
        let term = match vocabulary.get(&raw_name) {
            Some(t) => t,
            None => return false,
        };

        term.hypernyms.iter().any(|h| h == ancestor_name)
    }

    /// Remove tags that are ancestors of other tags in the list.
    ///
    /// For each pair of tags (A, B): if A is an ancestor of B (via WordNet
    /// hypernyms), suppress A. The more specific tag B survives regardless
    /// of confidence ordering.
    pub fn deduplicate(tags: &[Tag], vocabulary: &Vocabulary) -> Vec<Tag> {
        let mut suppressed: HashSet<usize> = HashSet::new();

        for i in 0..tags.len() {
            if suppressed.contains(&i) {
                continue;
            }

            for j in 0..tags.len() {
                if i == j || suppressed.contains(&j) {
                    continue;
                }

                // Check if tags[i] is an ancestor of tags[j]
                if Self::is_ancestor(vocabulary, &tags[j].name, &tags[i].name) {
                    suppressed.insert(i);
                    break;
                }
            }
        }

        tags.iter()
            .enumerate()
            .filter(|(i, _)| !suppressed.contains(i))
            .map(|(_, tag)| tag.clone())
            .collect()
    }

    /// Add abbreviated hierarchy paths to tags.
    ///
    /// Path format: "grandparent > parent > term"
    /// Shows at most `max_ancestors` levels, skipping very generic terms.
    pub fn add_paths(tags: &mut [Tag], vocabulary: &Vocabulary, max_ancestors: usize) {
        for tag in tags.iter_mut() {
            let raw_name = tag.name.replace(' ', "_");
            let term = match vocabulary.get(&raw_name) {
                Some(t) => t,
                None => continue,
            };

            if term.hypernyms.is_empty() {
                continue;
            }

            // Filter out overly generic ancestors
            let meaningful: Vec<&str> = term
                .hypernyms
                .iter()
                .map(|h| h.as_str())
                .filter(|h| !SKIP_TERMS.contains(h))
                .collect();

            if meaningful.is_empty() {
                continue;
            }

            // Hypernyms are stored most-specific-first.
            // Take the last N (most general of the meaningful ones),
            // then reverse so the path reads general → specific.
            let ancestors: Vec<&str> = if meaningful.len() > max_ancestors {
                meaningful[meaningful.len() - max_ancestors..].to_vec()
            } else {
                meaningful
            };

            let mut path_parts: Vec<&str> = ancestors.into_iter().rev().collect();
            path_parts.push(&tag.name);

            tag.path = Some(path_parts.join(" > "));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tagging::vocabulary::Vocabulary;
    use std::io::Write;

    /// Build a vocabulary from (name, hypernym_chain) pairs and supplemental terms.
    fn vocab_with_hierarchy(wordnet: &[(&str, &str)], supplemental: &[(&str, &str)]) -> Vocabulary {
        let dir = tempfile::tempdir().unwrap();

        let nouns_path = dir.path().join("wordnet_nouns.txt");
        let mut f = std::fs::File::create(&nouns_path).unwrap();
        for (name, hyps) in wordnet {
            writeln!(f, "{}\t00000000\t{}", name, hyps).unwrap();
        }

        let supp_path = dir.path().join("supplemental.txt");
        let mut f = std::fs::File::create(&supp_path).unwrap();
        for (name, cat) in supplemental {
            writeln!(f, "{}\t{}", name, cat).unwrap();
        }

        Vocabulary::load(dir.path()).unwrap()
    }

    fn tag(name: &str, confidence: f32) -> Tag {
        Tag::new(name, confidence)
    }

    // ── is_ancestor tests ──

    #[test]
    fn test_is_ancestor_direct_parent() {
        let vocab = vocab_with_hierarchy(
            &[
                (
                    "labrador_retriever",
                    "retriever|sporting dog|dog|canine|animal",
                ),
                ("retriever", "sporting dog|dog|canine|animal"),
            ],
            &[],
        );
        assert!(HierarchyDedup::is_ancestor(
            &vocab,
            "labrador retriever",
            "retriever"
        ));
    }

    #[test]
    fn test_is_ancestor_grandparent() {
        let vocab = vocab_with_hierarchy(
            &[(
                "labrador_retriever",
                "retriever|sporting dog|dog|canine|animal",
            )],
            &[],
        );
        assert!(HierarchyDedup::is_ancestor(
            &vocab,
            "labrador retriever",
            "dog"
        ));
        assert!(HierarchyDedup::is_ancestor(
            &vocab,
            "labrador retriever",
            "animal"
        ));
    }

    #[test]
    fn test_is_ancestor_unrelated() {
        let vocab = vocab_with_hierarchy(
            &[
                ("labrador_retriever", "retriever|dog|animal"),
                ("carpet", "covering|floor covering"),
            ],
            &[],
        );
        assert!(!HierarchyDedup::is_ancestor(
            &vocab,
            "labrador retriever",
            "carpet"
        ));
    }

    #[test]
    fn test_is_ancestor_self() {
        let vocab = vocab_with_hierarchy(&[("dog", "canine|animal")], &[]);
        // A term is NOT its own ancestor (hypernyms don't include self)
        assert!(!HierarchyDedup::is_ancestor(&vocab, "dog", "dog"));
    }

    #[test]
    fn test_is_ancestor_supplemental() {
        let vocab = vocab_with_hierarchy(&[], &[("sunset", "scene"), ("indoor", "scene")]);
        // Supplemental terms have no hypernyms
        assert!(!HierarchyDedup::is_ancestor(&vocab, "sunset", "indoor"));
        assert!(!HierarchyDedup::is_ancestor(&vocab, "indoor", "sunset"));
    }

    // ── deduplicate tests ──

    #[test]
    fn test_dedup_suppresses_ancestors() {
        let vocab = vocab_with_hierarchy(
            &[
                ("labrador_retriever", "retriever|dog|animal"),
                ("retriever", "dog|animal"),
                ("dog", "canine|animal"),
                ("animal", "organism|entity"),
            ],
            &[],
        );

        let tags = vec![
            tag("labrador retriever", 0.87),
            tag("retriever", 0.81),
            tag("dog", 0.68),
            tag("animal", 0.45),
        ];

        let result = HierarchyDedup::deduplicate(&tags, &vocab);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "labrador retriever");
    }

    #[test]
    fn test_dedup_preserves_unrelated() {
        let vocab = vocab_with_hierarchy(
            &[
                ("labrador_retriever", "retriever|dog|animal"),
                ("carpet", "covering|floor covering"),
            ],
            &[],
        );

        let tags = vec![tag("labrador retriever", 0.87), tag("carpet", 0.74)];

        let result = HierarchyDedup::deduplicate(&tags, &vocab);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].name, "labrador retriever");
        assert_eq!(result[1].name, "carpet");
    }

    #[test]
    fn test_dedup_multiple_chains() {
        let vocab = vocab_with_hierarchy(
            &[
                ("labrador_retriever", "retriever|dog|animal"),
                ("dog", "canine|animal"),
                ("pizza", "food|dish"),
                ("food", "substance"),
            ],
            &[],
        );

        let tags = vec![
            tag("labrador retriever", 0.87),
            tag("dog", 0.68),
            tag("pizza", 0.72),
            tag("food", 0.55),
        ];

        let result = HierarchyDedup::deduplicate(&tags, &vocab);
        assert_eq!(result.len(), 2);
        let names: Vec<&str> = result.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"labrador retriever"));
        assert!(names.contains(&"pizza"));
    }

    #[test]
    fn test_dedup_no_hypernyms() {
        let vocab = vocab_with_hierarchy(&[], &[("sunset", "scene"), ("indoor", "scene")]);

        let tags = vec![tag("sunset", 0.80), tag("indoor", 0.71)];

        let result = HierarchyDedup::deduplicate(&tags, &vocab);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_dedup_empty_tags() {
        let vocab = vocab_with_hierarchy(&[], &[]);
        let result = HierarchyDedup::deduplicate(&[], &vocab);
        assert!(result.is_empty());
    }

    #[test]
    fn test_dedup_preserves_order() {
        let vocab = vocab_with_hierarchy(
            &[
                ("labrador_retriever", "retriever|dog|animal"),
                ("dog", "canine|animal"),
                ("carpet", "covering|floor covering"),
            ],
            &[("indoor", "scene")],
        );

        let tags = vec![
            tag("labrador retriever", 0.87),
            tag("carpet", 0.74),
            tag("indoor", 0.71),
            tag("dog", 0.68),
        ];

        let result = HierarchyDedup::deduplicate(&tags, &vocab);
        // "dog" suppressed; remaining order preserved
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].name, "labrador retriever");
        assert_eq!(result[1].name, "carpet");
        assert_eq!(result[2].name, "indoor");
    }

    // ── add_paths tests ──

    #[test]
    fn test_add_paths_basic() {
        let vocab = vocab_with_hierarchy(
            &[(
                "labrador_retriever",
                "retriever|sporting dog|dog|canine|animal",
            )],
            &[],
        );

        let mut tags = vec![tag("labrador retriever", 0.87)];
        HierarchyDedup::add_paths(&mut tags, &vocab, 2);

        assert_eq!(
            tags[0].path.as_deref(),
            Some("animal > canine > labrador retriever")
        );
    }

    #[test]
    fn test_add_paths_skips_generic() {
        // Hypernym chain includes "entity" and "object" which should be filtered
        let vocab = vocab_with_hierarchy(
            &[(
                "dog",
                "canine|carnivore|mammal|animal|organism|living thing|entity",
            )],
            &[],
        );

        let mut tags = vec![tag("dog", 0.68)];
        HierarchyDedup::add_paths(&mut tags, &vocab, 2);

        let path = tags[0].path.as_deref().unwrap();
        assert!(!path.contains("entity"));
        assert!(!path.contains("living thing"));
        assert!(!path.contains("organism"));
        // Should pick from the meaningful ancestors
        assert!(path.contains("dog"));
    }

    #[test]
    fn test_add_paths_max_ancestors() {
        let vocab = vocab_with_hierarchy(
            &[(
                "labrador_retriever",
                "retriever|sporting dog|dog|canine|animal",
            )],
            &[],
        );

        // max_ancestors=1: only 1 ancestor level
        let mut tags = vec![tag("labrador retriever", 0.87)];
        HierarchyDedup::add_paths(&mut tags, &vocab, 1);

        let path = tags[0].path.as_deref().unwrap();
        let parts: Vec<&str> = path.split(" > ").collect();
        // 1 ancestor + the term itself = 2 parts
        assert_eq!(parts.len(), 2);
        assert_eq!(*parts.last().unwrap(), "labrador retriever");
    }

    #[test]
    fn test_add_paths_supplemental_no_path() {
        let vocab = vocab_with_hierarchy(&[], &[("sunset", "scene")]);

        let mut tags = vec![tag("sunset", 0.80)];
        HierarchyDedup::add_paths(&mut tags, &vocab, 2);

        assert!(tags[0].path.is_none());
    }

    #[test]
    fn test_add_paths_short_chain() {
        // Only 1 hypernym
        let vocab = vocab_with_hierarchy(&[("puppy", "dog")], &[]);

        let mut tags = vec![tag("puppy", 0.75)];
        HierarchyDedup::add_paths(&mut tags, &vocab, 2);

        assert_eq!(tags[0].path.as_deref(), Some("dog > puppy"));
    }

    #[test]
    fn test_add_paths_all_generic_hypernyms() {
        // All hypernyms are in the skip list
        let vocab = vocab_with_hierarchy(&[("item", "thing|object|entity")], &[]);

        let mut tags = vec![tag("item", 0.5)];
        HierarchyDedup::add_paths(&mut tags, &vocab, 2);

        // All hypernyms filtered → no path added
        assert!(tags[0].path.is_none());
    }
}
