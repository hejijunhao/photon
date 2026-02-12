# Hardening Phase 2 Completion — Safety & Robustness

> Completed: 2026-02-12

---

## Summary

Eliminated both `unsafe set_var` calls by threading the API key through the type system. Replaced `toml` with `toml_edit` to preserve config file comments during API key saves. Made download errors graceful — failures now display a red error and return to the model menu instead of crashing the interactive session. Zero `unsafe` blocks remaining in the interactive module.

---

## Changes

### Task 2.1: Removed `unsafe set_var` — API key through types

**Problem:** `setup.rs` used `unsafe { std::env::set_var(...) }` on two code paths. This is unsound in tokio's multi-threaded runtime (deprecated as safe since Rust 1.83).

**Solution:** Thread the API key through `LlmSelection` → `ProcessArgs` → `create_enricher()` via config injection.

**Files changed:**

| File | Change |
|------|--------|
| `interactive/setup.rs` | Added `api_key: Option<String>` to `LlmSelection`. Both `set_var` calls replaced with `session_api_key = Some(key)`. Returned via `LlmSelection.api_key`. |
| `interactive/process.rs` | Extracts `api_key` from `LlmSelection` before destructuring. Passes to `ProcessArgs.api_key`. |
| `cli/process.rs` | Added `#[arg(skip)] api_key: Option<String>` to `ProcessArgs` + `Default`. Added `inject_api_key()` helper that sets the key on the appropriate provider config section. `create_enricher()` now clones `config.llm`, injects the key, and passes the modified config to `LlmProviderFactory::create()`. |

**Why `inject_api_key()` instead of `set_var()`:** The API key flows through the type system — `LlmSelection.api_key` → `ProcessArgs.api_key` → `config.llm.<provider>.api_key` — never touching global process state. This is sound in any threading model.

### Task 2.2: Graceful download error handling

**Problem:** Download calls in `interactive/models.rs` used `?` to propagate errors, killing the entire interactive session on network failure.

**Solution:** Wrapped all three download actions in `match` blocks.

**File:** `crates/photon/src/cli/interactive/models.rs`

| Action | Before | After |
|--------|--------|-------|
| `DownloadVision` | `download_vision(...).await?; download_shared(...).await?; install_vocabulary(...)?;` | `async { ... }.await` → `match Ok/Err` with red error message |
| `DownloadShared` | `download_shared(...).await?;` | `match download_result { Ok/Err }` with red error message |
| `InstallVocabulary` | `install_vocabulary(...)?;` | `match install_vocabulary(...) { Ok/Err }` with red error message |

On failure, the user sees `✗ Download failed: <error>` and returns to the model menu.

### Task 2.3: Config comment preservation with `toml_edit`

**Problem:** `save_key_to_config()` used `toml::Table` which strips all comments and formatting during round-trip.

**Solution:** Replaced with `toml_edit::DocumentMut` which preserves comments, whitespace, and key ordering.

| File | Change |
|------|--------|
| `Cargo.toml` | Replaced `toml.workspace = true` with `toml_edit = "0.22"` (`toml` was only used in `setup.rs`) |
| `interactive/setup.rs` | `save_key_to_config()` rewritten: `toml::Table` → `toml_edit::DocumentMut`, `toml::Value` → `toml_edit::value()` |

---

## Verification

| Check | Result |
|-------|--------|
| `cargo test --workspace` | 164 tests passing |
| `cargo clippy --workspace -- -D warnings` | Pass (0 warnings) |
| `cargo fmt --all -- --check` | Pass |
| `grep -r "unsafe" interactive/` | 0 matches — zero `unsafe` blocks remaining |
