# Interactive CLI — Implementation Plan

> Step-by-step execution plan for adding guided interactive mode to Photon.
> Based on the design spec in [interactive-cli.md](./interactive-cli.md).

---

## Overview

Transform `photon` (bare, no subcommand) into a guided interactive experience using `dialoguer` prompts, while keeping all existing `photon process ...` flag-based usage untouched. The interactive module collects user choices and delegates to the existing `cli::process::execute()` — zero duplication of processing logic.

**Estimated new code:** ~500–700 lines across 6 new files + ~100 lines of refactoring in 3 existing files
**New dependencies:** `dialoguer 0.11`, `console 0.15`, `toml 0.8` (workspace), `shellexpand 3` (workspace)
**Breaking changes:** None

---

## Execution Status

| Phase | Status | Notes |
|-------|--------|-------|
| 1. Foundation | **Done** | Dependencies, scaffold, `Option<Commands>` + TTY routing. See `docs/completions/phase-1-foundation.md` |
| 2. Theme & Banner | **Done** | `photon_theme()` (ColorfulTheme overrides) + `print_banner()`. See `docs/completions/phase-2-theme-banner.md` |
| 3. Main Menu | **Done** | `Select` loop with 4 options, `interact_opt()` for clean Esc/Ctrl+C. See `docs/completions/phase-3-main-menu.md` |
| 4. Refactors | **Done** | `ProcessArgs::Default`, `InstalledModels`, `check_installed()`, `download_vision/shared()`. See `docs/completions/phase-4-refactors.md` |
| 5. Models | **Done** | Dynamic menu based on `check_installed()`, download dispatch. See `docs/completions/phase-5-6-7-combined.md` |
| 6. Process | **Done** | 8-step guided flow → builds `ProcessArgs` → delegates to `execute()`. See `docs/completions/phase-5-6-7-combined.md` |
| 7. LLM Setup | **Done** | Provider selection, API key via Password, save-to-config, model picker. See `docs/completions/phase-5-6-7-combined.md` |
| 8. Post-Process | **Done** | "Process more" / "Back to menu" after processing (inlined in Phase 6). |
| 9. Config Viewer | **Done** | Read-only summary + full TOML + path, LLM provider summary. See `docs/completions/phase-9-config-viewer.md` |
| 10. Polish | **Done** | `handle_interrupt()` helper, empty-dir re-prompt, final verification. See `docs/completions/phase-10-polish.md` |

136 tests passing, 0 clippy warnings, formatting clean throughout.

---

## Phase 1: Foundation — Dependencies, Scaffold, Entry Point

**Goal:** `photon` with no subcommand compiles, routes to a placeholder, and exits cleanly. All existing commands still work.

### Task 1.1: Add dependencies

**File:** `crates/photon/Cargo.toml`

Add:
```toml
# Interactive CLI
dialoguer = "0.11"
console = "0.15"
```

Verify: `cargo check -p photon` compiles.

### Task 1.2: Create interactive module scaffold

Create directory and files:

```
crates/photon/src/cli/interactive/
├── mod.rs          # pub async fn run() + main menu loop
├── process.rs      # Guided process flow (stub)
├── models.rs       # Guided model management (stub)
├── setup.rs        # LLM provider setup (stub)
└── theme.rs        # Custom dialoguer theme
```

**`cli/interactive/mod.rs`** — Initial stub:
```rust
pub mod models;
pub mod process;
pub mod setup;
pub mod theme;

pub async fn run(config: &photon_core::Config) -> anyhow::Result<()> {
    eprintln!("Interactive mode coming soon.");
    Ok(())
}
```

**`cli/mod.rs`** — Add:
```rust
pub mod interactive;
```

### Task 1.3: Make `Cli.command` optional and route bare invocation

**File:** `crates/photon/src/main.rs`

Changes:
1. Change `command: Commands` → `command: Option<Commands>`
2. Add TTY detection (use `std::io::IsTerminal` — stable since Rust 1.70, no extra dep)
3. Route `None` → `cli::interactive::run()` when stdin is a TTY, or print help otherwise

```rust
use std::io::IsTerminal;

// In Cli struct:
#[command(subcommand)]
command: Option<Commands>,

// In main():
match cli.command {
    Some(Commands::Process(args)) => cli::process::execute(args).await,
    Some(Commands::Models(args)) => cli::models::execute(args).await,
    Some(Commands::Config(args)) => cli::config::execute(args).await,
    None => {
        if std::io::stdin().is_terminal() {
            cli::interactive::run(&config).await
        } else {
            Cli::parse_from(["photon", "--help"]);
            Ok(())
        }
    }
}
```

### Task 1.4: Verify

- `cargo check -p photon` — compiles
- `cargo test -p photon` — all existing tests pass
- `cargo clippy --workspace -- -D warnings` — no warnings
- `cargo run -- process tests/fixtures/images/test.png` — unchanged behavior
- `cargo run` (bare) — prints placeholder and exits

---

## Phase 2: Theme and Banner

**Goal:** Consistent visual identity for all interactive prompts.

### Task 2.1: Implement custom theme

**File:** `crates/photon/src/cli/interactive/theme.rs`

Create a custom `dialoguer::theme::Theme` implementation (`PhotonTheme`) that sets:
- Prompt prefix: cyan `?`
- Active item indicator: `▸` (cyan)
- Success prefix: green `✓`
- Warning prefix: yellow `⚠`
- Error prefix: red `✗`
- Consistent spacing and color scheme using `console::Style`

Also include a `print_banner()` function:
```
  ╔══════════════════════════════════════════╗
  ║              Photon v0.4.17             ║
  ║    AI-powered image processing pipeline ║
  ╚══════════════════════════════════════════╝
```

Use `photon_core::VERSION` for the version string.

### Task 2.2: Verify

- `cargo check -p photon` — theme compiles
- Manually run `cargo run` — banner displays correctly

---

## Phase 3: Main Menu Loop

**Goal:** Bare `photon` shows a main menu with arrow-key navigation. Selecting "Exit" quits cleanly.

### Task 3.1: Implement main menu

**File:** `crates/photon/src/cli/interactive/mod.rs`

Replace the stub with:
1. Call `theme::print_banner()`
2. Show a `dialoguer::Select` menu with 4 options:
   - Process images
   - Download / manage models
   - Configure settings
   - Exit
3. Loop: dispatch to sub-module, then show menu again (unless Exit)
4. Handle `Ctrl+C` / `dialoguer::Error` gracefully (clean exit, no panic)

All prompts go to **stderr** (dialoguer defaults to stderr — verify this). Processing output stays on stdout.

### Task 3.2: Verify

- `cargo run` — shows banner + menu
- Arrow keys navigate, Enter selects
- "Exit" quits cleanly
- Ctrl+C exits without panic
- "Process images" / "Models" / "Configure" print "coming soon" stubs and return to menu

---

## Phase 4: Prerequisite Refactors

**Goal:** Extract reusable functions from existing code so the interactive module can call them without duplicating logic.

### Task 4.1: Make `ProcessArgs` constructable outside of clap

**File:** `crates/photon/src/cli/process.rs`

Add a manual `Default` impl for `ProcessArgs`:
```rust
impl Default for ProcessArgs {
    fn default() -> Self {
        Self {
            input: PathBuf::new(),
            output: None,
            format: OutputFormat::Json,
            parallel: 4,
            skip_existing: false,
            no_thumbnail: false,
            no_embedding: false,
            no_tagging: false,
            no_description: false,
            quality: Quality::Fast,
            thumbnail_size: 256,
            llm: None,
            llm_model: None,
            show_tag_paths: false,
            no_dedup_tags: false,
        }
    }
}
```

This lets the interactive module build `ProcessArgs` field-by-field and pass it to `execute()`.

### Task 4.2: Extract reusable model functions from `cli::models`

**File:** `crates/photon/src/cli/models.rs`

Extract 3 public functions from the monolithic `execute()` match arm:

```rust
/// Status of each model file on disk.
pub struct InstalledModels {
    pub vision_224: bool,
    pub vision_384: bool,
    pub text_encoder: bool,
    pub tokenizer: bool,
    pub vocabulary: bool,
}

/// Check which models are currently installed.
pub fn check_installed(config: &Config) -> InstalledModels { ... }

/// Download vision model variant(s) by index (0 = Base 224, 1 = Base 384).
/// Skips already-downloaded files.
pub async fn download_vision(
    variant_indices: &[usize],
    config: &Config,
    client: &reqwest::Client,
) -> anyhow::Result<()> { ... }

/// Download shared text encoder and tokenizer. Skips if already present.
pub async fn download_shared(
    config: &Config,
    client: &reqwest::Client,
) -> anyhow::Result<()> { ... }
```

The existing `ModelsCommand::Download` arm calls these extracted functions (behavior unchanged). The interactive module also calls them.

Also make `install_vocabulary()` public: `pub fn install_vocabulary(...)`.

### Task 4.3: Add model existence check utility

**File:** `crates/photon/src/cli/models.rs` (add to `InstalledModels`)

```rust
impl InstalledModels {
    /// Returns true if the minimum required models are present for processing.
    pub fn can_process(&self) -> bool {
        (self.vision_224 || self.vision_384)
            && self.text_encoder
            && self.tokenizer
    }
}
```

### Task 4.4: Verify

- `cargo test -p photon` — all tests pass
- `cargo run -- models list` — unchanged behavior
- `cargo run -- models download` — unchanged behavior
- `cargo run -- process tests/fixtures/images/test.png` — unchanged behavior

---

## Phase 5: Guided Model Management

**Goal:** "Download / manage models" menu item shows installed status and offers downloads.

### Task 5.1: Implement interactive model management

**File:** `crates/photon/src/cli/interactive/models.rs`

Flow:
1. Call `cli::models::check_installed(config)` to get status
2. Display installed/missing status with checkmarks:
   ```
     ✓ SigLIP Base (224)     visual.onnx    348 MB
     ✗ SigLIP Base (384)     not installed
     ✓ Text encoder          text_model.onnx 443 MB
     ✓ Tokenizer             tokenizer.json    1 MB
   ```
3. Build a dynamic `Select` menu based on what's missing:
   - "Download Base (384) variant" (only if missing)
   - "Re-download all models"
   - "Show model directory"
   - "Back"
4. On download selection → call `cli::models::download_vision()` / `download_shared()`
5. Show `indicatif` progress (already used by the download functions via tracing)
6. Return to model menu after download completes

### Task 5.2: Wire into main menu

**File:** `crates/photon/src/cli/interactive/mod.rs`

Replace "Models" stub with call to `models::guided_models(config).await?`

### Task 5.3: Verify

- `cargo run` → "Download / manage models" shows correct install status
- Selecting "Show model directory" prints path and returns to menu
- "Back" returns to main menu

---

## Phase 6: Guided Process Flow

**Goal:** "Process images" walks the user through input → quality → LLM → output → confirm → process.

### Task 6.1: Implement guided process flow

**File:** `crates/photon/src/cli/interactive/process.rs`

Implement `pub async fn guided_process(config: &Config) -> anyhow::Result<()>`:

**Step 1 — Input path:**
```rust
let input: String = Input::with_theme(&theme)
    .with_prompt("Path to image or folder")
    .interact_text()?;
```
Validate path exists. If not, show error and re-prompt.

**Step 2 — File discovery:**
```rust
let processor = ImageProcessor::new(config);
let files = processor.discover(&PathBuf::from(&input));
// Print: "Found {n} images ({size})"
```

**Step 3 — Model check:**
If models aren't installed, offer inline download (call `interactive::models` flow), then continue. This is the "first-run experience" — no separate step needed.

**Step 4 — Quality preset:**
```rust
Select: ["Fast (default) — 224px model", "High detail — 384px model"]
```

**Step 5 — LLM provider:**
```rust
Select: ["Skip for now", "Anthropic (Claude)", "OpenAI", "Ollama (local)", "Hyperbolic"]
```
If a provider is selected → delegate to `setup::select_llm_provider()` for API key handling.

**Step 6 — Output format:**
```rust
Select: ["JSONL file (recommended for batches)", "JSON array", "Stream to stdout"]
```
If file output selected → prompt for output file path with default `./results.jsonl`.

**Step 7 — Confirmation:**
```
  Ready to process 247 images
  Quality: fast | LLM: off | Output: ./results.jsonl

? Start processing? (Y/n)
```

**Step 8 — Build ProcessArgs and delegate:**
```rust
let args = ProcessArgs {
    input: PathBuf::from(input),
    output,
    format,
    quality,
    llm,
    llm_model,
    ..ProcessArgs::default()
};
cli::process::execute(args).await
```

### Task 6.2: Wire into main menu

Replace "Process images" stub with call to `process::guided_process(config).await?`

### Task 6.3: Verify

- `cargo run` → "Process images" → walk through full flow with a test image
- Confirm the output matches `cargo run -- process tests/fixtures/images/test.png`
- Ctrl+C at any prompt exits cleanly

---

## Phase 7: LLM Setup Flow

**Goal:** When the user selects an LLM provider in the process flow, handle API key detection, input, and optional persistence.

### Task 7.1: Implement LLM setup

**File:** `crates/photon/src/cli/interactive/setup.rs`

Implement `pub fn select_llm_provider(config: &Config) -> anyhow::Result<(Option<LlmProvider>, Option<String>)>`:

1. Check env var for the selected provider:
   - Anthropic → `ANTHROPIC_API_KEY`
   - OpenAI → `OPENAI_API_KEY`
   - Hyperbolic → `HYPERBOLIC_API_KEY`
   - Ollama → no key needed (skip this step)

2. If env var not set:
   ```
     ⚠ ANTHROPIC_API_KEY not set.

   ? Enter your Anthropic API key (or press Esc to skip):
   ```
   Use `dialoguer::Password` for masked input.

3. If key entered:
   ```
   ? Save this key for future sessions?
     ▸ Yes, save to ~/.photon/config.toml
       No, use for this session only
   ```
   - "Yes" → write key to config TOML under `[llm.anthropic]`
   - "No" → set as env var for this process only (`std::env::set_var`)

4. Model selection:
   - Anthropic: `["claude-sonnet-4-20250514 (recommended)", "claude-haiku-4-5-20251001 (faster, cheaper)", "Custom model name..."]`
   - OpenAI: `["gpt-4o (recommended)", "gpt-4o-mini (faster, cheaper)", "Custom model name..."]`
   - Ollama: `Input` prompt for model name with default `"llama3.2-vision"`
   - Hyperbolic: `Input` prompt for model name

5. Return `(Some(LlmProvider), Some(model_name))`

### Task 7.2: Verify

- Select Anthropic with key set → skips key prompt, shows model selection
- Select Anthropic without key → shows masked input, then save prompt
- Press Esc at key prompt → skips LLM gracefully (returns `(None, None)`)
- Select Ollama → no key prompt, direct to model input

---

## Phase 8: Post-Processing Menu

**Goal:** After processing completes, offer contextual next actions instead of just exiting.

### Task 8.1: Implement post-processing menu

**File:** `crates/photon/src/cli/interactive/process.rs`

After `cli::process::execute(args).await` returns, show:
```
? What next?
  ▸ Process more images
    Exit
```

- "Process more images" → restart `guided_process()` from Step 1
- "Exit" → return to main menu (or exit if that's the intent)

Keep it simple — the spec's "View failed images" item is nice but adds complexity. Start with just these two options and add more later if needed.

### Task 8.2: Verify

- After processing completes, menu appears
- "Process more images" loops back
- "Exit" returns to main menu

---

## Phase 9: Configure Settings (Read-Only)

**Goal:** "Configure settings" shows current config and points to the file.

### Task 9.1: Implement settings viewer

**File:** `crates/photon/src/cli/interactive/mod.rs` (inline, no separate file needed)

Flow:
```
  Current configuration:
    Config file:  ~/.photon/config.toml
    Model dir:    ~/.photon/models/
    Parallel:     4 workers
    Thumbnail:    256px WebP

? What would you like to configure?
  ▸ View full config (TOML)
    Open config file location
    Back
```

- "View full config" → call `config.to_toml()` and print
- "Open config file location" → print path
- "Back" → return to main menu

Full interactive config editing is out of scope (per spec). This is just a viewer.

### Task 9.2: Verify

- "Configure settings" shows current values
- "View full config" prints TOML
- "Back" returns to main menu

---

## Phase 10: Polish and Edge Cases

### Task 10.1: Ctrl+C handling

Ensure all `dialoguer` interactions handle `Ctrl+C` gracefully. `dialoguer` returns `Err(dialoguer::Error::IO(..))` on interrupt — catch this and exit with code 0 (not a panic).

Add a wrapper helper:
```rust
fn handle_interrupt<T>(result: Result<T, dialoguer::Error>) -> anyhow::Result<Option<T>> {
    match result {
        Ok(v) => Ok(Some(v)),
        Err(dialoguer::Error::IO(e)) if e.kind() == std::io::ErrorKind::Interrupted => Ok(None),
        Err(e) => Err(e.into()),
    }
}
```

Use throughout — if `None` is returned, exit the current flow gracefully.

### Task 10.2: Empty directory handling

In the process flow, if `processor.discover()` returns 0 files:
- Show message: "No supported images found at that path."
- Re-prompt for path (don't exit the flow)

### Task 10.3: Format and lint

```bash
cargo fmt --all
cargo clippy --workspace -- -D warnings
```

### Task 10.4: Final verification

Full manual test checklist:
1. `photon` (bare) → shows menu, all paths work
2. `photon process tests/fixtures/images/test.png` → unchanged behavior
3. `photon models list` → unchanged behavior
4. `photon config show` → unchanged behavior
5. `echo "test" | photon` → prints help (non-TTY detection)
6. Ctrl+C at every prompt → clean exit
7. `cargo test --workspace` → all tests pass
8. `cargo clippy --workspace -- -D warnings` → clean

---

## File Change Summary

| File | Action | Phase |
|------|--------|-------|
| `crates/photon/Cargo.toml` | Add `dialoguer`, `console` | 1 |
| `crates/photon/src/main.rs` | `Option<Commands>`, TTY routing | 1 |
| `crates/photon/src/cli/mod.rs` | Add `pub mod interactive;` | 1 |
| `crates/photon/src/cli/process.rs` | Add `Default` for `ProcessArgs` | 4 |
| `crates/photon/src/cli/models.rs` | Extract `check_installed()`, `download_vision()`, `download_shared()`, pub `install_vocabulary()` | 4 |
| `crates/photon/src/cli/interactive/mod.rs` | **New** — main menu loop, settings viewer | 1, 3, 9 |
| `crates/photon/src/cli/interactive/theme.rs` | **New** — custom theme + banner | 2 |
| `crates/photon/src/cli/interactive/process.rs` | **New** — guided process flow + post-process menu | 6, 8 |
| `crates/photon/src/cli/interactive/models.rs` | **New** — guided model management | 5 |
| `crates/photon/src/cli/interactive/setup.rs` | **New** — LLM provider + API key setup | 7 |

---

## Dependency Graph

```
Phase 1 (scaffold)
  └─ Phase 2 (theme)
      └─ Phase 3 (main menu)
          ├─ Phase 4 (refactors) ─── prerequisite for 5, 6, 7
          │   ├─ Phase 5 (models)
          │   ├─ Phase 6 (process) ── requires Phase 5 for first-run model check
          │   │   └─ Phase 8 (post-process menu)
          │   └─ Phase 7 (LLM setup) ── used by Phase 6
          └─ Phase 9 (config viewer)
              └─ Phase 10 (polish)
```

**Critical path:** 1 → 2 → 3 → 4 → 6 (the process flow is the core value).
**Can be parallelized:** Phase 5 and Phase 7 are independent; Phase 9 is independent of 5–8.

---

## Testing Strategy

### Automated (unit tests)

| Test | Validates |
|------|-----------|
| `ProcessArgs::default()` fields match expected values | Phase 4 |
| `InstalledModels::can_process()` logic | Phase 4 |
| `check_installed()` with mock paths | Phase 4 |
| Existing test suite passes unchanged | All phases |

### Manual (interactive, inherently)

Each phase has its own "Verify" section with specific manual checks. The full checklist in Phase 10.4 is the acceptance test.

### CI safety

Interactive mode only activates on TTY stdin. CI pipes → `is_terminal()` returns false → help text printed. No CI changes needed.

---

## What's Explicitly Out of Scope

Per the design spec:
- Persistent REPL / shell mode
- Full-screen TUI (`ratatui`)
- Interactive config editing wizard
- Windows testing
- API key validation (lightweight API call) — deferred, can add later
