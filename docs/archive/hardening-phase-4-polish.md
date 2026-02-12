# Hardening Phase 4 Completion — Polish

> Completed: 2026-02-12

---

## Summary

Replaced recursive `Box::pin()` async with a simple loop in `guided_process()`, eliminating heap allocation and unbounded recursion. Standardized all user-facing MB calculations to SI units (1,000,000 bytes) across both `models.rs` and `process.rs`. Hoisted repeated `Style` allocations to function scope in `interactive/process.rs` and `interactive/setup.rs` for cleaner reads and fewer redundant constructions.

---

## Changes

### Task 4.1: Recursive async → loop

**Problem:** `guided_process()` used `Box::pin(guided_process(config)).await?` to let the user "Process more images" after a run completes. This heap-allocates a pinned future on every iteration and creates theoretically unbounded recursion depth.

**Solution:** Wrapped the entire function body in `loop { ... }` and replaced the recursive call with `break` / continue-to-top-of-loop.

**File:** `crates/photon/src/cli/interactive/process.rs`

| Before | After |
|--------|-------|
| `if matches!(post_choice, Some(0)) { Box::pin(guided_process(config)).await?; }` | `if !matches!(post_choice, Some(0)) { break; }` |

The `theme`, `dim`, `warn`, and `bold` styles are now created once before the loop and reused across iterations instead of being reconstructed on each recursion.

### Task 4.2: Consistent MB calculation

**Problem:** `models.rs` used binary mebibytes (`1024.0 * 1024.0`) for download sizes while `process.rs` used SI megabytes (`1_000_000.0`) for throughput stats — a ~4.86% display difference.

**Solution:** Standardized on SI (`1_000_000.0`) across all user-facing output, since most users interpret "MB" as SI.

**File:** `crates/photon/src/cli/models.rs` — 3 occurrences changed:

| Location | Before | After |
|----------|--------|-------|
| Vision encoder complete log | `file_size as f64 / (1024.0 * 1024.0)` | `file_size as f64 / 1_000_000.0` |
| Text encoder complete log | `file_size as f64 / (1024.0 * 1024.0)` | `file_size as f64 / 1_000_000.0` |
| Download progress size log | `size as f64 / (1024.0 * 1024.0)` | `size as f64 / 1_000_000.0` |

### Task 4.3: Reduce redundant Style allocations

**Problem:** `Style::new().for_stderr().dim()` and `.yellow()` were constructed multiple times within the same function scope.

**Files and changes:**

| File | Before | After |
|------|--------|-------|
| `interactive/process.rs` | `dim` created at lines 61 and 178; `warn` created at lines 37, 49, 75; `bold` at line 177 | All three hoisted to function top (lines 20–22), reused throughout |
| `interactive/setup.rs` | `dim` created at lines 55 and 292; `warn` at lines 61 and 88 | `dim` and `warn` hoisted to function top of `select_llm_provider()` |

Note: `prompt_output_path()` retains its own inline `warn` since it's a separate function scope.

---

## Verification

| Check | Result |
|-------|--------|
| `cargo test --workspace` | 164 tests passing (31 CLI + 123 core + 10 integration) |
| `cargo clippy --workspace -- -D warnings` | Pass (0 warnings) |
| `cargo fmt --all -- --check` | Pass |
