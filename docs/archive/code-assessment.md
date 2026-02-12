# Code Assessment — Photon v0.5.1

> Assessed 2026-02-12. 10,226 lines across 2 crates (6,990 photon-core + 2,793 photon CLI + 443 integration tests). 164 tests passing, zero clippy warnings, zero formatting violations.

---

## Executive Summary

The codebase is in **good shape overall** — well-structured workspace, clean separation between library and CLI, comprehensive test coverage (164 tests), and zero linter issues. The main findings are:

- **5 files exceed 500 lines** and need refactoring for maintainability
- **1 bug** in progressive encoding cache that can corrupt the label bank cache file
- **1 minor safety issue** (unwrap on fallible iterator)

**Overall quality: 8/10** — solid foundation with targeted improvements needed.

---

## Table of Contents

1. [Files Requiring Refactoring (>500 lines)](#1-files-requiring-refactoring-500-lines)
2. [Bugs](#2-bugs)
3. [File-by-File Summary](#3-file-by-file-summary)
4. [Recommendations](#4-recommendations)

---

## 1. Files Requiring Refactoring (>500 lines)

| # | File | Lines | Severity | Recommended Split |
|---|------|-------|----------|-------------------|
| 1 | `crates/photon/src/cli/process.rs` | **843** | HIGH | Extract types, enrichment, utilities |
| 2 | `crates/photon-core/src/tagging/relevance.rs` | **670** | MEDIUM | Tests are 55% of file; acceptable if split |
| 3 | `crates/photon-core/src/config.rs` | **667** | MEDIUM | Extract validation + defaults |
| 4 | `crates/photon-core/src/pipeline/processor.rs` | **567** | LOW | Borderline; tagging block is large but cohesive |
| 5 | `crates/photon/src/cli/models.rs` | **544** | LOW | Borderline; tests are ~190 lines of the 544 |

### 1.1 — `process.rs` (843 lines) — HIGH priority

This is the largest file in the codebase and mixes concerns: CLI type definitions, pipeline orchestration, enrichment helpers, progress bar creation, hash loading, summary printing, and tests.

**Current structure:**
```
ProcessArgs (struct + Default impl)     lines 16-160    ~145 lines
OutputFormat, Quality, LlmProvider      lines 82-132    ~50 lines (enums + Display)
ProcessContext (struct)                 lines 162-170   ~8 lines
execute()                              lines 172-190   ~18 lines
setup_processor()                      lines 192-305   ~113 lines
process_single()                       lines 307-362   ~55 lines
process_batch()                        lines 364-574   ~210 lines  ← largest block
run_enrichment_collect()               lines 576-607   ~31 lines
run_enrichment_stdout()                lines 609-639   ~30 lines
create_progress_bar()                  lines 641-656   ~15 lines
load_existing_hashes()                 lines 658-692   ~34 lines
print_summary()                        lines 694-728   ~34 lines
create_enricher()                      lines 730-757   ~27 lines
inject_api_key()                       lines 759-780   ~21 lines
log_enrichment_stats()                 lines 781-787   ~6 lines
tests                                  lines 789-843   ~54 lines
```

**Recommended split:**

| New file | Contents | ~Lines |
|----------|----------|--------|
| `cli/process/mod.rs` | `ProcessArgs`, `ProcessContext`, `execute()`, re-exports | ~200 |
| `cli/process/types.rs` | `OutputFormat`, `Quality`, `LlmProvider` enums + Display impls | ~60 |
| `cli/process/setup.rs` | `setup_processor()`, `create_enricher()`, `inject_api_key()` | ~170 |
| `cli/process/batch.rs` | `process_batch()`, `load_existing_hashes()`, `create_progress_bar()`, `print_summary()` | ~300 |
| `cli/process/enrichment.rs` | `run_enrichment_collect()`, `run_enrichment_stdout()`, `log_enrichment_stats()` | ~70 |

This turns one 843-line file into a `process/` module where no single file exceeds ~300 lines.

### 1.2 — `relevance.rs` (670 lines) — MEDIUM priority

Of the 670 lines, **~365 are tests** (lines 305-670). The production code is ~305 lines, which is well within limits. The test code is comprehensive (19 tests covering all pool transitions).

**Recommendation:** Acceptable as-is. The tests are co-located with the implementation they test, which aids maintainability. Only refactor if the production code grows further.

### 1.3 — `config.rs` (667 lines) — MEDIUM priority

Mixes struct definitions (~250 lines), Default impls (~120 lines), validation (~80 lines), helper methods (~100 lines), and tests (~110 lines).

**Recommended split:**

| New file | Contents | ~Lines |
|----------|----------|--------|
| `config/mod.rs` | `Config` struct, `load()`, `default_path()`, re-exports | ~150 |
| `config/types.rs` | `ProcessingConfig`, `LimitsConfig`, `EmbeddingConfig`, `ThumbnailConfig`, `TaggingConfig`, `RelevanceConfig`, `ProgressiveConfig`, `OutputConfig`, LLM config structs + all Default impls | ~350 |
| `config/validate.rs` | `validate()` + validation tests | ~150 |

### 1.4 — `processor.rs` (567 lines) — LOW priority

Borderline. The tagging block in `process()` (lines 430-504) is ~75 lines of complex match logic, but it's cohesive — the match arms handle different combinations of scorer + tracker + embedding availability. Tests are only ~12 lines.

**Recommendation:** Leave as-is unless it grows past 600 lines.

### 1.5 — `models.rs` (544 lines) — LOW priority

Of the 544 lines, ~190 are tests. The production code is ~350 lines covering model download, checksum verification, and installation. Cohesive and well-structured.

**Recommendation:** Leave as-is.

---

## 2. Bugs

### 2.1 — Progressive encoding saves corrupt cache on chunk failure

**File:** `crates/photon-core/src/tagging/progressive.rs:121-166`
**Severity:** HIGH
**Impact:** Broken cache on next startup requiring manual deletion

**Problem:** When a chunk encoding fails (lines 121-130), the `continue` correctly skips updating `running_bank` and `encoded_indices` for the current session (runtime behavior is correct). However, at the end of `background_encode()` (line 166), the incomplete `running_bank` is saved to disk with the full vocabulary hash.

On next startup, the cache loading logic at `processor.rs:121-125`:
1. `cache_valid()` passes — vocab hash matches (it's the full hash)
2. `LabelBank::load(path, vocabulary.len())` — passes full vocabulary length as `term_count`
3. Size check at `label_bank.rs:162` fails — file has fewer bytes than expected
4. User gets a confusing error: `"Label bank size mismatch: expected X bytes, got Y bytes"`

The cache is now stuck in a broken state. The vocab hash matches so `cache_valid()` always returns true, but `load()` always fails on size. The user must manually delete `~/.photon/taxonomy/label_bank.bin` and `label_bank.meta`.

**Fix:** Track whether all chunks succeeded and only save the cache on full success:
```rust
// In background_encode():
let mut all_succeeded = true;

// In the error handlers (lines 123-130):
all_succeeded = false;
continue;

// At the end (line 166), wrap in:
if all_succeeded {
    if let Err(e) = running_bank.save(&ctx.cache_path, &ctx.vocab_hash) { ... }
}
```

**Note:** This bug only manifests if ONNX text encoding fails mid-batch, which requires either a corrupted model file or a system-level error. Rare in practice, but when it happens it creates a sticky failure state.

### 2.2 — Unsafe unwrap in text encoder

**File:** `crates/photon-core/src/tagging/text_encoder.rs:156`
**Severity:** LOW
**Impact:** Panic if ONNX model returns empty tensor (theoretically possible, practically unlikely)

```rust
pub fn encode(&self, text: &str) -> Result<Vec<f32>, PipelineError> {
    let batch = self.encode_batch(&[text.to_string()])?;
    Ok(batch.into_iter().next().unwrap())  // ← should use ok_or_else
}
```

**Fix:**
```rust
Ok(batch.into_iter().next().ok_or_else(|| PipelineError::Model {
    message: "Text encoder returned empty result for single input".to_string(),
})?)
```

---

## 3. File-by-File Summary

### photon-core (library)

| File | Lines | Status | Notes |
|------|-------|--------|-------|
| `lib.rs` | 44 | Clean | Re-exports only |
| `config.rs` | 667 | **Refactor** | Split into types + validation |
| `error.rs` | 147 | Clean | Well-structured error types with hints |
| `output.rs` | 221 | Clean | OutputWriter with JSON/JSONL support |
| `types.rs` | 295 | Clean | Data types, well-tested |
| `math.rs` | ~30 | Clean | L2 normalize helper |
| **pipeline/** | | | |
| `processor.rs` | 567 | Borderline | Cohesive but long; defer |
| `decode.rs` | ~180 | Clean | Timeout + spawn_blocking pattern |
| `metadata.rs` | ~180 | Clean | EXIF extraction |
| `hash.rs` | ~80 | Clean | BLAKE3 + perceptual hashing |
| `thumbnail.rs` | ~100 | Clean | WebP thumbnail generation |
| `validate.rs` | ~130 | Clean | Format validation |
| `discovery.rs` | ~100 | Clean | File discovery |
| **tagging/** | | | |
| `scorer.rs` | 305 | Clean | Core scoring logic |
| `vocabulary.rs` | 316 | Clean | WordNet vocabulary |
| `label_bank.rs` | 316 | Clean | Embedding matrix cache |
| `relevance.rs` | 670 | **Borderline** | 55% tests; acceptable |
| `hierarchy.rs` | 435 | Clean | O(n^2) dedup, acceptable at max_tags=15 |
| `progressive.rs` | 176 | **Bug** | Cache corruption on chunk failure |
| `text_encoder.rs` | 163 | **Minor** | Unwrap at line 156 |
| `neighbors.rs` | 147 | Clean | WordNet neighbor expansion |
| `seed.rs` | 222 | Clean | Seed term selection |
| **llm/** | | | |
| `provider.rs` | 262 | Clean | Trait + factory |
| `enricher.rs` | 178 | Clean | Concurrent enrichment orchestration |
| `retry.rs` | ~100 | Clean | Well-tested retry classification |
| `anthropic.rs` | ~170 | Clean | Empty response guard present |
| `openai.rs` | ~175 | Clean | Empty choices guard present |
| `ollama.rs` | ~150 | Clean | |
| `hyperbolic.rs` | ~130 | Clean | |
| **embedding/** | | | |
| `siglip.rs` | ~170 | Clean | ONNX inference |
| `preprocess.rs` | ~80 | Clean | Image normalization |

### photon (CLI binary)

| File | Lines | Status | Notes |
|------|-------|--------|-------|
| `main.rs` | 96 | Clean | Entry point with TTY detection |
| `cli/mod.rs` | 6 | Clean | Module declarations |
| `cli/process.rs` | 843 | **Refactor** | Primary refactoring target |
| `cli/models.rs` | 544 | Borderline | ~190 lines are tests; OK |
| `cli/config.rs` | 70 | Clean | |
| `cli/interactive/mod.rs` | 289 | Clean | Menu + config viewer |
| `cli/interactive/theme.rs` | 55 | Clean | |
| `cli/interactive/process.rs` | 282 | Clean | Guided wizard |
| `cli/interactive/models.rs` | 179 | Clean | Model management UI |
| `cli/interactive/setup.rs` | 372 | Clean | LLM setup flow |
| `logging.rs` | 57 | Clean | |

---

## 4. Recommendations

### Priority 1 — Fix the progressive encoding cache bug
- **Effort:** ~10 lines changed
- **Risk:** LOW (isolated change)
- Track chunk success, conditionally save cache

### Priority 2 — Refactor `cli/process.rs` (843 → 5 files)
- **Effort:** ~2 hours (move code, no logic changes)
- **Risk:** LOW (pure structural refactoring)
- Convert to `cli/process/` module directory

### Priority 3 — Fix text_encoder unwrap
- **Effort:** ~3 lines changed
- **Risk:** NONE

### Priority 4 — Refactor `config.rs` (667 → 3 files)
- **Effort:** ~1 hour
- **Risk:** LOW
- Split into `config/mod.rs`, `config/types.rs`, `config/validate.rs`

### Deferred
- `relevance.rs` (670 lines) — acceptable, 55% is tests
- `processor.rs` (567 lines) — borderline, cohesive
- `models.rs` (544 lines) — borderline, 35% is tests
