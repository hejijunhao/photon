# Hardening Phase 3 Completion — UX Edge Cases

> Completed: 2026-02-12

---

## Summary

Added output path validation with overwrite confirmation to `prompt_output_path()`. Replaced both `unreachable!()` match arms with safe fallbacks to prevent panics. Added empty/whitespace guards to Ollama and Hyperbolic model name inputs. One additional clippy fix applied.

---

## Changes

### Task 3.1: Output path validation

**File:** `crates/photon/src/cli/interactive/process.rs` — `prompt_output_path()`

The function body was wrapped in a `loop` with two new validation checks:

1. **Parent directory check**: After tilde expansion, if the parent directory doesn't exist, a yellow warning is printed and the user is re-prompted.
   ```
   ⚠ Directory does not exist: /nonexistent/path
   ```

2. **Overwrite confirmation**: If the output file already exists, a `Confirm` dialog asks whether to overwrite (default: `false`). If declined, the user is re-prompted for a different path.
   ```
   ? ./results.jsonl already exists. Overwrite? (y/N)
   ```

The `Confirm` import was already present in the file (used by the processing confirmation step).

### Task 3.2: Replaced `unreachable!()` with safe fallbacks

**File:** `crates/photon/src/cli/interactive/mod.rs`

| Location | Before | After | Effect |
|----------|--------|-------|--------|
| `run()` main menu match (line 55) | `_ => unreachable!()` | `_ => {}` | Ignores unexpected index, re-shows menu |
| `show_config()` config menu match (line 162) | `_ => unreachable!()` | `_ => break` | Treats unexpected index as "Back" |

These are functionally equivalent in normal operation (dialoguer won't return out-of-bounds indices) but prevent a panic if assumptions are ever violated.

### Task 3.3: Empty model name guards

**File:** `crates/photon/src/cli/interactive/setup.rs` — `select_model()`

Both `LlmProvider::Ollama` and `LlmProvider::Hyperbolic` arms changed from returning the raw `handle_interrupt()` result to filtering through a `match`:

```rust
match model {
    Some(m) if !m.trim().is_empty() => Ok(Some(m)),
    _ => Ok(None),
}
```

This ensures whitespace-only input is treated as cancellation (`Ok(None)`), preventing an empty model name from reaching `ProcessArgs`.

### Additional: Clippy fix

**File:** `crates/photon/src/cli/interactive/setup.rs` — `save_key_to_config()`

Changed `.map_or(false, |t| t.contains_key(section_name))` to `.is_some_and(|t| t.contains_key(section_name))` to satisfy clippy's `unnecessary_map_or` lint.

---

## Verification

| Check | Result |
|-------|--------|
| `cargo check -p photon` | Pass |
| `cargo test --workspace` | 164 tests passing |
| `cargo clippy --workspace -- -D warnings` | Pass (0 warnings) |
| `cargo fmt --all -- --check` | Pass |
