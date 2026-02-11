# Phase 4c+4d: Relevance Pruning & Neighbor Expansion

> **Status:** Planned
> **Depends on:** Phase 4b (progressive encoding)
> **Goal:** Self-organizing vocabulary that converges to the user's image library. Irrelevant terms are demoted to reduce scoring cost; neighbors of active terms are prioritized for deeper coverage where it matters.

---

## Why Bundle 4c and 4d?

These phases are tightly coupled:

- **4c (Relevance Pruning)** introduces the three-pool system (active/warm/cold) with per-term statistics
- **4d (Neighbor Expansion)** triggers when a term enters the active pool, promoting its WordNet neighbors

Neighbor expansion depends on pool transitions (a 4c concept). Building 4c without 4d leaves an incomplete feedback loop. Building them together costs ~20% more effort but delivers a cohesive self-organizing system.

---

## Problem

Phase 4a/4b score every image against the full encoded vocabulary (~68K terms). This works — dot product over 68K terms takes ~2ms. But:

1. **Most terms are irrelevant.** A food photographer will never match "submarine" or "constellation". Scoring against these wastes ~70% of compute.
2. **Coverage gaps in relevant areas.** If "labrador" scores highly, related terms like "golden retriever" and "puppy" should be checked — but they might be in the cold pool (not yet encoded) or never sampled in the seed set.
3. **No learning.** Processing 10,000 food photos teaches the system nothing about what the user cares about.

## Solution

A three-pool system where terms self-organize based on scoring history:

```
                    ┌─────────────────────────┐
                    │       ACTIVE POOL       │
                    │    3K–15K terms         │
                    │   Scored every image     │
                    └──────┬──────────────────┘
                           │
              ┌────────────┴────────────┐
     promote  │                         │ demote
     (scored  │                         │ (no match
      above   │                         │  in N days)
    threshold)│                         │
              │                         │
   ┌──────────┴──────────┐   ┌─────────┴──────────┐
   │     WARM POOL       │   │     COLD POOL      │
   │   10K–30K terms     │   │    Remainder       │
   │  Checked every Nth  │   │   Not scored       │
   │  image              │   │   (re-check on     │
   └─────────────────────┘   │    rebuild)        │
                              └────────────────────┘
```

Terms flow between pools based on scoring results. Neighbor expansion widens coverage in areas the user actually photographs.

---

## Design Decisions

### 1. Per-Term Statistics: What to Track

Each vocabulary term accumulates lightweight statistics:

```rust
pub struct TermStats {
    /// How many times this term scored above min_confidence
    pub hit_count: u32,
    /// Running sum of confidence scores when matched (for computing average)
    pub score_sum: f32,
    /// Timestamp of last match (Unix seconds, 0 = never)
    pub last_hit_ts: u64,
    /// Current pool assignment
    pub pool: Pool,
}
```

**Decision: Why not track every score?** Full score history would grow unbounded. We only need: "does this term ever match?" (hit_count), "how well?" (score_sum / hit_count), and "how recently?" (last_hit_ts). These three signals drive all pool transitions.

**Memory:** 4 + 4 + 8 + 1 = 17 bytes per term. For 68K terms: ~1.1MB. Negligible.

### 2. Pool Transition Rules

| Transition | Condition | When Checked |
|------------|-----------|--------------|
| Warm → Active | Term scores above `promotion_threshold` during a warm-pool check | Every `warm_check_interval` images |
| Cold → Warm | Term is encoded (background or neighbor expansion) | After encoding |
| Active → Warm | No hits in `active_demotion_days` days | Periodic sweep (every 1000 images) |
| Warm → Cold | No hits after `warm_demotion_images` warm-pool checks | Periodic sweep |

**Decision: Why not demote immediately on low score?** A term might be relevant for seasonal content (e.g., "christmas tree" in December). Time-based demotion (90 days default) gives generous runway. Users can adjust via config.

### 3. Warm Pool Scoring Strategy

The warm pool is scored against a subset of images — not every image. This controls cost:

- Every `warm_check_interval` images (default: 100), the current image is also scored against warm-pool terms
- This adds ~2ms every 100 images (negligible overhead)
- Terms that score above `promotion_threshold` are immediately promoted to active

**Decision: Why not just score warm terms on every image?** With 30K warm terms, that's ~2ms extra per image. For batch processing at 50 img/min, this adds 100ms/min — acceptable. But the interval-based approach is more conservative and makes the warm pool effectively "sampled", which prevents noisy promotions from single outlier images.

### 4. Neighbor Expansion Trigger

When a term transitions from warm/cold → active:

1. Look up its WordNet hypernym chain
2. Find all sibling terms (terms sharing the same immediate parent)
3. If siblings are in cold pool → promote to warm (and encode if not yet encoded)
4. If siblings are already warm or active → no-op

Example:
```
"labrador_retriever" promoted to active
  Parent: "retriever"
  Siblings: "golden_retriever", "Chesapeake_Bay_retriever", "curly-coated_retriever", ...
  Action: promote unencoded siblings to warm, queue for encoding
```

**Decision: Why only siblings, not children/grandchildren?** WordNet trees can be deep. Expanding too aggressively floods the warm pool. Siblings are the most semantically similar — if "labrador" matches, "golden retriever" is likely relevant too. Deeper expansion (children of children) can happen naturally as siblings promote to active and trigger their own expansions.

### 5. Persistence Format

Statistics are saved to `~/.photon/taxonomy/relevance.json`:

```json
{
  "version": 1,
  "images_processed": 5432,
  "last_updated": "2026-02-11T14:30:00Z",
  "terms": {
    "labrador_retriever": {
      "hit_count": 847,
      "score_sum": 695.54,
      "last_hit_ts": 1739284200,
      "pool": "active"
    },
    "submarine": {
      "hit_count": 0,
      "score_sum": 0.0,
      "last_hit_ts": 0,
      "pool": "cold"
    }
  }
}
```

**Decision: Why JSON not binary?** Relevance data is small (~2-3MB for 68K terms). JSON is human-readable, debuggable, and trivially parseable. Binary would save ~1MB but isn't worth the complexity.

**Decision: When to save?** After every batch run completes (not per-image). If the process crashes mid-batch, statistics from that batch are lost — acceptable since they'll be recollected on the next run.

### 6. Scoring Architecture Change

Currently, `TagScorer::score()` scores against ALL terms in the label bank. With pools:

- `score()` scores only **active** terms (fast path, every image)
- `score_warm()` scores only **warm** terms (called every Nth image)
- Pool membership is tracked via a `Vec<Pool>` parallel to the vocabulary

The label bank matrix stays flat (all encoded terms). Pool membership determines which rows are used during scoring. This avoids matrix restructuring.

```rust
// Scoring active pool (every image):
for i in 0..n {
    if self.pools[i] != Pool::Active { continue; }
    // ... dot product ...
}

// Scoring warm pool (every Nth image):
for i in 0..n {
    if self.pools[i] != Pool::Warm { continue; }
    // ... dot product ...
}
```

**Decision: Why not partition the matrix by pool?** Partitioning would give better cache locality but requires matrix restructuring on every pool transition. With 68K terms and a 768-dim vector, the full matrix is ~200MB — scanning active-only skips rows in-place, which modern CPUs handle efficiently with branch prediction. Partitioning is a future optimization if profiling shows it matters.

---

## New Types

### `Pool` Enum (new: `tagging/relevance.rs`)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Pool {
    Active,
    Warm,
    Cold,
}
```

### `TermStats` (new: `tagging/relevance.rs`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TermStats {
    pub hit_count: u32,
    pub score_sum: f32,
    pub last_hit_ts: u64,
    pub pool: Pool,
}

impl TermStats {
    pub fn avg_confidence(&self) -> f32 {
        if self.hit_count == 0 { 0.0 } else { self.score_sum / self.hit_count as f32 }
    }
}
```

### `RelevanceTracker` (new: `tagging/relevance.rs`)

```rust
/// Tracks per-term scoring statistics and manages pool assignments.
pub struct RelevanceTracker {
    /// Per-term statistics, indexed parallel to vocabulary.
    stats: Vec<TermStats>,
    /// Total images processed since last save.
    images_since_save: u64,
    /// Total images processed overall.
    images_processed: u64,
    /// Configuration for pool transitions.
    config: RelevanceConfig,
}

impl RelevanceTracker {
    /// Create a new tracker with all terms in the initial pool.
    /// Terms that are encoded start in Active; unencoded start in Cold.
    pub fn new(term_count: usize, encoded_mask: &[bool], config: RelevanceConfig) -> Self;

    /// Load previously saved statistics from disk.
    pub fn load(path: &Path, term_count: usize) -> Result<Self, PipelineError>;

    /// Save current statistics to disk.
    pub fn save(&self, path: &Path, vocabulary: &Vocabulary) -> Result<(), PipelineError>;

    /// Record scoring results for one image.
    /// Called after every score() with the (term_index, confidence) pairs
    /// that exceeded min_confidence.
    pub fn record_hits(&mut self, hits: &[(usize, f32)]);

    /// Check if warm-pool scoring should happen this image.
    pub fn should_check_warm(&self) -> bool;

    /// Run pool transition sweep. Returns indices of newly promoted terms
    /// (for neighbor expansion).
    pub fn sweep(&mut self) -> Vec<usize>;

    /// Get the pool assignment for a term.
    pub fn pool(&self, term_index: usize) -> Pool;

    /// Get pool counts for logging.
    pub fn pool_counts(&self) -> (usize, usize, usize); // (active, warm, cold)

    /// Promote terms to warm pool (for neighbor expansion).
    pub fn promote_to_warm(&mut self, indices: &[usize]);
}
```

### `RelevanceConfig` (addition to `TaggingConfig`)

```rust
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
            enabled: false,  // Off by default — opt-in for 4c
            warm_check_interval: 100,
            promotion_threshold: 0.3,
            active_demotion_days: 90,
            warm_demotion_checks: 50,
            neighbor_expansion: true,
        }
    }
}
```

### `NeighborExpander` (new: `tagging/neighbors.rs`)

```rust
/// Finds WordNet neighbors of terms for vocabulary expansion.
pub struct NeighborExpander;

impl NeighborExpander {
    /// Given a term index that was just promoted to active,
    /// find sibling terms (shared parent in WordNet hierarchy).
    /// Returns indices of sibling terms in the vocabulary.
    pub fn find_siblings(
        vocabulary: &Vocabulary,
        term_index: usize,
    ) -> Vec<usize>;

    /// Batch expansion: find siblings for multiple promoted terms.
    /// Deduplicates results.
    pub fn expand_all(
        vocabulary: &Vocabulary,
        promoted_indices: &[usize],
    ) -> Vec<usize>;
}
```

---

## File-by-File Changes

### New Files

| File | Purpose |
|------|---------|
| `crates/photon-core/src/tagging/relevance.rs` | `Pool`, `TermStats`, `RelevanceTracker`, `RelevanceConfig` |
| `crates/photon-core/src/tagging/neighbors.rs` | `NeighborExpander` — WordNet sibling lookup |

### Modified Files

| File | Changes |
|------|---------|
| `crates/photon-core/src/tagging/mod.rs` | Add `pub mod relevance;` and `pub mod neighbors;`, re-export `RelevanceTracker` |
| `crates/photon-core/src/tagging/scorer.rs` | Add pool-aware `score_active()` and `score_warm()` methods; `score()` delegates based on relevance tracker presence |
| `crates/photon-core/src/tagging/vocabulary.rs` | Add `Vocabulary::parent_of(term_index)` and `Vocabulary::siblings_of(term_index)` methods using hypernym data |
| `crates/photon-core/src/pipeline/processor.rs` | Wire `RelevanceTracker` into `ImageProcessor`, call `record_hits()` after scoring, call `sweep()` periodically |
| `crates/photon-core/src/config.rs` | Add `RelevanceConfig` to `TaggingConfig` |

---

## Task Breakdown

### Task 1: `Pool` Enum and `TermStats`

**File:** `tagging/relevance.rs` (new)

Define the core data structures. `Pool` is a simple 3-variant enum. `TermStats` tracks per-term scoring history.

**Tests:**
- `test_avg_confidence_zero_hits` — returns 0.0 when hit_count is 0
- `test_avg_confidence_calculation` — 3 hits with scores 0.8+0.6+0.7 → avg 0.7
- `test_pool_serde_roundtrip` — Pool::Active serializes as "active", deserializes back

### Task 2: `RelevanceTracker` Core

**File:** `tagging/relevance.rs`

Implement the tracker's constructor, `record_hits()`, and `pool()` accessor.

```rust
pub fn new(term_count: usize, encoded_mask: &[bool], config: RelevanceConfig) -> Self {
    let stats = (0..term_count)
        .map(|i| TermStats {
            hit_count: 0,
            score_sum: 0.0,
            last_hit_ts: 0,
            pool: if encoded_mask[i] { Pool::Active } else { Pool::Cold },
        })
        .collect();
    Self {
        stats,
        images_since_save: 0,
        images_processed: 0,
        config,
    }
}

pub fn record_hits(&mut self, hits: &[(usize, f32)]) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    for &(idx, confidence) in hits {
        let stat = &mut self.stats[idx];
        stat.hit_count += 1;
        stat.score_sum += confidence;
        stat.last_hit_ts = now;
    }
    self.images_processed += 1;
    self.images_since_save += 1;
}
```

**Tests:**
- `test_new_encoded_terms_active` — encoded terms start in Active pool
- `test_new_unencoded_terms_cold` — unencoded terms start in Cold pool
- `test_record_hits_updates_stats` — hit_count, score_sum, last_hit_ts updated
- `test_record_hits_increments_image_count` — images_processed incremented

### Task 3: Pool Transitions (`sweep()`)

**File:** `tagging/relevance.rs`

```rust
pub fn sweep(&mut self) -> Vec<usize> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let demotion_threshold_secs =
        self.config.active_demotion_days as u64 * 86400;

    let mut newly_promoted = Vec::new();

    for (i, stat) in self.stats.iter_mut().enumerate() {
        match stat.pool {
            Pool::Active => {
                // Demote if no hits in N days
                if stat.last_hit_ts > 0
                    && now.saturating_sub(stat.last_hit_ts) > demotion_threshold_secs
                {
                    stat.pool = Pool::Warm;
                }
                // Also demote if never hit at all and enough images have been processed
                if stat.hit_count == 0 && self.images_processed > 1000 {
                    stat.pool = Pool::Warm;
                }
            }
            Pool::Warm => {
                // Promote if recent hits with high enough confidence
                if stat.hit_count > 0
                    && stat.avg_confidence() >= self.config.promotion_threshold
                {
                    stat.pool = Pool::Active;
                    newly_promoted.push(i);
                }
            }
            Pool::Cold => {
                // Cold terms can only be promoted externally via promote_to_warm()
            }
        }
    }

    newly_promoted
}

pub fn should_check_warm(&self) -> bool {
    self.images_processed % self.config.warm_check_interval == 0
}
```

**Tests:**
- `test_sweep_demotes_stale_active` — active term with old last_hit_ts → warm
- `test_sweep_promotes_warm_with_hits` — warm term with high avg confidence → active
- `test_sweep_returns_promoted_indices` — promoted indices returned for neighbor expansion
- `test_sweep_preserves_recent_active` — active term with recent hits stays active
- `test_should_check_warm_interval` — returns true at correct intervals

### Task 4: Persistence (Load/Save)

**File:** `tagging/relevance.rs`

```rust
#[derive(Serialize, Deserialize)]
struct RelevanceFile {
    version: u32,
    images_processed: u64,
    last_updated: u64, // Unix timestamp (seconds)
    terms: HashMap<String, TermStats>,
}

pub fn save(&self, path: &Path, vocabulary: &Vocabulary) -> Result<(), PipelineError> {
    let terms: HashMap<String, TermStats> = vocabulary
        .all_terms()
        .iter()
        .zip(self.stats.iter())
        .map(|(term, stat)| (term.name.clone(), stat.clone()))
        .collect();

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let file = RelevanceFile {
        version: 1,
        images_processed: self.images_processed,
        last_updated: now,
        terms,
    };

    // NOTE: no chrono dependency — we use Unix timestamps throughout.
    // PipelineError has no From<serde_json::Error> or From<io::Error>,
    // so all error conversions use explicit .map_err().
    let json = serde_json::to_string_pretty(&file).map_err(|e| PipelineError::Model {
        message: format!("Failed to serialize relevance data: {e}"),
    })?;
    std::fs::write(path, &json).map_err(|e| PipelineError::Model {
        message: format!("Failed to write relevance data to {:?}: {e}", path),
    })?;
    Ok(())
}

pub fn load(path: &Path, vocabulary: &Vocabulary, config: RelevanceConfig) -> Result<Self, PipelineError> {
    let content = std::fs::read_to_string(path).map_err(|e| PipelineError::Model {
        message: format!("Failed to read relevance data from {:?}: {e}", path),
    })?;
    let file: RelevanceFile = serde_json::from_str(&content).map_err(|e| PipelineError::Model {
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
                pool: Pool::Cold, // Unknown terms start cold
            })
        })
        .collect();

    Ok(Self {
        stats,
        images_since_save: 0,
        images_processed: file.images_processed,
        config,
    })
}
```

**Key detail:** Loading aligns by term name, not index. This handles vocabulary changes gracefully — new terms start cold, removed terms are dropped.

**Tests:**
- `test_save_load_roundtrip` — save then load, verify stats preserved
- `test_load_with_vocabulary_change` — load with modified vocabulary, new terms get cold stats
- `test_load_missing_file_error` — missing file returns appropriate error

### Task 5: Pool-Aware Scoring

**File:** `tagging/scorer.rs`

Add pool-aware scoring methods alongside the existing `score()`.

**Critical design note:** `score_pool()` takes `&RelevanceTracker` (read-only) so it can run without holding a write lock. Hit recording is split into a separate step. This prevents write-lock serialization during parallel batch processing — scoring (~2ms) runs concurrently, only the brief `record_hits()` call needs the write lock.

Also, both `score()` and `score_with_relevance()` share the same filter/sort/truncate post-processing. Extract it into a private `hits_to_tags()` helper to avoid logic divergence.

```rust
impl TagScorer {
    /// Convert raw (term_index, confidence) hits into filtered, sorted, truncated tags.
    /// Shared by score() and score_with_relevance().
    fn hits_to_tags(&self, hits: Vec<(usize, f32)>) -> Vec<Tag> {
        let terms = self.vocabulary.all_terms();
        let mut tags: Vec<Tag> = hits
            .into_iter()
            .filter(|(_, conf)| *conf >= self.config.min_confidence)
            .map(|(idx, confidence)| Tag {
                name: terms[idx].display_name.clone(),
                confidence,
                category: terms[idx].category.clone(),
            })
            .collect();

        tags.sort_by(|a, b| b.confidence.total_cmp(&a.confidence));
        tags.truncate(self.config.max_tags);
        tags
    }

    /// Score against terms in a specific pool only.
    /// Takes &RelevanceTracker (read-only) — no write lock needed.
    /// Returns (term_index, confidence) pairs above threshold.
    pub fn score_pool(
        &self,
        image_embedding: &[f32],
        tracker: &RelevanceTracker,
        pool: Pool,
    ) -> Vec<(usize, f32)> {
        let n = self.label_bank.term_count();
        let dim = self.label_bank.embedding_dim();
        let matrix = self.label_bank.matrix();
        let mut hits = Vec::new();

        for i in 0..n {
            if tracker.pool(i) != pool { continue; }

            let offset = i * dim;
            let cosine: f32 = (0..dim)
                .map(|j| image_embedding[j] * matrix[offset + j])
                .sum();
            let confidence = Self::cosine_to_confidence(cosine);

            if confidence >= self.config.min_confidence {
                hits.push((i, confidence));
            }
        }

        hits
    }

    /// Pool-aware scoring: active terms + optional warm check.
    ///
    /// NOTE: This method does NOT call record_hits() or mutate the tracker.
    /// The caller is responsible for recording hits separately — this allows
    /// scoring to run under a read lock while only the brief record_hits()
    /// call needs a write lock. See Task 8 for the integration pattern.
    pub fn score_with_pools(
        &self,
        image_embedding: &[f32],
        tracker: &RelevanceTracker,
    ) -> (Vec<Tag>, Vec<(usize, f32)>) {
        // 1. Score active pool (every image)
        let mut all_hits = self.score_pool(image_embedding, tracker, Pool::Active);

        // 2. Optionally score warm pool (every Nth image)
        if tracker.should_check_warm() {
            let warm_hits = self.score_pool(image_embedding, tracker, Pool::Warm);
            all_hits.extend(warm_hits);
        }

        // 3. Convert to tags using shared helper
        let tags = self.hits_to_tags(all_hits.clone());

        // Return both tags (for output) and raw hits (for recording)
        (tags, all_hits)
    }
}
```

The existing `score()` method is updated to use `hits_to_tags()` internally (replacing its inline filter/sort/truncate) but its signature and behavior are unchanged.

**Tests:**
- `test_score_pool_filters_by_pool` — only active-pool terms scored when pool=Active
- `test_score_with_pools_returns_hits` — raw hits returned for separate recording
- `test_hits_to_tags_filters_sorts_truncates` — shared helper works correctly

### Task 6: Neighbor Expansion

**File:** `tagging/neighbors.rs` (new)

```rust
impl NeighborExpander {
    pub fn find_siblings(vocabulary: &Vocabulary, term_index: usize) -> Vec<usize> {
        let term = &vocabulary.all_terms()[term_index];

        // Get the immediate parent (first hypernym)
        let parent = match term.hypernyms.first() {
            Some(p) => p,
            None => return vec![], // Supplemental terms have no hypernyms
        };

        // Find all terms whose first hypernym matches
        vocabulary
            .all_terms()
            .iter()
            .enumerate()
            .filter(|(i, t)| {
                *i != term_index
                    && t.hypernyms.first().map(|h| h.as_str()) == Some(parent.as_str())
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub fn expand_all(vocabulary: &Vocabulary, promoted_indices: &[usize]) -> Vec<usize> {
        let mut siblings = HashSet::new();
        for &idx in promoted_indices {
            for sib in Self::find_siblings(vocabulary, idx) {
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
```

**Performance note:** `find_siblings` does a linear scan of the vocabulary. For 68K terms this is ~microseconds. If profiling shows it matters, build a parent→children index at load time.

**Tests:**
- `test_find_siblings_shared_parent` — terms with same first hypernym are found
- `test_find_siblings_excludes_self` — the input term is not in the result
- `test_find_siblings_no_hypernyms` — supplemental terms return empty vec
- `test_expand_all_deduplicates` — siblings shared between promoted terms appear once
- `test_expand_all_excludes_promoted` — promoted terms not in result

### Task 7: Vocabulary Sibling Lookup Helpers

**File:** `vocabulary.rs`

Add helper methods for hierarchy navigation:

```rust
impl Vocabulary {
    /// Get the immediate parent (first hypernym) of a term.
    pub fn parent_of(&self, term_index: usize) -> Option<&str> {
        self.terms.get(term_index)
            .and_then(|t| t.hypernyms.first())
            .map(|s| s.as_str())
    }

    /// Build a parent → children index for fast sibling lookup.
    /// Returns a HashMap from parent display_name to Vec of child term indices.
    pub fn build_parent_index(&self) -> HashMap<String, Vec<usize>> {
        let mut index: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, term) in self.terms.iter().enumerate() {
            if let Some(parent) = term.hypernyms.first() {
                index.entry(parent.clone()).or_default().push(i);
            }
        }
        index
    }
}
```

**Tests:**
- `test_parent_of_wordnet_term` — returns first hypernym
- `test_parent_of_supplemental_term` — returns None (no hypernyms)
- `test_build_parent_index` — sibling terms grouped under same parent

### Task 8: Wire Into `ImageProcessor`

**File:** `pipeline/processor.rs`

Add `RelevanceTracker` to `ImageProcessor`:

```rust
pub struct ImageProcessor {
    // ... existing fields ...
    tag_scorer: Option<Arc<RwLock<TagScorer>>>,
    relevance_tracker: Option<RwLock<RelevanceTracker>>, // New
}
```

Update `load_tagging()` to load or create the relevance tracker:

```rust
// After creating the TagScorer:
let relevance_tracker = if config.tagging.relevance.enabled {
    let relevance_path = taxonomy_dir.join("relevance.json");
    if relevance_path.exists() {
        match RelevanceTracker::load(&relevance_path, &vocabulary, config.tagging.relevance.clone()) {
            Ok(tracker) => {
                let (active, warm, cold) = tracker.pool_counts();
                tracing::info!("Loaded relevance data: {active} active, {warm} warm, {cold} cold");
                Some(RwLock::new(tracker))
            }
            Err(e) => {
                tracing::warn!("Failed to load relevance data: {e} — starting fresh");
                let encoded_mask = vec![true; vocabulary.len()]; // All encoded at this point
                Some(RwLock::new(RelevanceTracker::new(
                    vocabulary.len(), &encoded_mask, config.tagging.relevance.clone()
                )))
            }
        }
    } else {
        let encoded_mask = vec![true; vocabulary.len()];
        Some(RwLock::new(RelevanceTracker::new(
            vocabulary.len(), &encoded_mask, config.tagging.relevance.clone()
        )))
    }
} else {
    None
};
```

Update the scoring section in `process_with_options()`.

**Key pattern: split read-lock scoring from write-lock recording.** Scoring (~2ms) runs under a read lock on the tracker (or no lock at all since `score_pool` only reads). Only the brief `record_hits()` call needs a write lock. This prevents serialization during parallel processing.

```rust
let tags = if !options.skip_tagging {
    match (&self.tag_scorer, &self.relevance_tracker, &embedding) {
        (Some(scorer_lock), Some(tracker_lock), emb) if !emb.is_empty() => {
            // Phase 1: Score under READ lock (concurrent, ~2ms)
            let (tags, raw_hits) = {
                let scorer = scorer_lock.read().unwrap();
                let tracker = tracker_lock.read().unwrap();
                scorer.score_with_pools(emb, &tracker)
            };
            // Read locks dropped here

            // Phase 2: Record hits under WRITE lock (brief, ~μs)
            {
                let mut tracker = tracker_lock.write().unwrap();
                tracker.record_hits(&raw_hits);
            }
            // Write lock dropped here

            tags
        }
        (Some(scorer_lock), None, emb) if !emb.is_empty() => {
            // No relevance tracking — score all terms (4a behavior)
            let scorer = scorer_lock.read().unwrap();
            scorer.score(emb)
        }
        _ => vec![],
    }
} else {
    vec![]
};
```

Add a method to save relevance data at the end of a batch:

```rust
impl ImageProcessor {
    /// Save relevance tracking data to disk.
    /// Call this at the end of a batch processing run.
    pub fn save_relevance(&self, config: &Config) -> Result<()> {
        if let (Some(scorer_lock), Some(tracker_lock)) = (&self.tag_scorer, &self.relevance_tracker) {
            let scorer = scorer_lock.read().unwrap();
            let tracker = tracker_lock.read().unwrap();
            let path = config.taxonomy_dir().join("relevance.json");
            tracker.save(&path, scorer.vocabulary())?;
        }
        Ok(())
    }
}
```

### Task 9: CLI Integration

**File:** `crates/photon/src/cli/process.rs`

After batch processing completes, save relevance data:

```rust
// After the processing loop:
if let Err(e) = processor.save_relevance(&config) {
    tracing::warn!("Failed to save relevance data: {e}");
}
```

### Task 10: Neighbor Expansion Integration

Wire neighbor expansion into the sweep cycle:

```rust
// In process_with_options(), after scoring:
// Periodically run sweep + neighbor expansion
if self.relevance_tracker.is_some() && /* every 1000 images */ {
    let promoted = tracker.sweep();
    if !promoted.is_empty() && config.tagging.relevance.neighbor_expansion {
        let siblings = NeighborExpander::expand_all(scorer.vocabulary(), &promoted);
        let cold_siblings: Vec<usize> = siblings
            .iter()
            .filter(|&&i| tracker.pool(i) == Pool::Cold)
            .copied()
            .collect();
        tracker.promote_to_warm(&cold_siblings);
        tracing::debug!(
            "Neighbor expansion: {} terms promoted, {} siblings queued",
            promoted.len(), cold_siblings.len()
        );
    }
}
```

**Note:** Promoting cold terms to warm means they need to be encoded. If using progressive encoding (4b), the background encoder can pick up newly-warm terms. Without 4b, warm terms that aren't encoded are simply scored when the label bank is next rebuilt (vocabulary change). This is an acceptable limitation — 4b + 4c+4d work best together.

---

## Test Plan

### Unit Tests

| Test | File | What it verifies |
|------|------|-----------------|
| `test_avg_confidence_zero_hits` | `relevance.rs` | Returns 0.0 for zero hits |
| `test_avg_confidence_calculation` | `relevance.rs` | Correct average computation |
| `test_pool_serde_roundtrip` | `relevance.rs` | Pool enum serializes/deserializes |
| `test_new_encoded_terms_active` | `relevance.rs` | Encoded terms start Active |
| `test_new_unencoded_terms_cold` | `relevance.rs` | Unencoded terms start Cold |
| `test_record_hits_updates_stats` | `relevance.rs` | Stats updated after recording |
| `test_sweep_demotes_stale_active` | `relevance.rs` | Old active terms demoted |
| `test_sweep_promotes_warm_with_hits` | `relevance.rs` | High-scoring warm terms promoted |
| `test_sweep_returns_promoted_indices` | `relevance.rs` | Promoted indices returned |
| `test_save_load_roundtrip` | `relevance.rs` | Persistence works |
| `test_load_with_vocabulary_change` | `relevance.rs` | New terms get cold stats |
| `test_find_siblings_shared_parent` | `neighbors.rs` | Siblings found via hypernym |
| `test_find_siblings_excludes_self` | `neighbors.rs` | Self excluded |
| `test_expand_all_deduplicates` | `neighbors.rs` | No duplicate siblings |
| `test_score_pool_filters_by_pool` | `scorer.rs` | Only target pool scored |
| `test_parent_of_wordnet_term` | `vocabulary.rs` | First hypernym returned |
| `test_build_parent_index` | `vocabulary.rs` | Parent→children index correct |

### Integration Tests

| Test | What it verifies |
|------|-----------------|
| `test_relevance_disabled_matches_4a` | With relevance disabled, output identical to Phase 4a |
| `test_pool_convergence` | Process 100 images, verify active pool shrinks from 68K to ~5-15K |

---

## Acceptance Criteria

1. **Pool convergence** — after processing ~500 images, active pool is measurably smaller than full vocabulary
2. **No quality regression** — tags for individual images are at least as good as Phase 4a (active pool contains all relevant terms)
3. **Relevance persists** — pool assignments survive across runs via `relevance.json`
4. **Neighbor expansion fires** — promoting "labrador" to active triggers sibling encoding
5. **Backward compatible** — `relevance.enabled = false` gives identical behavior to 4a/4b
6. **Warm check overhead < 1%** — warm pool scoring every 100 images adds negligible latency
7. **All existing tests pass** — no regressions

---

## Configuration (TOML)

```toml
[tagging.relevance]
enabled = false              # Opt-in (default off until stable)
warm_check_interval = 100    # Score warm pool every N images
promotion_threshold = 0.3    # Min confidence for warm → active
active_demotion_days = 90    # Demote active terms with no hits in N days
warm_demotion_checks = 50    # Demote warm terms after N checks with no hits
neighbor_expansion = true    # Auto-expand WordNet siblings of promoted terms
```

---

## Estimated Scope

| Component | Lines of Code (est.) | Complexity |
|-----------|---------------------|------------|
| `relevance.rs` | ~250 | Medium |
| `neighbors.rs` | ~60 | Low |
| `scorer.rs` additions | ~60 | Low |
| `vocabulary.rs` additions | ~30 | Low |
| `processor.rs` changes | ~50 | Medium |
| `config.rs` additions | ~30 | Low |
| `process.rs` CLI changes | ~10 | Low |
| Tests | ~300 | Medium |
| **Total** | **~790** | **Medium** |
