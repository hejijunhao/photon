# Code Assessment Fixes — Implementation Plan

> Source: `docs/plans/code-assessment.md` (assessed 2026-02-12, rated 8/10)
> Target: 9/10 — zero known bugs, no files >500 production lines, zero unsafe unwraps

---

## Overview

4 phases, ordered by priority. Each phase is self-contained: implement, test, verify, commit.

| Phase | Item | Files Changed | Estimated Lines |
|-------|------|---------------|-----------------|
| **1** | Fix progressive encoding cache bug | 1 file | ~8 lines changed |
| **2** | Fix text encoder unwrap | 1 file | ~4 lines changed |
| **3** | Refactor `cli/process.rs` → module directory | 7 files (1 deleted, 6 created) | ~843 lines moved |
| **4** | Refactor `config.rs` → module directory | 4 files (1 deleted, 3 created) | ~667 lines moved |

---

## Phase 1 — Fix Progressive Encoding Cache Bug

**Priority:** HIGH
**File:** `crates/photon-core/src/tagging/progressive.rs`
**Bug:** When a chunk encoding fails mid-batch, `background_encode()` skips the failed chunk via `continue` but still saves the incomplete `running_bank` to disk at line 166 with the full vocabulary hash. On next startup, `cache_valid()` passes (hash matches), but `LabelBank::load()` fails on size mismatch — creating a sticky broken state requiring manual deletion of `~/.photon/taxonomy/label_bank.*`.

### Root Cause

```
background_encode():
  for chunk in remaining.chunks(chunk_size):
    if encoding fails → continue        // ✓ runtime correct: skip chunk
  // ✗ BUG: always reaches here, even with skipped chunks
  running_bank.save(&cache_path, &vocab_hash)  // saves partial bank with full hash
```

The `save()` writes `vocab_hash` (computed from the full vocabulary) into `label_bank.meta`, but the actual `.bin` file only contains embeddings for successfully-encoded terms. On reload, the hash matches so the cache is treated as valid, but the byte count doesn't match `vocabulary.len() * 768 * 4`.

### Implementation

1. Add a `all_chunks_succeeded: bool` tracking variable before the chunk loop (line 110)
2. Set it to `false` in both error branches (lines 123-130)
3. Wrap the cache save block (lines 157-174) in `if all_chunks_succeeded { ... } else { log warning }`

### Exact Changes

In `background_encode()`, before the `for chunk in ...` loop (after line 108):
```rust
let mut all_chunks_succeeded = true;
```

In both `continue` branches (lines 124-125 and lines 128-129):
```rust
all_chunks_succeeded = false;
continue;
```

Wrap the save block at line 157:
```rust
if all_chunks_succeeded {
    // [existing save logic, lines 158-174]
} else {
    tracing::warn!(
        "Progressive encoding had failures — skipping cache save to avoid corruption. \
         {} of {} terms encoded this session.",
        encoded_indices.len(),
        total_terms,
    );
}
```

### Verification

- `cargo test -p photon-core` — all existing tests pass
- `cargo clippy --workspace -- -D warnings` — no new warnings
- Manual reasoning: on next startup after a partial failure, there's no cached file, so the system falls back to re-encoding from scratch (correct self-healing behavior)

---

## Phase 2 — Fix Text Encoder Unwrap

**Priority:** LOW (but trivial to fix)
**File:** `crates/photon-core/src/tagging/text_encoder.rs:156`
**Bug:** `.unwrap()` on `batch.into_iter().next()` — panics if ONNX returns an empty tensor.

### Current Code (line 154-157)

```rust
pub fn encode(&self, text: &str) -> Result<Vec<f32>, PipelineError> {
    let batch = self.encode_batch(&[text.to_string()])?;
    Ok(batch.into_iter().next().unwrap())
}
```

### Replacement

```rust
pub fn encode(&self, text: &str) -> Result<Vec<f32>, PipelineError> {
    let batch = self.encode_batch(&[text.to_string()])?;
    batch.into_iter().next().ok_or_else(|| PipelineError::Model {
        message: "Text encoder returned empty result for single input".to_string(),
    })
}
```

### Verification

- `cargo test -p photon-core` — all existing tests pass
- `cargo clippy --workspace -- -D warnings` — no new warnings
- The `Ok(...)` wrapper is removed because `ok_or_else` already returns `Result`

---

## Phase 3 — Refactor `cli/process.rs` (843 lines → 5-file module)

**Priority:** HIGH (structural)
**File:** `crates/photon/src/cli/process.rs` → `crates/photon/src/cli/process/`

This is a pure structural refactoring — **zero logic changes**. Every function, struct, enum, and test moves verbatim. The only new code is `mod` declarations and `use` re-exports.

### Target Structure

```
crates/photon/src/cli/process/
├── mod.rs          ~200 lines  — ProcessArgs, ProcessContext, execute(), re-exports
├── types.rs         ~60 lines  — OutputFormat, Quality, LlmProvider enums + Display impls
├── setup.rs        ~170 lines  — setup_processor(), create_enricher(), inject_api_key()
├── batch.rs        ~300 lines  — process_batch(), load_existing_hashes(), create_progress_bar(), print_summary()
└── enrichment.rs    ~70 lines  — run_enrichment_collect(), run_enrichment_stdout(), log_enrichment_stats()
```

### Step-by-step

#### 3.1 — Create `process/types.rs`

Move these items from `process.rs`:
- `OutputFormat` enum + `Display` impl (lines 83-98)
- `Quality` enum (lines 100-108)
- `LlmProvider` enum + `Display` impl (lines 110-132)

Add necessary imports:
```rust
use clap::ValueEnum;
```

#### 3.2 — Create `process/enrichment.rs`

Move these functions from `process.rs`:
- `run_enrichment_collect()` (lines 576-604)
- `run_enrichment_stdout()` (lines 609-636)
- `log_enrichment_stats()` (lines 781-787)

Add necessary imports:
```rust
use photon_core::llm::{EnrichResult, Enricher};
use photon_core::types::{OutputRecord, ProcessedImage};
```

#### 3.3 — Create `process/setup.rs`

Move these items from `process.rs`:
- `setup_processor()` (lines 192-302)
- `create_enricher()` (lines 730-756)
- `inject_api_key()` (lines 758-779)

This file needs access to `ProcessArgs`, `ProcessContext`, `OutputFormat`, `Quality`, `LlmProvider` — import from sibling modules via `super::`.

Add necessary imports:
```rust
use photon_core::embedding::EmbeddingEngine;
use photon_core::llm::{EnrichOptions, Enricher, LlmProviderFactory};
use photon_core::output::OutputFormat as CoreOutputFormat;
use photon_core::{Config, ImageProcessor, ProcessOptions};

use super::{ProcessArgs, ProcessContext};
use super::types::{LlmProvider, OutputFormat, Quality};
```

#### 3.4 — Create `process/batch.rs`

Move these functions from `process.rs`:
- `process_batch()` (lines 364-569)
- `load_existing_hashes()` (lines 658-691)
- `create_progress_bar()` (lines 641-655)
- `print_summary()` (lines 694-727)

Add necessary imports:
```rust
use std::collections::HashSet;
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;

use photon_core::output::OutputFormat as CoreOutputFormat;
use photon_core::pipeline::DiscoveredFile;
use photon_core::types::{OutputRecord, ProcessedImage};
use photon_core::OutputWriter;

use super::{ProcessArgs, ProcessContext};
use super::types::OutputFormat;
use super::enrichment::{run_enrichment_collect, run_enrichment_stdout};
```

#### 3.5 — Create `process/mod.rs`

Keep these items in the root module:
- `ProcessArgs` struct + `Default` impl (lines 15-158)
- `ProcessContext` struct (lines 162-169)
- `execute()` function (lines 172-187)
- `process_single()` function (lines 307-359) — stays here because it's small and closely tied to `execute()`
- All `#[cfg(test)] mod tests` (lines 789-843)

Add module declarations:
```rust
mod batch;
mod enrichment;
mod setup;
pub mod types;

// Re-export the public types that other modules depend on
pub use types::{LlmProvider, OutputFormat, Quality};
```

Use statements for cross-module calls:
```rust
use batch::process_batch;
use setup::setup_processor;
use enrichment::{run_enrichment_collect, run_enrichment_stdout};
```

#### 3.6 — Update external references

Any file that imports from `cli::process` needs to be checked. The key external consumers:

- `crates/photon/src/main.rs` — imports `ProcessArgs` → no change needed if re-exported from `mod.rs`
- `crates/photon/src/cli/interactive/process.rs` — imports `ProcessArgs`, `execute`, `OutputFormat`, `Quality`, `LlmProvider` → verify these are all re-exported

Verify: `cargo build` must succeed with zero changes to any file outside `cli/process/`.

### Verification

- `cargo build` — compiles successfully
- `cargo test -p photon` — all 31 CLI tests pass
- `cargo clippy --workspace -- -D warnings` — no new warnings
- `cargo fmt --all -- --check` — no formatting violations
- Line count check: no file in `process/` exceeds 300 lines

---

## Phase 4 — Refactor `config.rs` (667 lines → 3-file module)

**Priority:** MEDIUM (structural)
**File:** `crates/photon-core/src/config.rs` → `crates/photon-core/src/config/`

Pure structural refactoring — zero logic changes.

### Target Structure

```
crates/photon-core/src/config/
├── mod.rs          ~160 lines  — Config struct, load(), load_from(), validate(), default_path(), helpers, re-exports
├── types.rs        ~350 lines  — All sub-config structs + Default impls (GeneralConfig through OpenAiConfig)
└── validate.rs     ~160 lines  — validate() method + all validation tests
```

### Step-by-step

#### 4.1 — Create `config/types.rs`

Move all sub-config structs and their `Default` impls (lines 162-573):
- `GeneralConfig` + Default
- `ProcessingConfig` + Default
- `PipelineConfig` + Default
- `LimitsConfig` + Default
- `EmbeddingConfig` + Default + `image_size_for_model()`
- `ThumbnailConfig` + Default
- `TaggingConfig` + Default
- `ProgressiveConfig` + Default
- `VocabularyConfig` + Default
- `OutputConfig` + Default
- `LoggingConfig` + Default
- `LlmConfig` + Default
- `OllamaConfig` + Default
- `HyperbolicConfig` + Default
- `AnthropicConfig` + Default
- `OpenAiConfig` + Default

Add necessary imports:
```rust
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use crate::tagging::relevance::RelevanceConfig;
```

#### 4.2 — Create `config/validate.rs`

Extract `validate()` as a standalone function (or keep as an `impl Config` block in this file):
```rust
use crate::error::ConfigError;
use super::Config;

impl Config {
    pub(crate) fn validate(&self) -> Result<(), ConfigError> {
        // [existing validation logic, lines 68-115]
    }
}
```

Move all validation tests here (lines 627-667):
- `test_default_config_passes_validation`
- `test_validate_rejects_zero_parallel_workers`
- `test_validate_rejects_zero_thumbnail_size`
- `test_validate_rejects_zero_timeout`
- `test_validate_rejects_invalid_min_confidence`

#### 4.3 — Create `config/mod.rs`

Keep the `Config` struct definition and its non-validation methods:
- `Config` struct (lines 12-44)
- `impl Config`: `load()`, `load_from()`, `default_path()`, `model_dir()`, `vocabulary_dir()`, `taxonomy_dir()`, `to_toml()`
- Non-validation tests: `test_default_config`, `test_config_to_toml`, `test_progressive_config_defaults`, `test_tagging_config_includes_progressive`, `test_tagging_config_hierarchy_defaults`, `test_tagging_config_includes_relevance`

Module declarations and re-exports:
```rust
mod types;
mod validate;

pub use types::*;
```

This ensures all existing `use crate::config::TaggingConfig` etc. in the rest of `photon-core` continue to work unchanged.

#### 4.4 — Update `lib.rs` re-exports

Check `crates/photon-core/src/lib.rs` — it currently does `pub use config::Config`. Since `config` becomes a directory module, `config::Config` still resolves to `config/mod.rs::Config`. **No change needed** if re-exports are correct.

### Verification

- `cargo build --workspace` — compiles successfully
- `cargo test --workspace` — all 164 tests pass (tests moved, not removed)
- `cargo clippy --workspace -- -D warnings` — no new warnings
- `cargo fmt --all -- --check` — no formatting violations
- Line count check: no file in `config/` exceeds 350 lines

---

## Final Verification (All Phases Complete)

After all 4 phases:

```bash
cargo fmt --all -- --check        # Zero formatting violations
cargo clippy --workspace -- -D warnings  # Zero warnings
cargo test --workspace            # 164+ tests passing
cargo build --release             # Release build succeeds
```

### Post-implementation state

| Metric | Before | After |
|--------|--------|-------|
| Known bugs | 2 (1 HIGH, 1 LOW) | **0** |
| Files >500 production lines | 2 | **0** |
| Unsafe unwraps on fallible paths | 1 | **0** |
| Tests | 164 | 164+ (no regressions) |
| Clippy warnings | 0 | 0 |
| Assessed quality | 8/10 | **~9/10** |

---

## Commit Strategy

One commit per phase, each independently passing CI:

1. `Fix progressive encoding cache: skip save on chunk failure`
2. `Fix text encoder unwrap: .unwrap() → .ok_or_else()`
3. `Refactor cli/process.rs into process/ module (843 → 5 files, zero logic changes)`
4. `Refactor config.rs into config/ module (667 → 3 files, zero logic changes)`
