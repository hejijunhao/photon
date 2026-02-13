# Code Assessment — 2026-02-13

Comprehensive code assessment across the full Photon repository (55 Rust files, ~11.4K lines). Assessed for functional correctness, accuracy, maintainability, and clean code.

**Test suite**: 205 tests passing, zero clippy warnings, zero formatting violations.

---

## Files Exceeding 500 Lines

| File | Lines | Recommendation |
|------|------:|----------------|
| `tagging/relevance.rs` | 754 | Extract tests (~400 lines) into `tests/relevance.rs`; consider extracting pool transition logic into `pool.rs` |
| `tests/integration.rs` | 680 | Split by domain: `tests/pipeline.rs`, `tests/boundaries.rs`, `tests/output.rs` |
| `llm/enricher.rs` | 615 | Extract tests (~435 lines) into `tests/enricher.rs`; core logic is only ~180 lines |
| `pipeline/processor.rs` | 561 | Extract tagging orchestration (scoring + sweep + neighbor expansion, ~150 lines) into `pipeline/tagging.rs` |
| `cli/models.rs` | 535 | Extract download logic (`download_file`, `verify_blake3`, `download_vision`, `download_shared`) into `cli/models/download.rs` |

---

## Findings

### F1 — Double timeout in LLM enricher + providers

**Severity**: HIGH
**Files**: `llm/enricher.rs:149-152`, `llm/anthropic.rs:123`, `llm/openai.rs:132`, `llm/ollama.rs:90`

The enricher wraps every `provider.generate()` call in `tokio::time::timeout(options.timeout_ms)`, but each provider implementation *also* applies `.timeout(self.timeout())` on the reqwest HTTP client. This creates two competing timeouts:

- Enricher default: 60s (`config.limits.llm_timeout_ms`)
- Provider default: 60s (anthropic/openai), 120s (ollama)

When the inner provider timeout fires first (e.g. ollama at 120s > enricher at 60s, the enricher timeout wins — making the provider timeout unreachable). Conversely, if a user raises `llm_timeout_ms` above the provider timeout, the provider timeout silently caps the actual duration.

**Recommendation**: Remove `.timeout()` from provider HTTP calls. Let the enricher's `tokio::time::timeout` be the single source of truth — it already handles the `Err(_)` timeout case with retry logic.

---

### F2 — Progressive encoder scorer-vocabulary mismatch risk

**Severity**: HIGH
**Files**: `tagging/progressive.rs:155-160`

In the background encoding loop, if `running_bank.append(&chunk_bank)` fails at line 155, execution continues to the next chunk *without* extending `encoded_indices`. On the next *successful* append, a new `TagScorer` is created with a vocabulary built from `encoded_indices` that is now out of sync with the actual `running_bank` contents. This could cause an index-out-of-bounds panic during scoring.

The `all_chunks_succeeded` flag prevents corrupted cache writes to disk, but the in-memory scorer in `scorer_slot` can briefly hold a misaligned vocabulary/label_bank pair.

**Recommendation**: On append failure, either (a) abort the entire progressive encoding pass, or (b) skip the scorer update for that chunk entirely so the existing scorer remains consistent.

---

### F3 — Enricher semaphore closure silently drops images

**Severity**: MEDIUM
**Files**: `llm/enricher.rs:74-79`

```rust
let permit = semaphore.clone().acquire_owned().await;
if permit.is_err() {
    break; // Semaphore closed
}
let permit = permit.unwrap();
```

If the semaphore closes mid-batch (e.g. due to a task panic), the loop silently `break`s — remaining images are never enriched and no warning is logged. The caller only sees the count of completed images, with no indication that a subset was silently skipped.

Additionally, the `permit.unwrap()` on line 79 is safe (guarded by the `is_err()` check) but fragile — an `if let Ok(permit)` or `let Ok(permit) = ... else { break }` pattern would be clearer and more idiomatic.

**Recommendation**: Log a warning when the semaphore closes unexpectedly, and refactor to `let Ok(permit) = ... else { ... }`.

---

### F4 — Unbounded channel in CLI enrichment helper

**Severity**: MEDIUM
**Files**: `cli/process/enrichment.rs:14`

`run_enrichment_collect` creates an unbounded `std::sync::mpsc::channel`. For large batch operations (thousands of images) with slow LLM providers, all enrichment patches accumulate in memory before being written. This effectively negates the streaming architecture for file-targeted enrichment.

**Recommendation**: Use a bounded channel (e.g. `std::sync::mpsc::sync_channel(64)`) to apply backpressure, or stream patches to the file incrementally as they arrive.

---

### F5 — Silent JSON parsing failure in `--skip-existing`

**Severity**: MEDIUM
**Files**: `cli/process/batch.rs:149-156`

When loading existing records for `--skip-existing` with JSON format, deserialization failure falls through silently — the hash set remains empty and all images are reprocessed. No warning is logged to tell the user their existing output file wasn't loaded.

**Recommendation**: Log a warning when deserialization of the existing output file fails, so users know their `--skip-existing` flag had no effect.

---

### F6 — Missing path context in SigLIP embedding errors

**Severity**: MEDIUM
**Files**: `embedding/siglip.rs:72-87`

When creating ONNX input tensors or on inference failure, error context uses `path: Default::default()` — producing an empty `PathBuf`. When multiple images are processed concurrently, this makes it impossible to correlate errors to specific files.

**Recommendation**: Either pass the image path into `SigLipSession::infer()` for error context, or handle the error at the caller (`processor.rs`) where the path is available — which already happens for the timeout/panic cases but not for the inner ONNX error.

---

### F7 — LabelBank progress logging reports wrong count

**Severity**: LOW
**Files**: `tagging/label_bank.rs:104-105`

The progress log calculates `encoded = (batch_idx + 1) * batch_size`, which overshoots for the final partial batch. For 5,005 terms with `batch_size=5000`, it logs "10,000 terms encoded" when only 5,005 were processed. Data integrity is unaffected — the final count at line 114 uses actual matrix length.

**Recommendation**: Calculate from actual matrix length: `matrix.len() / embedding_dim`.

---

### F8 — Timeout error messages lack attempt context

**Severity**: LOW
**Files**: `llm/enricher.rs:171`

When `tokio::time::timeout` fires, the error message is `"Timeout after 60000ms"` on every attempt. With retries, the user sees the identical message 3 times with no indication of which attempt they're on.

**Recommendation**: Include attempt number: `format!("Timeout after {}ms (attempt {}/{})", ...)`.

---

## Cross-Cutting Observations

| Check | Result |
|-------|--------|
| `unwrap()` in non-test code | None found (clean) |
| `expect()` in non-test code | None found (clean) |
| `todo!()` / `unimplemented!()` | None found |
| `unsafe` blocks | None found |
| Clippy warnings | Zero |
| Formatting violations | Zero |

The codebase has excellent hygiene — all panicking calls are confined to test code.

---

## Summary

| Severity | Count | Findings |
|----------|------:|----------|
| HIGH | 2 | F1 (double timeout), F2 (progressive encoder mismatch) |
| MEDIUM | 4 | F3 (semaphore drop), F4 (unbounded channel), F5 (silent parse fail), F6 (missing path) |
| LOW | 2 | F7 (progress logging), F8 (timeout message) |
| **Refactor** | **5** | Files over 500 lines (see table above) |

**Overall assessment**: The codebase is well-structured with strong error handling, comprehensive test coverage, and zero clippy/fmt issues. The two HIGH findings are real correctness risks that should be addressed — F1 creates confusing timeout behavior and F2 can cause panics during progressive encoding. The MEDIUM findings are robustness gaps that won't cause crashes in normal operation but degrade the user experience when things go wrong.
