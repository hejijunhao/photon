# Phase 4e: Hierarchy Deduplication

> **Status:** Planned
> **Depends on:** Phase 4a (zero-shot tagging — complete). Independent of 4b/4c/4d.
> **Goal:** Suppress redundant ancestor tags and optionally display hierarchy paths for cleaner, more informative output.

---

## Problem

Phase 4a outputs the top-N tags by confidence without considering semantic overlap. A photo of a Labrador might produce:

```json
[
  {"name": "labrador retriever", "confidence": 0.87},
  {"name": "retriever",          "confidence": 0.81},
  {"name": "sporting dog",       "confidence": 0.74},
  {"name": "dog",                "confidence": 0.68},
  {"name": "canine",             "confidence": 0.59},
  {"name": "carnivore",          "confidence": 0.45},
  {"name": "mammal",             "confidence": 0.38},
  {"name": "carpet",             "confidence": 0.74},
  {"name": "indoor",             "confidence": 0.71}
]
```

Five of those nine tags ("labrador retriever" through "mammal") are the same concept at different specificity levels. They waste tag slots and clutter the output. The user wants "labrador retriever" and "carpet", not six levels of the animal taxonomy.

## Solution

Post-process scorer output to:

1. **Suppress ancestors** — if "labrador retriever" and "dog" both pass threshold, suppress "dog" because it's a hypernym of "labrador retriever"
2. **Optionally add hierarchy paths** — annotate surviving tags with their WordNet path for context: `animal > dog > labrador retriever`

After deduplication, the output becomes:

```json
[
  {"name": "labrador retriever", "confidence": 0.87, "path": "animal > dog > labrador retriever"},
  {"name": "carpet",             "confidence": 0.74, "path": "covering > floor covering > carpet"},
  {"name": "indoor",             "confidence": 0.71}
]
```

---

## Design Decisions

### 1. Suppression Direction: Always Suppress Ancestors

When both "labrador retriever" (0.87) and "dog" (0.68) pass threshold:

- **Keep the most specific (descendant):** "labrador retriever"
- **Suppress the more general (ancestor):** "dog"

This is always the right choice for tagging because:
- The specific term is strictly more informative
- The ancestor is implied — knowing it's a "labrador retriever" already tells you it's a "dog"
- SigLIP tends to give higher confidence to more specific matches anyway

**Decision: What if the ancestor has higher confidence?** Still suppress it. This can happen when SigLIP's text encoder produces a better embedding for "dog" than "labrador retriever". But the specific tag is still more useful — the ancestor is always recoverable from the hypernym chain.

### 2. How to Detect Ancestors

Every `VocabTerm` already has a `hypernyms: Vec<String>` field containing the ancestor chain (most specific first). To check if term A is an ancestor of term B:

```rust
term_b.hypernyms.contains(&term_a.display_name)
```

This is an O(h) lookup where h is the hypernym chain length (typically 5-12). For N filtered tags, the full dedup is O(N^2 * h) — with N ≤ 50 and h ≤ 12, this is ~6000 comparisons. Microseconds.

**Decision: Why not use synset IDs?** Hypernym chains store display names (human-readable), not synset IDs. Matching by display name works because the vocabulary is deterministic — each term maps to exactly one display name. Synset ID matching would be more precise for polysemous terms but adds complexity for negligible gain.

### 3. Path Display

When `show_paths = true`, each surviving tag gets an optional `path` field showing its position in the WordNet hierarchy. The path is constructed from the hypernym chain:

```
hypernyms: ["retriever", "sporting dog", "dog", "canine", "carnivore", "mammal", "animal"]
→ path: "animal > dog > labrador retriever"
```

The path is **abbreviated** — it shows only 2-3 ancestor levels for readability, not the full chain (which can be 10+ levels deep). The algorithm:

1. Take the hypernym chain
2. Select at most 2 representative ancestors (skip very generic ones like "entity", "object", "organism")
3. Append the term itself
4. Join with " > "

**Decision: How many levels?** Two ancestor levels gives enough context without clutter. "animal > dog > labrador retriever" tells you the domain and the parent category. Adding more ("organism > animal > mammal > carnivore > canine > dog > sporting dog > retriever > labrador retriever") is noise.

### 4. Supplemental Terms (No Hierarchy)

Supplemental terms (scenes, moods, styles) have no WordNet hierarchy. They cannot be ancestors or descendants of anything. They are:

- Never suppressed by ancestor logic
- Never suppress other terms
- Get no `path` field (or `path: null`)
- Pass through deduplication unchanged

### 5. Where in the Pipeline

Deduplication is a **post-processing step** applied after scoring, before output. It lives in the scorer module as a separate method, not mixed into the scoring loop.

```
score() → raw tags (sorted by confidence)
    ↓
deduplicate() → cleaned tags (ancestors removed)
    ↓
add_paths() → tags with hierarchy paths (optional)
    ↓
Output
```

This keeps concerns separated and makes dedup easy to toggle via config.

---

## New Types

### `Tag` Extension

Add an optional `path` field to the existing `Tag` struct:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub name: String,
    pub confidence: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,

    // NEW: Hierarchy path for context (Phase 4e)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}
```

The `path` field is `None` by default and only populated when `show_paths = true` in config. Existing output is unchanged when the feature is disabled — `skip_serializing_if` ensures the field is omitted from JSON.

### `HierarchyDedup` (new logic in `scorer.rs` or separate `tagging/hierarchy.rs`)

```rust
/// Post-processes scored tags to remove ancestor redundancy.
pub struct HierarchyDedup;

impl HierarchyDedup {
    /// Remove tags that are ancestors of other higher-confidence tags.
    pub fn deduplicate(tags: &[Tag], vocabulary: &Vocabulary) -> Vec<Tag>;

    /// Add hierarchy path strings to surviving tags.
    pub fn add_paths(tags: &mut [Tag], vocabulary: &Vocabulary, max_depth: usize);
}
```

---

## File-by-File Changes

### New Files

| File | Purpose |
|------|---------|
| `crates/photon-core/src/tagging/hierarchy.rs` | `HierarchyDedup` — ancestor suppression and path annotation |

### Modified Files

| File | Changes |
|------|---------|
| `crates/photon-core/src/tagging/mod.rs` | Add `pub mod hierarchy;` |
| `crates/photon-core/src/tagging/scorer.rs` | Call `HierarchyDedup::deduplicate()` at end of `score()` when config flag is set |
| `crates/photon-core/src/types.rs` | Add `path: Option<String>` to `Tag` struct |
| `crates/photon-core/src/config.rs` | Add `deduplicate_ancestors` and `show_paths` to `TaggingConfig` |

---

## Task Breakdown

### Task 1: Add `path` Field to `Tag`

**File:** `types.rs`

```rust
pub struct Tag {
    pub name: String,
    pub confidence: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,  // NEW
}
```

Update `Tag::new()` and `Tag::with_category()` to set `path: None`.

**Also update the struct literal in `scorer.rs:69-76`** where `Tag` is constructed during scoring:

```rust
// scorer.rs — update the Tag construction in score() (and hits_to_tags() if 4c is done):
Tag {
    name: term.display_name.clone(),
    confidence,
    category: term.category.clone(),
    path: None,  // ← ADD THIS — compile error without it
}
```

This struct literal is the only place outside `types.rs` where `Tag` is constructed inline (not via `Tag::new()`). Missing this field will cause a compile error.

**Tests:**
- `test_tag_serde_without_path` — path omitted from JSON when None (backward compatible)
- `test_tag_serde_with_path` — path included in JSON when Some("animal > dog > labrador retriever")

### Task 2: Ancestor Detection

**File:** `tagging/hierarchy.rs` (new)

```rust
impl HierarchyDedup {
    /// Check if `ancestor_name` appears in `term`'s hypernym chain.
    fn is_ancestor(vocabulary: &Vocabulary, term_name: &str, ancestor_name: &str) -> bool {
        let term = match vocabulary.get(term_name) {
            Some(t) => t,
            None => {
                // Try matching by display name (terms may use display names)
                // Lookup via the raw name (underscores)
                let raw_name = term_name.replace(' ', "_");
                match vocabulary.get(&raw_name) {
                    Some(t) => t,
                    None => return false,
                }
            }
        };

        term.hypernyms.iter().any(|h| h == ancestor_name)
    }
}
```

**Key detail:** Tag names are display names (spaces) but vocabulary lookup uses raw names (underscores). The helper normalizes between the two.

**Tests:**
- `test_is_ancestor_direct_parent` — "retriever" is ancestor of "labrador retriever"
- `test_is_ancestor_grandparent` — "dog" is ancestor of "labrador retriever"
- `test_is_ancestor_unrelated` — "carpet" is not ancestor of "labrador retriever"
- `test_is_ancestor_self` — a term is NOT an ancestor of itself (hypernym chain doesn't include self)
- `test_is_ancestor_supplemental` — supplemental terms have no ancestors, always false

### Task 3: Ancestor Suppression

**File:** `tagging/hierarchy.rs`

```rust
impl HierarchyDedup {
    /// Remove tags that are ancestors of other tags in the list.
    ///
    /// For each pair of tags (A, B): if A is an ancestor of B (via WordNet
    /// hypernyms), suppress A. The more specific tag B survives.
    ///
    /// Tags are expected to be sorted by confidence (highest first).
    /// A suppressed ancestor is always the less informative tag.
    pub fn deduplicate(tags: &[Tag], vocabulary: &Vocabulary) -> Vec<Tag> {
        let mut suppressed: HashSet<usize> = HashSet::new();

        for i in 0..tags.len() {
            if suppressed.contains(&i) { continue; }

            for j in 0..tags.len() {
                if i == j || suppressed.contains(&j) { continue; }

                // Check if tags[i] is an ancestor of tags[j]
                if Self::is_ancestor(vocabulary, &tags[j].name, &tags[i].name) {
                    // tags[i] is an ancestor of tags[j] — suppress i
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
}
```

**Algorithm complexity:** O(N^2 * h) where N = number of tags (≤ max_tags, typically 15-50) and h = max hypernym chain length (~12). This is fast enough — ~900 string comparisons for 15 tags.

**Tests:**
- `test_dedup_suppresses_ancestors` — "labrador retriever" + "dog" + "animal" → only "labrador retriever" survives
- `test_dedup_preserves_unrelated` — "labrador retriever" + "carpet" → both survive
- `test_dedup_multiple_chains` — "labrador retriever" + "pizza" each suppress their own ancestors
- `test_dedup_no_hypernyms` — all supplemental terms → no suppression
- `test_dedup_empty_tags` — empty input → empty output
- `test_dedup_preserves_order` — surviving tags maintain original confidence ordering

### Task 4: Path Annotation

**File:** `tagging/hierarchy.rs`

```rust
/// Terms to skip when building abbreviated paths (too generic to be useful).
const SKIP_TERMS: &[&str] = &[
    "entity", "physical entity", "object", "whole",
    "thing", "organism", "living thing", "abstraction",
    "matter", "substance", "body", "unit",
];

impl HierarchyDedup {
    /// Add abbreviated hierarchy paths to tags.
    ///
    /// Path format: "grandparent > parent > term"
    /// Shows at most `max_ancestors` levels, skipping very generic terms.
    pub fn add_paths(tags: &mut [Tag], vocabulary: &Vocabulary, max_ancestors: usize) {
        for tag in tags.iter_mut() {
            let raw_name = tag.name.replace(' ', "_");
            let term = match vocabulary.get(&raw_name) {
                Some(t) => t,
                None => continue, // Supplemental terms — no path
            };

            if term.hypernyms.is_empty() {
                continue; // No hierarchy available
            }

            // Filter out overly generic ancestors
            let meaningful: Vec<&str> = term.hypernyms
                .iter()
                .map(|h| h.as_str())
                .filter(|h| !SKIP_TERMS.contains(h))
                .collect();

            // Take the last N meaningful ancestors (most general first)
            let ancestors: Vec<&str> = if meaningful.len() > max_ancestors {
                meaningful[meaningful.len() - max_ancestors..].to_vec()
            } else {
                meaningful
            };

            // Build path: general > specific > term
            let mut path_parts: Vec<&str> = ancestors.into_iter().rev().collect();
            // Reverse because hypernyms are stored most-specific-first,
            // but path reads left-to-right general-to-specific
            path_parts.reverse();
            path_parts.push(&tag.name);

            tag.path = Some(path_parts.join(" > "));
        }
    }
}
```

**Tests:**
- `test_add_paths_basic` — "labrador retriever" with hypernyms gets "animal > dog > labrador retriever"
- `test_add_paths_skips_generic` — "entity" and "object" are skipped
- `test_add_paths_max_ancestors` — max_ancestors=2 limits depth
- `test_add_paths_supplemental_no_path` — supplemental terms get path=None
- `test_add_paths_short_chain` — term with only 1 hypernym → "parent > term"

### Task 5: Config Additions

**File:** `config.rs`

Add to `TaggingConfig`:

```rust
pub struct TaggingConfig {
    // ... existing fields ...

    /// Remove ancestor tags when a more specific descendant matches.
    /// E.g., suppress "dog" when "labrador retriever" is present.
    pub deduplicate_ancestors: bool,

    /// Include hierarchy paths in tag output.
    /// E.g., "animal > dog > labrador retriever"
    pub show_paths: bool,

    /// Maximum ancestor levels to show in hierarchy paths.
    pub path_max_depth: usize,
}

// In Default impl:
Self {
    // ... existing ...
    deduplicate_ancestors: false, // Off by default — opt-in to avoid silent behavior change for existing users
    show_paths: false,            // Off by default — opt-in for richer output
    path_max_depth: 2,            // 2 ancestor levels
}
```

**Tests:**
- `test_tagging_config_defaults` — verify new fields have expected defaults

### Task 6: Wire Into Scorer

**File:** `tagging/scorer.rs`

Add dedup + path annotation at the end of `score()`:

```rust
pub fn score(&self, image_embedding: &[f32]) -> Vec<Tag> {
    // ... existing scoring logic ...
    // tags is sorted by confidence, truncated to max_tags

    // Phase 4e: Hierarchy deduplication
    let mut tags = if self.config.deduplicate_ancestors {
        HierarchyDedup::deduplicate(&tags, &self.vocabulary)
    } else {
        tags
    };

    // Phase 4e: Path annotation
    if self.config.show_paths {
        HierarchyDedup::add_paths(&mut tags, &self.vocabulary, self.config.path_max_depth);
    }

    tags
}
```

**Important:** Dedup runs AFTER truncation to max_tags. This means dedup may reduce the tag count below max_tags. This is correct — the alternative (dedup before truncation) would require scoring more terms initially, adding complexity. Having fewer but more informative tags is the desired outcome.

Also wire into `score_with_pools()` (from Phase 4c) if it exists. Since `score_with_pools()` delegates tag construction to `hits_to_tags()`, the cleanest integration point is inside `hits_to_tags()` itself — apply dedup + paths there so all scoring paths benefit automatically:

```rust
fn hits_to_tags(&self, hits: Vec<(usize, f32)>) -> Vec<Tag> {
    // ... filter, sort, truncate ...

    // Phase 4e: Hierarchy deduplication
    let mut tags = if self.config.deduplicate_ancestors {
        HierarchyDedup::deduplicate(&tags, &self.vocabulary)
    } else {
        tags
    };

    // Phase 4e: Path annotation
    if self.config.show_paths {
        HierarchyDedup::add_paths(&mut tags, &self.vocabulary, self.config.path_max_depth);
    }

    tags
}
```

This way, both `score()` and `score_with_pools()` get dedup+paths automatically since they both go through `hits_to_tags()`.

### Task 7: CLI Flag (Optional)

**File:** `crates/photon/src/cli/process.rs`

Add optional CLI flags:

```rust
/// Show hierarchy paths in tag output
#[arg(long)]
show_tag_paths: bool,

/// Disable ancestor deduplication in tags
#[arg(long)]
no_dedup_tags: bool,
```

Apply before processing:

```rust
if args.show_tag_paths {
    config.tagging.show_paths = true;
}
if args.no_dedup_tags {
    config.tagging.deduplicate_ancestors = false;
}
```

---

## Test Plan

### Unit Tests

| Test | File | What it verifies |
|------|------|-----------------|
| `test_tag_serde_without_path` | `types.rs` | path=None → omitted from JSON |
| `test_tag_serde_with_path` | `types.rs` | path=Some → included in JSON |
| `test_is_ancestor_direct_parent` | `hierarchy.rs` | Direct parent detected |
| `test_is_ancestor_grandparent` | `hierarchy.rs` | Transitive ancestor detected |
| `test_is_ancestor_unrelated` | `hierarchy.rs` | Unrelated terms not falsely detected |
| `test_is_ancestor_self` | `hierarchy.rs` | Self is not ancestor |
| `test_is_ancestor_supplemental` | `hierarchy.rs` | No false ancestors for supplemental terms |
| `test_dedup_suppresses_ancestors` | `hierarchy.rs` | Ancestor chain collapsed to most specific |
| `test_dedup_preserves_unrelated` | `hierarchy.rs` | Unrelated terms untouched |
| `test_dedup_multiple_chains` | `hierarchy.rs` | Multiple independent chains deduped |
| `test_dedup_empty_tags` | `hierarchy.rs` | Empty input handled |
| `test_dedup_preserves_order` | `hierarchy.rs` | Confidence ordering maintained |
| `test_add_paths_basic` | `hierarchy.rs` | Correct path string generated |
| `test_add_paths_skips_generic` | `hierarchy.rs` | "entity", "object" filtered |
| `test_add_paths_max_ancestors` | `hierarchy.rs` | Depth limited correctly |
| `test_add_paths_supplemental_no_path` | `hierarchy.rs` | Supplemental terms → None |
| `test_add_paths_short_chain` | `hierarchy.rs` | Short chains handled |
| `test_tagging_config_new_defaults` | `config.rs` | New fields have expected defaults |

### Integration Tests

| Test | What it verifies |
|------|-----------------|
| `test_dedup_off_matches_4a` | With `deduplicate_ancestors=false`, output identical to Phase 4a |
| `test_dedup_reduces_tag_count` | With dedup on, fewer tags than without (for images with hierarchical matches) |
| `test_paths_json_format` | Full pipeline output includes well-formatted path strings |

---

## Acceptance Criteria

1. **Ancestor suppression works** — "labrador retriever" + "dog" + "animal" → only "labrador retriever"
2. **No false suppression** — unrelated terms are never suppressed
3. **Supplemental terms untouched** — scene/mood/style tags are never suppressed or given paths
4. **Path format is readable** — "animal > dog > labrador retriever" (not "entity > physical entity > object > organism > ...")
5. **Backward compatible** — existing JSON output unchanged with defaults (`deduplicate_ancestors=false`, `show_paths=false`). Both features are opt-in to avoid surprising existing consumers.
6. **`path` field absent when disabled** — `skip_serializing_if` ensures no JSON bloat
7. **Performance unchanged** — dedup + path annotation adds < 0.1ms per image
8. **All existing tests pass** — no regressions

---

## Configuration (TOML)

```toml
[tagging]
deduplicate_ancestors = false   # Default: false (opt-in — set true to suppress redundant ancestor tags)
show_paths = false              # Default: false (opt-in for hierarchy path display)
path_max_depth = 2              # Max ancestor levels in path strings
```

**Recommendation:** Enable `deduplicate_ancestors = true` in new installations. The default is `false` only to preserve backward compatibility for existing users.

---

## Estimated Scope

| Component | Lines of Code (est.) | Complexity |
|-----------|---------------------|------------|
| `hierarchy.rs` | ~120 | Low |
| `types.rs` change | ~5 | Trivial |
| `scorer.rs` additions | ~15 | Low |
| `config.rs` additions | ~15 | Trivial |
| `process.rs` CLI flags | ~10 | Trivial |
| Tests | ~200 | Low |
| **Total** | **~365** | **Low** |

---

## Note on Independence

Phase 4e can be implemented **at any time** — it has no dependency on 4b (progressive encoding) or 4c/4d (relevance pruning). It only requires the Phase 4a vocabulary with hypernym chains, which already exists.

If desired, 4e could be implemented *before* 4b/4c/4d as a quick quality-of-life improvement to tag output. It's the lowest-effort, highest-immediate-impact phase of the four.
