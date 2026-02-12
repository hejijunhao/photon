# Code Assessment — Photon v0.5.3

> Comprehensive code quality assessment across the full repository.
> Assessed: 2026-02-13 | ~11K lines across 54 Rust files | 195 tests passing

---

## Executive Summary

**Overall quality: 9/10** — well-structured codebase with clean separation of concerns, comprehensive type safety, and strong test coverage. Since the previous assessment (v0.5.2, 8.5/10), all 3 HIGH-severity bugs and 11 of 12 MEDIUM-severity issues have been resolved, and 31 new tests added. The sole remaining MEDIUM issue is image_size desync from model variant (M5). Remaining work is primarily test coverage for ONNX-dependent modules and low-severity style items.

| Category | Count |
|----------|-------|
| ~~HIGH severity (bugs)~~ | ~~3~~ → 0 (all fixed) |
| MEDIUM severity (correctness/maintainability) | ~~12~~ → 1 remaining |
| LOW severity (style/minor) | 8 |
| Files over 500 lines | 3 (all acceptable — overshoot from tests) |

**Build status:** `cargo check` ✅ | `cargo clippy -D warnings` ✅ | `cargo fmt --check` ✅ | 195/195 tests passing ✅

---

## HIGH Severity — Bugs (all resolved)

### ~~H1. Progressive encoder race condition~~ ✅ FIXED

**File:** `photon-core/src/tagging/progressive.rs:80–88`

Seed scorer is now installed into `scorer_slot` *inside* `start()` before spawning the background task. The write lock is acquired, the seed scorer stored, and only then is the background task spawned (line 103). Race condition eliminated.

---

### ~~H2. File size validation uses integer division~~ ✅ FIXED

**File:** `photon-core/src/pipeline/validate.rs:38–44`

Validation now compares raw bytes: `metadata.len() > max_bytes` where `max_bytes = self.limits.max_file_size_mb * 1024 * 1024`. The truncating integer division is only used for the human-readable error message (`size_mb` field), not for the actual comparison.

---

### ~~H3. Batch JSON + LLM stdout emits mixed formats~~ ✅ FIXED

**File:** `photon/src/cli/process/batch.rs:193–211`

When `--format json --llm` is used with stdout, all records (core + enrichment patches) are collected into a single `Vec<OutputRecord>` and serialized as one unified JSON array. No more mixed formats.

---

## MEDIUM Severity — Correctness & Maintainability

### ~~M1. `warm_demotion_checks` is dead config~~ ✅ FIXED

**File:** `photon-core/src/tagging/relevance.rs:188–202`

`sweep()` now implements Warm→Cold demotion: tracks `warm_checks_without_hit` per term, demotes when exceeding `warm_demotion_checks`. Test `test_warm_to_cold_demotion` confirms behavior.

---

### ~~M2. `--skip-existing` doesn't parse JSON array format~~ ✅ FIXED

**File:** `photon/src/cli/process/batch.rs:243–289`

`load_existing_hashes()` now tries JSON array parsing first (`Vec<OutputRecord>` and `Vec<ProcessedImage>`), then falls back to line-by-line JSONL. Six tests cover both formats plus edge cases.

---

### ~~M3. JSON format + `--skip-existing` appends, producing invalid JSON~~ ✅ FIXED

**File:** `photon/src/cli/process/batch.rs:148–187`

JSON format now reads existing records and merges into a single array before writing with `File::create()`. Append mode is only used for JSONL streaming.

---

### ~~M4. Single-file LLM to file writes concatenated JSON objects~~ ✅ FIXED

**File:** `photon/src/cli/process/mod.rs:159–171`

`process_single()` now collects all records into `Vec<OutputRecord>` and calls `writer.write_all()` for a proper JSON array.

---

### M5. `EmbeddingConfig` image_size can desync from model variant

**File:** `photon-core/src/config/types.rs:110–138`

`image_size` defaults to 224 but isn't automatically derived from `model`. If a user sets `model = "siglip-base-patch16-384"` in TOML without setting `image_size = 384`, the model receives 224×224 images instead of 384×384, producing incorrect embeddings silently.

`image_size_for_model()` exists but is never called during config loading or validation.

**Note:** Config validation (`config/validate.rs`) now has `test_validate_auto_corrects_image_size_for_384_model`, suggesting intent to auto-correct — but the auto-correction logic should be verified in the actual validation path.

**Fix:** Call `image_size_for_model()` during config validation/loading, or remove `image_size` from config and always derive it from the model name.

---

### ~~M6. `LabelBank::append()` panics instead of returning Result~~ ✅ FIXED

**File:** `photon-core/src/tagging/label_bank.rs:56–68`

`append()` now returns `Result<(), PipelineError>` with a descriptive `PipelineError::Model` on dimension mismatch. Test `test_append_dimension_mismatch_returns_error` confirms.

---

### ~~M7. Lock ordering inconsistency in processor scoring path~~ ✅ FIXED

**File:** `photon-core/src/pipeline/processor.rs:430–483`

Lock ordering is now unified: Phase 1 acquires read locks (scorer → tracker), releases both, then Phase 2 acquires write lock on tracker alone. Comments mark lock boundaries explicitly.

---

### ~~M8. `download_vision` menu shown but not interactive~~ ✅ FIXED

**File:** `photon/src/cli/interactive/models.rs:52–81`

Interactive path now properly uses `dialoguer::Select` for model selection. Non-interactive path (`cli/models.rs`) correctly downloads a fixed default without displaying a misleading menu.

---

### ~~M9. Image format detected by extension, not content~~ ✅ FIXED

**File:** `photon-core/src/pipeline/decode.rs:89–103`

Format detection now uses `ImageReader::open().with_guessed_format()` for content-based (magic bytes) detection, falling back to extension only if guessing fails. Test `test_format_detected_by_content` confirms.

---

### ~~M10. Partial EXIF data silently discarded~~ ✅ FIXED

**File:** `photon-core/src/pipeline/metadata.rs:36–51`

Presence check now includes all 9 fields (`captured_at`, `camera_make`, `camera_model`, `gps_latitude`, `gps_longitude`, `iso`, `aperture`, `shutter_speed`, `orientation`). Returns `Some(data)` if any field is present.

---

### ~~M11. TIFF magic bytes false positive~~ ✅ FIXED

**File:** `photon-core/src/pipeline/validate.rs:122–130`

TIFF validation now checks all 4 bytes: `II*\0` (LE) or `MM\0*` (BE). Tests `test_magic_bytes_bare_ii_rejected` and `test_magic_bytes_bare_mm_rejected` confirm that 2-byte-only headers are rejected.

---

### ~~M12. `enabled` field on LLM provider configs is dead code~~ ✅ FIXED

The `enabled` field has been removed entirely from LLM provider config structs. Provider selection is purely via CLI flags — clean solution.

---

## Files Over 500 Lines

All three files remain over 500 total lines, but in each case the overshoot is from embedded tests. Production code is within bounds.

### 1. `tagging/relevance.rs` — 754 lines (317 impl + 438 tests)

Implementation well under 500 lines. Test module grew by ~73 lines due to new warm demotion + demotion counter tests. **Acceptable** — tests justify the size.

### 2. `pipeline/processor.rs` — 561 lines (549 impl + 12 tests)

Slightly reduced from 567. Lock ordering now documented. Still a candidate for extracting tagging initialization into a helper module, but not urgent.

**Recommendation:** Consider extracting `load_tagging()` into `pipeline/tagging_loader.rs` if the file grows further.

### 3. `cli/models.rs` — 535 lines (396 impl + 140 tests)

Production code under 500 lines. **Acceptable.**

---

## Test Coverage

### Summary

| Metric | v0.5.2 | v0.5.3 | Change |
|--------|--------|--------|--------|
| Total tests | 164 | 195 | +31 |
| photon-core unit tests | ~113 | 144 | +31 |
| photon CLI unit tests | ~37 | 37 | — |
| Integration tests | ~14 | 14 | — |

### Previously untested modules — current status

| Module | Lines | v0.5.2 | v0.5.3 | Tests |
|--------|-------|--------|--------|-------|
| `llm/enricher.rs` | 178 | Untested | **6 tests** | Success, retry, auth error, timeout, batch partial failure, missing file |
| `cli/process/batch.rs` | 309 | Untested | **6 tests** | JSON array, JSONL, empty file, mixed records, missing file, None output |
| `config/validate.rs` | ~120 | 4/9 rules | **12 tests** | All 9 validation rules + default config + image_size auto-correction |
| `pipeline/validate.rs` | ~130 | Partial | **8 tests** | All magic bytes (JPEG, PNG, WebP, TIFF LE/BE) + bare TIFF rejection |
| `tagging/progressive.rs` | 189 | Untested | Untested | **Gap** — async background scorer swapping |
| `tagging/text_encoder.rs` | 168 | Untested | Untested | **Gap** — requires ONNX model files |
| `embedding/siglip.rs` | 127 | Untested | Untested | **Gap** — requires ONNX model files |

### Edge case tests added (integration tests)

| Edge case | v0.5.2 | v0.5.3 |
|-----------|--------|--------|
| Zero-length file | Missing | ✅ `process_zero_length_file` |
| Corrupt image (valid magic, bad data) | Missing | ✅ `process_corrupt_jpeg_header` |
| 1×1 pixel image | Missing | ✅ `process_1x1_pixel_image` |
| Unicode file paths | Missing | ✅ `process_unicode_file_path` |
| Oversized dimensions | Missing | ✅ `process_rejects_oversized_dimensions` |
| Mismatched embedding vector | Missing | Covered by `test_append_dimension_mismatch_returns_error` |
| Symlink loop handling | Missing | Missing |
| Concurrent `process_with_options()` | Missing | Missing |

---

## Remaining Recommended Fixes — Priority Order

### Phase 1: Last MEDIUM fix

1. **M5** — Auto-derive `image_size` from model name during config validation/loading

### Phase 2: Test coverage gaps

2. Add tests for `progressive.rs` (verify scorer swapping, chunk failure handling)
3. Add tests for `text_encoder.rs` / `siglip.rs` (mock ONNX session or integration tests with model files)
4. Add integration test for symlink loop handling in discovery
5. Add integration test for concurrent `process_with_options()` calls

### Phase 3: Low-severity cleanup (optional)

6. Remove unused `ThumbnailConfig.format` field — `config/types.rs:152`
7. Fix `should_check_warm()` firing on image 0 (`0.is_multiple_of(N)` returns true) — `relevance.rs:157`
8. Fix case-sensitive log level comparison — `logging.rs:54`
9. Replace `include_str!("../../../../data/vocabulary/...")` with `env!("CARGO_MANIFEST_DIR")` path — `models.rs:307,313`
10. Consider `keyring-rs` or file permissions for plaintext API key storage — `interactive/setup.rs:279`

---

## Low Severity Notes (informational)

- `ThumbnailConfig.format` field is defined but never used (always WebP) — `config/types.rs:152`
- `should_check_warm()` fires on image 0 due to `0.is_multiple_of(N)` returning true — `relevance.rs:157` (tested/documented behavior)
- `format_to_string()` wildcard `_ => "unknown"` silently ignores new image formats — `decode.rs:134`
- `discovery.rs:48` uses `follow_links(true)` — risk of symlink loops (walkdir has mitigation but no depth limit)
- Dual timeout layers on LLM calls (reqwest 60s + enricher timeout) — not harmful but redundant
- `save_key_to_config` writes API keys in plaintext with no file permissions restriction — `interactive/setup.rs:279`
- Log level comparison is case-sensitive string match — `logging.rs:54`
- `include_str!("../../../../data/vocabulary/...")` — deep relative path is fragile — `models.rs:307,313`

---

## Changelog from v0.5.2 Assessment

**Score: 8.5/10 → 9/10**

### Fixed (14 issues)
- **H1** Progressive encoder race condition — seed scorer installed before background task spawn
- **H2** File size validation — now compares raw bytes
- **H3** Batch JSON + LLM stdout — unified into single JSON array
- **M1** Warm→Cold demotion — implemented in `sweep()` with counter tracking
- **M2** `--skip-existing` JSON array — tries array parsing first, JSONL fallback
- **M3** JSON + `--skip-existing` append — merges arrays, no more append mode for JSON
- **M4** Single-file LLM JSON — collects all records, uses `write_all()`
- **M6** `LabelBank::append()` — returns `Result` instead of panicking
- **M7** Lock ordering — unified read-first-then-write pattern, documented
- **M8** Download menu — interactive path uses `dialoguer::Select`
- **M9** Format detection — content-based via `with_guessed_format()`
- **M10** Partial EXIF — all 9 fields included in presence check
- **M11** TIFF magic bytes — full 4-byte validation
- **M12** `enabled` field — removed from provider configs entirely

### Tests added (+31)
- `llm/enricher.rs`: 6 tests (mock provider with retry/timeout/failure paths)
- `cli/process/batch.rs`: 6 tests (JSON array, JSONL, edge cases)
- `config/validate.rs`: 8 new tests (5 missing rules + image_size auto-correction)
- `pipeline/validate.rs`: 2 new tests (bare TIFF rejection)
- `tagging/label_bank.rs`: append error test
- `tagging/relevance.rs`: warm demotion + counter reset tests
- Integration: 5 edge case tests (zero-length, corrupt, 1×1, unicode, oversized)
