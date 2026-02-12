# Interactive CLI Hardening — 7/10 → 9.5/10

> Implementation plan to address all issues identified in the post-implementation assessment.
> Builds on the completed interactive CLI (10 phases in `interactive-cli-plan.md`).

---

## Assessment Recap

| Dimension | Current | Target | Primary lever |
|-----------|---------|--------|---------------|
| Test coverage | 4/10 | 9/10 | Unit tests for pure functions, plan-promised tests |
| Robustness | 6/10 | 9/10 | Eliminate `unsafe set_var`, graceful download errors, preserve config comments |
| Edge case handling | 7/10 | 9/10 | Output path validation, defensive matches |
| Code quality | 8/10 | 9.5/10 | Remove recursive async, reduce allocations, consistent units |

**Estimated new/changed code:** ~250 lines of tests + ~120 lines of fixes across 7 files
**New dependencies:** `toml_edit 0.22` (replaces `toml` for config round-tripping)
**Breaking changes:** None

---

## Execution Status

| Phase | Status | Notes |
|-------|--------|-------|
| 1. Test Coverage | **Done** | 21 new tests, 164 total. See `docs/completions/hardening-phase-1-tests.md` |
| 2. Safety & Robustness | **Done** | Zero `unsafe`, `toml_edit`, graceful downloads. See `docs/completions/hardening-phase-2-safety.md` |
| 3. UX Edge Cases | **Done** | Path validation, overwrite confirm, defensive matches. See `docs/completions/hardening-phase-3-ux.md` |
| 4. Polish | **Done** | Recursive async → loop, consistent SI MB, style dedup. See below. |

---

## Phase 1: Test Coverage

**Goal:** Cover all pure functions in the interactive module with unit tests. Deliver the tests promised in the original plan's testing strategy but never implemented.

### Task 1.1: Unit tests for `handle_interrupt()`

**File:** `crates/photon/src/cli/interactive/mod.rs`

Add `#[cfg(test)] mod tests` at the bottom of `mod.rs` with:

```rust
#[test]
fn handle_interrupt_ok_returns_some() {
    let result: dialoguer::Result<usize> = Ok(42);
    assert_eq!(handle_interrupt(result).unwrap(), Some(42));
}

#[test]
fn handle_interrupt_interrupted_returns_none() {
    let err = dialoguer::Error::IO(std::io::Error::new(
        std::io::ErrorKind::Interrupted,
        "interrupted",
    ));
    let result: dialoguer::Result<usize> = Err(err);
    assert!(handle_interrupt(result).unwrap().is_none());
}

#[test]
fn handle_interrupt_other_io_error_propagates() {
    let err = dialoguer::Error::IO(std::io::Error::new(
        std::io::ErrorKind::BrokenPipe,
        "broken pipe",
    ));
    let result: dialoguer::Result<usize> = Err(err);
    assert!(handle_interrupt(result).is_err());
}
```

### Task 1.2: Unit tests for `llm_summary()`

**File:** `crates/photon/src/cli/interactive/mod.rs` (same test module)

Tests:
- Config with no LLM providers → `"none configured"`
- Config with Anthropic enabled → `"Anthropic"`
- Config with Anthropic + Ollama enabled → `"Ollama, Anthropic"` (order matches check order)
- Config with provider present but `enabled: false` → `"none configured"`

Requires constructing `Config` with specific LLM settings. Use `Config::default()` and modify fields.

### Task 1.3: Unit tests for `config_has_key()`

**File:** `crates/photon/src/cli/interactive/setup.rs`

Make `config_has_key` `pub(super)` (visible to sibling test modules) or add tests inline.

Tests:
- Anthropic with real key → `true`
- Anthropic with empty key → `false`
- Anthropic with `${ANTHROPIC_API_KEY}` placeholder → `false`
- Anthropic with `None` section → `false`
- Ollama (no key needed) → `true`

### Task 1.4: Unit tests for `env_var_for()` and `provider_label()`

**File:** `crates/photon/src/cli/interactive/setup.rs`

Simple mapping tests — verify each provider maps to the correct env var name and label. These prevent regressions if providers are added or reordered.

### Task 1.5: Unit tests for `InstalledModels::can_process()`

**File:** `crates/photon/src/cli/models.rs`

This was explicitly listed in the original plan's testing strategy (Phase 4) but never written.

Tests:
- Both vision models + text + tokenizer → `true`
- Only 224 + text + tokenizer → `true`
- Only 384 + text + tokenizer → `true`
- No vision models → `false`
- Vision but no text encoder → `false`
- Vision but no tokenizer → `false`
- All false → `false`

### Task 1.6: Unit test for `ProcessArgs::default()`

**File:** `crates/photon/src/cli/process.rs`

Also explicitly listed in the original plan's testing strategy (Phase 4) but never written.

Test that each field of `ProcessArgs::default()` matches the clap `#[arg(default_value = ...)]` annotations. This prevents drift if someone changes a clap default without updating `Default`.

```rust
#[test]
fn process_args_default_matches_clap_defaults() {
    let args = ProcessArgs::default();
    assert_eq!(args.parallel, 4);
    assert!(matches!(args.format, OutputFormat::Json));
    assert!(matches!(args.quality, Quality::Fast));
    assert_eq!(args.thumbnail_size, 256);
    assert!(!args.skip_existing);
    assert!(!args.no_thumbnail);
    assert!(!args.no_embedding);
    assert!(!args.no_tagging);
    assert!(!args.no_description);
    assert!(args.llm.is_none());
    assert!(args.llm_model.is_none());
    assert!(args.output.is_none());
    assert!(!args.show_tag_paths);
    assert!(!args.no_dedup_tags);
}
```

### Task 1.7: Verify

- `cargo test --workspace` — all existing tests pass + new tests pass
- `cargo clippy --workspace -- -D warnings` — clean

---

## Phase 2: Safety & Robustness

**Goal:** Eliminate the `unsafe set_var`, handle download errors gracefully, and preserve config comments during API key saves.

### Task 2.1: Remove `unsafe set_var` — thread API key through `ProcessArgs`

**Files:**
- `crates/photon/src/cli/process.rs` — add `api_key: Option<String>` field to `ProcessArgs` + `Default`
- `crates/photon/src/cli/interactive/setup.rs` — return the key in `LlmSelection` instead of calling `set_var`
- `crates/photon/src/cli/interactive/process.rs` — pass key from `LlmSelection` into `ProcessArgs`

**Changes:**

1. Add to `ProcessArgs`:
   ```rust
   /// API key for the selected LLM provider (session-only, not persisted).
   /// Used by interactive mode to pass keys without mutating env vars.
   #[arg(skip)]
   pub api_key: Option<String>,
   ```
   `#[arg(skip)]` means clap ignores this field — it's only set programmatically.

2. Add `api_key: Option<String>` to `LlmSelection` struct.

3. In `setup.rs`, replace both `unsafe { std::env::set_var(...) }` calls with storing the key in `LlmSelection.api_key`.

4. In `process.rs` guided flow, pass the key:
   ```rust
   let args = ProcessArgs {
       input,
       output,
       format,
       quality,
       llm,
       llm_model,
       api_key: llm_selection.and_then(|s| s.api_key),
       ..ProcessArgs::default()
   };
   ```

5. In `cli/process.rs` `create_enricher()`, if `args.api_key` is set, inject it into the provider config before creating the factory. The cleanest approach: temporarily set the env var *only* within `create_enricher()` scope using a guard pattern:
   ```rust
   // Inside create_enricher, before LlmProviderFactory::create:
   let _env_guard = args.api_key.as_ref().map(|key| {
       let var = match &args.llm {
           Some(LlmProvider::Anthropic) => "ANTHROPIC_API_KEY",
           Some(LlmProvider::Openai) => "OPENAI_API_KEY",
           Some(LlmProvider::Hyperbolic) => "HYPERBOLIC_API_KEY",
           _ => return None,
       };
       // SAFETY: This runs synchronously before any concurrent LLM calls.
       // The env var is set, factory reads it, then guard restores the original.
       let old = std::env::var(var).ok();
       unsafe { std::env::set_var(var, key) };
       Some((var, old))
   }).flatten();
   ```

   Actually, the better approach is to check if `LlmProviderFactory::create()` can accept an explicit API key parameter. If the factory reads from config/env, we should modify the config object to inject the key directly:
   ```rust
   if let Some(ref key) = args.api_key {
       inject_api_key(&mut config.llm, &provider_name, key);
   }
   ```

   This avoids `unsafe` entirely. Write a small `inject_api_key(llm_config, provider, key)` helper that sets the `api_key` field on the appropriate provider config section.

### Task 2.2: Graceful download error handling

**File:** `crates/photon/src/cli/interactive/models.rs`

Wrap download calls in `match` instead of `?`:

```rust
ModelAction::DownloadVision(indices) => {
    let client = reqwest::Client::new();
    match download_vision(indices, config, &client).await
        .and_then(|_| async { download_shared(config, &client).await }.await)
        .and_then(|_| install_vocabulary(config).map_err(Into::into))
    {
        Ok(()) => {
            let done = Style::new().for_stderr().green();
            eprintln!("{}", done.apply_to("  Downloads complete."));
        }
        Err(e) => {
            let err = Style::new().for_stderr().red();
            eprintln!("  {} Download failed: {e}", err.apply_to("✗"));
            eprintln!("  Check your network connection and try again.");
        }
    }
    eprintln!();
}
```

Same pattern for `DownloadShared` and `InstallVocabulary` actions. On failure, the user returns to the model menu instead of the session crashing.

### Task 2.3: Preserve config comments with `toml_edit`

**File:** `crates/photon/Cargo.toml` — replace `toml = "0.8"` with `toml_edit = "0.22"`
**File:** `crates/photon/src/cli/interactive/setup.rs` — rewrite `save_key_to_config()`

Replace:
```rust
let mut doc: toml::Table = content.parse().unwrap_or_default();
```

With:
```rust
let mut doc: toml_edit::DocumentMut = content.parse().unwrap_or_default();
```

`toml_edit::DocumentMut` preserves comments, whitespace, and formatting during round-trips. The table manipulation API is similar:

```rust
doc["llm"][section_name]["api_key"] = toml_edit::value(key);
if doc["llm"][section_name].get("enabled").is_none() {
    doc["llm"][section_name]["enabled"] = toml_edit::value(true);
}
```

Check if `toml` is used elsewhere in the `photon` crate — if the `toml` dependency is only used by `save_key_to_config()`, we can replace it entirely. If it's used elsewhere (e.g., config deserialization in photon-core), keep both.

### Task 2.4: Verify

- `cargo test --workspace` — all tests pass
- `cargo clippy --workspace -- -D warnings` — no `unsafe` warnings
- Manual test: create a config file with comments, save an API key via interactive, confirm comments are preserved

---

## Phase 3: UX Edge Cases

**Goal:** Validate output paths at prompt time, remove panic-capable `unreachable!()`, guard against edge cases.

### Task 3.1: Output path validation in `prompt_output_path()`

**File:** `crates/photon/src/cli/interactive/process.rs`

After the user enters an output path, validate before returning:

```rust
fn prompt_output_path(...) -> anyhow::Result<Option<PathBuf>> {
    loop {
        let Some(raw) = super::handle_interrupt(...)? else {
            return Ok(None);
        };
        let path = PathBuf::from(shellexpand::tilde(&raw).into_owned());

        // Check parent directory exists
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                let warn = Style::new().for_stderr().yellow();
                eprintln!("  {}", warn.apply_to(
                    format!("Directory does not exist: {}", parent.display())
                ));
                continue;
            }
        }

        // Warn if file already exists
        if path.exists() {
            let confirm = Confirm::with_theme(theme)
                .with_prompt(format!("{} already exists. Overwrite?", path.display()))
                .default(false)
                .interact_opt()?;
            if !matches!(confirm, Some(true)) {
                continue;
            }
        }

        return Ok(Some(path));
    }
}
```

### Task 3.2: Replace `unreachable!()` with safe fallbacks

**File:** `crates/photon/src/cli/interactive/mod.rs`

Replace both `_ => unreachable!()` arms:

```rust
// In run(), line 55:
_ => {} // Ignore unexpected index, re-show menu

// In show_config(), line 162:
_ => break, // Treat unexpected index as "Back"
```

These are not functionally different (dialoguer won't return out-of-bounds indices), but they prevent a panic if assumptions are ever violated.

### Task 3.3: Guard against empty custom model names

**File:** `crates/photon/src/cli/interactive/setup.rs`

`prompt_custom_model()` already checks for empty strings (line 187-191), which is good. But `select_model()` for Ollama/Hyperbolic doesn't — an empty `Input` with no default validation could pass through. Add the same empty-string guard:

```rust
LlmProvider::Ollama => {
    let model = super::handle_interrupt(
        Input::<String>::with_theme(theme)
            .with_prompt("Ollama model name")
            .default("llama3.2-vision".to_string())
            .interact_text(),
    )?;
    match model {
        Some(m) if !m.is_empty() => Ok(Some(m)),
        _ => Ok(None),
    }
}
```

Note: `Input` with `.default()` will return the default on empty Enter, so this is mostly a safety net. But if a user types spaces-only, this catches it.

### Task 3.4: Verify

- `cargo test --workspace` — all tests pass
- Manual test: enter a nonexistent parent directory as output path → re-prompts
- Manual test: enter an existing file as output path → asks to overwrite
- Manual test: press Enter with spaces-only on Ollama model → treated as cancel

---

## Phase 4: Polish

**Goal:** Clean up minor inconsistencies and unnecessary allocations.

### Task 4.1: Replace recursive async with loop

**File:** `crates/photon/src/cli/interactive/process.rs`

Wrap the entire `guided_process` body in a loop, replacing the recursive `Box::pin()` call:

```rust
pub async fn guided_process(config: &Config) -> anyhow::Result<()> {
    let theme = photon_theme();

    loop {
        // ... existing steps 1-8 unchanged ...

        // ── Post-processing menu (replaces Box::pin recursion) ────────
        eprintln!();
        let post_items = &["Process more images", "Back to main menu"];
        let post_choice = Select::with_theme(&theme)
            .with_prompt("What next?")
            .items(post_items)
            .default(0)
            .interact_opt()?;

        if !matches!(post_choice, Some(0)) {
            break;
        }
        // Loop continues → re-run from Step 1
    }

    Ok(())
}
```

This eliminates the `Box::pin()` heap allocation and removes the theoretical unbounded recursion. The `theme` is also created once and reused across iterations instead of being recreated on each recursion.

### Task 4.2: Consistent MB calculation

**File:** `crates/photon/src/cli/interactive/process.rs`

Change line 67 from SI megabytes to binary mebibytes to match `models.rs`:

```rust
// Before:
total_size as f64 / 1_000_000.0

// After:
total_size as f64 / (1024.0 * 1024.0)
```

Or, if you prefer SI units project-wide, change the model download logs to match. Pick one convention and apply consistently. The recommendation: **keep `1_000_000.0` (SI)** since most users think of "MB" as SI, and update `models.rs` to match. Either way, be consistent.

### Task 4.3: Reduce redundant `Style` allocations

**File:** `crates/photon/src/cli/interactive/process.rs`

`dim` is created twice (line 61 and 177). Hoist it to the top of the function:

```rust
pub async fn guided_process(config: &Config) -> anyhow::Result<()> {
    let theme = photon_theme();
    let dim = Style::new().for_stderr().dim();
    let warn = Style::new().for_stderr().yellow();
    // ... use throughout ...
}
```

Same opportunity in `setup.rs` where `Style::new().for_stderr().yellow()` is created in 2 places.

This is a minor allocation reduction — `Style` is small — but it improves readability by naming styles once at the top.

### Task 4.4: Format and lint

```bash
cargo fmt --all
cargo clippy --workspace -- -D warnings
```

### Task 4.5: Final verification

Full checklist:
1. `cargo test --workspace` — all tests pass (existing + new)
2. `cargo clippy --workspace -- -D warnings` — clean, no unsafe warnings
3. `cargo fmt --all -- --check` — clean
4. `photon` (bare) → full manual walkthrough
5. `photon process tests/fixtures/images/test.png` → unchanged behavior
6. `photon models list` → unchanged behavior
7. Manual: save API key with commented config → comments preserved
8. Manual: download with network disabled → error displayed, session survives

---

## File Change Summary

| File | Action | Phase |
|------|--------|-------|
| `crates/photon/Cargo.toml` | Replace `toml` with `toml_edit` (or add alongside) | 2 |
| `crates/photon/src/cli/process.rs` | Add `api_key` field to `ProcessArgs` + `Default`; add `inject_api_key()` helper | 2 |
| `crates/photon/src/cli/models.rs` | Add `can_process` tests | 1 |
| `crates/photon/src/cli/interactive/mod.rs` | Add unit tests; replace `unreachable!()` | 1, 3 |
| `crates/photon/src/cli/interactive/setup.rs` | Remove `unsafe set_var`; return key in `LlmSelection`; rewrite `save_key_to_config` with `toml_edit`; add unit tests | 1, 2 |
| `crates/photon/src/cli/interactive/process.rs` | Validate output paths; recursive → loop; consistent MB; hoist styles | 3, 4 |
| `crates/photon/src/cli/interactive/models.rs` | Graceful download error handling | 2 |

---

## Dependency Graph

```
Phase 1 (tests) ─────────────── independent, can start immediately
Phase 2 (robustness) ────────── independent, can start immediately
  Task 2.1 (set_var) ──── requires ProcessArgs change (process.rs + setup.rs + process guided)
  Task 2.2 (downloads) ── isolated to models.rs
  Task 2.3 (toml_edit) ── isolated to setup.rs + Cargo.toml
Phase 3 (edge cases) ────────── independent, can start immediately
Phase 4 (polish) ────────────── should run last (format/lint/verify after all changes)
```

**Phases 1, 2, and 3 can be executed in parallel.** Phase 4 is a final pass.

---

## Expected Score After Completion

| Dimension | Before | After | Why |
|-----------|--------|-------|-----|
| Test coverage | 4/10 | 9/10 | ~15-20 unit tests covering all pure functions |
| Robustness | 6/10 | 9.5/10 | No unsafe, graceful errors, comment-preserving config |
| Edge case handling | 7/10 | 9/10 | Path validation, overwrite confirmation, defensive matches |
| Code quality | 8/10 | 9.5/10 | No recursion, consistent units, clean allocations |
| Architecture | 9/10 | 9/10 | Unchanged (already strong) |
| Documentation | 8/10 | 9/10 | This plan + completion docs |
| UX polish | 8/10 | 9/10 | Better error messages, validated paths |
| **Overall** | **7/10** | **9.5/10** | |
