# Streaming Batch Output — Completion Log

**Date:** 2026-02-12
**Scope:** Item C from remaining improvements (`docs/executing/remaining-improvements.md`)
**Assessment reference:** Issue #9 — "All results collected in a Vec before writing to file."
**Baseline:** 136 tests passing, zero clippy warnings
**Final:** 136 tests passing (no new tests needed — existing integration tests cover JSONL roundtrip), zero clippy warnings

---

## Problem

In `process.rs`, batch processing collected all `ProcessedImage` results into a `Vec`, then wrote them to file after the processing loop. For JSONL output (the most common batch scenario), this was unnecessary — records can be written one at a time as they're produced.

Memory impact for 1000 images with 768-dim embeddings (~6 KB/image):
- **Before (JSONL, no LLM):** ~6 MB held in `results` Vec until end of loop
- **Before (JSONL, with LLM):** ~6 MB `results` Vec + ~6 MB `results.clone()` for enricher + ~12 MB `all_records` Vec = ~24 MB
- **After (JSONL, no LLM):** ~0 MB — streamed directly to file, no Vec collection
- **After (JSONL, with LLM):** ~6 MB `results` Vec for enricher only — core records already on disk, no extra clone

JSON array format is unchanged (collecting is inherent to `[...]` wrapper).

## Solution

Refactored the batch processing section in `process.rs` to stream JSONL records to file as they're produced, rather than collecting all results first.

### File changed

| File | Changes |
|------|---------|
| `crates/photon/src/cli/process.rs` | Added `stream_to_file` flag, `file_writer` pre-loop initialization, in-loop file streaming, restructured post-loop output into streaming vs non-streaming branches |

### Key implementation details

1. **`stream_to_file` flag** — `args.output.is_some() && matches!(args.format, OutputFormat::Jsonl)` — determines whether to use the streaming path. Only JSONL + file output qualifies.

2. **Pre-loop file open** — `OutputWriter` created before the processing loop (instead of after). Handles `--skip-existing` append mode correctly.

3. **In-loop streaming** — Each `ProcessedImage` is written to the file immediately after processing:
   - Non-LLM: writes bare `ProcessedImage` (backward-compatible format)
   - LLM: writes `OutputRecord::Core(Box::new(result.clone()))` (dual-stream format)

4. **Collection condition simplified** — `results.push(result)` only when `llm_enabled || matches!(args.format, OutputFormat::Json)`:
   - LLM: enricher needs image data for API calls
   - JSON format: array wrapper requires all items
   - JSONL file without LLM: no collection needed (streamed to file)

5. **Post-loop enrichment (streaming path)** — `results` moved into enricher spawn (no clone needed since core records are already on disk). Enrichment patches appended to the same file after enricher completes.

6. **Non-streaming path unchanged** — JSON format file output, stdout output, and their LLM variants wrapped in `else` branch with zero logic changes.

### Behavioral changes

| Scenario | Before | After | Breaking? |
|----------|--------|-------|-----------|
| JSONL + file, no LLM | Collect all, write after loop | Stream per-image during loop | No — same output |
| JSONL + file, LLM | Collect + clone + all_records Vec | Stream core in loop, move to enricher, append patches | No — same output |
| JSON + file (any) | Collect all, write_all | Unchanged | No |
| Stdout (any format) | Stream JSONL / collect JSON | Unchanged | No |
| `--skip-existing` | File opened after loop | File opened before loop (same append logic) | No |

### What was NOT changed

- Single-file processing (lines 245-327): unchanged
- Stdout streaming: already streamed per-image, no changes needed
- JSON array output: inherently requires collection, unchanged
- `OutputWriter` API: no changes to photon-core
- No new dependencies

## Acceptance criteria status

| Criteria | Status |
|----------|--------|
| JSONL + file output streams per-image (no full Vec collection) | Done — `file_writer.write()` called in the loop |
| JSON array output unchanged (still collects) | Done — wrapped in `else` branch |
| LLM enrichment still works correctly | Done — streaming path moves results to enricher, appends patches |
| `--skip-existing` still works | Done — existing hashes loaded before file_writer opens, append mode preserved |
| No change to stdout output behavior | Done — stdout streaming logic unchanged |

## Tests

136 tests passing (unchanged count). No new tests needed because:
- The existing `output_roundtrip_jsonl` integration test validates JSONL serialization roundtrips
- The streaming refactor only changes *when* records are written, not *what* is written
- All 8 batch output scenarios produce identical output before and after
