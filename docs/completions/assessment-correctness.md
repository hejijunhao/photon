# Code Assessment — MEDIUM Severity Correctness Fixes

> Completed: 2026-02-12
> Source plan: `docs/executing/assessment-correctness.md` (M1–M12, 10 issues)
> Prerequisite: v0.5.3 (all HIGH-severity bugs already fixed)

---

## Summary

Fixed 10 MEDIUM-severity issues across 4 phases, ordered by user impact. Each phase is self-contained. All fixes verified: 185 tests passing (up from 164), zero clippy warnings, zero formatting violations.

| Phase | Issues | Files Changed | New Tests |
|-------|--------|---------------|-----------|
| 1 | M2+M3: `--skip-existing` + JSON format | 2 | 4 |
| 2 | M5, M8, M11, M12: Config & validation | 7 | 6 |
| 3 | M1, M6: Tagging subsystem | 3 | 3 |
| 4 | M9, M10: Pipeline accuracy | 2 | 1 |

---

## Phase 1 — Fix `--skip-existing` with JSON Format (M2+M3)

**File:** `crates/photon/src/cli/process/batch.rs`

### M2: `load_existing_hashes()` fails on JSON array files

**Problem:** The function parsed line-by-line with `serde_json::from_str` per line. JSON array files (`[{...}, {...}]`) span multiple lines, so line-by-line parsing silently failed — zero hashes loaded, everything reprocessed.

**Fix:** Added a two-pass approach. Try `serde_json::from_str::<Vec<OutputRecord>>` and `Vec<ProcessedImage>` on the whole content first (handles JSON arrays). Fall back to line-by-line JSONL parsing only if array parsing fails.

### M3: JSON append produces invalid `[...][...]`

**Problem:** When `--skip-existing` was active, the file was opened with `append(true)`. Appending a second JSON array produces `[...][...]` — not valid JSON.

**Fix:** For the JSON (non-streaming) path, read existing records via `serde_json::from_str::<Vec<OutputRecord>>`, merge with new results, and overwrite the file with the combined set. The JSONL streaming path was already correct (JSONL append is valid).

### Tests added (4)

- `test_load_existing_hashes_json_array` — JSON array file, verify hashes load
- `test_load_existing_hashes_jsonl` — JSONL file, verify hashes load (regression)
- `test_load_existing_hashes_empty_file` — empty file returns empty set
- `test_load_existing_hashes_mixed_records` — JSON array with Core + Enrichment records

### Other changes

- Added `tempfile = "3"` as dev-dependency to `crates/photon/Cargo.toml`

---

## Phase 2 — Config & Validation Fixes (M5, M8, M11, M12)

### M5: Auto-derive `image_size` from model name

**File:** `crates/photon-core/src/config/validate.rs`, `crates/photon-core/src/config/mod.rs`

**Problem:** `EmbeddingConfig::image_size_for_model()` existed but was never called. A user setting `model = "siglip-base-patch16-384"` without updating `image_size` would silently produce wrong embeddings (224px input to a 384px model).

**Fix:** Added auto-correction at the end of `Config::validate()`. If `image_size` doesn't match what the model name implies, it's overridden with a tracing warning. Changed `validate()` signature from `&self` to `&mut self`; updated `load_from()` accordingly.

**Tests:** `test_validate_auto_corrects_image_size_for_384_model`, `test_validate_preserves_correct_image_size`

### M11: TIFF magic bytes false positives

**File:** `crates/photon-core/src/pipeline/validate.rs`

**Problem:** The TIFF check only looked for `II` or `MM` at bytes 0-1. Any non-image file starting with those ASCII sequences (e.g., a text file starting with "MM") would pass validation.

**Fix:** Now requires the full 4-byte TIFF signature: `II` + version 42 (LE: `0x2A 0x00`) or `MM` + version 42 (BE: `0x00 0x2A`).

**Tests:** `test_magic_bytes_tiff_le`, `test_magic_bytes_tiff_be`, `test_magic_bytes_bare_ii_rejected`, `test_magic_bytes_bare_mm_rejected`

### M12: Remove dead `enabled` field from LLM configs

**Files:** `crates/photon-core/src/config/types.rs`, `crates/photon/src/cli/interactive/mod.rs`, `crates/photon/src/cli/interactive/setup.rs`

**Problem:** `pub enabled: bool` on `OllamaConfig`, `HyperbolicConfig`, `AnthropicConfig`, `OpenAiConfig` was never read by `LlmProviderFactory::create()`. Provider selection is purely via the `--llm` CLI flag. The field only misled config file users.

**Fix:** Removed the field from all 4 structs and their `Default` impls. Updated `llm_summary()` in the interactive module to check `config.llm.<provider>.is_some()` instead of `.enabled`. Removed the `enabled = true` write in `setup.rs` (API key save). Serde's default behavior ignores unknown fields, so existing user configs with `enabled = true` won't break.

**Tests updated:** `llm_summary_*` tests rewritten to match new `Some`-based semantics. The "provider present but disabled" test removed (concept no longer exists).

### M8: Download command misleading menu

**File:** `crates/photon/src/cli/models.rs`

**Problem:** `photon models download` printed a 3-option interactive menu but immediately downloaded option 1 without waiting for input.

**Fix:** Removed the misleading menu. The non-interactive path now just logs "Downloading Base (224) vision encoder..." (the interactive path in `interactive/models.rs` already uses `dialoguer::Select`).

---

## Phase 3 — Tagging Subsystem Fixes (M1, M6)

### M1: Implement Warm→Cold demotion

**File:** `crates/photon-core/src/tagging/relevance.rs`

**Problem:** `warm_demotion_checks` config field (default 50) was dead code. `sweep()` never demoted Warm→Cold. Once a term entered the Warm pool (via `promote_to_warm()`), it stayed forever, causing unbounded Warm pool growth.

**Fix:**
1. Added `warm_checks_without_hit: u32` field to `TermStats` (with `#[serde(default)]` for backwards-compatible deserialization).
2. In `sweep()`, the `Pool::Warm` arm now increments the counter on no-hit sweeps and demotes to Cold when `>= warm_demotion_checks`. Promotion to Active resets the counter.
3. In `record_hits()`, any hit resets `warm_checks_without_hit` to 0.
4. All `TermStats` construction sites updated (new, load, tests).

**Tests:** `test_warm_to_cold_demotion`, `test_warm_hit_resets_demotion_counter`, `test_warm_promotion_resets_counter`

### M6: `LabelBank::append()` panic → Result

**File:** `crates/photon-core/src/tagging/label_bank.rs`, `crates/photon-core/src/tagging/progressive.rs`

**Problem:** `append()` used `assert_eq!` which panics on dimension mismatch. In production (background encoding), this would crash the entire process.

**Fix:** Changed signature to `pub fn append(&mut self, other: &LabelBank) -> Result<(), PipelineError>`. Returns `PipelineError::Model` on mismatch. The caller in `progressive.rs` (`background_encode`) handles the error with `if let Err(e)` + logging + `all_chunks_succeeded = false` (matching the existing chunk failure pattern).

**Tests updated:** All `append()` test calls updated to `.unwrap()`. The `#[should_panic]` test replaced with `test_append_dimension_mismatch_returns_error` which asserts on `Result::Err`.

---

## Phase 4 — Pipeline Accuracy (M9, M10)

### M9: Content-based image format detection

**File:** `crates/photon-core/src/pipeline/decode.rs`

**Problem:** `ImageFormat::from_path()` detects format by extension only. A `photo.png` that's actually JPEG gets mislabeled in the output `format` field.

**Fix:** Replaced extension-only detection with `ImageReader::open()` + `with_guessed_format()`, which reads magic bytes to detect the actual format. Falls back to extension-based detection only if content sniffing fails.

**Test:** `test_format_detected_by_content` — copies `test.png` fixture to `test_misnamed.jpg`, decodes it, asserts format is `Png` not `Jpeg`.

### M10: EXIF field presence check incomplete

**File:** `crates/photon-core/src/pipeline/metadata.rs`

**Problem:** Images with only `iso`, `aperture`, `shutter_speed`, `focal_length`, or `orientation` EXIF data had their EXIF silently dropped. The presence check only looked at 4 of 10 fields.

**Fix:** Added the remaining 6 fields to the `is_some()` chain: `gps_longitude`, `iso`, `aperture`, `shutter_speed`, `focal_length`, `orientation`.

---

## Incidental Fix

**Pre-existing clippy lint** in `crates/photon-core/src/pipeline/processor.rs`: replaced `tracker.images_processed() % self.sweep_interval == 0` with `.is_multiple_of()` to satisfy `clippy::manual_is_multiple_of` (new in Rust 1.93).

---

## Metrics

| Metric | Before | After |
|--------|--------|-------|
| Known MEDIUM bugs | 10 | **0** |
| Tests | 164 | **185** (+21) |
| Clippy warnings | 0 | **0** |
| Formatting violations | 0 | **0** |
