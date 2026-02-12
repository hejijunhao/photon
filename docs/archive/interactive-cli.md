# Interactive CLI Mode

> Transform Photon from a flags-only batch tool into a guided, user-friendly CLI experience — while keeping the existing non-interactive interface fully intact for scripting and CI.

---

## Motivation

Today, using Photon requires knowing the right flags upfront:

```bash
export ANTHROPIC_API_KEY="sk-ant-..."
photon models download
photon process ./photos/ --output results.jsonl --llm anthropic --quality high
```

This is fine for automation, but intimidating for a first-time user who just installed Photon and wants to process some photos. The goal is a **guided mode** that feels intuitive from the first run — similar to tools like Claude Code, `npm init`, or `gh auth login`.

---

## Design Principles

1. **Zero-knowledge start** — A user who types `photon` with no arguments should be able to process images within 60 seconds, guided step-by-step.
2. **Non-interactive stays untouched** — All existing `photon process ...` flag-based usage continues to work identically. No breaking changes.
3. **Progressive disclosure** — Show simple choices first, offer advanced options only when asked.
4. **Graceful degradation** — If models aren't downloaded, offer to download them. If no LLM key is set, skip descriptions gracefully.
5. **Stdout is sacred** — Interactive prompts and UI go to stderr. Structured JSON output stays on stdout. Piping still works.

---

## UX Flow

### Entry Point: Bare `photon` (No Subcommand)

When the user runs `photon` with no arguments, launch interactive mode:

```
$ photon

  ╔══════════════════════════════════════════╗
  ║              Photon v0.4.11              ║
  ║    AI-powered image processing pipeline  ║
  ╚══════════════════════════════════════════╝

? What would you like to do?
  ▸ Process images
    Download / manage models
    Configure settings
    Exit
```

### Flow 1: Process Images (Happy Path)

```
? What would you like to do? › Process images

? Select images to process:
  Path or drag file/folder here: █
  > ./photos/

  Found 247 images (1.8 GB) in ./photos/

? Quality preset:
  ▸ Fast (default) — 224px model, ~50 img/min
    High detail — 384px model, ~15 img/min

? Enable LLM descriptions? (adds rich text descriptions via AI)
  ▸ Skip for now
    Anthropic (Claude)
    OpenAI
    Ollama (local)
    Hyperbolic

? Output format:
  ▸ JSONL file (recommended for batches)
    JSON array
    Stream to stdout

? Output file path:
  > ./results.jsonl

  Ready to process 247 images
  Quality: fast | LLM: off | Output: ./results.jsonl

? Start processing? (Y/n) › Y

  ████████████████████░░░░░░░░░░  142/247  3.2 img/sec  ETA 33s

  ====================================
               Summary
  ====================================
    Succeeded:           245
    Failed:                2
    Duration:         77.3s
    Rate:            3.2 img/sec
    Throughput:     23.8 MB/sec
  ====================================

? What next?
  ▸ Process more images
    View failed images
    Exit
```

### Flow 2: First Run (Models Not Downloaded)

```
? What would you like to do? › Process images

  ⚠ SigLIP model not found. Photon needs this model for
    embeddings and tagging (~350 MB one-time download).

? Download now?
  ▸ Yes, download Base (224) — 350 MB, fast, good for most use cases
    Yes, download Base (384) — 350 MB, higher detail, 3-4× slower
    Yes, download both — 700 MB
    No, skip (processing will work without embedding/tagging)

  Downloading SigLIP Base (224)...
  ████████████████████████████████  350 MB  done (45s)

  Downloading text encoder + tokenizer...
  ████████████████████████████████  443 MB  done (38s)

  ✓ Models installed to ~/.photon/models/

  [continues to process flow above]
```

### Flow 3: LLM Provider Selection (With API Key Prompt)

```
? Enable LLM descriptions? › Anthropic (Claude)

  ⚠ ANTHROPIC_API_KEY not set.

? Enter your Anthropic API key (or press Esc to skip):
  > sk-ant-api03-████████████████

  ✓ API key validated.

? Save this key for future sessions?
  ▸ Yes, save to ~/.photon/config.toml
    No, use for this session only

? Which model?
  ▸ claude-sonnet-4-20250514 (recommended)
    claude-haiku-4-5-20251001 (faster, cheaper)
    Custom model name...
```

### Flow 4: Download / Manage Models

```
? What would you like to do? › Download / manage models

  Installed models:
    ✓ SigLIP Base (224)     visual.onnx    348 MB
    ✗ SigLIP Base (384)     not installed
    ✓ Text encoder          text_model.onnx 443 MB
    ✓ Tokenizer             tokenizer.json    1 MB

? Select an action:
  ▸ Download Base (384) variant
    Re-download all models
    Show model directory
    Back
```

### Flow 5: Configure Settings

```
? What would you like to do? › Configure settings

  Current configuration:
    Config file:  ~/.photon/config.toml
    Model dir:    ~/.photon/models/
    Parallel:     4 workers
    Thumbnail:    256px WebP

? What would you like to configure?
  ▸ LLM providers
    Processing defaults
    Output format
    View full config (TOML)
    Back
```

---

## Architecture

### New Dependency

Add [`dialoguer`](https://crates.io/crates/dialoguer) — the most mature interactive prompt library in the Rust ecosystem. Used by `cargo-generate`, `wasm-pack`, and many others. It provides:

- `Select` — arrow-key single selection (the main UX element)
- `Input` — text input with validation
- `Confirm` — yes/no prompts
- `Password` — masked input (for API keys)
- `MultiSelect` — checkbox-style multi-selection

Also add [`console`](https://crates.io/crates/console) (pulled in by `dialoguer`) for terminal styling — colors, bold, styled text. Both are maintained by the `console-rs` org.

```toml
# crates/photon/Cargo.toml
[dependencies]
dialoguer = "0.11"
console = "0.15"
```

### Module Structure

```
crates/photon/src/
├── main.rs                    # Add: dispatch to interactive when no subcommand
├── cli/
│   ├── mod.rs                 # Add: pub mod interactive;
│   ├── process.rs             # Unchanged — non-interactive batch mode
│   ├── models.rs              # Modified — extract download logic into reusable fn
│   ├── config.rs              # Unchanged
│   └── interactive/
│       ├── mod.rs             # Main menu loop + dispatch
│       ├── process.rs         # Guided process flow (collects args, calls cli::process::execute)
│       ├── models.rs          # Guided model management (calls cli::models download logic)
│       ├── setup.rs           # LLM provider configuration + API key prompts
│       └── theme.rs           # Custom dialoguer theme (colors, symbols, spacing)
```

### Key Design: Interactive Collects Args, Then Calls Existing Code

The interactive module does **not** reimplement processing logic. It collects user choices into a `ProcessArgs` struct (the same one clap produces), then calls the existing `cli::process::execute(args)`. This means:

1. Zero duplication of processing logic
2. Interactive mode is guaranteed to have feature parity with flags
3. Any new flags automatically become available for interactive prompts

```rust
// crates/photon/src/cli/interactive/process.rs (simplified)

pub async fn guided_process(config: &Config) -> Result<()> {
    // Step 1: Collect input path
    let input = Input::<String>::new()
        .with_prompt("Path to image or folder")
        .interact_text()?;

    // Step 2: Discover files and show count
    let processor = ImageProcessor::new();
    let files = processor.discover(&PathBuf::from(&input))?;
    eprintln!("  Found {} images", files.len());

    // Step 3: Prompt for options
    let quality = select_quality()?;
    let llm = select_llm_provider(config)?;
    let output = select_output(files.len())?;

    // Step 4: Build the same ProcessArgs struct that clap produces
    let args = ProcessArgs {
        input: PathBuf::from(input),
        output,
        quality: Some(quality),
        llm,
        ..ProcessArgs::default()
    };

    // Step 5: Delegate to existing non-interactive execute()
    cli::process::execute(args).await
}
```

### Entry Point Change

```rust
// crates/photon/src/main.rs

#[derive(Subcommand)]
enum Commands {
    /// Process image(s) through the pipeline
    Process(ProcessArgs),
    /// Manage SigLIP models
    Models(ModelsArgs),
    /// View or modify configuration
    Config(ConfigArgs),
}

#[derive(Parser)]
#[command(name = "photon", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,  // ← Change from Commands to Option<Commands>
    #[arg(long, global = true)]
    verbose: bool,
    #[arg(long, global = true)]
    json_logs: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = Config::load().unwrap_or_default();

    match cli.command {
        Some(Commands::Process(args)) => cli::process::execute(args).await?,
        Some(Commands::Models(args)) => cli::models::execute(args).await?,
        Some(Commands::Config(args)) => cli::config::execute(args)?,
        None => cli::interactive::run(&config).await?,  // ← No subcommand = interactive
    }

    Ok(())
}
```

This means:
- `photon` → interactive mode
- `photon process ...` → batch mode (unchanged)
- `photon models ...` → model management (unchanged)

---

## Implementation Steps

### Step 1: Add Dependencies and Module Scaffold

- Add `dialoguer` and `console` to `crates/photon/Cargo.toml`
- Create `crates/photon/src/cli/interactive/` directory
- Create `mod.rs`, `theme.rs` with basic structure
- Make `Cli.command` optional in `main.rs`
- Wire up: bare `photon` prints "Interactive mode coming soon" and exits

**Validates:** Dependency compiles, entry point routing works.

### Step 2: Custom Theme

Create a Photon-branded `dialoguer` theme in `theme.rs`:
- Consistent prefix symbols (`▸`, `✓`, `⚠`, `✗`)
- Color scheme using `console` crate (cyan for prompts, green for success, yellow for warnings)
- Banner/header for interactive mode

### Step 3: Main Menu Loop

Implement `interactive/mod.rs`:
- Display Photon banner
- Show main menu: Process / Models / Config / Exit
- Loop until Exit selected
- Each menu item dispatches to its sub-module

### Step 4: Guided Model Management

Implement `interactive/models.rs`:
- Check which models are installed (reuse logic from `cli::models`)
- Show installed/missing status with checkmarks
- Interactive selection for downloading missing models
- Progress bar during download (reuse `indicatif`)

**Prerequisite refactor:** Extract the core download logic from `cli::models::execute()` into a reusable function that both the interactive and non-interactive paths can call. Currently the download logic is embedded directly in the `execute()` function — needs to be split into:
  - `download_vision_model(variant_index)` — downloads a specific model
  - `download_text_encoder()` — downloads text model + tokenizer
  - `check_installed_models()` → `InstalledModels` — checks what's present

### Step 5: Guided Process Flow

Implement `interactive/process.rs`:
- Input path prompt (with drag-and-drop paste support — this is just text input)
- File discovery + count display
- Quality preset selection
- LLM provider selection (or skip)
- Output format and path selection
- Confirmation summary
- Build `ProcessArgs` and call `cli::process::execute()`

**Prerequisite refactor:** `ProcessArgs` needs a `Default` impl (or a builder) so the interactive module can construct it field-by-field. Currently all fields are populated by clap; add `#[arg(default_value_t)]` or manual `Default`.

### Step 6: LLM Setup Flow

Implement `interactive/setup.rs`:
- Detect missing API keys via env var check
- Masked password input for API key entry
- Optional: validate key with a lightweight API call
- Optional: persist to config file with user consent
- Model selection per provider (with sensible defaults)

### Step 7: Post-Processing Menu

After processing completes, offer contextual next actions:
- Process more images
- View failed images (list paths + error reasons)
- View output file location
- Exit

### Step 8: First-Run Experience

When models aren't installed and the user goes to "Process images":
- Detect missing models before processing starts
- Offer inline download (don't require separate `photon models download`)
- Seamless flow: download → process, no restart needed

---

## Refactoring Required

These changes to existing code are needed to support the interactive module without duplicating logic:

### 1. Make `ProcessArgs` constructable outside of clap

```rust
// crates/photon/src/cli/process.rs

impl Default for ProcessArgs {
    fn default() -> Self {
        Self {
            input: PathBuf::new(),
            output: None,
            format: OutputFormat::Jsonl,
            parallel: 4,
            skip_existing: false,
            no_thumbnail: false,
            no_embedding: false,
            no_tagging: false,
            no_description: false,
            quality: None,
            thumbnail_size: 256,
            llm: None,
            llm_model: None,
            show_tag_paths: false,
            no_dedup_tags: false,
        }
    }
}
```

### 2. Extract model download logic into reusable functions

```rust
// crates/photon/src/cli/models.rs

/// Check which models are currently installed
pub fn check_installed() -> InstalledModels { ... }

/// Download a specific vision model variant (0 = Base 224, 1 = Base 384)
pub async fn download_vision(variant: usize, client: &reqwest::Client) -> Result<()> { ... }

/// Download shared text encoder and tokenizer
pub async fn download_text_encoder(client: &reqwest::Client) -> Result<()> { ... }

/// Install vocabulary files from embedded data
pub fn install_vocabulary() -> Result<()> { ... }  // already exists, just make pub
```

### 3. Model existence check utility

The interactive process flow needs to check whether models are present before processing. Add a lightweight check:

```rust
// crates/photon-core/src/config.rs or a new util

pub fn models_installed(config: &Config) -> bool {
    let model_dir = config.model_dir();
    model_dir.join("siglip-base-patch16/visual.onnx").exists()
        && model_dir.join("text_model.onnx").exists()
        && model_dir.join("tokenizer.json").exists()
}
```

---

## What This Does NOT Include

Keeping scope focused — these are explicitly out of scope for this plan:

- **Persistent REPL / shell mode** — This is a guided launcher, not a long-running shell. After processing completes and the user exits, the process ends.
- **TUI dashboard** — No `ratatui` full-screen terminal UI. Just sequential prompts.
- **Key rebinding or slash commands** — Not applicable to this interaction model.
- **Config file wizard** — The "Configure settings" menu shows current config and links to the file. Full interactive config editing is a future enhancement.
- **Windows support** — `dialoguer` supports Windows, but we don't test on it. Should work but is not a target.

---

## Testing Strategy

### Manual Testing

Interactive CLI is inherently manual to test. Create a testing checklist:

1. `photon` with no args → shows menu
2. Navigate all menu paths with arrow keys
3. Process single image via interactive flow
4. Process directory via interactive flow
5. First-run flow: delete `~/.photon/models/`, run `photon`, verify download prompt
6. LLM flow: unset API key, verify prompt and skip behavior
7. Cancel at each prompt with Ctrl+C → clean exit, no panic
8. Pipe detection: `photon | cat` should NOT launch interactive mode (detect non-TTY)

### Automated Testing

- Unit test `ProcessArgs::default()` produces valid args
- Unit test model check utilities
- Integration test: verify `photon process` (with subcommand) still works identically

### TTY Guard

Interactive mode should only activate in a real terminal, not when piped:

```rust
// main.rs
None => {
    if atty::is(atty::Stream::Stdin) {
        cli::interactive::run(&config).await?;
    } else {
        // Print help when piped with no subcommand
        Cli::parse_from(["photon", "--help"]);
    }
}
```

Add `atty = "0.2"` (or use `std::io::IsTerminal` on Rust 1.70+) to detect TTY.

---

## Dependency Summary

| Crate | Version | Purpose | Size Impact |
|-------|---------|---------|-------------|
| `dialoguer` | 0.11 | Interactive prompts (Select, Input, Confirm, Password) | ~50 KB |
| `console` | 0.15 | Terminal styling, colors (transitive dep of dialoguer) | ~80 KB |

Both are from the well-maintained [`console-rs`](https://github.com/console-rs) org. No additional transitive dependencies beyond what `indicatif` already pulls in (they share `console`).

---

## Summary

| Step | What | Touches | Effort |
|------|------|---------|--------|
| 1 | Dependencies + scaffold + entry point | `Cargo.toml`, `main.rs`, `cli/mod.rs` | Small |
| 2 | Custom theme | New: `interactive/theme.rs` | Small |
| 3 | Main menu loop | New: `interactive/mod.rs` | Small |
| 4 | Guided model management | New: `interactive/models.rs`, refactor: `cli/models.rs` | Medium |
| 5 | Guided process flow | New: `interactive/process.rs`, refactor: `cli/process.rs` | Medium |
| 6 | LLM setup + API key flow | New: `interactive/setup.rs` | Medium |
| 7 | Post-processing menu | Extend: `interactive/process.rs` | Small |
| 8 | First-run detection | Extend: `interactive/process.rs` + `interactive/models.rs` | Small |

**Total new code:** ~400–600 lines across the interactive module
**Refactoring:** ~50–80 lines of existing code extracted into reusable functions
**Breaking changes:** None — all existing CLI behavior is preserved
