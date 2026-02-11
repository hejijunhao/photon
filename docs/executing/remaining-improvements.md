# Remaining Improvements — Implementation Plan

> Addresses all open items from the code assessment (`docs/executing/finish-testing.md`) and the planned interactive CLI feature (`docs/plans/interactive-cli.md`).

---

## Overview

| # | Item | Priority | Effort | Category |
|---|------|----------|--------|----------|
| **A** | Integration tests | High | Medium | Assessment #2 |
| **B** | Model download checksum verification | Medium | Medium | Assessment #5 |
| **C** | Streaming batch output | Low | Medium | Assessment #9 |
| **D** | Interactive CLI | Feature | Large | New feature |

Items A–C are hardening/quality work. Item D is the only new feature. All items are independent and can be done in any order, though A (integration tests) is recommended first — it creates a safety net for the rest.

---

## A. Integration Tests

**Assessment reference:** Issue #2 — "No tests that exercise the full pipeline end-to-end."

### Problem

All 123 tests are unit tests for individual modules. Nothing verifies that `ImageProcessor::process()` produces a correct `ProcessedImage` from a real image file. A regression in how modules connect (e.g., wrong field mapping between decode and output, or embedding/tagging opt-in breaking) would go undetected.

### What to test

The integration tests exercise `ImageProcessor` directly (the core library), not the CLI. This keeps them fast and avoids needing to shell out to a binary.

| Test | What it verifies | Model needed? |
|------|-----------------|---------------|
| `full_pipeline_without_models` | Decode, EXIF, hash, thumbnail all populate correctly; embedding and tags are empty | No |
| `full_pipeline_skips_thumbnail` | `ProcessOptions::skip_thumbnail` produces `None` thumbnail | No |
| `full_pipeline_skips_perceptual_hash` | `ProcessOptions::skip_perceptual_hash` produces `None` | No |
| `process_multiple_formats` | Process `test.png`, `beach.jpg`, `dog.jpg` — all succeed with correct format strings | No |
| `process_nonexistent_file` | Returns `PipelineError::FileNotFound` | No |
| `process_oversized_file` | Returns `PipelineError::FileTooLarge` with custom limits | No |
| `discover_finds_fixtures` | `discover()` returns all 4 test images from `tests/fixtures/images/` | No |
| `output_roundtrip_json` | Process → serialize to JSON → deserialize → fields match | No |
| `output_roundtrip_jsonl` | Same for JSONL format | No |

### Where to put them

```
crates/photon-core/tests/
    integration.rs          # All integration tests in one file
```

Cargo automatically discovers files in `tests/` as integration test crates. Each `#[tokio::test]` function gets its own binary, but grouping them in one file keeps compile times down.

### Implementation sketch

```rust
// crates/photon-core/tests/integration.rs

use photon_core::config::Config;
use photon_core::pipeline::{ImageProcessor, ProcessOptions};
use std::path::Path;

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/images")
        .join(name)
}

#[tokio::test]
async fn full_pipeline_without_models() {
    let config = Config::default();
    let processor = ImageProcessor::new(&config);

    let result = processor.process(fixture("beach.jpg").as_path()).await.unwrap();

    // Core fields populated
    assert_eq!(result.format, "jpeg");
    assert_eq!(result.file_name, "beach.jpg");
    assert!(!result.content_hash.is_empty());
    assert!(result.content_hash.len() == 64); // BLAKE3 hex
    assert!(result.width > 0);
    assert!(result.height > 0);
    assert!(result.file_size > 0);

    // Thumbnail generated (enabled by default)
    assert!(result.thumbnail.is_some());

    // Perceptual hash generated
    assert!(result.perceptual_hash.is_some());

    // No models loaded → empty embedding and tags
    assert!(result.embedding.is_empty());
    assert!(result.tags.is_empty());

    // Description is None (no LLM)
    assert!(result.description.is_none());
}
```

### Acceptance criteria

- All new integration tests pass in CI without models installed
- `cargo test -p photon-core` runs both unit and integration tests
- No changes to production code required

---

## B. Model Download Checksum Verification

**Assessment reference:** Issue #5 — "Downloaded ONNX models (~350–441 MB) are not verified against checksums."

### Problem

`download_file()` in `models.rs` streams bytes to disk but never verifies integrity. A truncated transfer, disk error, or CDN corruption would leave a broken `.onnx` file that causes confusing inference failures (e.g., "invalid protobuf" from ONNX Runtime).

### Approach: BLAKE3 post-download verification

HuggingFace provides SHA256 via their API, but that adds an HTTP call per file and ties us to their API format. Simpler approach: embed known BLAKE3 checksums as constants (same hash algorithm the project already uses for image dedup) and verify after each download.

### Implementation

**Step 1: Compute checksums for current model files**

Run once locally to get the expected hashes:

```bash
b3sum ~/.photon/models/siglip-base-patch16/visual.onnx
b3sum ~/.photon/models/siglip-base-patch16-384/visual.onnx
b3sum ~/.photon/models/text_model.onnx
b3sum ~/.photon/models/tokenizer.json
```

**Step 2: Add checksums to `ModelVariant` and shared constants**

```rust
// crates/photon/src/cli/models.rs

struct ModelVariant {
    name: &'static str,
    label: &'static str,
    repo: &'static str,
    remote_path: &'static str,
    blake3: &'static str,  // NEW: expected BLAKE3 hex digest
}

const VISION_VARIANTS: &[ModelVariant] = &[
    ModelVariant {
        name: "siglip-base-patch16",
        // ...
        blake3: "abcdef...",  // actual hash from b3sum
    },
    // ...
];

const TEXT_ENCODER_BLAKE3: &str = "...";
const TOKENIZER_BLAKE3: &str = "...";
```

**Step 3: Add verification to `download_file()`**

Extend the existing `download_file()` to accept an optional expected hash and verify after download completes:

```rust
async fn download_file(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    expected_blake3: Option<&str>,
) -> anyhow::Result<()> {
    // ... existing streaming download logic ...

    // Verify checksum if provided
    if let Some(expected) = expected_blake3 {
        let actual = photon_core::pipeline::Hasher::content_hash(dest)
            .map_err(|e| anyhow::anyhow!("Checksum computation failed: {e}"))?;
        if actual != expected {
            // Remove corrupt file so next run re-downloads
            let _ = std::fs::remove_file(dest);
            anyhow::bail!(
                "Checksum mismatch for {}: expected {}, got {}. \
                 Corrupt file removed — try downloading again.",
                dest.display(), expected, actual
            );
        }
        tracing::debug!("  Checksum verified: {}", &actual[..16]);
    }

    Ok(())
}
```

**Step 4: Update all call sites**

Pass the expected hash at each `download_file()` call:

```rust
download_file(&client, &url, &dest, Some(variant.blake3)).await?;
// ...
download_file(&client, &url, &text_dest, Some(TEXT_ENCODER_BLAKE3)).await?;
// ...
download_file(&client, &url, &tok_dest, Some(TOKENIZER_BLAKE3)).await?;
```

### Handling checksum changes

When HuggingFace updates a model file (rare but possible), the embedded hash becomes stale. The error message explicitly says "try downloading again" — but if it fails twice, the user knows the model changed upstream and can file an issue. This is the same trade-off `cargo` and `go mod` make with their checksum databases.

### Testing

- Unit test: download a small file to a tempdir, verify checksum passes
- Unit test: corrupt one byte, verify checksum fails and file is removed
- Manual: delete models, re-download, verify "Checksum verified" in debug output

### Acceptance criteria

- All three model files verified after download
- Corrupt/truncated files detected and removed with clear error message
- Existing `download_file` signature updated (non-breaking — only called internally)
- No new dependencies (reuses existing `blake3` via `Hasher::content_hash`)

---

## C. Streaming Batch Output

**Assessment reference:** Issue #9 — "All results collected in a Vec before writing to file."

### Problem

In `process.rs` (lines 353–460), batch processing collects all `ProcessedImage` results into a `Vec`, then writes them to file after the loop. For very large batches (thousands of images, each with embeddings = 768 floats = ~6 KB per image), this holds the full result set in memory.

Additionally, when LLM enrichment is enabled, results are **cloned** (line 429) for the enricher task, doubling memory usage.

### Current flow

```
for each image:
    process → push to results Vec
end

open output file
if LLM:
    clone results → send to enricher
    collect enrichment patches
    build all_records Vec (core + enrichments)
    write_all(all_records)
else:
    write_all(results)
```

### Proposed flow (JSONL only)

```
open output file
for each image:
    process → write to file immediately
    if LLM: send to enricher channel (no clone — just hash + path)
end

if LLM:
    await enricher completion
    for each enrichment patch:
        append to file
```

### Scope limitation

Streaming only applies to **JSONL format with file output** — the most common batch scenario. JSON array format inherently requires collecting all results before writing (for the `[...]` wrapper). Stdout JSONL already streams (lines 371–378).

### Implementation

**Step 1: Write core records as they arrive**

```rust
// process.rs — inside the batch processing loop

let mut output_writer = if let Some(output_path) = &args.output {
    let file = if args.skip_existing && output_path.exists() {
        std::fs::OpenOptions::new().append(true).open(output_path)?
    } else {
        File::create(output_path)?
    };
    Some(OutputWriter::new(BufWriter::new(file), output_format, false))
} else {
    None
};

for file in &files {
    // ... skip logic ...
    match processor.process_with_options(&file.path, &options).await {
        Ok(result) => {
            succeeded += 1;
            total_bytes += result.file_size;

            // Stream to file immediately (JSONL)
            if let Some(writer) = &mut output_writer {
                if matches!(output_format, OutputFormat::Jsonl) && !llm_enabled {
                    writer.write(&result)?;
                }
            }

            // ... stdout streaming (unchanged) ...

            // Collect only if needed (LLM or JSON array)
            if llm_enabled || matches!(args.format, OutputFormat::Json) {
                results.push(result);
            }
        }
        // ...
    }
}
```

**Step 2: Append enrichment patches to file**

For LLM mode, core records still need collecting (enricher needs them for image data). But enrichment patches can be appended as they arrive instead of collected:

```rust
// After enricher completes, append patches directly
if let Some(writer) = &mut output_writer {
    for patch in file_writer_rx.try_iter() {
        writer.write(&patch)?;
    }
}
```

**Step 3: JSON array fallback**

For JSON format, fall back to the existing collect-then-write pattern — this is unavoidable for valid JSON array output.

### Impact

| Scenario | Before | After |
|----------|--------|-------|
| 1000 images, JSONL, no LLM | ~6 MB in Vec + 6 MB file write | ~0 MB in Vec, streamed to file |
| 1000 images, JSONL, with LLM | ~12 MB (Vec + clone) + file write | ~6 MB (Vec for enricher only) |
| 1000 images, JSON array | ~6 MB in Vec | ~6 MB in Vec (unchanged) |

### Testing

- Integration test: process 4 fixture images to JSONL file, verify all 4 records present
- Verify `--skip-existing` still works with streaming (reads existing file before opening for append)
- Verify LLM dual-stream output unchanged

### Acceptance criteria

- JSONL + file output streams per-image (no full Vec collection)
- JSON array output unchanged (still collects)
- LLM enrichment still works correctly
- `--skip-existing` still works
- No change to stdout output behavior

---

## D. Interactive CLI

**Full plan:** `docs/plans/interactive-cli.md`

This is the largest item. The interactive CLI adds a guided mode when the user runs bare `photon` with no subcommand. It uses `dialoguer` for prompts and delegates to existing processing logic — no duplication of pipeline code.

### Summary of steps

| Step | What | Files touched | Effort |
|------|------|---------------|--------|
| 1 | Add `dialoguer`/`console` deps, scaffold module, make `Cli.command` optional | `Cargo.toml`, `main.rs`, `cli/mod.rs` | Small |
| 2 | Custom theme (`theme.rs`) — colors, symbols, banner | New: `interactive/theme.rs` | Small |
| 3 | Main menu loop — Process / Models / Config / Exit | New: `interactive/mod.rs` | Small |
| 4 | Guided model management — check installed, offer downloads | New: `interactive/models.rs`, refactor `cli/models.rs` | Medium |
| 5 | Guided process flow — input, quality, LLM, output selection | New: `interactive/process.rs`, refactor `cli/process.rs` | Medium |
| 6 | LLM setup — API key prompt, validation, optional persist | New: `interactive/setup.rs` | Medium |
| 7 | Post-processing menu — process more, view failures, exit | Extend `interactive/process.rs` | Small |
| 8 | First-run detection — offer model download before processing | Extend `interactive/process.rs` + `interactive/models.rs` | Small |

### Prerequisites (refactoring existing code)

1. **`ProcessArgs` needs `Default` impl** — so interactive mode can construct it field-by-field
2. **Extract model download logic** — split `cli::models::execute()` into reusable functions: `check_installed()`, `download_vision()`, `download_text_encoder()`
3. **TTY guard** — use `std::io::IsTerminal` (stable since Rust 1.70) to only launch interactive mode in a real terminal

### New dependencies

| Crate | Version | Purpose | Size |
|-------|---------|---------|------|
| `dialoguer` | 0.11 | Interactive prompts (Select, Input, Confirm, Password) | ~50 KB |
| `console` | 0.15 | Terminal styling (transitive dep of dialoguer, already pulled in by `indicatif`) | ~80 KB |

### Key design constraint

Interactive mode collects user choices into a `ProcessArgs` struct, then calls the existing `cli::process::execute(args)`. Zero duplication of processing logic. Any new flags automatically become available to interactive prompts.

### Acceptance criteria

- `photon` (bare) → interactive menu in a TTY
- `photon | cat` → prints help (non-TTY guard)
- `photon process ...` → unchanged batch mode
- All existing CLI tests still pass
- Manual testing checklist from the plan (8 scenarios)

---

## Recommended execution order

```
A. Integration tests          (creates safety net for everything else)
    ↓
B. Checksum verification      (independent, improves model reliability)
    ↓
C. Streaming batch output     (refactors process.rs — do before D)
    ↓
D. Interactive CLI             (largest item, builds on refactored process.rs and models.rs)
```

C before D is recommended because:
- The interactive CLI plan requires refactoring `process.rs` and `models.rs`
- Streaming output also touches `process.rs`
- Doing C first avoids merge conflicts and means D builds on cleaner code

---

## Metrics

After all items are complete:

| Metric | Before | After |
|--------|--------|-------|
| Test count | 123 | ~135+ (integration tests) |
| Assessment issues open | 4 | 1 (`ort` RC — external) |
| New dependencies | — | `dialoguer` 0.11, `console` 0.15 |
| New code (est.) | — | ~800–1000 lines |
| Breaking changes | — | None |
