# Phase 9 Completion — Config Viewer

> Completed: 2026-02-12

---

## Summary

Read-only interactive config viewer replacing the "Configure settings" placeholder in the main menu. Displays a concise summary of key settings (8 fields), detects whether the config file exists on disk, summarises enabled LLM providers, and offers to print the full TOML serialization or the config file path. Follows the same patterns as `models.rs` (themed `Select` loop, `interact_opt()` for Esc/Ctrl+C). 136 tests passing, 0 clippy warnings.

---

## Implementation

**File:** `crates/photon/src/cli/interactive/mod.rs` (~100 lines added, placeholder removed)

### `show_config(config: &Config) -> anyhow::Result<()>`

Loop-based viewer with a summary header and a 3-item action menu:

**Summary display (8 fields):**

| Field | Source | Example |
|-------|--------|---------|
| Config file | `Config::default_path()` + `.exists()` check | `~/.photon/config.toml (exists)` or `(using defaults)` |
| Model dir | `config.model_dir()` | `~/.photon/models` |
| Parallel | `config.processing.parallel_workers` | `4 workers` |
| Thumbnail | `config.thumbnail.size` + `.format` | `256px webp` |
| Embedding model | `config.embedding.model` | `siglip-base-patch16` |
| Tagging | `config.tagging.enabled` / `.max_tags` / `.min_confidence` | `up to 15 tags (min confidence: 0)` |
| Log level | `config.logging.level` | `info` |
| LLM providers | `llm_summary()` helper | `Anthropic, Ollama` or `none configured` |

**Action menu:**

| Option | Action |
|--------|--------|
| View full config (TOML) | `config.to_toml()` → print between dim separator lines |
| Show config file path | `Config::default_path()` → print |
| Back | Break loop, return to main menu |

### `llm_summary(config: &Config) -> String`

Iterates over all 4 `LlmConfig` provider fields (`ollama`, `anthropic`, `openai`, `hyperbolic`), collects names of enabled providers, returns comma-joined string or `"none configured"`.

---

## Key Design Decisions

- **Read-only by design**: The plan explicitly scopes this as a viewer, not an editor. Full interactive config editing is out of scope per the design spec.
- **Config file existence detection**: `Config::default_path().exists()` distinguishes between "config file exists on disk" vs "running on compiled-in defaults". This helps users understand whether they've customized anything.
- **TOML error handling**: `config.to_toml()` can theoretically fail (serialization error). Rather than unwrapping, we display a red `✗` error and continue — the viewer remains usable even if serialization breaks.
- **LLM summary as separate helper**: Extracted to `llm_summary()` for readability. Checks both `Option::is_some()` and `.enabled` — a provider section can exist in TOML but be disabled.
- **Signature change**: `show_config` changed from `fn(_: &Config)` (infallible, no return) to `fn(&Config) -> anyhow::Result<()>` to propagate `dialoguer` errors. Call site updated with `?`.

---

## File Change Summary

| File | Action |
|------|--------|
| `crates/photon/src/cli/interactive/mod.rs` | Replaced placeholder `show_config()` with full implementation (~100 lines); added `llm_summary()` helper; added `use console::Style`; updated call site to `show_config(config)?` |
| `docs/executing/interactive-cli-plan.md` | Phase 9 status → **Done** |

---

## Verification

| Check | Result |
|-------|--------|
| `cargo check -p photon` | Pass |
| `cargo clippy --workspace -- -D warnings` | Pass (0 warnings) |
| `cargo fmt --all -- --check` | Pass |
| `cargo test --workspace` | 136 tests passing (unchanged) |

---

## What's Next

Phase 10: Polish — Ctrl+C edge case handling, empty directory re-prompting, final manual verification pass.
