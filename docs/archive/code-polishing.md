# Code Polishing — Completion Log

**Date:** 2026-02-11
**Scope:** All open items from the code assessment report (`docs/executing/code-assessment.md`)
**Baseline:** 120 tests passing, zero clippy warnings
**Final:** 118 tests passing (−2 from removed dead code), zero clippy warnings, zero compiler warnings

---

## Item #1 — CLAUDE.md Data Directory Layout (High Priority)

**File:** `CLAUDE.md`
**Problem:** The Data Directory Layout section documented `text_model.onnx` and `tokenizer.json` inside the variant subdirectory (`siglip-base-patch16/`), but the code consistently stores and loads them from the models root (`~/.photon/models/`). The text encoder is shared across visual model variants (224 and 384), so it lives at the root — not inside any variant directory.

**Fix:** Moved `text_model.onnx` and `tokenizer.json` to the root level in the documented layout, with comments clarifying they are shared resources. Also corrected the stale test count (50 → 120+).

**Verified against:**
- `models.rs:119` — downloads to `model_dir.join("text_model.onnx")`
- `text_encoder.rs:28-29` — loads from `model_dir.join("text_model.onnx")`
- `text_encoder.rs:161` — existence check at `model_dir.join("text_model.onnx")`

---

## Item #3 — Eliminate Hot-Path Clone in `hits_to_tags` (High Priority)

**File:** `crates/photon-core/src/tagging/scorer.rs`
**Problem:** `score_with_pools()` called `self.hits_to_tags(all_hits.clone())` because it needed to both convert hits to tags *and* return the raw hits to the caller. Since `hits_to_tags` took `Vec<(usize, f32)>` by value, the clone was required — but this ran on every image against a potentially large hit vector.

**Fix:** Changed `hits_to_tags` signature from `Vec<(usize, f32)>` to `&[(usize, f32)]`. The function only needs to iterate and filter — it never needs ownership. This eliminated the `.clone()` in `score_with_pools` and the implicit move in `score`. Updated `.into_iter()` to `.iter()` and added dereferences for the now-reference destructured values (`*idx`, `*confidence`).

**Impact:** Eliminates one allocation per image on the scoring hot path. With a 68K vocabulary, the cloned vector could be several hundred KB per image in worst case.

**Tests:** All 5 scorer tests pass (sigmoid, monotonic, hits_to_tags, score_pool, score_with_pools).

---

## Item #4 — Precomputed Parent Index in Neighbor Lookups (Medium Priority)

**File:** `crates/photon-core/src/tagging/neighbors.rs`
**Problem:** `find_siblings()` did a linear scan of all ~68K vocabulary terms for each promoted term. When `expand_all()` was called with K promoted terms, this was O(N×K) — scanning the entire vocabulary K times. The `Vocabulary` already provides `build_parent_index()` which builds a `HashMap<String, Vec<usize>>` from parent → children, but `NeighborExpander` wasn't using it.

**Fix:** Introduced `find_siblings_indexed()` (private) that takes a precomputed parent index and does O(1) HashMap lookup instead of O(N) scan. `expand_all()` now builds the index once and uses it for all K promoted terms — total cost O(N + K) instead of O(N×K). The public `find_siblings()` still works for single-term lookups by building the index internally.

**Tests:** All 6 neighbor tests pass (shared_parent, excludes_self, no_hypernyms, different_parent, expand_all_deduplicates, expand_all_excludes_promoted).

---

## Item #5 — Replace `mem::forget` with Proper TempDir Lifetime (Medium Priority)

**Files:** `crates/photon-core/src/tagging/scorer.rs`, `crates/photon-core/src/tagging/neighbors.rs`
**Problem:** Test helpers used `std::mem::forget(dir)` to prevent `tempfile::TempDir` from being dropped (which would delete the directory while the `Vocabulary` still referenced it). This leaked temporary directories on every test run — they were never cleaned up.

**Fix:** Changed test helpers to return `TempDir` alongside their results:
- `scorer.rs`: `test_scorer()` returns `(TagScorer, Vec<f32>, TempDir)` — callers bind the third element as `_dir`
- `neighbors.rs`: `test_vocab()` returns `(Vocabulary, TempDir)` — callers bind the second element as `_dir`

The `_dir` binding keeps the `TempDir` alive for the test's duration. When the test function returns, `_dir` is dropped normally, which cleans up the temporary directory.

**Tests:** All 11 tests across both files pass (5 scorer + 6 neighbor).

---

## Item #7 — Deduplicate Enricher Creation in `process.rs` (Medium Priority)

**File:** `crates/photon/src/cli/process.rs`
**Problem:** `create_enricher(&args, &config)?` was called at four separate sites inside mutually exclusive branches (single-file-to-file, single-file-to-stdout, batch-to-file, batch-to-stdout). Each call reconstructed the HTTP client and resolved environment variables. Although only one branch executes per run, the code duplication made the control flow harder to follow and any change to enricher setup needed updating in four places.

**Fix:** Moved enricher creation to a single call before the branching logic: `let mut enricher = if llm_enabled { create_enricher(...) } else { None }`. Each branch now uses `enricher.take()` to consume the pre-built enricher. This is safe because only one branch ever executes — `take()` returns `Some` on first use and `None` thereafter, which aligns with the mutually exclusive branch structure.

**Note:** The callback patterns (channel-based for file output, direct println for stdout) remain separate — they're fundamentally different I/O strategies, and forcing them into a single abstraction would increase complexity without benefit.

**Tests:** Full workspace — 120 tests passing, zero warnings.

---

## Item #8 — Remove Dead `quality` Config Field (Low Priority)

**Files:** `crates/photon-core/src/config.rs`, `crates/photon-core/src/pipeline/thumbnail.rs`
**Problem:** `ThumbnailConfig.quality` was defined (default: 80) but never passed to the WebP encoder. Investigation confirmed this isn't fixable: the `image` crate v0.25's `WebPEncoder` only supports **lossless** encoding — there is no quality parameter available. The field was genuinely dead.

**Fix:** Removed `quality: u8` from `ThumbnailConfig` struct, its default value, and all three test constructor sites. The field was not referenced by any production code, CLI arguments, or benchmarks.

**Note:** If lossy WebP with quality control is needed in the future, it would require adding the `webp` crate (libwebp bindings) as a dependency — the pure-Rust `image` crate doesn't support it.

**Tests:** All 3 thumbnail tests pass.

---

## Item #9 — Remove Dead `device` Config Field (Low Priority)

**Files:** `crates/photon-core/src/config.rs`, `crates/photon/src/cli/process.rs`
**Problem:** `EmbeddingConfig.device` was defined (default: `"cpu"`) but never referenced by the embedding pipeline. ONNX Runtime selects execution providers automatically at build time based on available hardware — the field had no effect.

**Fix:** Removed `device: String` from `EmbeddingConfig` struct, its default value, and the one reference in `process.rs` that copied it into a temporary struct for `model_exists()` (which also never used it).

**Note:** If explicit device selection is needed in the future (e.g., forcing CPU on a Metal-capable system), it would need to be wired into the `ort::SessionBuilder` execution provider configuration.

**Tests:** Compiles cleanly, no tests directly affected.

---

## Item #10 — Remove Unused `PipelineStage` / `bounded_channel` (Low Priority)

**Files:** `crates/photon-core/src/pipeline/channel.rs` (deleted), `crates/photon-core/src/pipeline/mod.rs`
**Problem:** `PipelineStage` and `bounded_channel` were defined and tested but never used by any production code. The pipeline currently uses a sequential loop in `process.rs`, not channel-based stages. This was pre-built infrastructure for a future parallel pipeline that was never implemented.

**Fix:** Deleted `channel.rs` entirely and removed its `pub mod channel` declaration and doc comment from `mod.rs`. This removes 116 lines of dead code and 2 tests.

**Impact:** Test count drops from 120 → 118 (the two channel tests were the only consumers).

---

## Item #11 — Reuse `reqwest::Client` Across Download Calls (Low Priority)

**File:** `crates/photon/src/cli/models.rs`
**Problem:** `download_file()` created a new `reqwest::Client::new()` on every call. During `photon models download`, this function is called up to 3 times (vision model, text encoder, tokenizer), creating 3 separate clients with 3 separate connection pools. Since all downloads go to the same HuggingFace host, reusing a single client enables HTTP connection pooling.

**Fix:** Added `client: &reqwest::Client` parameter to `download_file()`. The client is created once at the top of the download command and passed to all three download calls. The LLM providers (Anthropic, Ollama, OpenAI) already create their client once in their constructors and reuse it across calls, so those were not changed.

**Tests:** Compiles cleanly, no functional tests affected.

---

## Summary

| # | Priority | Action | Files Changed |
|---|----------|--------|---------------|
| 1 | High | Fixed CLAUDE.md directory layout for text encoder | `CLAUDE.md` |
| 3 | High | Eliminated hot-path `.clone()` via `&[]` signature | `scorer.rs` |
| 4 | Medium | Precomputed parent index for O(K) sibling lookups | `neighbors.rs` |
| 5 | Medium | Replaced `mem::forget` with proper `TempDir` lifetime | `scorer.rs`, `neighbors.rs` |
| 7 | Medium | Deduplicated enricher creation (4× → 1×) | `process.rs` |
| 8 | Low | Removed dead `quality` config (WebP encoder doesn't support it) | `config.rs`, `thumbnail.rs` |
| 9 | Low | Removed dead `device` config (ONNX Runtime ignores it) | `config.rs`, `process.rs` |
| 10 | Low | Removed unused `PipelineStage` / `bounded_channel` (116 LOC) | `channel.rs` (deleted), `mod.rs` |
| 11 | Low | Reused `reqwest::Client` across download calls | `models.rs` |

**Net effect:** −120 lines of dead code, 0 new allocations on hot path, O(N×K) → O(N+K) sibling lookups, no leaked tempdirs, cleaner control flow in CLI executor.
