# Phase 4e: Hierarchy Deduplication — Completion

> **Status:** Complete
> **Depends on:** Phase 4a (zero-shot tagging — complete). Independent of 4b/4c/4d.
> **Result:** Post-processing step that suppresses redundant ancestor tags and optionally annotates surviving tags with abbreviated WordNet hierarchy paths. Backward compatible — both features are off by default. Existing JSON output is byte-identical when disabled.

---

## Summary

Implemented a two-stage post-processing step applied after scoring, before output:

1. **Ancestor suppression** — if both "labrador retriever" (0.87) and "dog" (0.68) pass threshold, suppress "dog" because it's a hypernym of "labrador retriever". The specific term is strictly more informative; the ancestor is always recoverable from the hypernym chain.

2. **Path annotation** — surviving tags get an optional `path` field showing their abbreviated position in the WordNet hierarchy: `"animal > canine > labrador retriever"`. Overly generic ancestors ("entity", "object", "organism", etc.) are filtered out; depth is capped at `path_max_depth` levels (default 2).

Before:
```json
[
  {"name": "labrador retriever", "confidence": 0.87},
  {"name": "retriever",          "confidence": 0.81},
  {"name": "dog",                "confidence": 0.68},
  {"name": "carpet",             "confidence": 0.74},
  {"name": "indoor",             "confidence": 0.71}
]
```

After (with `deduplicate_ancestors = true`, `show_paths = true`):
```json
[
  {"name": "labrador retriever", "confidence": 0.87, "path": "animal > canine > labrador retriever"},
  {"name": "carpet",             "confidence": 0.74, "path": "floor covering > carpet"},
  {"name": "indoor",             "confidence": 0.71}
]
```

120 tests passing (+20 new), zero clippy warnings.

---

## What Was Built

### New Files

| File | Lines | Purpose |
|------|-------|---------|
| `crates/photon-core/src/tagging/hierarchy.rs` | ~120 impl + ~200 tests | `HierarchyDedup` — ancestor detection, suppression, and path annotation |

### Modified Files

| File | Changes |
|------|---------|
| `crates/photon-core/src/types.rs` | Added `path: Option<String>` to `Tag` with `#[serde(skip_serializing_if)]`; updated `Tag::new()` and `Tag::with_category()` constructors; 2 new serde tests |
| `crates/photon-core/src/config.rs` | Added `deduplicate_ancestors: bool`, `show_paths: bool`, `path_max_depth: usize` to `TaggingConfig` (defaults: false/false/2); 1 new test |
| `crates/photon-core/src/tagging/mod.rs` | Added `pub mod hierarchy;` |
| `crates/photon-core/src/tagging/scorer.rs` | Import `HierarchyDedup`; wire dedup + path annotation into `hits_to_tags()` so both `score()` and `score_with_pools()` benefit; added `path: None` to `Tag` struct literal |
| `crates/photon/src/cli/process.rs` | Added `--show-tag-paths` and `--no-dedup-tags` CLI flags with config override wiring |

---

## Key Design Decisions

### 1. Single Integration Point in `hits_to_tags()`

Both `score()` and `score_with_pools()` (Phase 4cd) delegate tag construction to the shared `hits_to_tags()` helper. Dedup and path annotation are wired at the end of this method, so all scoring paths benefit automatically without code duplication:

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

### 2. Suppression Direction: Always Suppress Ancestors

When both a specific term and its ancestor pass threshold, the ancestor is **always** suppressed — even if the ancestor has higher confidence. This can happen when SigLIP's text encoder produces a better embedding for "dog" than "labrador retriever", but the specific tag is still more informative. The ancestor is always recoverable from the hypernym chain.

### 3. Dedup After Truncation

Dedup runs after `max_tags` truncation, not before. This means the final tag count may be less than `max_tags` after ancestor removal. This is the correct trade-off — fewer but more informative tags is the desired outcome. The alternative (dedup before truncation) would require scoring extra terms initially, adding complexity for marginal benefit.

### 4. O(N² × h) Complexity is Negligible

For each pair of N filtered tags (≤ `max_tags`, default 15), we check if one is an ancestor of the other by scanning the hypernym chain (length h, typically 5–12). Worst case: 15² × 12 = 2,700 string comparisons — microseconds per image, compared to ~85ms for SigLIP embedding inference.

### 5. Display Name ↔ Raw Name Normalization

Tag names use display format (spaces: "labrador retriever") but vocabulary lookup uses raw format (underscores: "labrador_retriever"). Both `is_ancestor()` and `add_paths()` normalize via `name.replace(' ', "_")` before vocabulary lookup.

### 6. Supplemental Terms Pass Through Unchanged

Supplemental terms (scenes, moods, styles, weather, time) have no WordNet hierarchy — empty `hypernyms` vec. They:
- Are never suppressed by ancestor logic
- Never suppress other terms
- Get no `path` field (`None`, omitted from JSON)

### 7. Off by Default for Backward Compatibility

Both `deduplicate_ancestors` and `show_paths` default to `false`. Existing JSON output is byte-identical when disabled — `skip_serializing_if = "Option::is_none"` ensures no `"path": null` noise. Users opt in via config or CLI flags.

### 8. Abbreviated Paths with Generic Term Filtering

The `SKIP_TERMS` list filters out overly generic WordNet ancestors that add no informational value:

```rust
const SKIP_TERMS: &[&str] = &[
    "entity", "physical entity", "object", "whole",
    "thing", "organism", "living thing", "abstraction",
    "matter", "substance", "body", "unit",
];
```

After filtering, only `max_ancestors` (default 2) meaningful ancestors are shown. If all hypernyms are generic, no path is added.

---

## API Surface

### Tag Struct (extended)

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

### HierarchyDedup (new)

```rust
pub struct HierarchyDedup;

impl HierarchyDedup {
    /// Remove tags that are ancestors of other tags in the list.
    pub fn deduplicate(tags: &[Tag], vocabulary: &Vocabulary) -> Vec<Tag>;

    /// Add abbreviated hierarchy paths to surviving tags.
    pub fn add_paths(tags: &mut [Tag], vocabulary: &Vocabulary, max_ancestors: usize);
}
```

### CLI Flags (new)

```
--show-tag-paths      Show hierarchy paths in tag output
--no-dedup-tags       Disable ancestor deduplication
```

---

## Test Coverage

### New Tests (20)

**types.rs (2):**
- `test_tag_serde_without_path` — path=None → omitted from JSON (backward compatible)
- `test_tag_serde_with_path` — path=Some → included in JSON with correct value

**config.rs (1):**
- `test_tagging_config_hierarchy_defaults` — new fields have expected defaults (false/false/2)

**hierarchy.rs — is_ancestor (5):**
- `test_is_ancestor_direct_parent` — "retriever" detected as ancestor of "labrador retriever"
- `test_is_ancestor_grandparent` — transitive ancestors "dog", "animal" detected
- `test_is_ancestor_unrelated` — "carpet" not falsely detected as ancestor
- `test_is_ancestor_self` — a term is NOT its own ancestor
- `test_is_ancestor_supplemental` — supplemental terms always return false

**hierarchy.rs — deduplicate (6):**
- `test_dedup_suppresses_ancestors` — full ancestor chain collapsed to most specific
- `test_dedup_preserves_unrelated` — unrelated terms untouched
- `test_dedup_multiple_chains` — two independent chains deduped independently
- `test_dedup_no_hypernyms` — supplemental terms → no suppression
- `test_dedup_empty_tags` — empty input → empty output
- `test_dedup_preserves_order` — surviving tags maintain original confidence ordering

**hierarchy.rs — add_paths (6):**
- `test_add_paths_basic` — correct path string for multi-level hierarchy
- `test_add_paths_skips_generic` — "entity", "organism", "living thing" filtered
- `test_add_paths_max_ancestors` — depth limited correctly (1 ancestor + term)
- `test_add_paths_supplemental_no_path` — supplemental terms get path=None
- `test_add_paths_short_chain` — single hypernym → "parent > term"
- `test_add_paths_all_generic_hypernyms` — all hypernyms in skip list → no path added

---

## Configuration (TOML)

```toml
[tagging]
deduplicate_ancestors = false   # Default: false (opt-in to suppress redundant ancestor tags)
show_paths = false              # Default: false (opt-in for hierarchy path display)
path_max_depth = 2              # Max ancestor levels in path strings
```

**Recommendation:** Enable `deduplicate_ancestors = true` in new installations. The default is `false` only to preserve backward compatibility for existing consumers.

---

## Deviations from Plan

1. **No `hierarchy.rs` struct state**: The plan suggested `HierarchyDedup` as a struct. Implemented as a unit struct with associated functions instead — there's no state to hold, so `HierarchyDedup::deduplicate()` and `HierarchyDedup::add_paths()` are pure functions that take vocabulary by reference.

2. **`is_ancestor()` simplified**: The plan included a two-stage lookup (try display name, then raw name). Simplified to a single `name.replace(' ', "_")` normalization since all tag names come from `VocabTerm.display_name` (which always uses spaces for WordNet terms) and vocabulary lookup always uses raw names (underscores). No need for the fallback path.

3. **Path ancestor ordering**: Hypernyms stored most-specific-first are sliced from the tail (most general meaningful ancestors), then reversed via `.rev()` to produce the general → specific → term path format. The plan's pseudo-code had a redundant double-reverse that was not carried forward.

4. **Guard for all-generic hypernyms**: Added a `meaningful.is_empty()` guard after filtering SKIP_TERMS, ensuring terms whose entire hypernym chain is generic (e.g., "item" with hypernyms "thing > object > entity") get no path rather than an empty one. This case wasn't in the plan's test list.
