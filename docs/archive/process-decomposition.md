# process.rs Decomposition

> Addresses assessment issue #4: `execute()` complexity in `crates/photon/src/cli/process.rs`.

## Summary

Decomposed the monolithic `execute()` function from ~475 lines to 16 lines of pure orchestration. Extracted 5 focused helper functions, consolidating 5 duplicated enrichment blocks into 2 reusable functions. Pure refactor with zero behavior changes. 136 tests passing, zero clippy warnings.

## What changed

### File modified

`crates/photon/src/cli/process.rs` — 722 lines -> 726 lines (+4 net)

### New struct

| Struct | Lines | Purpose |
|--------|-------|---------|
| `ProcessContext` | 130–138 | Bundles processor, options, enricher, config, format, and LLM flag — replaces scattered locals |

### New functions

| Function | Lines | Extracted from | Purpose |
|----------|-------|---------------|---------|
| `setup_processor()` | 161–271 | `execute()` lines 132–241 | Input validation, config loading, quality preset, model loading, options + enricher creation |
| `process_single()` | 276–328 | `execute()` lines 245–327 | Single-file processing with LLM/no-LLM and file/stdout branching |
| `process_batch()` | 333–542 | `execute()` lines 328–594 | Batch processing: skip-existing, progress bar, streaming, post-loop output |
| `run_enrichment_collect()` | 549–577 | 3 duplicated spawn+channel blocks | Spawns enricher task, collects patches via mpsc channel, returns `Vec<OutputRecord>` |
| `run_enrichment_stdout()` | 582–609 | 2 duplicated inline-callback blocks | Runs enricher with callback that prints patches to stdout (pretty or compact) |

### Imports moved to module level

Imports previously scoped inside `execute()` (`photon_core::*`, `std::fs::File`, `std::io::BufWriter`) moved to module level since they're now used across multiple functions.

## Why

The original `execute()` managed a 2x2x2 matrix of concerns:

| Dimension | Options |
|-----------|---------|
| Input mode | Single file vs. batch directory |
| LLM mode | Enrichment enabled vs. disabled |
| Output target | File vs. stdout |

This produced deeply nested conditionals with near-identical enrichment code duplicated 5 times. Each copy created a channel, spawned an enricher task, collected results, and wrote output — the same pattern with slight variations.

The decomposition extracts by **responsibility** rather than by branch:
- `setup_processor()` — configuration concern
- `process_single()` / `process_batch()` — input mode concern
- `run_enrichment_collect()` / `run_enrichment_stdout()` — enrichment concern

## Metrics

| Metric | Before | After |
|--------|--------|-------|
| `execute()` length | ~475 lines | 16 lines |
| Enrichment code copies | 5 | 2 (`_collect` + `_stdout`) |
| Total file length | 722 lines | 726 lines |
| Test count | 136 | 136 (unchanged) |
| Clippy warnings | 0 | 0 |

## Testing

- `cargo test` — 136 tests passing (3 CLI + 123 core + 10 integration)
- `cargo clippy -- -D warnings` — zero warnings
- `cargo fmt --check` — clean

## Unchanged functions

These helper functions at the bottom of the file were not modified:

- `create_progress_bar()` (lines 614–628)
- `load_existing_hashes()` (lines 631–664)
- `print_summary()` (lines 667–700)
- `create_enricher()` (lines 703–722)
- `log_enrichment_stats()` (lines 724–730)
