# Phase 4c+4d: Relevance Pruning & Neighbor Expansion — Completion

> **Status:** Complete
> **Depends on:** Phase 4b (progressive encoding — complete)
> **Result:** Self-organizing vocabulary with three-pool system (Active/Warm/Cold). Irrelevant terms are demoted to reduce scoring cost; WordNet neighbors of active terms are promoted for deeper coverage. Backward compatible — `relevance.enabled = false` (the default) gives identical behavior to Phase 4a/4b.

---

## Summary

Implemented a three-pool vocabulary system where terms self-organize based on scoring history:

1. **Active pool** — scored every image (the fast path)
2. **Warm pool** — scored every Nth image (sampled for promotion)
3. **Cold pool** — not scored (promoted externally via neighbor expansion)

Per-term statistics (hit count, average confidence, last match timestamp) drive pool transitions. When a term is promoted to Active, its WordNet siblings are promoted to Warm for evaluation — creating a feedback loop that deepens coverage in areas the user actually photographs.

Relevance data persists across runs via `~/.photon/taxonomy/relevance.json`, aligned by term name so vocabulary changes are handled gracefully.

100 tests passing (+32 new), zero clippy warnings.

---

## What Was Built

### New Files

| File | Lines | Purpose |
|------|-------|---------|
| `crates/photon-core/src/tagging/relevance.rs` | ~290 | `Pool` enum, `TermStats`, `RelevanceTracker`, `RelevanceConfig`, persistence (save/load) |
| `crates/photon-core/src/tagging/neighbors.rs` | ~55 | `NeighborExpander` — WordNet sibling lookup via shared first hypernym |

### Modified Files

| File | Changes |
|------|---------|
| `crates/photon-core/src/config.rs` | Added `RelevanceConfig` import and `relevance: RelevanceConfig` field on `TaggingConfig`, plus config test |
| `crates/photon-core/src/tagging/mod.rs` | Added `pub mod relevance;` and `pub mod neighbors;`, re-exports `Pool`, `RelevanceConfig`, `RelevanceTracker` |
| `crates/photon-core/src/tagging/scorer.rs` | Extracted `hits_to_tags()` shared helper; added `score_pool()` (single-pool scoring) and `score_with_pools()` (pool-aware scoring that returns raw hits for separate recording) |
| `crates/photon-core/src/tagging/vocabulary.rs` | Added `parent_of(term_index)` and `build_parent_index()` methods, plus tests |
| `crates/photon-core/src/tagging/label_bank.rs` | Added `from_raw()` constructor for test/external use |
| `crates/photon-core/src/pipeline/processor.rs` | Added `relevance_tracker: Option<RwLock<RelevanceTracker>>`, `sweep_interval`, `neighbor_expansion` fields; added `load_relevance_tracker()` and `save_relevance()` methods; rewrote tagging stage in `process_with_options()` with split read-lock scoring / write-lock recording pattern, periodic sweep + neighbor expansion |
| `crates/photon/src/cli/process.rs` | Added `save_relevance()` calls after both single-file and batch processing |

---

## Key Design Decisions

### 1. Split Read-Lock Scoring from Write-Lock Recording

The pool-aware scoring path uses a two-phase locking pattern:

```
Phase 1: READ lock on scorer + tracker → score_with_pools() → ~2ms
Phase 2: WRITE lock on tracker → record_hits() → ~μs
```

Scoring (the expensive operation) runs under a read lock, allowing concurrent access during parallel batch processing. Only the brief `record_hits()` call needs a write lock. This prevents serialization — the design from the plan doc was followed exactly.

### 2. Relevance Only on Cached Label Bank

The relevance tracker is only loaded on the **fast path** (cached `label_bank.bin` exists). During progressive encoding (first run), pool assignments don't make sense because terms are being encoded incrementally. Once the full label bank is cached, subsequent runs enable relevance tracking.

### 3. Sweep + Neighbor Expansion Inside Write Lock

Pool sweeps (every 1000 images) and neighbor expansion run inside the same write lock that holds `record_hits()`. This avoids a second lock acquisition and keeps the critical section brief. The scorer read lock acquired for `NeighborExpander::expand_all()` inside the tracker write lock is safe because:
- `scorer_lock` and `tracker_lock` are different `RwLock` instances
- We only take a **read** lock on scorer (never write) while holding tracker write

### 4. Persistence by Term Name

Relevance data is serialized/deserialized by **term name**, not by index position. This handles vocabulary changes between runs gracefully — new terms start Cold, removed terms are dropped, existing terms retain their pool assignments and statistics.

### 5. Off by Default

`RelevanceConfig.enabled` defaults to `false`. Users opt in via config:

```toml
[tagging.relevance]
enabled = true
warm_check_interval = 100
promotion_threshold = 0.3
active_demotion_days = 90
neighbor_expansion = true
```

When disabled, the tagging path falls through to the existing `scorer.score()` — identical to Phase 4a/4b behavior.

### 6. `hits_to_tags()` Refactor

The filter → sort → truncate logic was extracted from `score()` into a shared `hits_to_tags()` helper, used by both `score()` and `score_with_pools()`. This prevents logic divergence between the two scoring paths.

---

## Data Structures

### Pool Enum

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Pool { Active, Warm, Cold }
```

### TermStats

```rust
pub struct TermStats {
    pub hit_count: u32,     // Times scored above min_confidence
    pub score_sum: f32,     // Running sum for avg computation
    pub last_hit_ts: u64,   // Unix timestamp of last match
    pub pool: Pool,         // Current pool assignment
}
```

Memory: 17 bytes per term × 68K terms = ~1.1MB. Negligible.

### Pool Transition Rules

| Transition | Condition |
|------------|-----------|
| Active → Warm | No hits in `active_demotion_days` days, OR never hit after 1000 images |
| Warm → Active | `avg_confidence >= promotion_threshold` |
| Cold → Warm | Neighbor expansion (sibling of newly Active term) |

---

## Test Coverage

### New Tests (32)

**relevance.rs (20):**
- `test_avg_confidence_zero_hits`, `test_avg_confidence_calculation`
- `test_pool_serde_roundtrip`
- `test_new_encoded_terms_active`, `test_new_unencoded_terms_cold`
- `test_record_hits_updates_stats`, `test_record_hits_increments_image_count`
- `test_sweep_demotes_stale_active`, `test_sweep_demotes_never_hit_active`
- `test_sweep_promotes_warm_with_hits`, `test_sweep_returns_promoted_indices`
- `test_sweep_preserves_recent_active`, `test_should_check_warm_interval`
- `test_pool_counts`, `test_promote_to_warm`
- `test_save_load_roundtrip`, `test_load_with_vocabulary_change`, `test_load_missing_file_error`
- `test_relevance_config_defaults`

**neighbors.rs (6):**
- `test_find_siblings_shared_parent`, `test_find_siblings_excludes_self`
- `test_find_siblings_no_hypernyms`, `test_find_siblings_different_parent`
- `test_expand_all_deduplicates`, `test_expand_all_excludes_promoted`

**scorer.rs (3):**
- `test_hits_to_tags_filters_sorts_truncates`
- `test_score_pool_filters_by_pool`, `test_score_with_pools_returns_hits`

**vocabulary.rs (3):**
- `test_parent_of_wordnet_term`, `test_parent_of_supplemental_term`
- `test_build_parent_index`

**config.rs (1):**
- `test_tagging_config_includes_relevance` (renamed from plan's name for consistency)

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

## Deviations from Plan

1. **`warm_demotion_checks` not used in sweep**: The plan specified demoting warm terms after N consecutive checks with no hits, but the current `sweep()` implementation uses the same time-based approach for warm demotion as for active. This can be refined in a follow-up if needed — the config field is present and wired.

2. **`build_parent_index()` added but not used in hot path**: The `NeighborExpander` does a linear scan of the vocabulary rather than using the pre-built parent index. For 68K terms this is microseconds. The index method is available for future optimization if profiling shows it matters.

3. **No `score_warm()` separate method**: Instead of a dedicated `score_warm()`, the generic `score_pool()` method takes a `Pool` argument. This is more flexible and avoids code duplication.
