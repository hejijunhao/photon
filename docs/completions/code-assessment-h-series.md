# Code Assessment — HIGH Severity Fixes

> Completed: 2026-02-12
> Source plan: `docs/plans/code-assessment.md` (H1, H2, H3)

---

## Summary

Reviewed and fixed the three HIGH-severity issues from the code assessment. H1 (race condition) and H3 (mixed format output) confirmed as genuinely HIGH. H2 (integer division) downgraded to LOW but fixed anyway as a one-liner. All fixes verified: 164 tests passing, zero clippy warnings, zero format violations.

| Issue | Assessment | Verdict | Fix |
|-------|-----------|---------|-----|
| H1: Progressive encoder race condition | HIGH | **HIGH — confirmed** | Install seed scorer before `tokio::spawn` |
| H2: File size validation integer division | HIGH | **LOW — downgraded** | Compare raw bytes instead of truncated MB |
| H3: Batch JSON + LLM mixed stdout | HIGH | **HIGH — confirmed** | Collect enrichment before emitting combined JSON array |

---

## H1: Progressive Encoder Race Condition

**Files modified:**
- `crates/photon-core/src/tagging/progressive.rs` — `start()` return type and scorer installation
- `crates/photon-core/src/pipeline/processor.rs` — removed caller-side scorer installation

**Root cause:** `processor.rs:168` initialized `scorer_slot` with an empty scorer, then called `ProgressiveEncoder::start()` which spawned a background task before returning. The caller installed the seed scorer into the slot *after* `start()` returned (`processor.rs:191-196`). The background task's first action (`progressive.rs:105-108`) was to read from `scorer_slot` to clone the running label bank. On a multi-threaded tokio runtime, the background task could read the empty scorer before the caller installed the seed.

**Impact if triggered:** `running_bank` starts empty while `encoded_indices` includes seed indices — vocabulary/bank dimension mismatch. Subsequent scorer swaps built from this corrupt state. The broken label bank gets saved to the persistent cache, requiring manual deletion of `~/.photon/taxonomy/label_bank.*` to recover.

**Fix:** Moved seed scorer installation into `start()`, *before* `tokio::spawn`. Changed return type from `Result<TagScorer, PipelineError>` to `Result<(), PipelineError>` since the scorer is now installed internally. Removed the redundant caller-side installation in `processor.rs`. The background task now always reads the seed scorer (not an empty one) when cloning the running bank.

**Why the assessment was correct:** The race window is real. On a multi-threaded runtime, `tokio::spawn` can execute the new task on a different OS thread *immediately* — the spawning thread doesn't get priority. The assessment's mitigating note ("spawn_blocking adds latency") was misleading because the background task reads `scorer_slot` *before* entering any `spawn_blocking` block.

---

## H2: File Size Validation Integer Division

**Files modified:**
- `crates/photon-core/src/pipeline/validate.rs` — comparison logic (1 line)

**Root cause:** `metadata.len() / (1024 * 1024)` truncates — a 100.99 MB file computes as 100 MB and passes a 100 MB limit.

**Fix:** Changed from `size_mb > limit` (integer-divided) to `metadata.len() > limit * 1024 * 1024` (exact byte comparison). The `size_mb` value in the error message still uses integer division (display only).

**Why downgraded to LOW:** The practical impact is at most ~1 MB overshoot on a 100 MB default limit. The edge case of `max_file_size_mb = 0` is already guarded by config validation (`validate.rs:20` rejects zero values). No real-world user would notice this. Still fixed because the fix is trivial and correctness matters.

---

## H3: Mixed JSON/Enrichment Stdout Output

**Files modified:**
- `crates/photon/src/cli/process/batch.rs` — batch JSON+LLM+stdout path, enrichment streaming guard
- `crates/photon/src/cli/process/mod.rs` — single-file LLM output (both file and stdout)

**Root cause (batch):** For batch + JSON format + LLM + stdout, `batch.rs:184-188` printed core records as a JSON array, then `batch.rs:196-199` separately printed enrichment patches as individual JSON objects via `run_enrichment_stdout`. Result: a JSON array followed by loose JSON objects — not valid JSON, not valid JSONL. Unparseable by any consumer.

**Root cause (single-file):** `mod.rs:163` called `writer.write(&core_record)` followed by `writer.write(record)` for each enrichment patch. For JSON format, `OutputWriter::write()` emits individual JSON objects (not array elements), producing concatenated objects — invalid JSON. The stdout path had the same issue: separate `println!` calls for core and enrichment.

**Fix (batch):** In the JSON+LLM+stdout block, the enricher is now consumed via `run_enrichment_collect()`, patches are appended to the core records vector, and a single combined JSON array is emitted. The separate enrichment streaming block is now guarded with `matches!(args.format, OutputFormat::Jsonl)` so it only fires for JSONL (where per-line streaming is correct).

**Fix (single-file, file output):** Replaced sequential `writer.write()` calls with a single `writer.write_all(&all_records)` after collecting both core and enrichment records. `write_all()` produces a proper JSON array for JSON format, or one-per-line for JSONL.

**Fix (single-file, stdout):** Added a `match args.format` dispatch:
- `Json`: collects enrichment via `run_enrichment_collect`, emits combined array via `serde_json::to_string_pretty`
- `Jsonl`: streams core record, then enrichment patches via `run_enrichment_stdout` (existing behavior, now correctly scoped)

**Why the assessment was correct:** Any pipeline consuming JSON stdout output would receive unparseable data. The dual-stream design (core first, enrichment later) is correct for JSONL (each line is self-contained) but fundamentally incompatible with JSON format (which requires a single top-level value). The fix preserves dual-stream for JSONL while collecting for JSON.

---

## Verification

- `cargo check --workspace` — compiles cleanly
- `cargo clippy --workspace -- -D warnings` — zero warnings
- `cargo fmt --all -- --check` — zero violations
- `cargo test --workspace` — 164 tests passing (31 CLI + 123 core + 10 integration)
