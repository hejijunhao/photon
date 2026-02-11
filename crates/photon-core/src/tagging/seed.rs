//! Seed term selection for progressive encoding.
//!
//! Selects a high-value subset of vocabulary terms for fast first-run startup.
//! Priority: supplemental terms > curated seed file matches > random sample.

use std::collections::HashSet;
use std::path::Path;

use rand::seq::SliceRandom;
use rand::SeedableRng;

use super::vocabulary::Vocabulary;

/// Selects a high-value seed vocabulary for fast first-run startup.
pub struct SeedSelector;

impl SeedSelector {
    /// Select seed terms from the full vocabulary.
    ///
    /// Priority order:
    /// 1. All supplemental terms (scenes, moods, styles — high visual relevance)
    /// 2. Curated seed file matches (common visual nouns from `seed_terms.txt`)
    /// 3. Random sample from remaining terms (diversity for unexpected niches)
    ///
    /// Returns sorted indices into the full vocabulary's term list.
    pub fn select(vocabulary: &Vocabulary, seed_path: &Path, target_size: usize) -> Vec<usize> {
        let mut selected = HashSet::new();

        // 1. Include ALL supplemental terms (they have category != None)
        for (i, term) in vocabulary.all_terms().iter().enumerate() {
            if term.category.is_some() {
                selected.insert(i);
            }
        }

        let supp_count = selected.len();

        // 2. Include seed_terms.txt matches (if file exists)
        let mut seed_file_count = 0;
        if let Ok(content) = std::fs::read_to_string(seed_path) {
            let seed_names: HashSet<&str> = content
                .lines()
                .map(|l| l.trim())
                .filter(|l| !l.starts_with('#') && !l.is_empty())
                .collect();
            for (i, term) in vocabulary.all_terms().iter().enumerate() {
                if seed_names.contains(term.name.as_str()) && selected.insert(i) {
                    seed_file_count += 1;
                }
            }
        }

        // 3. Fill remainder with deterministic random sample from unselected terms
        let remaining = target_size.saturating_sub(selected.len());
        if remaining > 0 {
            let mut unselected: Vec<usize> = (0..vocabulary.len())
                .filter(|i| !selected.contains(i))
                .collect();

            // Deterministic shuffle using vocabulary content hash as seed.
            // Same vocabulary always produces the same seed set.
            let hash = vocabulary.content_hash();
            let hash_bytes = hash.as_bytes();
            let seed_value = u64::from_le_bytes(hash_bytes[..8].try_into().unwrap_or([0u8; 8]));
            let mut rng = rand::rngs::StdRng::seed_from_u64(seed_value);

            unselected.shuffle(&mut rng);
            for idx in unselected.into_iter().take(remaining) {
                selected.insert(idx);
            }
        }

        tracing::info!(
            "Seed selection: {} total ({} supplemental, {} from seed file, {} random fill)",
            selected.len(),
            supp_count,
            seed_file_count,
            selected.len().saturating_sub(supp_count + seed_file_count),
        );

        let mut indices: Vec<usize> = selected.into_iter().collect();
        indices.sort();
        indices
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Helper to create a vocabulary from term specs via temp files.
    fn vocab_from_terms(wordnet: &[(&str, &str)], supplemental: &[(&str, &str)]) -> Vocabulary {
        let dir = tempfile::tempdir().unwrap();

        let nouns_path = dir.path().join("wordnet_nouns.txt");
        let mut f = std::fs::File::create(&nouns_path).unwrap();
        writeln!(f, "# Test WordNet nouns").unwrap();
        for (name, hyp) in wordnet {
            writeln!(f, "{}\t00000000\t{}", name, hyp).unwrap();
        }

        let supp_path = dir.path().join("supplemental.txt");
        let mut f = std::fs::File::create(&supp_path).unwrap();
        writeln!(f, "# Test supplemental").unwrap();
        for (name, cat) in supplemental {
            writeln!(f, "{}\t{}", name, cat).unwrap();
        }

        Vocabulary::load(dir.path()).unwrap()
    }

    #[test]
    fn test_select_includes_supplemental() {
        let vocab = vocab_from_terms(
            &[("dog", "animal"), ("cat", "animal"), ("car", "vehicle")],
            &[("beach", "scene"), ("sunset", "mood")],
        );

        let result = SeedSelector::select(&vocab, Path::new("/nonexistent"), 10);

        // Supplemental terms are at indices 3 and 4 (after the 3 WordNet terms)
        assert!(result.contains(&3), "Should include 'beach' (supplemental)");
        assert!(
            result.contains(&4),
            "Should include 'sunset' (supplemental)"
        );
    }

    #[test]
    fn test_select_respects_target_size() {
        // Create 20 unique WordNet terms
        let wordnet: Vec<(&str, &str)> = vec![
            ("dog", "animal"),
            ("cat", "animal"),
            ("car", "vehicle"),
            ("tree", "plant"),
            ("fish", "animal"),
            ("bird", "animal"),
            ("boat", "vehicle"),
            ("hat", "clothing"),
            ("cup", "container"),
            ("pen", "tool"),
            ("box", "container"),
            ("bag", "container"),
            ("map", "artifact"),
            ("key", "artifact"),
            ("fan", "device"),
            ("bed", "furniture"),
            ("pot", "container"),
            ("net", "artifact"),
            ("nut", "food"),
            ("gem", "artifact"),
        ];

        let vocab = vocab_from_terms(&wordnet, &[("rain", "weather")]);

        // Target 5, vocab has 21 terms
        let result = SeedSelector::select(&vocab, Path::new("/nonexistent"), 5);
        assert_eq!(result.len(), 5);
        // Supplemental 'rain' (index 20) must be included
        assert!(result.contains(&20));
    }

    #[test]
    fn test_select_deterministic() {
        let vocab = vocab_from_terms(
            &[
                ("dog", "animal"),
                ("cat", "animal"),
                ("car", "vehicle"),
                ("tree", "plant"),
                ("fish", "animal"),
                ("bird", "animal"),
            ],
            &[("beach", "scene")],
        );

        let r1 = SeedSelector::select(&vocab, Path::new("/nonexistent"), 5);
        let r2 = SeedSelector::select(&vocab, Path::new("/nonexistent"), 5);
        assert_eq!(r1, r2, "Same vocabulary should produce same seed set");
    }

    #[test]
    fn test_select_without_seed_file() {
        let vocab = vocab_from_terms(
            &[("dog", "animal"), ("cat", "animal")],
            &[("beach", "scene")],
        );

        // Non-existent seed file should not panic
        let result = SeedSelector::select(&vocab, Path::new("/nonexistent/seed_terms.txt"), 5);
        assert!(!result.is_empty());
        // Should still include the supplemental term
        assert!(result.contains(&2)); // beach at index 2
    }

    #[test]
    fn test_select_with_seed_file() {
        let vocab = vocab_from_terms(
            &[
                ("dog", "animal"),
                ("cat", "animal"),
                ("car", "vehicle"),
                ("tree", "plant"),
                ("fish", "animal"),
            ],
            &[],
        );

        let mut seed_file = tempfile::NamedTempFile::new().unwrap();
        writeln!(seed_file, "# Seed terms").unwrap();
        writeln!(seed_file, "dog").unwrap();
        writeln!(seed_file, "car").unwrap();
        writeln!(seed_file, "nonexistent_term").unwrap(); // silently skipped

        let result = SeedSelector::select(&vocab, seed_file.path(), 3);
        assert!(result.contains(&0), "Should include 'dog' from seed file");
        assert!(result.contains(&2), "Should include 'car' from seed file");
        assert_eq!(result.len(), 3); // 2 from seed file + 1 random fill
    }
}
