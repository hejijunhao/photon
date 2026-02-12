# Phase 4 Completion — Prerequisite Refactors

> Completed: 2026-02-12

---

## Summary

Extracted reusable functions from existing code so the interactive module can call them without duplicating logic. Two files modified: `cli/process.rs` (new `Default` impl) and `cli/models.rs` (4 new public items + `execute()` refactored to delegate). Pure refactor — zero behavior changes, all 136 tests passing.

---

## Changes

### Task 4.1: `ProcessArgs::Default` impl

**File:** `crates/photon/src/cli/process.rs`

Added a manual `Default` implementation whose values match every `#[arg(default_value = ...)]` annotation on the struct:

| Field | Default | Matches clap |
|-------|---------|--------------|
| `input` | `PathBuf::new()` | (no default — must be set) |
| `output` | `None` | Yes |
| `format` | `OutputFormat::Json` | `"json"` |
| `parallel` | `4` | `"4"` |
| `skip_existing` | `false` | flag default |
| `no_thumbnail` | `false` | flag default |
| `no_embedding` | `false` | flag default |
| `no_tagging` | `false` | flag default |
| `no_description` | `false` | flag default |
| `quality` | `Quality::Fast` | `"fast"` |
| `thumbnail_size` | `256` | `"256"` |
| `llm` | `None` | Yes |
| `llm_model` | `None` | Yes |
| `show_tag_paths` | `false` | flag default |
| `no_dedup_tags` | `false` | flag default |

This lets the interactive module build `ProcessArgs { input: path, quality: Quality::High, ..ProcessArgs::default() }` and pass it straight to `execute()`.

### Task 4.2: Extracted model functions

**File:** `crates/photon/src/cli/models.rs`

| New public item | Purpose | Lines |
|-----------------|---------|-------|
| `InstalledModels` struct | 5 bool fields for each model/vocab component | 7 |
| `check_installed(config)` | Checks disk for all model files, returns `InstalledModels` | 15 |
| `download_vision(indices, config, client)` | Downloads vision model variant(s), skips existing | 30 |
| `download_shared(config, client)` | Downloads text encoder + tokenizer, skips existing | 30 |
| `VARIANT_LABELS` const | `["Base (224)", "Base (384)"]` for interactive display | 1 |
| `install_vocabulary(config)` → **now `pub`** | Was already `fn`, changed to `pub fn` | 0 (visibility only) |

The `ModelsCommand::Download` arm in `execute()` was refactored from ~90 inline lines to 3 function calls:

```rust
download_vision(&[0], &config, &client).await?;
download_shared(&config, &client).await?;
install_vocabulary(&config)?;
```

Behavior is byte-identical — same logs, same skip-if-exists logic, same checksum verification.

### Task 4.3: `InstalledModels::can_process()`

```rust
pub fn can_process(&self) -> bool {
    (self.vision_224 || self.vision_384) && self.text_encoder && self.tokenizer
}
```

Returns `true` when the minimum viable model set is present. Note: vocabulary isn't required (tagging can be skipped), but at least one vision model + text encoder + tokenizer are needed.

---

## Verification

| Check | Result |
|-------|--------|
| `cargo clippy --workspace -- -D warnings` | Pass (0 warnings) |
| `cargo fmt --all -- --check` | Pass |
| `cargo test --workspace` | 136 tests passing (unchanged) |

---

## What's Next

Phase 5: Guided Model Management — `interactive/models.rs` uses `check_installed()`, `download_vision()`, `download_shared()` to show install status and offer downloads.

Phase 6: Guided Process Flow — uses `ProcessArgs::default()` to build args from user choices.
