# Phase 3 Completion — Main Menu Loop

> Completed: 2026-02-12

---

## Summary

Bare `photon` now shows a banner followed by an interactive arrow-key menu with 4 options. Selecting "Exit" or pressing Esc/Ctrl+C quits cleanly. The other 3 options dispatch to their stub modules and return to the menu. All `#[allow(unused)]` annotations from Phases 1–2 removed since everything is now wired.

---

## Changes

### Task 3.1: Main menu implementation

**File:** `crates/photon/src/cli/interactive/mod.rs` (rewritten)

#### Structure

- `MENU_ITEMS` const — 4-option string slice: "Process images", "Download / manage models", "Configure settings", "Exit"
- `run(config)` — banner → theme → loop { Select → dispatch }
- `show_config(config)` — placeholder for Phase 9 settings viewer

#### Menu loop logic

```rust
let selection = Select::with_theme(&theme)
    .with_prompt("What would you like to do?")
    .items(MENU_ITEMS)
    .default(0)
    .interact_opt()?;
```

**Key design choice: `interact_opt()` instead of `interact()`.**

`interact()` returns `io::Result<usize>` — on Esc/Ctrl+C it returns an `Err`, which would propagate up as an error. `interact_opt()` returns `io::Result<Option<usize>>` — Esc/Ctrl+C returns `Ok(None)`, which we handle as a clean exit alongside the explicit "Exit" option (`Some(3)`). This avoids needing a separate error-matching helper for interrupt handling.

#### Dispatch

| Selection | Action |
|-----------|--------|
| `Some(0)` | `process::guided_process(config).await?` |
| `Some(1)` | `models::guided_models(config).await?` |
| `Some(2)` | `show_config(config)` |
| `Some(3)` / `None` | Break loop (clean exit) |

### Cleanup: removed `#[allow(unused)]` annotations

| File | Annotation removed | Reason |
|------|--------------------|--------|
| `interactive/process.rs` | `#[allow(unused)]` on `guided_process` | Now called from menu dispatch |
| `interactive/models.rs` | `#[allow(unused)]` on `guided_models` | Now called from menu dispatch |
| `interactive/theme.rs` | `#[allow(unused)]` on `photon_theme` | Now called in `run()` |

---

## Verification

| Check | Result |
|-------|--------|
| `cargo clippy --workspace -- -D warnings` | Pass (0 warnings) |
| `cargo fmt --all -- --check` | Pass |
| `cargo test --workspace` | 136 tests passing (unchanged) |

---

## What's Next

Phase 4: Prerequisite Refactors — add `Default` for `ProcessArgs`, extract reusable model functions from `cli::models`, add `InstalledModels` struct. These refactors enable Phases 5–7.
