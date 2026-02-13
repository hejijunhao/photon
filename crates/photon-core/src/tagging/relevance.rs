//! Relevance tracking for vocabulary pruning (Phase 4c).
//!
//! Tracks per-term scoring statistics and manages a three-pool system
//! (Active/Warm/Cold) where terms self-organize based on scoring history.
//! Irrelevant terms are demoted to reduce scoring cost; neighbors of active
//! terms are prioritized for deeper coverage.

use std::collections::HashMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::PipelineError;

use super::vocabulary::Vocabulary;

/// Pool assignment for a vocabulary term.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Pool {
    /// Scored every image — the hot path.
    Active,
    /// Scored every Nth image — sampled for promotion.
    Warm,
    /// Not scored — can be promoted externally via neighbor expansion.
    Cold,
}

/// Per-term scoring statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TermStats {
    /// How many times this term scored above min_confidence.
    pub hit_count: u32,
    /// Running sum of confidence scores when matched (for computing average).
    pub score_sum: f32,
    /// Unix timestamp (seconds) of last match. 0 = never matched.
    pub last_hit_ts: u64,
    /// Current pool assignment.
    pub pool: Pool,
    /// Consecutive warm sweep checks with no hits (for Warm→Cold demotion).
    #[serde(default)]
    pub warm_checks_without_hit: u32,
}

impl TermStats {
    /// Average confidence across all hits. Returns 0.0 if never hit.
    pub fn avg_confidence(&self) -> f32 {
        if self.hit_count == 0 {
            0.0
        } else {
            self.score_sum / self.hit_count as f32
        }
    }
}

/// Configuration for relevance pruning pool transitions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RelevanceConfig {
    /// Enable relevance pruning (three-pool system).
    /// When false, all encoded terms are scored every image (4a behavior).
    pub enabled: bool,

    /// Score warm-pool terms every N images.
    pub warm_check_interval: u64,

    /// Min confidence for a warm term to promote to active.
    pub promotion_threshold: f32,

    /// Demote active terms with no hits in this many days.
    pub active_demotion_days: u32,

    /// Demote warm terms after this many consecutive warm checks with no hits.
    pub warm_demotion_checks: u32,

    /// Enable neighbor expansion when terms are promoted.
    pub neighbor_expansion: bool,
}

impl Default for RelevanceConfig {
    fn default() -> Self {
        Self {
            enabled: false, // Off by default — opt-in
            warm_check_interval: 100,
            promotion_threshold: 0.3,
            active_demotion_days: 90,
            warm_demotion_checks: 50,
            neighbor_expansion: true,
        }
    }
}

/// On-disk format for relevance data.
#[derive(Serialize, Deserialize)]
struct RelevanceFile {
    version: u32,
    images_processed: u64,
    last_updated: u64,
    terms: HashMap<String, TermStats>,
}

/// Tracks per-term scoring statistics and manages pool assignments.
pub struct RelevanceTracker {
    /// Per-term statistics, indexed parallel to vocabulary.
    stats: Vec<TermStats>,
    /// Total images processed overall.
    images_processed: u64,
    /// Configuration for pool transitions.
    config: RelevanceConfig,
}

impl RelevanceTracker {
    /// Create a new tracker. Encoded terms start Active; unencoded start Cold.
    pub fn new(term_count: usize, encoded_mask: &[bool], config: RelevanceConfig) -> Self {
        let stats = (0..term_count)
            .map(|i| TermStats {
                hit_count: 0,
                score_sum: 0.0,
                last_hit_ts: 0,
                pool: if encoded_mask[i] {
                    Pool::Active
                } else {
                    Pool::Cold
                },
                warm_checks_without_hit: 0,
            })
            .collect();
        Self {
            stats,
            images_processed: 0,
            config,
        }
    }

    /// Record scoring results for one image.
    ///
    /// Called after every score() with the (term_index, confidence) pairs
    /// that exceeded min_confidence.
    pub fn record_hits(&mut self, hits: &[(usize, f32)]) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        for &(idx, confidence) in hits {
            if idx >= self.stats.len() {
                tracing::warn!(
                    "record_hits: term index {idx} out of bounds (max {}), skipping",
                    self.stats.len()
                );
                continue;
            }
            let stat = &mut self.stats[idx];
            stat.hit_count += 1;
            stat.score_sum += confidence;
            stat.last_hit_ts = now;
            stat.warm_checks_without_hit = 0;
        }
        self.images_processed += 1;
    }

    /// Check if warm-pool scoring should happen this image.
    pub fn should_check_warm(&self) -> bool {
        self.config.warm_check_interval > 0
            && self
                .images_processed
                .is_multiple_of(self.config.warm_check_interval)
    }

    /// Run pool transition sweep. Returns indices of terms newly promoted
    /// to Active (for neighbor expansion).
    pub fn sweep(&mut self) -> Vec<usize> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let demotion_secs = self.config.active_demotion_days as u64 * 86400;

        let mut newly_promoted = Vec::new();

        for (i, stat) in self.stats.iter_mut().enumerate() {
            match stat.pool {
                Pool::Active => {
                    // Demote if no hits in N days
                    if stat.last_hit_ts > 0 && now.saturating_sub(stat.last_hit_ts) > demotion_secs
                    {
                        stat.pool = Pool::Warm;
                    }
                    // Also demote if never hit and enough images processed
                    if stat.hit_count == 0 && self.images_processed > 1000 {
                        stat.pool = Pool::Warm;
                    }
                }
                Pool::Warm => {
                    // Promote if has hits with high enough average confidence
                    if stat.hit_count > 0
                        && stat.avg_confidence() >= self.config.promotion_threshold
                    {
                        stat.pool = Pool::Active;
                        stat.warm_checks_without_hit = 0;
                        newly_promoted.push(i);
                    } else {
                        stat.warm_checks_without_hit += 1;
                        if stat.warm_checks_without_hit >= self.config.warm_demotion_checks {
                            stat.pool = Pool::Cold;
                            stat.warm_checks_without_hit = 0;
                        }
                    }
                }
                Pool::Cold => {
                    // Cold terms only promoted externally via promote_to_warm()
                }
            }
        }

        newly_promoted
    }

    /// Get the pool assignment for a term.
    ///
    /// Returns `Pool::Cold` for out-of-bounds indices (safe default — unscored).
    pub fn pool(&self, term_index: usize) -> Pool {
        self.stats
            .get(term_index)
            .map(|s| s.pool)
            .unwrap_or(Pool::Cold)
    }

    /// Get pool counts for logging: (active, warm, cold).
    pub fn pool_counts(&self) -> (usize, usize, usize) {
        let mut active = 0;
        let mut warm = 0;
        let mut cold = 0;
        for stat in &self.stats {
            match stat.pool {
                Pool::Active => active += 1,
                Pool::Warm => warm += 1,
                Pool::Cold => cold += 1,
            }
        }
        (active, warm, cold)
    }

    /// Promote terms to warm pool (for neighbor expansion).
    /// Only promotes Cold terms; Active/Warm terms are left unchanged.
    /// Out-of-bounds indices are silently skipped.
    pub fn promote_to_warm(&mut self, indices: &[usize]) {
        for &idx in indices {
            if idx < self.stats.len() && self.stats[idx].pool == Pool::Cold {
                self.stats[idx].pool = Pool::Warm;
            }
        }
    }

    /// Total images processed.
    pub fn images_processed(&self) -> u64 {
        self.images_processed
    }

    /// Save current statistics to disk as JSON.
    pub fn save(&self, path: &Path, vocabulary: &Vocabulary) -> Result<(), PipelineError> {
        let terms: HashMap<String, TermStats> = vocabulary
            .all_terms()
            .iter()
            .zip(self.stats.iter())
            .map(|(term, stat)| (term.name.clone(), stat.clone()))
            .collect();

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let file = RelevanceFile {
            version: 1,
            images_processed: self.images_processed,
            last_updated: now,
            terms,
        };

        let json = serde_json::to_string_pretty(&file).map_err(|e| PipelineError::Model {
            message: format!("Failed to serialize relevance data: {e}"),
        })?;
        std::fs::write(path, json.as_bytes()).map_err(|e| PipelineError::Model {
            message: format!("Failed to write relevance data to {path:?}: {e}"),
        })?;
        Ok(())
    }

    /// Load previously saved statistics from disk.
    ///
    /// Aligns by term name (not index) so vocabulary changes between runs
    /// are handled gracefully — new terms start Cold, removed terms are dropped.
    pub fn load(
        path: &Path,
        vocabulary: &Vocabulary,
        config: RelevanceConfig,
    ) -> Result<Self, PipelineError> {
        let content = std::fs::read_to_string(path).map_err(|e| PipelineError::Model {
            message: format!("Failed to read relevance data from {path:?}: {e}"),
        })?;
        let file: RelevanceFile =
            serde_json::from_str(&content).map_err(|e| PipelineError::Model {
                message: format!("Failed to parse relevance data: {e}"),
            })?;

        // Rebuild stats vector aligned to current vocabulary
        let stats: Vec<TermStats> = vocabulary
            .all_terms()
            .iter()
            .map(|term| {
                file.terms.get(&term.name).cloned().unwrap_or(TermStats {
                    hit_count: 0,
                    score_sum: 0.0,
                    last_hit_ts: 0,
                    pool: Pool::Cold,
                    warm_checks_without_hit: 0,
                })
            })
            .collect();

        Ok(Self {
            stats,
            images_processed: file.images_processed,
            config,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> RelevanceConfig {
        RelevanceConfig::default()
    }

    // ── TermStats tests ──

    #[test]
    fn test_avg_confidence_zero_hits() {
        let stat = TermStats {
            hit_count: 0,
            score_sum: 0.0,
            last_hit_ts: 0,
            pool: Pool::Active,
            warm_checks_without_hit: 0,
        };
        assert_eq!(stat.avg_confidence(), 0.0);
    }

    #[test]
    fn test_avg_confidence_calculation() {
        let stat = TermStats {
            hit_count: 3,
            score_sum: 0.8 + 0.6 + 0.7,
            last_hit_ts: 1000,
            pool: Pool::Active,
            warm_checks_without_hit: 0,
        };
        let avg = stat.avg_confidence();
        assert!((avg - 0.7).abs() < 0.001);
    }

    #[test]
    fn test_pool_serde_roundtrip() {
        let pools = vec![Pool::Active, Pool::Warm, Pool::Cold];
        for pool in pools {
            let json = serde_json::to_string(&pool).unwrap();
            let deserialized: Pool = serde_json::from_str(&json).unwrap();
            assert_eq!(pool, deserialized);
        }
        assert_eq!(serde_json::to_string(&Pool::Active).unwrap(), "\"active\"");
        assert_eq!(serde_json::to_string(&Pool::Warm).unwrap(), "\"warm\"");
        assert_eq!(serde_json::to_string(&Pool::Cold).unwrap(), "\"cold\"");
    }

    // ── RelevanceTracker construction tests ──

    #[test]
    fn test_new_encoded_terms_active() {
        let mask = vec![true, true, false, true];
        let tracker = RelevanceTracker::new(4, &mask, default_config());
        assert_eq!(tracker.pool(0), Pool::Active);
        assert_eq!(tracker.pool(1), Pool::Active);
        assert_eq!(tracker.pool(3), Pool::Active);
    }

    #[test]
    fn test_new_unencoded_terms_cold() {
        let mask = vec![true, false, false];
        let tracker = RelevanceTracker::new(3, &mask, default_config());
        assert_eq!(tracker.pool(1), Pool::Cold);
        assert_eq!(tracker.pool(2), Pool::Cold);
    }

    #[test]
    fn test_record_hits_updates_stats() {
        let mask = vec![true, true, true];
        let mut tracker = RelevanceTracker::new(3, &mask, default_config());

        tracker.record_hits(&[(0, 0.8), (2, 0.5)]);

        assert_eq!(tracker.stats[0].hit_count, 1);
        assert!((tracker.stats[0].score_sum - 0.8).abs() < 0.001);
        assert!(tracker.stats[0].last_hit_ts > 0);

        assert_eq!(tracker.stats[1].hit_count, 0);
        assert_eq!(tracker.stats[1].last_hit_ts, 0);

        assert_eq!(tracker.stats[2].hit_count, 1);
    }

    #[test]
    fn test_record_hits_increments_image_count() {
        let mask = vec![true];
        let mut tracker = RelevanceTracker::new(1, &mask, default_config());
        assert_eq!(tracker.images_processed(), 0);

        tracker.record_hits(&[(0, 0.5)]);
        assert_eq!(tracker.images_processed(), 1);

        tracker.record_hits(&[]);
        assert_eq!(tracker.images_processed(), 2);
    }

    // ── Pool transition tests ──

    #[test]
    fn test_sweep_demotes_stale_active() {
        let mask = vec![true];
        let config = RelevanceConfig {
            active_demotion_days: 1, // 1 day for testing
            ..default_config()
        };
        let mut tracker = RelevanceTracker::new(1, &mask, config);

        // Simulate a hit from 2 days ago
        tracker.stats[0].hit_count = 1;
        tracker.stats[0].score_sum = 0.5;
        let two_days_ago = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - 2 * 86400;
        tracker.stats[0].last_hit_ts = two_days_ago;

        tracker.sweep();
        assert_eq!(tracker.pool(0), Pool::Warm);
    }

    #[test]
    fn test_sweep_demotes_never_hit_active() {
        let mask = vec![true, true];
        let mut tracker = RelevanceTracker::new(2, &mask, default_config());
        tracker.images_processed = 1001; // Past the 1000 threshold

        // Term 0 has hits, term 1 has none
        tracker.stats[0].hit_count = 5;
        tracker.stats[0].score_sum = 2.5;
        tracker.stats[0].last_hit_ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        tracker.sweep();
        assert_eq!(tracker.pool(0), Pool::Active); // Has hits → stays
        assert_eq!(tracker.pool(1), Pool::Warm); // Never hit → demoted
    }

    #[test]
    fn test_sweep_promotes_warm_with_hits() {
        let mask = vec![false]; // Starts cold
        let config = RelevanceConfig {
            promotion_threshold: 0.3,
            ..default_config()
        };
        let mut tracker = RelevanceTracker::new(1, &mask, config);

        // Manually set to Warm with good stats
        tracker.stats[0].pool = Pool::Warm;
        tracker.stats[0].hit_count = 5;
        tracker.stats[0].score_sum = 2.0; // avg = 0.4 > threshold 0.3

        let promoted = tracker.sweep();
        assert_eq!(tracker.pool(0), Pool::Active);
        assert_eq!(promoted, vec![0]);
    }

    #[test]
    fn test_sweep_returns_promoted_indices() {
        let mask = vec![false, false, false];
        let config = RelevanceConfig {
            promotion_threshold: 0.2,
            ..default_config()
        };
        let mut tracker = RelevanceTracker::new(3, &mask, config);

        // Term 0: warm with good stats → should promote
        tracker.stats[0].pool = Pool::Warm;
        tracker.stats[0].hit_count = 3;
        tracker.stats[0].score_sum = 0.9;

        // Term 1: warm but below threshold → stays
        tracker.stats[1].pool = Pool::Warm;
        tracker.stats[1].hit_count = 1;
        tracker.stats[1].score_sum = 0.1; // avg = 0.1 < 0.2

        // Term 2: cold → not touched
        let promoted = tracker.sweep();
        assert_eq!(promoted, vec![0]);
        assert_eq!(tracker.pool(0), Pool::Active);
        assert_eq!(tracker.pool(1), Pool::Warm);
        assert_eq!(tracker.pool(2), Pool::Cold);
    }

    #[test]
    fn test_sweep_preserves_recent_active() {
        let mask = vec![true];
        let config = RelevanceConfig {
            active_demotion_days: 90,
            ..default_config()
        };
        let mut tracker = RelevanceTracker::new(1, &mask, config);

        // Recent hit
        tracker.stats[0].hit_count = 1;
        tracker.stats[0].score_sum = 0.5;
        tracker.stats[0].last_hit_ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        tracker.sweep();
        assert_eq!(tracker.pool(0), Pool::Active);
    }

    #[test]
    fn test_should_check_warm_interval() {
        let config = RelevanceConfig {
            warm_check_interval: 100,
            ..default_config()
        };
        let mask = vec![true];
        let mut tracker = RelevanceTracker::new(1, &mask, config);

        // images_processed starts at 0 — 0 % 100 == 0, so first image checks warm
        assert!(tracker.should_check_warm());

        tracker.record_hits(&[]); // images_processed = 1
        assert!(!tracker.should_check_warm());

        // Simulate reaching 100
        tracker.images_processed = 100;
        assert!(tracker.should_check_warm());

        tracker.images_processed = 101;
        assert!(!tracker.should_check_warm());
    }

    // ── Pool counts test ──

    #[test]
    fn test_pool_counts() {
        let mask = vec![true, true, false, false, true];
        let mut tracker = RelevanceTracker::new(5, &mask, default_config());
        tracker.stats[0].pool = Pool::Warm; // Override one active → warm

        let (active, warm, cold) = tracker.pool_counts();
        assert_eq!(active, 2);
        assert_eq!(warm, 1);
        assert_eq!(cold, 2);
    }

    // ── Promote to warm test ──

    #[test]
    fn test_promote_to_warm() {
        let mask = vec![true, false, false];
        let mut tracker = RelevanceTracker::new(3, &mask, default_config());
        assert_eq!(tracker.pool(1), Pool::Cold);
        assert_eq!(tracker.pool(2), Pool::Cold);

        tracker.promote_to_warm(&[1, 2]);
        assert_eq!(tracker.pool(1), Pool::Warm);
        assert_eq!(tracker.pool(2), Pool::Warm);

        // Already-active terms are unaffected
        tracker.promote_to_warm(&[0]);
        assert_eq!(tracker.pool(0), Pool::Active);
    }

    // ── Persistence tests ──

    #[test]
    fn test_save_load_roundtrip() {
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();

        // Create a vocabulary
        let vocab_dir = dir.path().join("vocab");
        std::fs::create_dir_all(&vocab_dir).unwrap();
        let mut f = std::fs::File::create(vocab_dir.join("wordnet_nouns.txt")).unwrap();
        writeln!(f, "dog\t00000001\tanimal").unwrap();
        writeln!(f, "cat\t00000002\tanimal").unwrap();
        writeln!(f, "car\t00000003\tvehicle").unwrap();
        let vocab = Vocabulary::load(&vocab_dir).unwrap();

        // Create tracker with some data
        let mask = vec![true, true, false];
        let mut tracker = RelevanceTracker::new(3, &mask, default_config());
        tracker.record_hits(&[(0, 0.8), (1, 0.5)]);
        tracker.record_hits(&[(0, 0.9)]);

        // Save
        let save_path = dir.path().join("relevance.json");
        tracker.save(&save_path, &vocab).unwrap();

        // Load
        let loaded = RelevanceTracker::load(&save_path, &vocab, default_config()).unwrap();
        assert_eq!(loaded.stats[0].hit_count, 2);
        assert!((loaded.stats[0].score_sum - 1.7).abs() < 0.001);
        assert_eq!(loaded.stats[1].hit_count, 1);
        assert_eq!(loaded.stats[2].hit_count, 0);
        assert_eq!(loaded.pool(0), Pool::Active);
        assert_eq!(loaded.pool(2), Pool::Cold);
        assert_eq!(loaded.images_processed(), 2);
    }

    #[test]
    fn test_load_with_vocabulary_change() {
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();

        // Original vocabulary: dog, cat
        let vocab_dir = dir.path().join("vocab");
        std::fs::create_dir_all(&vocab_dir).unwrap();
        let mut f = std::fs::File::create(vocab_dir.join("wordnet_nouns.txt")).unwrap();
        writeln!(f, "dog\t00000001\tanimal").unwrap();
        writeln!(f, "cat\t00000002\tanimal").unwrap();
        let vocab1 = Vocabulary::load(&vocab_dir).unwrap();

        let mask = vec![true, true];
        let mut tracker = RelevanceTracker::new(2, &mask, default_config());
        tracker.record_hits(&[(0, 0.8)]);

        let save_path = dir.path().join("relevance.json");
        tracker.save(&save_path, &vocab1).unwrap();

        // New vocabulary: cat, fish (dog removed, fish added)
        let mut f = std::fs::File::create(vocab_dir.join("wordnet_nouns.txt")).unwrap();
        writeln!(f, "cat\t00000002\tanimal").unwrap();
        writeln!(f, "fish\t00000004\tanimal").unwrap();
        let vocab2 = Vocabulary::load(&vocab_dir).unwrap();

        let loaded = RelevanceTracker::load(&save_path, &vocab2, default_config()).unwrap();
        // cat kept its stats
        assert_eq!(loaded.stats[0].hit_count, 0); // cat was index 1 originally, 0 now
                                                  // fish is new → cold
        assert_eq!(loaded.stats[1].hit_count, 0);
        assert_eq!(loaded.pool(1), Pool::Cold);
    }

    #[test]
    fn test_load_missing_file_error() {
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();
        let vocab_dir = dir.path().join("vocab");
        std::fs::create_dir_all(&vocab_dir).unwrap();
        let mut f = std::fs::File::create(vocab_dir.join("wordnet_nouns.txt")).unwrap();
        writeln!(f, "dog\t00000001\tanimal").unwrap();
        let vocab = Vocabulary::load(&vocab_dir).unwrap();

        let result = RelevanceTracker::load(
            &dir.path().join("nonexistent.json"),
            &vocab,
            default_config(),
        );
        assert!(result.is_err());
    }

    // ── Config defaults test ──

    #[test]
    fn test_relevance_config_defaults() {
        let config = RelevanceConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.warm_check_interval, 100);
        assert!((config.promotion_threshold - 0.3).abs() < 0.001);
        assert_eq!(config.active_demotion_days, 90);
        assert_eq!(config.warm_demotion_checks, 50);
        assert!(config.neighbor_expansion);
    }

    // ── Warm→Cold demotion tests ──

    #[test]
    fn test_warm_to_cold_demotion() {
        let mask = vec![false];
        let config = RelevanceConfig {
            warm_demotion_checks: 3, // Demote after 3 sweeps with no hits
            ..default_config()
        };
        let mut tracker = RelevanceTracker::new(1, &mask, config);
        tracker.stats[0].pool = Pool::Warm;

        // Sweep 1, 2: still Warm
        tracker.sweep();
        assert_eq!(tracker.pool(0), Pool::Warm);
        tracker.sweep();
        assert_eq!(tracker.pool(0), Pool::Warm);

        // Sweep 3: demoted to Cold
        tracker.sweep();
        assert_eq!(tracker.pool(0), Pool::Cold);
    }

    #[test]
    fn test_warm_hit_resets_demotion_counter() {
        let mask = vec![false];
        let config = RelevanceConfig {
            warm_demotion_checks: 3,
            promotion_threshold: 0.9, // High threshold so hit doesn't promote
            ..default_config()
        };
        let mut tracker = RelevanceTracker::new(1, &mask, config);
        tracker.stats[0].pool = Pool::Warm;

        // Two sweeps with no hits
        tracker.sweep();
        tracker.sweep();
        assert_eq!(tracker.stats[0].warm_checks_without_hit, 2);

        // Record a hit — resets counter
        tracker.record_hits(&[(0, 0.1)]);
        assert_eq!(tracker.stats[0].warm_checks_without_hit, 0);

        // Two more sweeps — still Warm (counter was reset)
        tracker.sweep();
        tracker.sweep();
        assert_eq!(tracker.pool(0), Pool::Warm);
    }

    #[test]
    fn test_warm_promotion_resets_counter() {
        let mask = vec![false];
        let config = RelevanceConfig {
            warm_demotion_checks: 50,
            promotion_threshold: 0.2,
            ..default_config()
        };
        let mut tracker = RelevanceTracker::new(1, &mask, config);
        tracker.stats[0].pool = Pool::Warm;
        tracker.stats[0].warm_checks_without_hit = 10; // Some accumulated checks
        tracker.stats[0].hit_count = 5;
        tracker.stats[0].score_sum = 2.0; // avg = 0.4 > threshold 0.2

        let promoted = tracker.sweep();
        assert_eq!(tracker.pool(0), Pool::Active);
        assert_eq!(promoted, vec![0]);
        assert_eq!(tracker.stats[0].warm_checks_without_hit, 0);
    }

    // ── Bounds safety tests ──

    #[test]
    fn test_record_hits_out_of_bounds_skips() {
        let mask = vec![true, true];
        let mut tracker = RelevanceTracker::new(2, &mask, default_config());

        // Mix of valid and out-of-bounds indices
        tracker.record_hits(&[(0, 0.8), (999, 0.5), (1, 0.6)]);

        // Valid hits recorded
        assert_eq!(tracker.stats[0].hit_count, 1);
        assert_eq!(tracker.stats[1].hit_count, 1);
        // Image count still incremented
        assert_eq!(tracker.images_processed(), 1);
    }

    #[test]
    fn test_pool_out_of_bounds_returns_cold() {
        let mask = vec![true];
        let tracker = RelevanceTracker::new(1, &mask, default_config());

        assert_eq!(tracker.pool(0), Pool::Active); // In bounds
        assert_eq!(tracker.pool(999), Pool::Cold); // Out of bounds → Cold
    }

    #[test]
    fn test_promote_to_warm_out_of_bounds_skips() {
        let mask = vec![true, false];
        let mut tracker = RelevanceTracker::new(2, &mask, default_config());

        // Mix of valid and out-of-bounds
        tracker.promote_to_warm(&[1, 999]);
        assert_eq!(tracker.pool(1), Pool::Warm); // Valid promotion
        assert_eq!(tracker.pool(0), Pool::Active); // Unaffected
    }
}
