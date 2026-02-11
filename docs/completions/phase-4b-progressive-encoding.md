# Phase 4b: Progressive Encoding — Completion

> **Status:** Complete
> **Depends on:** Phase 4a (zero-shot tagging — complete)
> **Result:** First-run cold-start reduced from ~90 minutes blocking to ~30 seconds before first image can be processed. Background encoding continues in chunks, progressively improving tag quality.

---

## Summary

Implemented progressive vocabulary encoding to eliminate the ~90-minute first-run blocking encode. On first run, the system now:

1. Encodes a seed set of ~2K high-value terms synchronously (~30s)
2. Starts processing images immediately with the seed vocabulary
3. Background-encodes remaining ~66K terms in 5K-term chunks
4. Progressively swaps in larger scorers via `RwLock` as chunks complete
5. Saves the complete `label_bank.bin` cache when all terms are encoded

Subsequent runs load the cache instantly (unchanged from Phase 4a).

---

## What Was Built

### New Files

| File | Lines | Purpose |
|------|-------|---------|
| `crates/photon-core/src/tagging/seed.rs` | ~200 | `SeedSelector` — deterministic seed term selection (supplemental > curated file > random fill) |
| `crates/photon-core/src/tagging/progressive.rs` | ~180 | `ProgressiveEncoder` — background encoding orchestration with chunked scorer swaps |
| `data/vocabulary/seed_terms.txt` | ~1170 | 1041 curated visual nouns cross-referenced against `wordnet_nouns.txt` |

### Modified Files

| File | Changes |
|------|---------|
| `crates/photon-core/src/config.rs` | Added `ProgressiveConfig` struct (enabled, seed_size, chunk_size) as field on `TaggingConfig` |
| `crates/photon-core/src/tagging/mod.rs` | Added `pub mod seed;` and `pub mod progressive;` |
| `crates/photon-core/src/tagging/vocabulary.rs` | Added `Vocabulary::empty()`, `Vocabulary::subset(indices)`, and unit tests |
| `crates/photon-core/src/tagging/label_bank.rs` | Added `LabelBank::empty()`, `LabelBank::append()`, `derive(Clone)`, and unit tests |
| `crates/photon-core/src/tagging/scorer.rs` | Added `label_bank()` and `vocabulary()` accessor methods |
| `crates/photon-core/src/pipeline/processor.rs` | Changed `tag_scorer` from `Option<Arc<TagScorer>>` to `Option<Arc<RwLock<TagScorer>>>`, rewrote `load_tagging()` with three paths (cached/progressive/blocking), added `load_tagging_blocking()` fallback |
| `Cargo.toml` (workspace) | Added `rand = "0.8"` to workspace dependencies |
| `crates/photon-core/Cargo.toml` | Added `rand.workspace = true` and `tempfile = "3"` (dev-dependency) |

---

## Key Design Decisions

### 1. `RwLock` over `ArcSwap`
Standard library `RwLock` was chosen over the `arc-swap` crate. The read lock overhead is negligible for ~2ms scoring operations, and write contention is near-zero (at most ~14 swaps over 90 minutes). No new dependency needed.

### 2. Append-Only Label Bank Growth
The background encoder maintains a running `LabelBank` and _appends_ each chunk's embeddings rather than re-encoding everything. This keeps total work at O(N), not O(N × num_swaps). The `LabelBank::append()` method was added for this purpose.

### 3. Deterministic Seed Selection
`SeedSelector` uses the vocabulary's BLAKE3 content hash as the RNG seed, ensuring the same vocabulary always produces the same seed set. This makes first-run behavior reproducible.

### 4. Three-Path `load_tagging()`
The processor now has three code paths:
- **Fast path:** Cached `label_bank.bin` exists and is valid → load instantly (unchanged)
- **Progressive path:** No cache, `progressive.enabled = true`, tokio runtime available → seed + background encode
- **Blocking path:** No cache, `progressive.enabled = false` OR no tokio runtime → encode-all synchronously (legacy)

### 5. Tokio Runtime Guard
`ProgressiveEncoder::start()` calls `tokio::spawn()`, requiring an active tokio runtime. Library consumers outside tokio get a graceful fallback to blocking mode rather than a panic.

---

## Seed Terms Composition

The 1041-term seed file covers:

| Category | Count | Examples |
|----------|-------|---------|
| Animals | ~120 | dog, cat, elephant, eagle, dolphin, butterfly |
| Vehicles | ~55 | car, truck, bicycle, airplane, boat, helicopter |
| Nature | ~95 | tree, flower, mountain, river, ocean, cloud, volcano |
| Food | ~85 | pizza, cake, apple, bread, cheese, coffee, wine |
| People/Body | ~65 | person, child, face, hand, skull, skeleton |
| Buildings/Places | ~75 | house, church, castle, bridge, skyscraper, lighthouse |
| Furniture/Objects | ~140 | chair, table, lamp, clock, bottle, umbrella, mirror |
| Clothing | ~45 | shirt, dress, hat, shoe, jacket, sunglasses |
| Activities/Sports | ~70 | soccer, tennis, skiing, swimming, wrestling, archery |
| Technology | ~45 | computer, camera, television, robot, satellite |
| Music/Art | ~40 | guitar, piano, painting, sculpture, violin, drum |
| Weather/Sky | ~20 | sun, moon, star, rainbow, lightning, tornado |
| Miscellaneous | ~186 | fire, sword, book, coin, toy, anchor, propeller |

All terms were verified to exist in `wordnet_nouns.txt` via exact match.

---

## Test Results

**68 tests passing** (up from 50 pre-Phase 4b), zero clippy warnings.

### New Tests (18)

| Test | File | Verifies |
|------|------|----------|
| `test_empty_vocabulary` | vocabulary.rs | `Vocabulary::empty()` produces valid empty state |
| `test_subset_preserves_terms` | vocabulary.rs | Subset has correct terms in given order |
| `test_subset_empty` | vocabulary.rs | Empty index list → empty vocabulary |
| `test_subset_rebuilds_index` | vocabulary.rs | `by_name` lookup works correctly in subsets |
| `test_subset_preserves_hypernyms` | vocabulary.rs | Hypernym chains survive subsetting |
| `test_empty_label_bank` | label_bank.rs | `LabelBank::empty()` has 0 terms, 768 dim |
| `test_append_grows_matrix` | label_bank.rs | 3-term + 5-term → 8-term bank |
| `test_append_preserves_existing` | label_bank.rs | First N rows unchanged after append |
| `test_append_empty_to_empty` | label_bank.rs | Empty + empty → empty |
| `test_append_to_empty` | label_bank.rs | Empty + 3-term → 3-term |
| `test_append_dimension_mismatch` | label_bank.rs | 768 + 512 panics (assert) |
| `test_select_includes_supplemental` | seed.rs | All supplemental indices present |
| `test_select_respects_target_size` | seed.rs | Result ≤ target_size |
| `test_select_deterministic` | seed.rs | Same vocab → same indices |
| `test_select_without_seed_file` | seed.rs | Missing file handled gracefully |
| `test_select_with_seed_file` | seed.rs | File terms matched, non-existent skipped |
| `test_progressive_config_defaults` | config.rs | enabled=true, seed_size=2000, chunk_size=5000 |
| `test_tagging_config_includes_progressive` | config.rs | Progressive config nested in tagging config |

---

## Dependencies Added

| Crate | Version | Purpose |
|-------|---------|---------|
| `rand` | 0.8 | Seeded deterministic random sampling for seed term selection |
| `tempfile` | 3 (dev) | Test helpers for creating temporary vocabulary files |

---

## Configuration

```toml
[tagging.progressive]
enabled = true      # Enable progressive encoding (default)
seed_size = 2000    # Seed terms encoded synchronously
chunk_size = 5000   # Terms per background chunk
```

Set `enabled = false` to revert to Phase 4a blocking behavior.

---

## Backward Compatibility

- Without `label_bank.bin` cache: progressive encoding kicks in (new behavior)
- With `label_bank.bin` cache: loads instantly (unchanged from Phase 4a)
- `progressive.enabled = false`: exact legacy blocking behavior
- No changes to `ProcessedImage` output format
- No changes to CLI flags
- All 50 pre-existing tests continue to pass
