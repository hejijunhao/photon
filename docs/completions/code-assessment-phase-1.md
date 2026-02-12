# Code Assessment Fix — Phase 1: Progressive Encoding Cache Bug

> Completed: 2026-02-12
> Source plan: `docs/executing/code-assessment-fixes.md`

---

## Summary

Fixed a bug in `background_encode()` where a partial encoding failure (chunk skip via `continue`) would still save an incomplete label bank to disk with the full vocabulary hash — creating a sticky broken state requiring manual cache deletion.

## Bug Details

**File:** `crates/photon-core/src/tagging/progressive.rs`
**Function:** `ProgressiveEncoder::background_encode()`
**Severity:** HIGH — corrupted cache persists across restarts with no self-healing

**Root cause:** The chunk loop has two `continue` branches for error handling (encoding failure and task panic). After the loop, the save block ran unconditionally, writing `vocab_hash` (computed from the full vocabulary) into `label_bank.meta` while the `.bin` file only contained embeddings for successfully-encoded terms. On reload, the hash matched so the cache appeared valid, but the byte count didn't match `vocabulary.len() * 768 * 4`, causing `LabelBank::load()` to fail.

## Changes

**File:** `crates/photon-core/src/tagging/progressive.rs` (~8 lines changed)

1. **Added `all_chunks_succeeded` tracking** (line 110) — boolean flag initialized to `true` before the chunk loop
2. **Set flag to `false` in both error branches** (lines 127, 132) — alongside the existing `continue` statements
3. **Wrapped cache save block in conditional** (lines 161–187) — only saves when all chunks succeeded; logs a warning with encoded/total counts when skipping

## Self-healing behavior

After a partial failure, no cache file is written. On next startup, `cache_valid()` returns `false` (no file exists), so the system falls back to re-encoding from scratch — correct self-healing without manual intervention.

## Verification

- `cargo test -p photon-core` — 123 tests pass
- `cargo test` (integration) — 10 tests pass
- `cargo clippy --workspace -- -D warnings` — zero warnings
- No files outside `progressive.rs` were modified
