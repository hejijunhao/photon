# Phase 1 Completion — Foundation: Dependencies, Scaffold, Entry Point

> Completed: 2026-02-12

---

## Summary

Bare `photon` (no subcommand) now compiles, routes to a placeholder interactive module on TTY, and exits cleanly. All existing commands (`process`, `models`, `config`) are unchanged. Two new dependencies added, five new files created, three existing files modified.

---

## Changes

### Task 1.1: Dependencies added

**File:** `crates/photon/Cargo.toml`

Added:
```toml
dialoguer = "0.11"
console = "0.15"
```

`dialoguer` provides terminal prompts (Select, Input, Confirm, Password) used throughout the interactive flow. `console` is its companion for styled terminal output (colors, bold, etc.) — also a transitive dependency of `dialoguer`, but declared explicitly since we'll use it directly in the theme module.

### Task 1.2: Interactive module scaffold

**New files:**

| File | Purpose | Status |
|------|---------|--------|
| `crates/photon/src/cli/interactive/mod.rs` | Entry point (`pub async fn run()`) + sub-module declarations | Active — called from `main.rs` |
| `crates/photon/src/cli/interactive/process.rs` | Guided process flow stub (`guided_process()`) | Stub — wired in Phase 6 |
| `crates/photon/src/cli/interactive/models.rs` | Guided model management stub (`guided_models()`) | Stub — wired in Phase 5 |
| `crates/photon/src/cli/interactive/setup.rs` | LLM provider setup (empty) | Stub — implemented in Phase 7 |
| `crates/photon/src/cli/interactive/theme.rs` | Custom dialoguer theme (empty) | Stub — implemented in Phase 2 |

**Modified file:** `crates/photon/src/cli/mod.rs` — added `pub mod interactive;`

Stub functions use `#[allow(unused)]` annotations to pass `clippy -D warnings` until they're wired into the menu in later phases.

### Task 1.3: Optional command routing

**File:** `crates/photon/src/main.rs`

| Change | Detail |
|--------|--------|
| Import added | `use std::io::IsTerminal;` |
| `Cli.command` type | `Commands` → `Option<Commands>` |
| Match arms | Wrapped existing arms in `Some(...)` |
| `None` branch | TTY → `cli::interactive::run(&config).await`; non-TTY → `Cli::parse_from(["photon", "--help"])` |

**Why `IsTerminal` instead of a crate?** `std::io::IsTerminal` is stable since Rust 1.70 (we target stable Rust). No extra dependency needed for TTY detection. This ensures `photon` invoked in a pipe (e.g., `echo "test" | photon`) shows help instead of hanging on interactive prompts.

---

## Verification

| Check | Result |
|-------|--------|
| `cargo check -p photon` | Pass |
| `cargo clippy --workspace -- -D warnings` | Pass (0 warnings) |
| `cargo fmt --all -- --check` | Pass |
| `cargo test --workspace` | 136 tests passing (3 CLI + 123 core + 10 integration) |

---

## What's Next

Phase 2: Theme and Banner — implement `PhotonTheme` and `print_banner()` in `interactive/theme.rs`.
