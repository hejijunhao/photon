# Phase 4b: Progressive Encoding

> **Status:** Planned
> **Depends on:** Phase 4a (zero-shot tagging — complete)
> **Goal:** Reduce first-run cold-start from ~90 minutes to ~30 seconds by encoding a seed vocabulary immediately, then background-encoding the remaining ~66K terms while images are already being processed.

---

## Problem

`LabelBank::encode_all()` currently encodes all ~68K vocabulary terms in a single blocking call during `load_tagging()`. On CPU this takes ~90 minutes. Users must wait for the full encode to complete before processing their first image. On subsequent runs the cached `label_bank.bin` loads instantly, so **this optimization targets the first-run experience only**.

## Solution

Encode a seed set of ~2K high-value terms synchronously (~30 seconds), start processing immediately, then encode remaining terms in background chunks — swapping in progressively larger scorers as they become available.

```
FIRST RUN TIMELINE
══════════════════

  t=0        t=~30s                                          t=~90min
  ┌──────────┬──────────────────────────────────────────────────┐
  │ Seed 2K  │ Background encoding in 5K-term chunks           │
  │ (sync)   │                                                  │
  └──────────┴──────────────────────────────────────────────────┘
              │                                                  │
              ▼                                                  ▼
        Start processing                               Save full cache
        with 2K terms                                   (label_bank.bin)
              │         │          │         │
              ▼         ▼          ▼         ▼
           Score     Score      Score     Score
           2K       7K terms   12K       68K terms
           terms    (swap 1)   (swap 2)  (final)
```

On subsequent runs: load `label_bank.bin` from cache as today (no change).

---

## Design Decisions

### 1. Seed Selection Strategy

The seed set determines tagging quality during the first ~90 minutes. Composition:

| Source | Count | Rationale |
|--------|-------|-----------|
| All supplemental terms | ~260 | Scenes, moods, styles, weather — high visual relevance, hand-curated |
| Top WordNet visual nouns | ~1,000 | Curated list of common visual objects (dog, car, tree, person, building, etc.) |
| Random WordNet sample | ~740 | Diversity — covers unexpected niches the curated set misses |
| **Total** | **~2,000** | Encodes in ~30 seconds on CPU |

The curated "top visual nouns" list is a new static file: `data/vocabulary/seed_terms.txt` (~1K lines). This is a one-time authoring effort — just the most common visual nouns a photographer or marketer would encounter.

**Decision: Why not fully random?** A random 2K sample from 68K WordNet nouns would include many non-visual terms (e.g., "absolution", "jurisprudence", "synecdoche") that waste encoding time and never match images.

### 2. Scorer Swapping via `RwLock`

The `ImageProcessor` currently holds `tag_scorer: Option<Arc<TagScorer>>`. This changes to:

```rust
tag_scorer: Option<Arc<RwLock<TagScorer>>>
```

- **Read path (scoring):** `scorer.read().unwrap().score(&embedding)` — concurrent, no blocking between images
- **Write path (swap):** Background task builds a complete new `TagScorer` outside the lock, then takes a brief write lock to swap it in
- **Lock hold time for writes:** Nanoseconds (just moves three fields: vocabulary, label_bank, config)

**Decision: Why `RwLock` over `ArcSwap`?** `RwLock` is stdlib — no new dependency. Scoring is sync and fast (~2ms), so the read lock overhead is negligible. Write contention is near-zero (swaps happen at most ~14 times over 90 minutes).

### 3. Background Encoding Chunks

The background encoder processes remaining terms in chunks of **5,000 terms**. After each chunk:

1. Build a new `Vocabulary` + `LabelBank` combining all encoded terms so far
2. Create a new `TagScorer`
3. Swap it into the `RwLock`

This gives ~14 swaps total (66K remaining / 5K per chunk). Each swap improves coverage.

**Decision: Why not encode all 66K in one background pass then swap once?** Because users processing a large batch during the background encode would get only 2K terms for all their images. With chunked swaps, later images in the batch benefit from progressively richer vocabulary.

### 4. Cache Format (Unchanged)

The final `label_bank.bin` format is identical to today's: flat N×768 f32 binary + `.meta` sidecar. The progressive encoding system writes the cache only once — when all terms are encoded. Partial caches are not persisted (simplicity over complexity).

---

## New Types

### `SeedSelector` (new: `tagging/seed.rs`)

```rust
/// Selects a high-value seed vocabulary for fast first-run startup.
pub struct SeedSelector;

impl SeedSelector {
    /// Select seed terms from the full vocabulary.
    ///
    /// Priority: supplemental terms > seed_terms.txt matches > random sample.
    /// Returns indices into the full vocabulary's term list.
    pub fn select(
        vocabulary: &Vocabulary,
        seed_path: &Path,     // path to seed_terms.txt
        target_size: usize,   // default: 2000
    ) -> Vec<usize>;
}
```

### `ProgressiveEncoder` (new: `tagging/progressive.rs`)

```rust
/// Orchestrates background vocabulary encoding with progressive scorer updates.
pub struct ProgressiveEncoder;

impl ProgressiveEncoder {
    /// Encode seed terms synchronously and return an initial TagScorer.
    ///
    /// Also spawns a background tokio task that encodes remaining terms
    /// and progressively swaps in larger scorers via the provided RwLock.
    pub fn start(
        vocabulary: Vocabulary,
        text_encoder: Arc<SigLipTextEncoder>,
        config: TaggingConfig,
        scorer_slot: Arc<RwLock<TagScorer>>,
        seed_indices: Vec<usize>,
        cache_path: PathBuf,
        vocab_hash: String,
        chunk_size: usize,       // default: 5000
    ) -> TagScorer;
    // Returns the seed scorer (caller installs it before spawning background)
}
```

### `ProgressiveConfig` (addition to `TaggingConfig`)

```rust
/// Progressive encoding settings (new sub-section of TaggingConfig).
pub struct ProgressiveConfig {
    /// Number of seed terms to encode synchronously on first run.
    /// Default: 2000
    pub seed_size: usize,

    /// Number of terms per background encoding chunk.
    /// Default: 5000
    pub chunk_size: usize,

    /// Enable progressive encoding (if false, falls back to encode-all-blocking).
    /// Default: true
    pub enabled: bool,
}
```

---

## File-by-File Changes

### New Files

| File | Purpose |
|------|---------|
| `crates/photon-core/src/tagging/seed.rs` | `SeedSelector` — seed term selection logic |
| `crates/photon-core/src/tagging/progressive.rs` | `ProgressiveEncoder` — background encoding orchestration |
| `data/vocabulary/seed_terms.txt` | Curated ~1K common visual nouns for seed selection |

### Modified Files

| File | Changes |
|------|---------|
| `crates/photon-core/src/tagging/mod.rs` | Add `pub mod seed;` and `pub mod progressive;` |
| `crates/photon-core/src/tagging/vocabulary.rs` | Add `Vocabulary::subset(indices)` and `Vocabulary::merge(other)` methods |
| `crates/photon-core/src/tagging/label_bank.rs` | Add `LabelBank::append()`, `LabelBank::empty()`, derive `Clone` |
| `crates/photon-core/src/pipeline/processor.rs` | Change `tag_scorer` from `Option<Arc<TagScorer>>` to `Option<Arc<RwLock<TagScorer>>>`, update `load_tagging()` to use progressive path on cache miss |
| `crates/photon-core/src/config.rs` | Add `ProgressiveConfig` to `TaggingConfig` |
| `crates/photon-core/src/tagging/scorer.rs` | No structural changes — `TagScorer::new()` and `score()` work as-is |

---

## Task Breakdown

### Task 1: Vocabulary Subsetting

**File:** `vocabulary.rs`

Add one method to `Vocabulary`:

```rust
/// Create a sub-vocabulary containing only the terms at the given indices.
/// Preserves term order as given in `indices`.
pub fn subset(&self, indices: &[usize]) -> Self
```

`subset()` is used to create the seed vocabulary and to build progressively larger vocabularies from accumulated index lists. No `merge()` method is needed — the progressive encoder tracks encoded indices and calls `full_vocabulary.subset(&all_encoded_indices)` each time.

**Tests:**
- `test_subset_preserves_terms` — subset of 3 terms from a 10-term vocab, verify names/hypernyms
- `test_subset_empty` — empty indices → empty vocabulary
- `test_subset_rebuilds_index` — `by_name` index is correct for the subset

### Task 2: Label Bank Subset Encoding

**File:** `label_bank.rs`

The existing `encode_all()` already encodes whatever vocabulary you give it — just pass a subset vocabulary. No new encoding method needed.

Add an `append()` method for growing the label bank incrementally (used by the progressive encoder to avoid re-encoding previously encoded terms):

```rust
/// Append another label bank's matrix to this one.
/// The caller must ensure vocabulary ordering matches.
pub fn append(&mut self, other: &LabelBank)
```

**Tests:**
- `test_append_grows_matrix` — appending 5-term bank to 3-term bank → 8-term bank
- `test_append_preserves_existing` — first N rows unchanged after append

### Task 3: Seed Term Selection

**File:** `tagging/seed.rs` (new)

```rust
impl SeedSelector {
    pub fn select(
        vocabulary: &Vocabulary,
        seed_path: &Path,
        target_size: usize,
    ) -> Vec<usize> {
        let mut selected = HashSet::new();

        // 1. Include ALL supplemental terms (they have category != None)
        for (i, term) in vocabulary.all_terms().iter().enumerate() {
            if term.category.is_some() {
                selected.insert(i);
            }
        }

        // 2. Include seed_terms.txt matches (if file exists)
        if let Ok(content) = std::fs::read_to_string(seed_path) {
            let seed_names: HashSet<&str> = content
                .lines()
                .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
                .collect();
            for (i, term) in vocabulary.all_terms().iter().enumerate() {
                if seed_names.contains(term.name.as_str()) {
                    selected.insert(i);
                }
            }
        }

        // 3. Fill remainder with random sample from unselected terms
        let remaining = target_size.saturating_sub(selected.len());
        if remaining > 0 {
            let unselected: Vec<usize> = (0..vocabulary.len())
                .filter(|i| !selected.contains(i))
                .collect();
            // Deterministic shuffle using content hash as seed
            // (so same vocabulary always produces same seed set)
            let mut rng = /* seeded from vocab hash */;
            for &idx in unselected.choose_multiple(&mut rng, remaining) {
                selected.insert(idx);
            }
        }

        let mut indices: Vec<usize> = selected.into_iter().collect();
        indices.sort(); // Maintain vocabulary order
        indices
    }
}
```

**Dependency:** `rand` crate (for seeded random sampling). Check if already in workspace — if not, add it.

**Tests:**
- `test_select_includes_supplemental` — all supplemental indices present in result
- `test_select_respects_target_size` — result.len() ≤ target_size
- `test_select_deterministic` — same vocabulary + seed file → same indices every time
- `test_select_without_seed_file` — gracefully handles missing seed_terms.txt

### Task 4: Curate Seed Terms File

**File:** `data/vocabulary/seed_terms.txt` (new)

Curate ~1,000 common visual nouns from the WordNet vocabulary. Categories to cover:

- **Animals** (~100): dog, cat, bird, horse, fish, elephant, lion, bear, etc.
- **Vehicles** (~50): car, truck, bicycle, airplane, boat, motorcycle, bus, train, etc.
- **Nature** (~80): tree, flower, mountain, river, ocean, lake, forest, cloud, etc.
- **Food** (~80): pizza, cake, fruit, bread, meat, vegetable, ice cream, salad, etc.
- **People/Body** (~60): person, face, hand, child, woman, man, baby, crowd, etc.
- **Buildings/Places** (~70): house, church, bridge, tower, castle, school, hospital, etc.
- **Furniture/Objects** (~100): chair, table, bed, lamp, clock, bottle, cup, phone, etc.
- **Clothing** (~40): shirt, dress, hat, shoe, jacket, tie, sunglasses, etc.
- **Activities/Sports** (~60): soccer, tennis, surfing, skiing, swimming, cooking, etc.
- **Technology** (~40): computer, camera, television, robot, keyboard, laptop, etc.
- **Music/Art** (~30): guitar, piano, painting, sculpture, violin, drum, etc.
- **Weather/Sky** (~20): sun, rain, snow, rainbow, lightning, etc.
- **Miscellaneous** (~170): remaining common objects, materials, textures

Format: one term per line, matching the `name` field in `wordnet_nouns.txt` (with underscores).

```
# Seed terms for progressive encoding
# ~1000 common visual nouns from WordNet
# Used to bootstrap the tagging vocabulary on first run
dog
cat
bird
car
...
```

Cross-reference against `wordnet_nouns.txt` to ensure every term in the seed file actually exists in the vocabulary. Terms that don't match are silently skipped.

### Task 5: Progressive Encoder Orchestration

**File:** `tagging/progressive.rs` (new)

This is the core orchestration logic:

```rust
impl ProgressiveEncoder {
    pub fn start(
        full_vocabulary: Vocabulary,
        text_encoder: Arc<SigLipTextEncoder>,
        config: TaggingConfig,
        scorer_slot: Arc<RwLock<TagScorer>>,
        seed_indices: Vec<usize>,
        cache_path: PathBuf,
        vocab_hash: String,
        chunk_size: usize,
    ) -> TagScorer {
        // 1. Create seed vocabulary + label bank (SYNCHRONOUS)
        let seed_vocab = full_vocabulary.subset(&seed_indices);
        let seed_bank = LabelBank::encode_all(&seed_vocab, &text_encoder, 64)
            .expect("Seed encoding failed");

        tracing::info!(
            "Seed vocabulary ready: {} terms encoded in seed set",
            seed_indices.len()
        );

        let seed_scorer = TagScorer::new(
            seed_vocab.clone(),
            seed_bank,  // We need to clone or rebuild for background
            config.clone(),
        );

        // 2. Determine remaining terms to encode
        let all_indices: HashSet<usize> = (0..full_vocabulary.len()).collect();
        let seed_set: HashSet<usize> = seed_indices.iter().copied().collect();
        let remaining: Vec<usize> = all_indices
            .difference(&seed_set)
            .copied()
            .sorted()
            .collect();

        if remaining.is_empty() {
            // All terms were in the seed — save cache and return
            // (unlikely but handle gracefully)
            return seed_scorer;
        }

        // 3. Spawn background encoding task (ASYNC)
        let bg_config = config.clone();
        let bg_scorer_slot = Arc::clone(&scorer_slot);

        tokio::spawn(async move {
            Self::background_encode(
                full_vocabulary,
                text_encoder,
                bg_config,
                bg_scorer_slot,
                seed_indices,
                remaining,
                cache_path,
                vocab_hash,
                chunk_size,
            ).await;
        });

        seed_scorer
    }

    async fn background_encode(
        full_vocabulary: Vocabulary,
        text_encoder: Arc<SigLipTextEncoder>,
        config: TaggingConfig,
        scorer_slot: Arc<RwLock<TagScorer>>,
        seed_indices: Vec<usize>,
        remaining_indices: Vec<usize>,
        cache_path: PathBuf,
        vocab_hash: String,
        chunk_size: usize,
    ) {
        // Accumulate all encoded indices + the running label bank matrix.
        // We APPEND new embeddings rather than re-encoding everything —
        // this keeps total encoding work at O(N) not O(N * num_swaps).
        let mut encoded_indices = seed_indices;
        let mut running_bank = {
            // Clone the seed label bank from the current scorer
            let scorer = scorer_slot.read().unwrap();
            scorer.label_bank().clone()
        };

        for chunk in remaining_indices.chunks(chunk_size) {
            // Encode ONLY this chunk's terms in a blocking task
            let chunk_indices: Vec<usize> = chunk.to_vec();
            let chunk_vocab = full_vocabulary.subset(&chunk_indices);
            let encoder = Arc::clone(&text_encoder);

            let chunk_bank = tokio::task::spawn_blocking(move || {
                LabelBank::encode_all(&chunk_vocab, &encoder, 64)
            })
            .await;

            let chunk_bank = match chunk_bank {
                Ok(Ok(bank)) => bank,
                Ok(Err(e)) => {
                    tracing::error!("Background encoding chunk failed: {e}");
                    continue; // Skip this chunk, try next
                }
                Err(e) => {
                    tracing::error!("Background encoding task panicked: {e}");
                    continue;
                }
            };

            // Append the new chunk's embeddings to the running bank
            running_bank.append(&chunk_bank);
            encoded_indices.extend_from_slice(&chunk_indices);

            // Build a new scorer from the accumulated data
            let combined_vocab = full_vocabulary.subset(&encoded_indices);
            let new_scorer = TagScorer::new(
                combined_vocab,
                running_bank.clone(),
                config.clone(),
            );

            // Atomic swap — write lock held only for the duration of a pointer swap
            {
                let mut lock = scorer_slot.write().unwrap();
                *lock = new_scorer;
            }

            tracing::info!(
                "Progressive encoding: {}/{} terms encoded",
                encoded_indices.len(),
                full_vocabulary.len()
            );
        }

        // All terms encoded — save complete cache to disk
        if let Err(e) = running_bank.save(&cache_path, &vocab_hash) {
            tracing::error!("Failed to save complete label bank cache: {e}");
        } else {
            tracing::info!(
                "Progressive encoding complete. Full vocabulary ({} terms) cached to {:?}",
                full_vocabulary.len(),
                cache_path
            );
        }
    }
}
```

**Note:** The `LabelBank` needs to implement `Clone` for the running bank pattern above. This is a simple `derive(Clone)` since the struct only contains `Vec<f32>`, `usize`, `usize`.

**Tests:**
- `test_progressive_seed_scorer_created` — verify seed scorer has correct term count
- `test_progressive_remaining_calculated` — verify remaining indices exclude seed
- Integration test (requires text encoder, may be `#[ignore]`):
  - `test_progressive_full_flow` — with a tiny vocabulary (50 terms, seed 10, chunk 10), verify all terms eventually encoded

### Task 6: Wire Into `ImageProcessor`

**File:** `pipeline/processor.rs`

Changes to `ImageProcessor`:

```rust
pub struct ImageProcessor {
    // ... existing fields ...
    // CHANGE: Arc<TagScorer> → Arc<RwLock<TagScorer>>
    tag_scorer: Option<Arc<RwLock<TagScorer>>>,
}
```

Update `load_tagging()`:

```rust
pub fn load_tagging(&mut self, config: &Config) -> Result<()> {
    let vocab_dir = config.vocabulary_dir();
    let taxonomy_dir = config.taxonomy_dir();
    let model_dir = config.model_dir();

    let vocabulary = Vocabulary::load(&vocab_dir)?;
    if vocabulary.is_empty() { /* unchanged */ }

    let label_bank_path = taxonomy_dir.join("label_bank.bin");
    let vocab_hash = vocabulary.content_hash();

    if LabelBank::exists(&label_bank_path)
        && LabelBank::cache_valid(&label_bank_path, &vocab_hash)
    {
        // FAST PATH (unchanged): Load cached label bank
        let label_bank = LabelBank::load(&label_bank_path, vocabulary.len())?;
        let scorer = TagScorer::new(vocabulary, label_bank, config.tagging.clone());
        self.tag_scorer = Some(Arc::new(RwLock::new(scorer)));
    } else if config.tagging.progressive.enabled {
        // PROGRESSIVE PATH (new): Encode seed, background-encode rest
        if !SigLipTextEncoder::model_exists(&model_dir) {
            tracing::warn!("Text encoder not found...");
            return Ok(());
        }
        let text_encoder = Arc::new(SigLipTextEncoder::new(&model_dir)?);

        let seed_path = vocab_dir.join("seed_terms.txt");
        let seed_indices = SeedSelector::select(
            &vocabulary,
            &seed_path,
            config.tagging.progressive.seed_size,
        );

        let scorer_slot = Arc::new(RwLock::new(
            TagScorer::new(Vocabulary::empty(), LabelBank::empty(), config.tagging.clone())
        ));
        self.tag_scorer = Some(Arc::clone(&scorer_slot));

        let seed_scorer = ProgressiveEncoder::start(
            vocabulary,
            text_encoder,
            config.tagging.clone(),
            scorer_slot,
            seed_indices,
            label_bank_path,
            vocab_hash,
            config.tagging.progressive.chunk_size,
        );

        // Install the seed scorer
        {
            let mut lock = self.tag_scorer.as_ref().unwrap().write().unwrap();
            *lock = seed_scorer;
        }
    } else {
        // BLOCKING PATH (legacy): Encode all terms synchronously
        // (existing code, unchanged)
    }

    Ok(())
}
```

Update `process_with_options()` scoring section:

```rust
// Before (4a):
(Some(scorer), emb) if !emb.is_empty() => scorer.score(emb),

// After (4b):
(Some(scorer_lock), emb) if !emb.is_empty() => {
    let scorer = scorer_lock.read().unwrap();
    scorer.score(emb)
}
```

**Tests:**
- `test_process_options_default` — unchanged
- `test_rwlock_scorer_concurrent_reads` — verify multiple threads can score simultaneously

### Task 7: Configuration

**File:** `config.rs`

Add to `TaggingConfig`:

```rust
pub struct TaggingConfig {
    // ... existing fields ...

    /// Progressive encoding settings for first-run optimization
    pub progressive: ProgressiveConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProgressiveConfig {
    /// Enable progressive encoding on first run.
    /// When false, falls back to blocking encode-all (legacy behavior).
    pub enabled: bool,

    /// Number of seed terms to encode synchronously.
    pub seed_size: usize,

    /// Number of terms per background encoding chunk.
    pub chunk_size: usize,
}

impl Default for ProgressiveConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            seed_size: 2000,
            chunk_size: 5000,
        }
    }
}
```

TOML example:

```toml
[tagging.progressive]
enabled = true
seed_size = 2000
chunk_size = 5000
```

### Task 8: Add TagScorer/LabelBank Accessors

Small additions needed by the progressive encoder:

**`scorer.rs`:**
```rust
impl TagScorer {
    /// Get a reference to the label bank (for cache saving).
    pub fn label_bank(&self) -> &LabelBank { &self.label_bank }

    /// Get a reference to the vocabulary.
    pub fn vocabulary(&self) -> &Vocabulary { &self.vocabulary }
}
```

**`label_bank.rs`:**
```rust
impl LabelBank {
    /// Create an empty label bank (placeholder for RwLock initialization).
    pub fn empty() -> Self {
        Self { matrix: vec![], embedding_dim: 768, term_count: 0 }
    }
}
```

**`vocabulary.rs`:**
```rust
impl Vocabulary {
    /// Create an empty vocabulary.
    pub fn empty() -> Self {
        Self { terms: vec![], by_name: HashMap::new() }
    }
}
```

---

## Test Plan

### Unit Tests (no model required)

| Test | File | What it verifies |
|------|------|-----------------|
| `test_subset_preserves_terms` | `vocabulary.rs` | Subset vocab has correct terms, names, hypernyms |
| `test_subset_empty` | `vocabulary.rs` | Empty indices → empty vocab |
| `test_subset_rebuilds_index` | `vocabulary.rs` | `by_name` lookup works in subset |
| `test_empty_vocabulary` | `vocabulary.rs` | `Vocabulary::empty()` works |
| `test_append_grows_matrix` | `label_bank.rs` | Append increases term count and matrix size |
| `test_append_preserves_existing` | `label_bank.rs` | First N rows unchanged after append |
| `test_empty_label_bank` | `label_bank.rs` | `LabelBank::empty()` works, term_count=0 |
| `test_select_includes_supplemental` | `seed.rs` | All supplemental term indices in result |
| `test_select_respects_target_size` | `seed.rs` | Result length ≤ target_size |
| `test_select_deterministic` | `seed.rs` | Same input → same output |
| `test_select_without_seed_file` | `seed.rs` | Missing file doesn't panic |
| `test_progressive_config_defaults` | `config.rs` | enabled=true, seed_size=2000, chunk_size=5000 |
| `test_rwlock_scorer_concurrent_reads` | `processor.rs` | Multiple threads can score simultaneously |

### Integration Tests (require model — may be `#[ignore]`)

| Test | What it verifies |
|------|-----------------|
| `test_progressive_full_flow` | Tiny vocab (50 terms), seed 10, chunk 10 — all terms eventually encoded, cache saved |
| `test_progressive_fallback_to_blocking` | With `progressive.enabled = false`, verify blocking behavior unchanged |

---

## Acceptance Criteria

1. **First-run cold start < 60 seconds** — time from `load_tagging()` call to first `score()` returning tags
2. **Tags available immediately** — seed scorer produces meaningful tags (supplemental + top visual nouns)
3. **Progressive improvement** — later images in a batch get richer tags as background encoding progresses
4. **Cache saved on completion** — `label_bank.bin` written after background encoding finishes, identical format to today
5. **Subsequent runs unchanged** — cached label bank loads instantly, no progressive encoding triggered
6. **Backward compatible** — `progressive.enabled = false` gives identical behavior to Phase 4a
7. **No panics** — background encoding failures are logged and handled gracefully (partial vocabulary is still usable)
8. **All existing tests pass** — no regressions in Phase 1-5 test suite

---

## Dependencies

| Crate | Version | Purpose | Status |
|-------|---------|---------|--------|
| `rand` | 0.8 | Seeded random sampling for seed selection | **New** — must be added to both `Cargo.toml` (workspace) and `crates/photon-core/Cargo.toml` |

## Runtime Assumptions

`ProgressiveEncoder::start()` calls `tokio::spawn()` to launch the background encoding task. This requires an active tokio runtime. When called from the CLI (`#[tokio::main]`), this is guaranteed. **Library consumers** calling `load_tagging()` outside a tokio context will get a panic. Guard with:

```rust
if tokio::runtime::Handle::try_current().is_err() {
    tracing::warn!("No tokio runtime — falling back to blocking encode");
    // ... fall back to encode_all() blocking path
}
```

---

## Estimated Scope

| Component | Lines of Code (est.) | Complexity |
|-----------|---------------------|------------|
| `seed.rs` | ~80 | Low |
| `progressive.rs` | ~150 | Medium |
| `vocabulary.rs` additions | ~40 | Low |
| `label_bank.rs` additions | ~30 | Low |
| `processor.rs` changes | ~40 | Low |
| `config.rs` additions | ~25 | Low |
| `seed_terms.txt` curation | ~1000 lines | Low (manual) |
| Tests | ~200 | Medium |
| **Total** | **~565** | **Medium** |
