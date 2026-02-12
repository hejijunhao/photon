# Phases 5–7 Completion — Models, Process, LLM Setup

> Completed: 2026-02-12

---

## Summary

All three core interactive flows implemented in one pass: guided model management (Phase 5), LLM provider setup (Phase 7), and the guided process flow (Phase 6) that ties them together. The full interactive experience is now functional: `photon` → banner → menu → guided walkthrough → processing. 136 tests passing, 0 clippy warnings.

**New dependencies added:** `toml` (workspace, for config editing), `shellexpand` (workspace, for `~` path expansion in prompts).

---

## Phase 5: Guided Model Management

**File:** `crates/photon/src/cli/interactive/models.rs` (replaced stub, ~147 lines)

### Flow

1. Call `check_installed(config)` → get `InstalledModels` status
2. `print_status()` — display checkmarks/crosses for each component with size info
3. Build dynamic `Select` menu based on what's missing (only offer downloads for absent components)
4. Dispatch download actions via `download_vision()` / `download_shared()` / `install_vocabulary()`
5. After download → re-display status (loop back to step 1)
6. "Back" or Esc → return to main menu

### Key design decisions

- **Dynamic menu items**: The menu adapts — if all models are installed, only "Show model directory" and "Back" appear. If both vision models are missing, a "Download both" option appears.
- **`ModelAction` enum**: Parallel `items` + `actions` vectors keyed by selection index. Avoids brittle positional matching.
- **Vision download always pulls shared**: Selecting a vision model also triggers `download_shared()` + `install_vocabulary()`, since those are prerequisites for processing.

### Removed `#[allow(unused)]` annotations

All 4 annotations from Phase 4 (`InstalledModels`, `can_process`, `check_installed`, `VARIANT_LABELS`) removed since they're now called.

---

## Phase 7: LLM Setup Flow

**File:** `crates/photon/src/cli/interactive/setup.rs` (replaced stub, ~278 lines)

### Flow

1. **Provider selection**: Select from Skip / Anthropic / OpenAI / Ollama / Hyperbolic
2. **API key handling** (skipped for Ollama):
   - Check env var (`ANTHROPIC_API_KEY`, etc.) and config for existing key
   - If missing: `Password` prompt (masked input), allow empty to skip
   - If entered: offer to save to `~/.photon/config.toml` or use session-only
3. **Model selection**: Provider-specific preset lists + "Custom model name" option
4. Return `LlmSelection { provider, model }` or `None` if user skips/cancels

### Key design decisions

- **`config_has_key()`**: Checks both env var AND config file. Keys starting with `${` are treated as unset (they're placeholder templates like `${ANTHROPIC_API_KEY}`).
- **`save_key_to_config()`**: Reads existing TOML, parses to `toml::Table`, inserts/updates the `[llm.<provider>]` section, writes back. Non-destructive — preserves all other config.
- **`unsafe { std::env::set_var() }`**: Required since Rust 1.83 deprecated safe `set_var`. Documented as safe in single-threaded CLI context. Used for session-only key persistence.
- **Password prompt with `allow_empty_password(true)`**: Lets users press Enter to skip without error. Empty string treated as cancellation.

---

## Phase 6: Guided Process Flow

**File:** `crates/photon/src/cli/interactive/process.rs` (replaced stub, ~250 lines)

### 8-Step Flow

| Step | Prompt | Widget |
|------|--------|--------|
| 1. Input path | "Path to image or folder" | `Input` with `~` expansion, re-prompts if not found |
| 2. File discovery | (automatic) | `FileDiscovery::discover()` → count + size display |
| 3. Model check | "Download models now?" | `Confirm` if `!can_process()`, delegates to `guided_models()` |
| 4. Quality preset | "Fast / High detail" | `Select` → `Quality::Fast` or `Quality::High` |
| 5. LLM provider | (delegates to `setup.rs`) | Full `select_llm_provider()` flow |
| 6. Output format | "JSONL file / JSON / stdout" | `Select` (options differ for single vs batch) |
| 7. Confirmation | "Start processing?" | `Confirm` with summary line |
| 8. Execute | (delegates to `cli::process::execute()`) | Builds `ProcessArgs` via `..Default::default()` |

### Post-processing menu (Phase 8 — inlined)

After `execute()` returns: "Process more images" (recurse via `Box::pin()`) or "Back to main menu".

### Key design decisions

- **`Box::pin(guided_process(config))`**: Required for recursive async functions. Without `Pin<Box<..>>`, the compiler can't determine the future's size. This only triggers if the user selects "Process more images".
- **Adaptive output options**: Single-file defaults to stdout JSON; batch defaults to JSONL file. The menu items change based on `files.len() > 1`.
- **`shellexpand::tilde()`**: Applied to both input path and output path, so users can type `~/photos` naturally.
- **Zero duplication**: The entire processing pipeline is `ProcessArgs { ... } → execute(args).await`. The interactive module only handles user interaction, never processing logic.

---

## File Change Summary

| File | Action | Phase |
|------|--------|-------|
| `crates/photon/Cargo.toml` | +`toml`, +`shellexpand` | 5, 6 |
| `crates/photon/src/cli/models.rs` | Removed 4 `#[allow(unused)]` annotations | 5 |
| `crates/photon/src/cli/interactive/models.rs` | Replaced stub → full implementation (~147 lines) | 5 |
| `crates/photon/src/cli/interactive/setup.rs` | Replaced stub → full implementation (~278 lines) | 7 |
| `crates/photon/src/cli/interactive/process.rs` | Replaced stub → full implementation (~250 lines) | 6 |

---

## Verification

| Check | Result |
|-------|--------|
| `cargo clippy --workspace -- -D warnings` | Pass (0 warnings) |
| `cargo fmt --all -- --check` | Pass |
| `cargo test --workspace` | 136 tests passing (unchanged) |

---

## What's Next

Phase 9: Configure Settings (read-only viewer) — show current config and point to file.
Phase 10: Polish — Ctrl+C edge cases, empty directory handling, final verification.
