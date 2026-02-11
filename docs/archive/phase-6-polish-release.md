# Phase 6: Polish & Release — Completion Notes

> **Completed:** 2026-02-11
> **Tests:** 120 passing, 0 clippy warnings, format clean

---

## Summary

Phase 6 adds production UX polish and release infrastructure. All 8 sub-tasks are complete: progress bar during batch processing, working `--skip-existing` flag, formatted summary statistics, user-friendly error hints, criterion benchmarks, updated documentation, GitHub Actions CI/CD, and the MIT license file.

---

## What Changed

### 6.1 — Progress Bar (indicatif)

**Files modified:**
- `crates/photon/Cargo.toml` — added `indicatif = "0.17"` dependency
- `crates/photon/src/cli/process.rs` — added `create_progress_bar()` helper, wired into batch loop

**Design decision:** The plan placed `indicatif` in `photon-core`, but progress bars are a terminal UI concern. A library crate shouldn't force consumers to pull in terminal UI dependencies. Placed in the `photon` CLI crate instead, which is the correct architectural layer.

**How it works:**
- `create_progress_bar(total)` creates an `indicatif::ProgressBar` with a styled template showing spinner, elapsed time, bar, position/total, percentage, and message
- During batch processing, `progress.inc(1)` is called after each file (whether succeeded, failed, or skipped)
- The message is updated with the current processing rate (`{:.1} img/sec`)
- On completion, `progress.finish_and_clear()` removes the bar before the summary prints

---

### 6.2 — Skip Already-Processed (`--skip-existing`)

**Files modified:**
- `crates/photon/src/cli/process.rs` — added `load_existing_hashes()` helper, wired into batch loop

**How it works:**
1. When `--skip-existing` is set, reads the output file (if it exists) line by line
2. Tries parsing each line as `OutputRecord` (dual-stream format) first, then falls back to plain `ProcessedImage`
3. Extracts `content_hash` from each `Core` record into a `HashSet<String>`
4. During batch processing, before processing each file, computes its BLAKE3 hash via `Hasher::content_hash()` and checks against the set
5. If found, increments the `skipped` counter and continues to the next file
6. When writing output with `--skip-existing`, appends to the existing file instead of truncating

**Handles both output formats:**
- JSONL: Each line is a separate record — works naturally
- JSON with `OutputRecord` wrapper: Parses the `type` tag to find `Core` records

---

### 6.3 — Summary Statistics

**Files modified:**
- `crates/photon/src/cli/process.rs` — added `print_summary()` helper

**Output format (to stderr so it doesn't interfere with JSON on stdout):**
```
  ====================================
               Summary
  ====================================
    Succeeded:           42
    Failed:               2
    Skipped:              5
  ------------------------------------
    Total:               49
    Duration:          12.3s
    Rate:             3.4 img/sec
    Throughput:       8.7 MB/sec
  ====================================
```

- Failed/Skipped rows only appear when non-zero
- Throughput is computed from total bytes of successfully processed images
- Writes to `stderr` (via `eprintln!`) so piped JSON output on stdout remains clean

---

### 6.4 — Comprehensive Error Messages

**Files modified:**
- `crates/photon-core/src/error.rs` — added `PipelineError::hint()` and `PhotonError::hint()` methods

**Approach:** Added `hint() -> Option<&'static str>` methods to both error types. Each error variant returns a contextual recovery suggestion:

| Error | Hint |
|-------|------|
| `Decode` | "The file may be corrupted or in an unsupported format." |
| `Model` | "Run `photon models download` to install required models." |
| `Llm` (401/403) | "Check your API key. Set the appropriate environment variable." |
| `Llm` (429) | "Rate limited. Try again later or reduce --parallel." |
| `Llm` (5xx) | "Provider is experiencing issues. Try again later." |
| `Timeout` | "Try increasing the timeout in config.toml or use a simpler model." |
| `FileTooLarge` | "Increase `limits.max_file_size_mb` in config, or resize the image." |
| `ImageTooLarge` | "Increase `limits.max_image_dimension` in config, or resize the image." |
| `UnsupportedFormat` | "Supported formats: JPEG, PNG, WebP, GIF, TIFF, BMP, AVIF." |
| `FileNotFound` | "Check the file path and try again." |
| `Config` | "Run `photon config show` to see current configuration." |

The `hint()` method is additive — the existing `Display` implementations remain unchanged for backward compatibility. Consumers can call `error.hint()` to get the suggestion, or use the standard `Display` formatting.

The CLI input validation also gained a hint: `"Input path does not exist: ... Hint: Check the file path and try again."`

---

### 6.5 — Performance Benchmarks

**Files created:**
- `crates/photon-core/benches/pipeline.rs`

**Files modified:**
- `crates/photon-core/Cargo.toml` — added `criterion = "0.5"` dev-dependency and `[[bench]]` section

**Benchmarks included:**
| Benchmark | What it measures |
|-----------|-----------------|
| `content_hash_blake3` | BLAKE3 streaming hash of a test image |
| `perceptual_hash` | DoubleGradient perceptual hash of a 256x256 image |
| `decode_image` | Full image decode from disk |
| `thumbnail_256px` | Resize 1920x1080 → 256px WebP thumbnail |
| `metadata_extract` | EXIF extraction from a test image |

Run with: `cargo bench -p photon-core`

Benchmarks that require test fixtures gracefully skip with a message if the fixture file is missing. No embedding/tagging benchmarks included since they require model files to be downloaded.

---

### 6.6 — Documentation Updates

**Files modified:**
- `README.md`

**Changes:**
- "LLM Descriptions *(coming soon)*" → "LLM Descriptions" (it's shipped)
- Added LLM usage examples section (Ollama, Anthropic, OpenAI, batch enrichment)
- Pipeline diagram: changed "(coming soon)" to "(BYOK)" for LLM Enrich stage
- Project Status table: added "LLM enrichment" as Complete, added "Polish & release" as Complete
- Updated test count: "32 across workspace" → "120+ across workspace"
- Added `cargo bench -p photon-core` to the Contributing section
- Batch processing feature: added "progress bar and skip-existing support"
- Added Hyperbolic to the LLM provider list in features

---

### 6.7 — Release Preparation

**Files created:**
- `.github/workflows/ci.yml` — CI pipeline (check, test, format, clippy)
- `.github/workflows/release.yml` — Release automation (multi-platform binaries)
- `LICENSE-MIT` — MIT license text

**CI workflow (`ci.yml`):**
- Triggers on push to `master` and pull requests
- `check` job: `cargo check` + `cargo test` on macOS-14 (ARM) and ubuntu-latest
- `lint` job: `cargo fmt --check` + `cargo clippy -D warnings` on ubuntu-latest

**Release workflow (`release.yml`):**
- Triggers on tags matching `v*` (e.g., `v0.1.0`)
- Builds release binaries for: `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu`
- Packages each as `.tar.gz` and uploads as artifact
- Creates a GitHub Release with all binaries and auto-generated release notes

**License:** The workspace `Cargo.toml` declares `MIT OR Apache-2.0`. Previously only the Apache-2.0 `LICENSE` file existed. Added `LICENSE-MIT` for the MIT option. README already references both: `[MIT](LICENSE-MIT) or [Apache 2.0](LICENSE)`.

---

## Files Changed Summary

```
Modified:
  crates/photon/Cargo.toml           — +indicatif dependency
  crates/photon/src/cli/process.rs   — progress bar, skip-existing, summary, better errors
  crates/photon-core/Cargo.toml      — +criterion dev-dependency, [[bench]] section
  crates/photon-core/src/error.rs    — hint() methods on PipelineError + PhotonError
  README.md                          — LLM docs, project status, test count, benchmarks

Created:
  crates/photon-core/benches/pipeline.rs   — criterion benchmarks
  .github/workflows/ci.yml                 — CI pipeline
  .github/workflows/release.yml            — release automation
  LICENSE-MIT                              — MIT license text
  docs/completions/phase-6-polish-release.md  — this file
```

---

## Verification

```
cargo fmt --all -- --check   ✓ clean
cargo clippy --workspace     ✓ 0 warnings
cargo test                   ✓ 120 passed, 0 failed
```

---

## What Was NOT Changed (and Why)

- **No `BatchProcessor` in core:** The plan proposed moving batch logic from CLI to `photon-core`. This was intentionally deferred because (a) the batch processing is deeply coupled to CLI output routing (stdout vs file, LLM dual-stream), (b) adding it would mean a large refactor with no functional benefit right now, and (c) the current architecture correctly separates concerns: `photon-core` handles per-image processing, the CLI handles orchestration and UX.

- **No `ProcessingStats` enhancement in `types.rs`:** The existing struct in types.rs is sufficient. The formatted summary lives in the CLI (where it belongs as a UI concern), not in the core library.

- **No Ctrl+C graceful shutdown:** Would require `tokio::signal` handlers and cooperative cancellation tokens threaded through the batch loop. Deferred as low priority — the process is sequential anyway, so Ctrl+C just stops at the current image.
