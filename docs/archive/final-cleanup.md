# Final Cleanup

> Context: Tasks 1, 3–6 from `pre-push-cleanup.md` are complete. Task 2 (correct `assessment-structure.md`) was not done. This plan addresses that gap plus remaining improvements before pushing.

---

## Phase 1 — Fix `assessment-structure.md` (blocking)

**Why:** The document was flagged as containing fabricated claims in `pending-issues.md`. Tasks 4–6 made Phases 2–4 accurate, but Phase 1 (processor.rs decomposition) was never implemented. The document still describes it as complete. This is the original blocking issue that prompted the cleanup.

**What to fix:**

| Section | Current claim | Reality | Action |
|---------|--------------|---------|--------|
| Summary (line 10) | "Executed all 4 phases" | Phase 1 was not executed | Change to "Executed Phases 2–4" |
| Summary table (line 14) | Phase 1: "Split `processor.rs` into focused modules, 4 files" | No split happened; orphaned files were deleted | Remove Phase 1 row, add note that it was descoped |
| Metrics (line 26) | `processor.rs` lines: 559 → **282** | Still 561 lines | Remove this row or change to "561 (unchanged)" |
| Phase 1 section (lines 34–83) | Entire section describes completed decomposition | Never happened | Replace with a note: "Descoped — orphaned `scoring.rs` and `tagging_loader.rs` files were deleted (see `pre-push-cleanup.md` Task 1). Decomposition deferred as a future improvement." |
| Phase 3 (line 105) | MockProvider uses "Configurable response queue (`Vec<Result<...>>`)" | Actual implementation uses factory function `Box<dyn Fn(u32) -> Result<...>>` | Correct to match `task-5-enricher-tests.md` |
| Phase 3 (lines 122–124) | Design decisions describe `Arc<AtomicU32>` for "with_responses()" | Actual constructors are `success()`, `failing()`, `fail_then_succeed()` | Update to match actual API |
| Metrics "before" (line 25) | Before: 174 tests | Before Tasks 4–6 the count was 185 (post-correctness-fixes) | Correct baseline to 185, or clarify which baseline is used |
| Total new tests (line 19) | 17 new tests | Phase 1 contributed 0 tests, Phases 2–4 contributed 17 — but 4 were pre-existing compilation fixes, and 10 more came from Tasks 5+6 after this doc | Reconcile with actual progression: 185 → 195 (+10 from Phases 2–4 of this doc, or clarify timeline) |

**Verification:** Read the corrected document end-to-end and confirm every factual claim against the codebase.

---

## Phase 2 — Decompose `processor.rs` (561 → ~3 files)

**Why:** `processor.rs` is the largest file in the codebase at 561 lines. It mixes pipeline orchestration with tagging initialization (~200 lines) and pool-aware scoring logic (~60 lines). The original `pre-push-cleanup.md` identified natural split points. This was the work Phase 1 of `assessment-structure.md` described but never implemented.

**Approach:** Extract two focused files from `processor.rs`, each containing a separate `impl ImageProcessor` block (Rust allows multiple `impl` blocks for the same type across files in the same crate). Zero logic changes — pure structural refactoring.

### Task 2a — Extract `tagging_loader.rs`

Create `crates/photon-core/src/pipeline/tagging_loader.rs` containing 4 methods:

| Method | Description |
|--------|-------------|
| `load_tagging(&mut self, config: &Config)` | Async tagging init with 3 paths (cached / progressive / blocking) |
| `load_tagging_blocking(...)` | Synchronous fallback encoding |
| `load_relevance_tracker(...)` | Loads or creates the three-pool relevance tracker |
| `save_relevance(&self, config: &Config)` | Persists relevance state to disk |

Add `pub(crate) mod tagging_loader;` to `pipeline/mod.rs`.

### Task 2b — Extract `scoring.rs`

Create `crates/photon-core/src/pipeline/scoring.rs` containing a free function:

```rust
pub(crate) fn score_with_relevance(
    scorer_lock: &RwLock<TagScorer>,
    tracker_lock: &RwLock<RelevanceTracker>,
    embedding: &[f32],
    sweep_interval: u64,
    neighbor_expansion: bool,
) -> Vec<Tag>
```

This encapsulates the pool-aware scoring logic currently inlined in `process_with_options()`. Replace the ~60-line inline block with a call to this function.

Add `pub(crate) mod scoring;` to `pipeline/mod.rs`.

### Task 2c — Adjust `processor.rs` field visibility

Change `ImageProcessor` fields accessed by the new `impl` blocks to `pub(crate)`:
- `config` fields used by `tagging_loader.rs`
- `scorer`, `relevance_tracker` used by both files

**Verification:** `cargo test --workspace` — count stays at 195. `cargo clippy --workspace -- -D warnings` stays clean. Zero logic changes.

---

## Phase 3 — Harden enricher.rs triple-unwrap

**Why:** `enricher.rs:327` has three chained `.unwrap()` calls:

```rust
let results = Arc::try_unwrap(results).unwrap().into_inner().unwrap();
```

These are safe by construction (single `Arc` owner after `join_all`, non-poisoned `Mutex`), but provide no diagnostic context if assumptions are violated. Every other lock operation in the codebase uses `.expect()`.

**Fix:** Replace with:

```rust
let results = Arc::try_unwrap(results)
    .expect("enricher: Arc should have single owner after join_all")
    .into_inner()
    .expect("enricher: results Mutex should not be poisoned");
```

**Verification:** `cargo clippy --workspace -- -D warnings` stays clean.

---

## Phase 4 — Update `assessment-structure.md` with Phase 2 results

**Why:** After Phase 2 completes the decomposition, `assessment-structure.md` Phase 1 section can be updated to accurately describe the completed work instead of just a "descoped" note.

**Action:** Update the Phase 1 section and metrics table to reflect the actual decomposition. Verify line counts match reality.

**Note:** If Phase 2 is skipped or deferred, this phase becomes "leave the descoped note from Phase 1 as-is" — no further action needed.

---

## Execution order

```
Phase 1  (fix doc)          — no code deps, unblocks push confidence
Phase 2a (tagging_loader)   — independent extraction
Phase 2b (scoring)          — independent extraction, can parallel with 2a
Phase 2c (field visibility) — depends on 2a + 2b
Phase 3  (enricher unwrap)  — independent, can parallel with Phase 2
Phase 4  (update doc)       — depends on Phase 2 completion
```

**Minimum viable push:** Phase 1 alone (fixes the documentation integrity issue).
**Recommended push:** Phases 1–3 (fixes doc + completes decomposition + hardens unwraps).
**Full completion:** Phases 1–4 (everything reconciled).
