# Taxonomy Vision: Self-Organizing Adaptive Taxonomy

> **Status:** Conceptual / Future direction
> **Depends on:** Phase 3 (SigLIP embeddings), Phase 4 (zero-shot tagging)
> **Core principle:** No LLM dependency — taxonomy is purely algorithmic using SigLIP embeddings, WordNet, and Rust logic. No training phase, no batch rebuilds. The vocabulary self-organizes continuously.

---

## Problem

A manually authored flat taxonomy is rigid, incomplete, and one-size-fits-all. A shipping intelligence platform analysing satellite images needs entirely different tags than a food & beverage group. Maintaining these taxonomies by hand doesn't scale.

## Vision

A taxonomy that requires zero manual authoring, works immediately on first image, and continuously self-organizes to the user's actual library — all through brute-force scoring against a large vocabulary with progressive refinement.

---

## Core Architecture

### The Vocabulary

Photon ships with a WordNet-derived vocabulary file (~80,000 nouns, ~2MB text file). WordNet provides two things for free:

1. **Terms** — 80,000 nouns covering virtually every visual concept SigLIP knows about
2. **Hierarchy** — every noun has a hypernym chain (`labrador retriever → retriever → sporting dog → dog → canine → carnivore → mammal → animal → organism → entity`)

The vocabulary is the only external data dependency. Everything else is derived at runtime.

### Scoring Model

Every image is scored against the vocabulary via a single matrix multiplication:

```
image embedding:        1 × 768 vector
vocabulary embeddings:  N × 768 matrix    (N = active vocabulary size)
result:                 N similarity scores

Time: ~1-5ms for 80K terms on CPU
```

No tree traversal needed for scoring — brute-force against the full active vocabulary is fast enough. Hierarchy is used only for organizing/deduplicating output.

---

## Startup: Progressive Encoding

Encoding all 80K terms through SigLIP's text encoder takes 2-3 minutes. Instead of blocking on this, Photon uses progressive encoding:

### First Run Ever

```
IMMEDIATE (2-3 seconds)
════════════════════════
  1. Load WordNet vocabulary file (80K terms)
  2. Random-sample 2,000 terms
  3. Encode those 2K through SigLIP text encoder
  4. Ready to process images

BACKGROUND (while processing images)
═════════════════════════════════════
  5. Process images against the initial 2K terms
  6. High-scoring terms trigger priority encoding of
     their WordNet neighbors:
       "labrador" scores 0.87 →
         immediately encode: "golden retriever", "poodle",
         "retriever", "puppy", "dog", "canine"...
  7. Encode remaining terms in chunks of 1-2K
  8. Low-relevance terms get encoded last (or deferred)
  9. Save all encoded terms to label_bank.bin (~240MB)
```

### Subsequent Runs

```
  1. Load label_bank.bin from disk (instant — memory-mapped)
  2. Load relevance scores from state file
  3. Active vocabulary = terms with relevance > 0
  4. Ready immediately
```

---

## Per-Image Tagging Flow

```
  image.jpg
       │
       ▼
  SigLIP vision encoder → 768-dim vector          (~10-50ms)
       │
       ▼
  Dot product against all active term embeddings   (~1-5ms)
       │
       ▼
  Sort by score, filter by min_confidence
       │
       ▼
  Deduplicate using WordNet hierarchy:
    "labrador" (0.87), "dog" (0.81), "animal" (0.72)
    → these are ancestors, keep most specific
    → emit: "labrador" with path animal > dog > labrador
       │
       ▼
  Output tags:
    [
      {"name": "labrador", "confidence": 0.87, "path": "animal > dog > labrador"},
      {"name": "carpet", "confidence": 0.74, "path": "covering > floor covering > carpet"},
      {"name": "indoor", "confidence": 0.71, "path": "scene > indoor"}
    ]
```

No taxonomy tree to walk. No training phase. No clustering. Each image scored independently against the full vocabulary. Hierarchy derived from WordNet lookups on the results.

---

## Relevance Pruning

Over time, the vocabulary self-organizes to the user's library.

### Per-Term Tracking

Each term accumulates statistics:

```json
{
  "term": "labrador",
  "encoded": true,
  "times_above_threshold": 847,
  "avg_score_when_matched": 0.82,
  "last_matched": "2026-02-09",
  "relevance": 0.95,
  "pool": "active"
}
```

```json
{
  "term": "submarine",
  "encoded": true,
  "times_above_threshold": 0,
  "avg_score_when_matched": 0.0,
  "last_matched": null,
  "relevance": 0.0,
  "pool": "cold"
}
```

### Three-Pool System

| Pool | Description | Scored per image? | Size (typical) |
|------|-------------|-------------------|----------------|
| **Active** | Matched at least once in recent history | Yes, every image | 3K-15K terms |
| **Warm** | Encoded but never/rarely matched | Periodically (every Nth image) | 10K-30K terms |
| **Cold** | Not yet encoded, or irrelevant | No (re-checked on rebuild request) | Remainder of 80K |

As images are processed:
- Terms that score above threshold → stay in / move to **active**
- Active terms that haven't matched in a long time → demote to **warm**
- Warm terms checked periodically; if they match → promote to **active**
- Cold terms encoded in background; if relevant → promote

The active pool naturally converges to what matters:
- Food photographer: ~3K terms (cuisine, ingredients, plating, lighting)
- Wildlife photographer: ~5K terms (species, habitats, behaviors)
- General personal album: ~10-15K terms (broad coverage)

### Neighbor Expansion

When a term enters the active pool, its WordNet neighbors get priority:

```
"labrador" becomes active
       │
       ▼
  WordNet lookup: siblings & children
    → "golden retriever", "poodle", "german shepherd",
      "chocolate labrador", "yellow labrador"
       │
       ▼
  Encode these immediately, add to warm pool
  (they'll promote to active if they start matching)
```

This means the vocabulary deepens automatically in areas where the user has images, without needing to encode the entire 80K upfront.

---

## Hierarchy from WordNet (Display-Time Only)

The hierarchy is NOT used for scoring. It's used for organizing output.

WordNet provides hypernym chains for every noun. When tags are emitted:

1. Look up each tag's full ancestor chain
2. Group tags that share ancestors
3. Deduplicate: if "labrador" (0.87) and "dog" (0.81) both pass threshold, suppress "dog" since it's an ancestor of "labrador"
4. Optionally display the path for context: `animal > dog > labrador`

This gives structured, hierarchical output without ever building or maintaining a taxonomy tree.

### Handling Non-Noun Concepts

WordNet nouns cover objects and entities well but are weaker on:
- **Scenes** (beach, kitchen, cityscape)
- **Moods** (peaceful, dramatic, melancholic)
- **Styles** (vintage, minimalist, macro)
- **Weather** (sunny, foggy, overcast)
- **Time** (sunset, night, dawn)

These are supplemented with a small curated list (~200-500 terms) shipped alongside the WordNet vocabulary. These adjective/scene terms don't have WordNet hierarchy but are tagged with a simple category label.

---

## Prompt Templates

Single words work as-is for scoring. Optionally, a small set of prompt templates can improve accuracy:

```rust
fn encode_term(term: &str) -> Vec<String> {
    vec![
        term.to_string(),
        format!("a photograph of {}", term),
        format!("a photo of a {}", term),
    ]
}
```

For each term, encode 2-3 template variants, average their embeddings into one vector. This is done once during encoding, costs nothing at scoring time.

CLIP/SigLIP research shows this ensemble averaging outperforms any single prompt. No optimization loop needed — the fixed templates work well universally.

---

## Storage

```
~/.photon/
  vocabulary/
    wordnet_nouns.txt       # shipped with Photon (~2MB, 80K terms)
    supplemental.txt        # scenes, moods, styles (~500 terms)

  taxonomy/
    label_bank.bin          # pre-computed term embeddings (grows progressively)
    label_bank.meta.json    # which terms are encoded, their pool assignments
    relevance.json          # per-term statistics (match counts, scores, pools)
    state.json              # encoding progress, active pool size, last rebuild
```

Total disk footprint: ~250MB (dominated by label_bank.bin when fully encoded).

---

## Configuration

```toml
[tagging]
max_tags = 15                    # max tags per image
min_confidence = 0.25            # minimum similarity score to emit a tag
deduplicate_ancestors = true     # suppress ancestor tags when descendant matches
show_paths = false               # include hierarchy paths in output

[tagging.vocabulary]
initial_sample_size = 2000       # terms to encode at first startup
background_chunk_size = 1000     # terms per background encoding batch
neighbor_expansion = true        # auto-encode WordNet neighbors of active terms
warm_check_interval = 100        # re-check warm pool every N images
cold_promotion_threshold = 0.3   # min score for a warm term to promote to active
active_demotion_days = 90        # demote active term if no match in N days
```

---

## Implementation Phases

| Phase | What | Complexity | Value |
|-------|------|------------|-------|
| **4a** | Flat brute-force scoring against shipped vocabulary (no pruning, no pools, encode all at startup) | Low | Working auto-taxonomy with zero config |
| **4b** | Progressive encoding (chunked startup, background encoding) | Medium | Fast startup |
| **4c** | Relevance pruning (three-pool system, per-term stats) | Medium | Self-organizing vocabulary |
| **4d** | Neighbor expansion (WordNet-driven priority encoding) | Low | Deeper coverage where it matters |
| **4e** | Hierarchy deduplication (ancestor suppression, path display) | Low | Cleaner output |

Phase 4a alone delivers the core value. Each subsequent phase is an optimization.

---

## Open Questions

- What's the right initial sample size? 2K may miss important terms for some libraries. Could use a curated "seed" set of ~1K common visual terms + 1K random for diversity.
- How to handle polysemy — "jaguar" (animal vs car), "bass" (fish vs instrument)? WordNet has separate synsets for these; could encode both and let context (other high-scoring terms) disambiguate.
- Should the supplemental vocabulary (scenes, moods, styles) be scored differently than nouns? Adjective-like concepts may need different confidence thresholds.
- Is 80K terms the right ceiling? Could start with a filtered ~30K (only visually concrete nouns) to reduce encoding time and noise.
- Memory profile: 80K × 768 × 4 bytes = ~240MB. Acceptable for desktop, may need quantization for constrained devices.
