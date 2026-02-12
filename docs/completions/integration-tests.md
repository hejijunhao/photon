# Integration Tests — Completion Log

**Date:** 2026-02-12
**Scope:** Item A from remaining improvements (`docs/executing/remaining-improvements.md`)
**Assessment reference:** Issue #2 — "No tests that exercise the full pipeline end-to-end."
**Baseline:** 123 tests passing (all unit tests), zero clippy warnings
**Final:** 133 tests passing (+10 integration tests), zero clippy warnings

---

## Problem

All 123 tests were unit tests for individual modules. Nothing verified that `ImageProcessor::process()` produces a correct `ProcessedImage` from a real image file. A regression in how modules connect (e.g., wrong field mapping between decode and output, or embedding/tagging opt-in breaking) would go undetected.

## Solution

Created `crates/photon-core/tests/integration.rs` — a single integration test file with 10 `#[tokio::test]` functions that exercise `ImageProcessor` directly (the core library, not the CLI). All tests use the shared fixtures at `tests/fixtures/images/` and run without ML models.

### File added

| File | Purpose |
|------|---------|
| `crates/photon-core/tests/integration.rs` | 10 end-to-end integration tests |

### Tests implemented

| Test | What it verifies |
|------|-----------------|
| `full_pipeline_without_models` | Decode, EXIF, hash, thumbnail all populate correctly; embedding/tags empty without models; description is None |
| `full_pipeline_skips_thumbnail` | `ProcessOptions::skip_thumbnail` produces `None` thumbnail while other fields remain populated |
| `full_pipeline_skips_perceptual_hash` | `ProcessOptions::skip_perceptual_hash` produces `None` while thumbnail still generates |
| `process_multiple_formats` | Process all 4 fixtures (`test.png`, `beach.jpg`, `dog.jpg`, `car.jpg`) — all succeed with correct format strings |
| `process_nonexistent_file` | Returns `PhotonError::Pipeline(PipelineError::FileNotFound)` with correct path |
| `process_rejects_oversized_dimensions` | Returns `PipelineError::ImageTooLarge` when `max_image_dimension = 1` |
| `discover_finds_fixtures` | `discover()` returns all 4 test images from fixtures directory |
| `output_roundtrip_json` | Process → serde_json serialize → deserialize → all fields match |
| `output_roundtrip_jsonl` | Process 2 images → OutputWriter JSONL → parse each line → content hashes match |
| `deterministic_content_hash` | Process same file twice, assert both content_hash and perceptual_hash are identical |

### Design decisions

1. **`ImageTooLarge` instead of `FileTooLarge`** — The plan specified a `FileTooLarge` test, but the validator uses integer division (`metadata.len() / (1024 * 1024)`), so all test fixtures (< 1 MB) round to `size_mb = 0` and can never trigger the error. Testing `ImageTooLarge` with `max_image_dimension = 1` is a reliable alternative that still validates the limit-checking pipeline stage.

2. **Deterministic hash test (bonus)** — Not in the original plan. Catches accidental non-determinism in the pipeline (e.g., if timestamps or random seeds ever leak into hash computation).

3. **OutputRecord wrapping for JSONL** — The JSONL roundtrip test uses `OutputRecord::Core(Box::new(...))` to match the actual output format used by the CLI's dual-stream output model. This ensures the tagged enum serialization (`{"type":"core",...}`) roundtrips correctly.

4. **Single file** — All 10 tests grouped in one file. Cargo compiles each file in `tests/` as a separate binary, so grouping avoids 10 separate compile units and keeps build times down.

### Production code changes

None. All tests use existing public APIs only.

### Acceptance criteria status

| Criteria | Status |
|----------|--------|
| All integration tests pass in CI without models installed | Done — no models required |
| `cargo test -p photon-core` runs both unit and integration tests | Done — 123 unit + 10 integration |
| No changes to production code required | Done — zero production changes |

---

## Fixture path resolution

Tests use the same `CARGO_MANIFEST_DIR` pattern established in the benchmark fix (v0.4.12):

```rust
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/images")
        .join(name)
}
```

This resolves correctly regardless of working directory (crate root, workspace root, or CI runner).
