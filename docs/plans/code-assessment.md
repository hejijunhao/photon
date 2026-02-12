# Code Assessment — Photon v0.5.2

> Comprehensive code quality assessment across the full repository.
> Assessed: 2026-02-12 | ~10.3K lines across 54 Rust files | 164 tests passing

---

## Executive Summary

**Overall quality: 8.5/10** — well-structured codebase with clean separation of concerns, comprehensive type safety, and good test coverage of core algorithms. The main issues are: (1) a race condition in progressive encoding, (2) several output format edge cases in the CLI batch/LLM paths, (3) three files over 500 production lines, and (4) missing Warm→Cold demotion in the relevance tracker.

| Category | Count |
|----------|-------|
| HIGH severity (bugs) | 3 |
| MEDIUM severity (correctness/maintainability) | 12 |
| LOW severity (style/minor) | 10+ |
| Files over 500 lines | 3 |

**Build status:** `cargo check` ✅ | `cargo clippy -D warnings` ✅ | `cargo fmt --check` ✅ | 164/164 tests passing ✅

---

## HIGH Severity — Bugs

### H1. Progressive encoder race condition

**File:** `photon-core/src/tagging/progressive.rs:89–107`

`ProgressiveEncoder::start()` spawns a background tokio task (line 89) and then returns the `seed_scorer` to the caller. The caller installs the seed scorer into the shared `scorer_slot` (processor.rs:195). But the background task reads from `scorer_slot` (line 106) to clone the running label bank — it may execute *before* the caller installs the seed scorer.

If the background task wins the race, `running_bank` starts empty, and the final cached label bank will be **missing all seed terms**. Subsequent runs will load this incomplete cache, producing degraded tagging until manual cache deletion.

**Mitigation:** In practice, the `spawn_blocking` inside `background_encode` adds enough latency that the caller almost always wins. But this is not guaranteed.

**Fix:** Install the seed scorer into `scorer_slot` *inside* `start()` before spawning the background task, or use a `tokio::sync::Notify`/barrier to ensure the background task waits until the seed scorer is installed.

---

### H2. File size validation uses integer division

**File:** `photon-core/src/pipeline/validate.rs:38–39`

```rust
let size_mb = metadata.len() / (1024 * 1024);
if size_mb > self.limits.max_file_size_mb {
```

Integer division truncates — a 100.99 MB file computes as 100 MB and passes a 100 MB limit. The effective limit is always `max_file_size_mb + 1 MB - 1 byte`. More critically, if `max_file_size_mb` is 0, **all files under 1 MB pass**, because `0 > 0` is false.

**Fix:** Compare raw bytes: `metadata.len() > self.limits.max_file_size_mb * 1024 * 1024` (handling overflow), or use floating-point comparison for the error message only.

---

### H3. Batch JSON + LLM stdout emits mixed formats

**File:** `photon/src/cli/process/batch.rs:182–200`

When processing a batch with `--llm` to stdout in JSON format:
1. Lines 184–188: Core records are printed as a pretty JSON array
2. Lines 196–199: `run_enrichment_stdout` then prints enrichment patches as individual JSON objects

The result is a JSON array followed by loose JSON objects — not valid JSON and not valid JSONL. The consumer gets mixed formats on stdout.

**Fix:** Either collect enrichment patches and emit a single combined JSON array, or switch to JSONL for all dual-stream stdout output.

---

## MEDIUM Severity — Correctness & Maintainability

### M1. `warm_demotion_checks` is dead config — no Warm→Cold demotion path

**File:** `photon-core/src/tagging/relevance.rs:72, 161–199`

`RelevanceConfig` defines `warm_demotion_checks: u32` (default 50), but the `sweep()` method never demotes Warm terms to Cold. Once a term enters the Warm pool, it stays forever unless promoted to Active. The config field misleads users into thinking demotion exists.

Over long runs, the Warm pool grows monotonically (via neighbor expansion promoting Cold→Warm), increasing scoring cost without bound.

**Fix:** Implement Warm→Cold demotion in `sweep()`: track consecutive warm checks with no hits per term, demote when exceeding `warm_demotion_checks`.

---

### M2. `--skip-existing` doesn't parse JSON array format

**File:** `photon/src/cli/process/batch.rs:224–256`

`load_existing_hashes()` reads the file line-by-line and parses each line as JSONL. If the previous output was a JSON array (`[{...}, {...}]`), line-by-line parsing fails silently, and `--skip-existing` reprocesses everything.

**Fix:** Try parsing the full file content as a JSON array first, then fall back to line-by-line JSONL parsing.

---

### M3. JSON format + `--skip-existing` appends, producing invalid JSON

**File:** `photon/src/cli/process/batch.rs:152–156`

When using JSON format (not JSONL) with `--skip-existing`, the code opens the file with `append(true)`. Appending a new JSON array to an existing JSON file produces two concatenated arrays — invalid JSON.

**Fix:** Either require JSONL for `--skip-existing`, or re-parse and merge the existing JSON array before writing.

---

### M4. Single-file LLM to file writes concatenated JSON objects

**File:** `photon/src/cli/process/mod.rs:159–172`

In `process_single` with LLM and file output, the code writes a core `OutputRecord` followed by enrichment patches via `writer.write()`. For JSON format, this produces multiple top-level JSON objects — not a valid JSON document.

**Fix:** Collect all records and use `writer.write_all()` for JSON format to produce a proper array.

---

### M5. `EmbeddingConfig` image_size can desync from model variant

**File:** `photon-core/src/config/types.rs:110–138`

`image_size` defaults to 224 but isn't automatically derived from `model`. If a user sets `model = "siglip-base-patch16-384"` in TOML without setting `image_size = 384`, the model receives 224×224 images instead of 384×384, producing incorrect embeddings silently.

`image_size_for_model()` exists but is never called during config loading or validation.

**Fix:** Call `image_size_for_model()` during config validation/loading, or remove `image_size` from config and always derive it from the model name.

---

### M6. `LabelBank::append()` panics instead of returning Result

**File:** `photon-core/src/tagging/label_bank.rs:56–58`

`append()` uses `assert_eq!` for dimension mismatch, which panics. This is called from `background_encode()` in the progressive encoder (progressive.rs:138). A panic here would poison the `scorer_slot` RwLock, causing all subsequent scoring to panic.

In practice, dimensions always match (same model), but a library API should return `Result`, not panic.

**Fix:** Replace `assert_eq!` with a `Result` return, propagate the error to the background task.

---

### M7. Lock ordering inconsistency in processor scoring path

**File:** `photon-core/src/pipeline/processor.rs:437–478`

The scoring path acquires locks in two different orders:
- **Phase 1** (lines 438–444): read `scorer_lock` → read `tracker_lock`
- **Phase 2** (lines 450–462): write `tracker_lock` → read `scorer_lock`

While the Phase 1 locks are released before Phase 2 (no nesting), the pattern is fragile. If future changes introduce concurrent access with different lock ordering, deadlock becomes possible.

**Fix:** Document the lock ordering contract. Consider using a single `RwLock<(TagScorer, RelevanceTracker)>` to eliminate ordering concerns.

---

### M8. `download_vision` menu shown but not interactive

**File:** `photon/src/cli/models.rs:206–217`

The `Download` command prints a 3-option model selection menu but immediately downloads option 1 (`&[0]`) without user input. The printed menu is misleading — it suggests a choice but doesn't offer one.

**Fix:** Either remove the menu display and just state what will be downloaded, or use `dialoguer::Select` to capture the user's choice (the interactive path already handles this correctly).

---

### M9. Image format detected by extension, not content

**File:** `photon-core/src/pipeline/decode.rs:91`

`ImageFormat::from_path()` uses the file extension only. A file named `photo.png` that is actually JPEG will report format as "png" in the output, even though `image::open()` decodes it correctly using content-based detection.

**Fix:** Use `image::ImageReader::open(path)?.with_guessed_format()?.format()` for content-based format detection, or use the format detected by `image::open()`.

---

### M10. Partial EXIF data silently discarded

**File:** `photon-core/src/pipeline/metadata.rs:37–44`

The `extract` method returns `None` unless at least one of `captured_at`, `camera_make`, `camera_model`, or `gps_latitude` is present. An image with only `iso`, `aperture`, `orientation`, or `focal_length` has its EXIF data silently dropped.

**Fix:** Include all fields in the presence check, or always return `Some(ExifData)` with whatever fields were found.

---

### M11. TIFF magic bytes false positive

**File:** `photon-core/src/pipeline/validate.rs:122–124`

The TIFF check only verifies the first two bytes (`II` or `MM`), which are too broad — many non-image files start with these bytes. A proper TIFF check requires verifying the magic number at bytes 2–3 (`0x002A`).

**Fix:** `(b'I', b'I', 0x2A, 0x00) || (b'M', b'M', 0x00, 0x2A)` — include the TIFF version number.

---

### M12. `enabled` field on LLM provider configs is dead code in photon-core

**File:** `photon-core/src/llm/provider.rs:151–210`

`LlmProviderFactory::create()` ignores the `enabled` field on provider configs — it creates the provider if the name matches, regardless of `enabled`. The field is only checked in CLI interactive display code.

**Fix:** Either check `enabled` in the factory and return an error if disabled, or remove the `enabled` field from the config structs and document that provider selection is purely via CLI flags.

---

## Files Over 500 Lines — Refactoring Needed

### 1. `tagging/relevance.rs` — 670 lines (305 impl + 365 tests)

The implementation is only ~305 lines and well-organized. The test module is 365 lines. **Borderline** — the overshoot is entirely from thorough tests.

**Recommendation:** Extract tests to `tagging/relevance/tests.rs` or leave as-is since the implementation code is well under 500 lines.

---

### 2. `pipeline/processor.rs` — 567 lines

The main `process_with_options()` method is ~213 lines. The `load_tagging` initialization is ~143 lines. Both are substantial.

**Recommendation:** Extract tagging initialization into `pipeline/tagging_loader.rs`. Move the pool-aware scoring block (lines 432–504) into a helper method `score_with_relevance()`.

After split: `processor.rs` ~350 lines, `tagging_loader.rs` ~200 lines.

---

### 3. `cli/models.rs` — 544 lines (404 impl + 140 tests)

Production code is ~404 lines — under 500. The overshoot is from tests.

**Recommendation:** Move tests to a separate file, or leave as-is since production code is within bounds.

---

## Test Coverage Gaps

### Untested modules (zero test coverage)

| Module | Lines | Risk |
|--------|-------|------|
| `llm/enricher.rs` | 178 | **High** — core LLM orchestration with retry/timeout/concurrency |
| `tagging/progressive.rs` | 189 | **High** — async background scorer swapping via RwLock |
| `tagging/text_encoder.rs` | 168 | Medium — requires ONNX model files |
| `embedding/siglip.rs` | 127 | Medium — requires ONNX model files |
| LLM providers (4 files) | ~523 | Medium — HTTP request/response construction |
| `error.rs` hint() logic | 166 | Low — error display formatting |
| `cli/process/batch.rs` | 309 | Medium — batch orchestration logic |

### Partially tested modules

| Module | What's tested | What's missing |
|--------|--------------|----------------|
| `pipeline/processor.rs` | `ProcessOptions::default()` | `process_with_options()` orchestration, pool-aware scoring, neighbor expansion |
| `pipeline/decode.rs` | `format_to_string()` | Async decode with timeout/spawn_blocking |
| `pipeline/metadata.rs` | Missing file case | Actual EXIF extraction from fixture images |
| `pipeline/validate.rs` | Magic byte recognition | Full `validate()` — file-too-large, empty file |
| `config/validate.rs` | 4 of 9 rules | Missing: `buffer_size`, `max_file_size_mb`, `max_image_dimension`, `embed_timeout_ms`, `llm_timeout_ms` |

### Missing edge case tests

- Zero-length file processing
- Corrupt image (valid magic bytes, bad data)
- 1×1 pixel image (degenerate dimensions)
- Mismatched embedding vector length (potential panic in scorer)
- Unicode/special characters in file paths
- Symlink loop handling in file discovery
- Concurrent `process_with_options()` calls (lock contention)

---

## Recommended Fixes — Priority Order

### Phase 1: Bug fixes (HIGH + critical MEDIUM)

1. **H1** — Fix progressive encoder race condition (add synchronization barrier)
2. **H2** — Fix file size validation integer division
3. **H3** — Fix batch JSON + LLM mixed stdout output
4. **M4** — Fix single-file LLM JSON file output
5. **M2** — Fix `--skip-existing` JSON array parsing
6. **M3** — Fix JSON + `--skip-existing` append producing invalid JSON

### Phase 2: Correctness improvements

7. **M1** — Implement Warm→Cold demotion (or remove dead `warm_demotion_checks` config)
8. **M5** — Auto-derive `image_size` from model name during config loading
9. **M6** — Replace `LabelBank::append()` assert with Result
10. **M9** — Use content-based format detection in decode
11. **M10** — Fix partial EXIF data discard
12. **M11** — Improve TIFF magic bytes check

### Phase 3: Refactoring (files over 500 lines)

13. **processor.rs** — Extract tagging initialization + scoring helper (~567→350 lines)
14. **relevance.rs** — Extract test module if desired (~670→305 lines)
15. **models.rs** — Extract test module if desired (~544→404 lines)

### Phase 4: Test coverage

16. Add tests for `enricher.rs` (mock provider, verify retry/timeout/concurrency)
17. Add tests for `progressive.rs` (verify scorer swapping, chunk failure handling)
18. Add tests for batch processing edge cases (JSON array, skip-existing, mixed formats)
19. Add missing config validation tests (5 remaining zero-value rules)
20. Add edge case tests (empty file, corrupt image, 1×1 image, unicode paths)

---

## Low Severity Notes (informational)

- `ThumbnailConfig.format` field is defined but never used (always WebP) — `thumbnail.rs:33`
- `should_check_warm()` fires on image 0 due to `0.is_multiple_of(N)` returning true — `relevance.rs:152`
- `format_to_string()` wildcard `_ => "unknown"` silently ignores new image formats — `decode.rs:127`
- `discovery.rs:48` uses `follow_links(true)` — risk of symlink loops (walkdir has mitigation but no depth limit)
- Dual timeout layers on LLM calls (reqwest 60s + enricher timeout) — not harmful but redundant
- `save_key_to_config` writes API keys in plaintext with no file permissions restriction — `interactive/setup.rs:279`
- Log level comparison is case-sensitive string match — `logging.rs:53`
- `include_str!("../../../../data/vocabulary/...")` — deep relative path is fragile — `models.rs:316`
