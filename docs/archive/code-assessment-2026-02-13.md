# Code Assessment — 2026-02-13

> Full codebase assessment across 55 Rust source files (~11.7K lines), 214 tests. Focused on functional correctness, maintainability, clean code, and files exceeding 500 lines.
>
> **Builds on**: `docs/plans/merged-assessment.md` (all 5 HIGH findings previously fixed). This assessment identifies **new findings** and tracks remaining MEDIUM items.

**Scope**: 55 Rust files, ~11,700 lines, 214 tests (156 unit + 38 CLI + 20 integration)

---

## 1. Files Exceeding 500 Lines — Refactoring Candidates

| File | Lines | Recommendation |
|------|-------|----------------|
| `tagging/relevance.rs` | **804** | Split into submodules (see R1) |
| `tests/integration.rs` | 680 | Acceptable — test file, not production code |
| `llm/enricher.rs` | 673 | Acceptable — 460 source + 213 test lines; test code inflates count |
| `pipeline/processor.rs` | **603** | Extract tagging coordination (see R2) |
| `cli/models.rs` | **535** | Extract download logic (see R3) |
| `cli/process/batch.rs` | 492 | Approaching threshold — monitor |

### R1. Split `relevance.rs` (804 lines)

The largest file in the codebase. Well-structured internally, but navigability suffers. Recommended split:

| New file | Contents | ~Lines |
|----------|----------|--------|
| `relevance/mod.rs` | `Pool` enum, `RelevanceConfig`, public re-exports | 30 |
| `relevance/stats.rs` | `TermStats` struct and its methods | 50 |
| `relevance/tracker.rs` | `RelevanceTracker` core logic (sweep, record, promote, save/load) | 280 |
| `relevance/tests.rs` | All `#[cfg(test)]` code | 440 |

### R2. Extract tagging coordination from `processor.rs` (603 lines)

`processor.rs` handles both pipeline orchestration and complex lock coordination for the three-pool tagging system. The tagging phase (lines 433–537) is 105 lines of nested match expressions with lock ordering concerns.

**Recommended extraction**:
- Move the pool-aware scoring + neighbor expansion + relevance tracking logic (lines 433–537) into a `TaggingCoordinator` struct or standalone function in `tagging/`.
- Move the duplicate `load_tagging()` / `load_tagging_blocking()` shared logic into helper functions (~40 lines of overlap).

### R3. Extract download logic from `cli/models.rs` (535 lines)

`models.rs` handles both subcommand dispatch and HTTP download with progress bars. The download logic (~200 lines) could move to a `download.rs` helper, leaving `models.rs` focused on subcommand orchestration.

---

## 2. New Findings

### N1. Silent Enrichment Output Drop on Serialization Failure

**File**: `cli/process/enrichment.rs:51–59`
**Severity**: MEDIUM

```rust
let json = if pretty {
    serde_json::to_string_pretty(&record)
} else {
    serde_json::to_string(&record)
};
if let Ok(json) = json {
    println!("{json}");
}
// Serialization failure → record silently dropped, no log
```

When `serde_json::to_string()` fails on an `OutputRecord::Enrichment`, the record is silently discarded. Users see no error — their enrichment output is simply incomplete. While serialization of `OutputRecord` is unlikely to fail (it's a simple struct), this violates the principle of no silent data loss.

**Fix**: Log the error:
```rust
match json {
    Ok(json) => println!("{json}"),
    Err(e) => tracing::error!("Failed to serialize enrichment record: {e}"),
}
```

---

### N2. Unvalidated Vocabulary/LabelBank Size Invariant in TagScorer

**File**: `tagging/scorer.rs:36–42` (constructor), `scorer.rs:71` (indexing)
**Severity**: MEDIUM

`TagScorer::new()` accepts any `Vocabulary` and `LabelBank` without validating that `vocabulary.len() == label_bank.term_count()`. The `hits_to_tags()` method at line 71 does `terms[*idx]` where `idx` comes from iterating `0..label_bank.term_count()`. If the label bank has more terms than the vocabulary, this panics with index-out-of-bounds.

In normal operation, both are built from the same data, so this can't happen. But `TagScorer::new()` is a public constructor — a misuse (or a progressive encoding race condition producing mismatched data) could trigger a panic mid-batch.

**Fix**: Add a validation check in `TagScorer::new()`:
```rust
pub fn new(vocabulary: Vocabulary, label_bank: LabelBank, config: TaggingConfig) -> Result<Self, PipelineError> {
    if vocabulary.len() != label_bank.term_count() {
        return Err(PipelineError::Tagging {
            path: PathBuf::new(),
            message: format!(
                "Vocabulary/LabelBank size mismatch: {} terms vs {} embeddings",
                vocabulary.len(), label_bank.term_count(),
            ),
        });
    }
    Ok(Self { vocabulary, label_bank, config })
}
```

This is a signature change, so all callers of `TagScorer::new()` will need updating (3 call sites: `processor.rs`, `progressive.rs`).

---

### N3. WebP Validation Accepts Incomplete RIFF Files

**File**: `pipeline/validate.rs:108–117`
**Severity**: LOW

```rust
if header[0] == b'R' && header[1] == b'I' && header[2] == b'F' && header[3] == b'F' {
    if bytes_read >= 12 {
        return header[8] == b'W' && header[9] == b'E' && header[10] == b'B' && header[11] == b'P';
    }
    // Could be WebP, allow it to proceed
    return true;  // ← Accepts any RIFF file < 12 bytes (AVI, WAV, etc.)
}
```

A file starting with `RIFF` but with fewer than 12 readable bytes passes validation. This includes non-image RIFF containers (AVI, WAV) that happen to be truncated. The decoder will reject it later, so no data corruption occurs, but the error message will be misleading ("decode failed" instead of "unsupported format").

**Fix**: Require full 12-byte read for WebP detection:
```rust
if bytes_read >= 12
    && header[0..4] == *b"RIFF"
    && header[8..12] == *b"WEBP"
{
    return true;
}
```

---

### N4. Provider `timeout()` Trait Method Is Vestigial

**File**: `llm/provider.rs:121` (trait definition), `enricher.rs` (never calls it)
**Severity**: LOW (dead code / design inconsistency)

All four LLM providers implement `fn timeout(&self) -> Duration` (Anthropic/OpenAI: 60s, Ollama: 120s), but the `Enricher` never calls it. Timeouts come exclusively from `EnrichOptions::timeout_ms` (sourced from config). The per-provider timeouts on HTTP calls were correctly removed in H6 fix, but the trait method remained.

This means Ollama's 120s timeout (recognizing local models need longer) is never respected — it gets the same config-level timeout as cloud providers.

**Fix**: Either:
- **(A)** Use `provider.timeout()` as the default when `config.limits.llm_timeout_ms` is 0 or absent, or
- **(B)** Remove `timeout()` from the trait and all implementations (dead code cleanup).

Option (A) is better UX — Ollama users won't need to manually configure longer timeouts.

---

### N5. Fragile Semaphore Permit Pattern in Enricher

**File**: `llm/enricher.rs:80–84`
**Severity**: LOW

```rust
let permit = semaphore.clone().acquire_owned().await;
if permit.is_err() {
    tracing::warn!("Enrichment semaphore closed unexpectedly — stopping batch");
    break;
}
let permit = permit.unwrap();  // Safe but fragile
```

The `.unwrap()` is logically safe (guarded by `is_err()` check above), but the pattern is fragile — a future refactor moving the `is_err()` check could introduce a panic. Idiomatic Rust would use `match` or `let Ok(permit) = permit else { ... }`.

**Fix**:
```rust
let permit = match semaphore.clone().acquire_owned().await {
    Ok(p) => p,
    Err(_) => {
        tracing::warn!("Enrichment semaphore closed unexpectedly — stopping batch");
        break;
    }
};
```

---

## 3. Status of Remaining MEDIUM Findings (from merged-assessment.md)

These were identified in the previous assessment and remain open:

| ID | Finding | Status | Notes |
|----|---------|--------|-------|
| M1 | Symlink cycle errors silently swallowed (`discovery.rs`) | **Open** | `filter_map(\|e\| e.ok())` drops all WalkDir errors |
| M2 | Unbounded memory in enrichment/batch collection | **Open** | Unbounded `mpsc::channel` + in-memory `Vec<ProcessedImage>` |
| M3 | Silent JSON parse failure in `--skip-existing` | **Open** | Hash set stays empty, all images reprocessed |
| M4 | Unbounded image read in enricher | **Fixed (741fa1d)** | File size guard added at enricher.rs:128–147 |
| M5 | Progressive + relevance mutual exclusion not enforced | **Open** | Relevance silently disabled when progressive active |
| M6 | Lock ordering undocumented in processor.rs | **Open** | Read-read is safe currently; write lock ordering fragile |
| M7 | Missing path context in SigLIP embedding errors | **Fixed (741fa1d)** | Path now passed through all error contexts |
| M8 | Progressive encoder chunk failure (downgraded from H3) | **Open** | Graceful degradation, but no user-facing summary |

**Remaining open: M1, M2, M3, M5, M6, M8** (6 items)

---

## 4. Test Coverage Gaps

### Critical Gaps (Zero Tests)

| Module | Lines | Risk |
|--------|-------|------|
| `tagging/text_encoder.rs` | 157 | ONNX text encoding — untested tokenization, batch encoding, model failures |
| `tagging/progressive.rs` | 214 | Progressive encoding orchestration — seed → background → scorer updates |
| `embedding/siglip.rs` | 131 | Vision embedding — timeout behavior, dimension validation, model loading |
| `llm/anthropic.rs` | 172 | Provider request/response formatting, auth, error mapping |
| `llm/openai.rs` | 173 | Same |
| `llm/ollama.rs` | 131 | Same |
| `error.rs` | 166 | Error Display implementations, variant construction |

These modules require external resources (ONNX models, API keys) for integration testing, which explains the gap. However, unit tests for request building, error mapping, and preprocessing logic are feasible without external dependencies.

### Weak Coverage (1–3 Tests)

| Module | Lines | Tests | Gap |
|--------|-------|-------|-----|
| `pipeline/processor.rs` | 603 | 1 | Only tests `ProcessOptions::default()` — no processing logic tested |
| `pipeline/metadata.rs` | 148 | 1 | Only basic EXIF extraction — no malformed data, missing fields |
| `pipeline/discovery.rs` | 162 | 3 | No symlink, permission error, or deep nesting tests |
| `pipeline/hash.rs` | 88 | 3 | No perceptual hash edge cases (near-identical images) |
| `pipeline/thumbnail.rs` | 117 | 3 | No varied dimensions or quality settings |

### Tests That Could Pass With Broken Code

1. **`integration.rs::full_pipeline_without_models`** — checks `!content_hash.is_empty()` but never validates the hash is deterministically correct (only a separate test does that).
2. **`scorer.rs::test_hits_to_tags_filters_sorts_truncates`** — checks `confidence >= previous` (allows equal) and `len <= 15` but doesn't verify truncation removes the right tags.
3. **`preprocess.rs::test_preprocess_normalization_range`** — only tests pure white (255) and pure black (0) pixels; doesn't verify mid-range (128 → ~0.0).

---

## 5. Summary of Actionable Items

### Priority 1 — Functionality & Correctness

| Item | Type | Effort |
|------|------|--------|
| N1. Log enrichment serialization failures | Fix | Small |
| N2. Validate vocabulary/label_bank size in TagScorer | Fix | Small (3 call sites) |
| M3. Log warning on `--skip-existing` parse failure | Fix | Small |
| M5. Warn when progressive disables relevance | Fix | Small |

### Priority 2 — Refactoring (>500 line files)

| Item | Type | Effort |
|------|------|--------|
| R1. Split `relevance.rs` (804 lines) into submodules | Refactor | Medium |
| R2. Extract tagging coordination from `processor.rs` (603 lines) | Refactor | Medium |
| R3. Extract download logic from `cli/models.rs` (535 lines) | Refactor | Small |

### Priority 3 — Robustness

| Item | Type | Effort |
|------|------|--------|
| M1. Handle WalkDir errors in discovery.rs | Fix | Small |
| M6. Document lock ordering in processor.rs | Fix | Small |
| M8. Log progressive chunk failure summary | Fix | Small |
| N4. Remove or use provider `timeout()` method | Cleanup | Small |
| N5. Replace fragile semaphore permit pattern | Cleanup | Small |
| N3. Tighten WebP validation | Fix | Small |

### Priority 4 — Test Coverage

| Item | Effort |
|------|--------|
| Add processor.rs integration tests (lock poisoning, failure modes) | Medium |
| Add LLM provider unit tests (request building, error mapping) | Medium |
| Add progressive.rs unit tests (seed → background → update cycle) | Medium |
| Strengthen existing weak assertions (hash determinism, normalization range) | Small |

### Not Recommended (Nice-to-Have, Low Value)

These were identified but filtered out as optional improvements:

- Config validation macro to reduce repetition (8 checks → macro) — working code, not worth the abstraction
- Named constants for config defaults — magic numbers are clear in context
- `expect()` instead of `unwrap()` in test code — standard test practice
- More granular `PipelineError` variants for hash/metadata stages — current wrapping is sufficient
- HOME env var fallback warning — edge case on real systems
