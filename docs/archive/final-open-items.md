# Final Open Items — Implementation Plan

> Addresses the two remaining issues from the code assessment (`docs/executing/finish-testing.md`).

---

## Overview

| # | Item | Assessment Issue | Priority | Effort | Status |
|---|------|-----------------|----------|--------|--------|
| **1** | process.rs decomposition | #4 (process.rs complexity) | Medium | Medium | Partially addressed (v0.4.16) |
| **2** | `ort` RC → stable upgrade | #7 (`ort` RC dependency) | Medium | Small | Blocked (external) |

---

## 1. process.rs Decomposition

**Assessment reference:** Issue #4 — "`execute()` handles single-file vs batch, LLM vs non-LLM, file vs stdout, JSON vs JSONL — all in deeply nested conditionals."

### Problem

`process.rs` is 721 lines. The `execute()` function alone spans lines 122–597 (~475 lines). It manages a 2×2×2 matrix of concerns:

| Dimension | Options |
|-----------|---------|
| Input mode | Single file vs. batch directory |
| LLM mode | Enrichment enabled vs. disabled |
| Output target | File vs. stdout |
| Output format | JSON vs. JSONL |

This produces **deeply nested conditional branches** with near-identical enrichment blocks duplicated 4 times (single-file/file, single-file/stdout, batch/streaming-file, batch/non-streaming). Each copy creates an `mpsc::channel`, spawns an enricher task, awaits it, and writes results — the same pattern with slight variations.

The v0.4.16 streaming refactor improved the batch path (cleaner `if stream_to_file` branching) but the overall structure remains hard to follow.

### Goal

Reduce `execute()` from ~475 lines to ~100 lines of orchestration, with clearly named helpers for each concern. No behavior changes — pure refactor.

### Approach: Extract by concern, not by branch

Instead of extracting per-branch (`process_single_llm_file`, `process_batch_no_llm_stdout`, etc.), extract **by responsibility**:

#### Step 1: Extract `setup_processor()` → lines 122–235

Pull config overrides, model loading, and option construction into a helper that returns the configured processor, options, and enricher:

```rust
struct ProcessContext {
    processor: ImageProcessor,
    options: ProcessOptions,
    enricher: Option<Enricher>,
    output_format: CoreOutputFormat,
    llm_enabled: bool,
}

fn setup_processor(args: &ProcessArgs) -> anyhow::Result<ProcessContext> {
    // Input validation, config loading, quality preset,
    // model loading, options construction, enricher creation
    // ...
}
```

**Lines saved:** ~110 lines from `execute()`.

#### Step 2: Extract `process_single()` → lines 245–327

```rust
async fn process_single(
    ctx: ProcessContext,
    args: &ProcessArgs,
) -> anyhow::Result<()>
```

Handles the single-file path. Internally still branches on LLM/output-target, but the logic is self-contained (~80 lines).

**Lines saved:** ~80 lines from `execute()`.

#### Step 3: Extract `process_batch()` → lines 328–594

```rust
async fn process_batch(
    ctx: ProcessContext,
    args: &ProcessArgs,
    files: Vec<DiscoveredFile>,
) -> anyhow::Result<()>
```

Handles the batch path including progress bar, skip-existing, and post-loop output.

**Lines saved:** ~265 lines from `execute()`.

#### Step 4: Consolidate enrichment into `run_enrichment()`

The biggest duplication is the enrichment pattern. All 4 copies follow:

```
create channel → spawn enricher task → await → collect results → write
```

Extract into one function:

```rust
/// Run LLM enrichment and return output records.
async fn run_enrichment(
    enricher: Enricher,
    results: Vec<ProcessedImage>,
) -> anyhow::Result<(Vec<OutputRecord>, usize, usize)> {
    let (tx, rx) = std::sync::mpsc::channel::<OutputRecord>();

    let handle = tokio::spawn(async move {
        enricher.enrich_batch(&results, move |enrich_result| match enrich_result {
            EnrichResult::Success(patch) => {
                let _ = tx.send(OutputRecord::Enrichment(patch));
            }
            EnrichResult::Failure(path, msg) => {
                tracing::error!("Enrichment failed: {path:?} - {msg}");
            }
        }).await
    });

    let (enriched, failed) = handle.await?;
    let records: Vec<OutputRecord> = rx.try_iter().collect();
    Ok((records, enriched, failed))
}
```

This replaces ~30 lines at each of the 4 call sites with a single function call (~5 lines each). Net saving: ~100 lines of duplicated enrichment code.

### Resulting structure

```rust
pub async fn execute(args: ProcessArgs) -> anyhow::Result<()> {
    let ctx = setup_processor(&args)?;

    let files = ctx.processor.discover(&args.input);
    if files.is_empty() {
        tracing::warn!("No supported image files found at {:?}", args.input);
        return Ok(());
    }
    tracing::info!("Found {} image(s) to process", files.len());

    if args.input.is_file() {
        process_single(ctx, &args).await
    } else {
        process_batch(ctx, &args, files).await
    }
}
```

`execute()` becomes ~15 lines of pure orchestration.

### Implementation steps

| Step | What | Estimated lines |
|------|------|----------------|
| 1 | Extract `setup_processor()` returning `ProcessContext` | ~120 lines in helper |
| 2 | Extract `run_enrichment()` consolidating 4 duplicated blocks | ~25 lines in helper |
| 3 | Extract `process_single()` using `run_enrichment()` | ~60 lines in helper |
| 4 | Extract `process_batch()` using `run_enrichment()` | ~200 lines in helper |
| 5 | Slim down `execute()` to orchestration | ~15 lines |
| 6 | Run full test suite — zero behavior changes | — |

### Testing

- **All 136 tests must pass unchanged** — this is a pure refactor with no behavior changes
- Manual verification of all output combinations:
  - Single file: JSON stdout, JSON file, JSONL stdout, JSONL file
  - Batch: JSON stdout, JSON file, JSONL stdout, JSONL file
  - Each of the above with and without `--llm`
- `cargo clippy -D warnings` clean

### Acceptance criteria

- `execute()` is ≤100 lines
- Enrichment logic exists in exactly one place (`run_enrichment()`)
- Zero behavior changes — output byte-identical for all flag combinations
- 136 tests pass, zero clippy warnings

### Interaction with Interactive CLI

The Interactive CLI (item D in `remaining-improvements.md`) also touches `process.rs` — it needs to construct `ProcessArgs` programmatically. This decomposition makes that easier: the interactive module can call `setup_processor()` + `process_single()`/`process_batch()` directly, rather than building a `ProcessArgs` and routing through `execute()`. Consider doing this refactor **before** the Interactive CLI.

---

## 2. `ort` RC → Stable Upgrade

**Assessment reference:** Issue #7 — "`ort` 2.0.0-rc.11 is a release candidate, not a stable release."

### Current state

- **Pinned version:** `ort = "2.0.0-rc.11"` in `crates/photon-core/Cargo.toml`
- **Latest upstream:** v2.0.0-rc.11 (January 7, 2025) — still the most recent release
- **Maintainer signal:** rc.11 release notes state "the next big release of ort should be, finally, 2.0.0" with a call for API feedback
- **No stable 2.0.0 yet** as of February 2026

### Photon's `ort` API surface (narrow)

Photon uses a small subset of `ort`'s API, which limits upgrade risk:

| API used | Location | Purpose |
|----------|----------|---------|
| `Session::builder()` | `siglip.rs:27`, `text_encoder.rs:49` | Create ONNX session |
| `.with_intra_threads(1)` | `siglip.rs:29`, `text_encoder.rs:51` | Single-thread inference |
| `.commit_from_file()` | `siglip.rs:32`, `text_encoder.rs:53` | Load model from disk |
| `Value::from_array((shape, data))` | `siglip.rs:72`, `text_encoder.rs:116` | Create input tensor |
| `session.run(inputs)` | `siglip.rs:84`, `text_encoder.rs:123` | Run inference |
| `ort::inputs![]` | `siglip.rs:77`, `text_encoder.rs:123` | Macro for named inputs |
| `ort::session::Session` | Both files | Type import |
| `ort::value::Value` | Both files | Type import |

**Total:** 2 files, ~12 API call sites. This is a minimal surface area — no custom execution providers, no training APIs, no advanced session options.

### Risk assessment

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| API rename (`Session::builder` → something else) | Low | Low | Small API surface, easy to grep-and-replace |
| `Value::from_array` signature change | Medium | Low | Already using the `(shape, data)` tuple pattern that ort recommends |
| `ort::inputs!` macro change | Low | Low | 2 call sites |
| ONNX Runtime version bump breaks model loading | Very low | Medium | Models are standard ONNX format, broadly compatible |
| Build system change (download behavior) | Medium | Low | Already works; may need Cargo feature flag changes |

**Overall risk: Low.** The narrow API surface means even a major refactor in `ort` would only require changes in 2 files (~12 lines).

### Action plan

This is a **monitor-and-upgrade** item, not an active development task.

#### Tracking

1. **Watch the repo** — [github.com/pykeio/ort](https://github.com/pykeio/ort) releases
2. **Check periodically** — `cargo outdated -p ort` or check [crates.io/crates/ort](https://crates.io/crates/ort)
3. **Subscribe to rc.12+ / 2.0.0 release notifications** via GitHub Watch → Releases

#### When stable ships

1. **Update version** in `crates/photon-core/Cargo.toml`:
   ```toml
   ort = "2.0"  # Remove RC pin
   ```

2. **Build and fix** any compile errors (expect minimal changes given narrow API surface):
   ```bash
   cargo build -p photon-core 2>&1 | head -50
   ```

3. **Run inference tests** to verify model loading and output correctness:
   ```bash
   cargo test -p photon-core
   # If models installed, also verify real inference:
   cargo run -- process tests/fixtures/images/beach.jpg
   ```

4. **Check for new features** that could benefit Photon:
   - Metal execution provider improvements (Apple Silicon perf)
   - Session caching or warm-start APIs
   - Better error types (could simplify our error mapping)

5. **Update CLAUDE.md** — change `ort v2.0.0-rc.11` reference to stable version

#### Interim: is this actually a problem?

No. The rc.11 release is production-quality — it bundles ONNX Runtime v1.23.2 (stable upstream), works correctly on aarch64/Metal, and has been the latest release for over a year. The "RC" label reflects API stability guarantees, not code quality. Photon's pin to `= "2.0.0-rc.11"` means `cargo update` won't accidentally pull a breaking RC. The only real risk is if a security fix ships in rc.12+ and we need to update — but ort's security surface is minimal (it loads local model files, no network).

### Acceptance criteria

- When `ort` 2.0.0 stable ships: version bumped, all 136+ tests pass, inference verified
- Until then: no action required, tracked as external dependency

---

## Recommended execution order

```
1. process.rs decomposition     (active work, unblocks Interactive CLI)
    ↓
2. ort upgrade                  (passive — upgrade when stable ships)
    ↓
D. Interactive CLI              (from remaining-improvements.md, builds on decomposed process.rs)
```

Doing (1) before the Interactive CLI is strongly recommended — the decomposition creates the clean function boundaries that interactive mode needs to call into.

---

## Metrics after completion

| Metric | Current | After item 1 | After item 2 |
|--------|---------|-------------|-------------|
| `execute()` length | ~475 lines | ~15 lines | ~15 lines |
| Enrichment code copies | 4 | 1 | 1 |
| Assessment issues open | 2 | 1 (ort only) | 0 |
| `ort` version | 2.0.0-rc.11 | 2.0.0-rc.11 | 2.0.0 (when available) |
| Assessment score | 8.5/10 | ~9/10 | ~9/10 |
