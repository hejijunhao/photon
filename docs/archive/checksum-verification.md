# Model Download Checksum Verification — Completion Log

**Date:** 2026-02-12
**Scope:** Item B from remaining improvements (`docs/executing/remaining-improvements.md`)
**Assessment reference:** Issue #5 — "Downloaded ONNX models (~350–441 MB) are not verified against checksums."
**Baseline:** 133 tests passing, zero clippy warnings
**Final:** 136 tests passing (+3 verification tests), zero clippy warnings

---

## Problem

`download_file()` in `models.rs` streamed bytes to disk but never verified integrity. A truncated transfer, disk error, or CDN corruption would leave a broken `.onnx` file that causes confusing inference failures (e.g., "invalid protobuf" from ONNX Runtime) with no indication of the actual cause.

## Solution

Embedded known BLAKE3 checksums as compile-time constants and added post-download verification. On mismatch, the corrupt file is automatically removed so the next download attempt starts fresh.

### Design

- **BLAKE3 over SHA256**: Reuses the existing `blake3` crate (already used for image content hashing via `Hasher::content_hash()`). No new dependencies.
- **Streaming hash**: `Hasher::content_hash()` uses a 64 KB `BufReader` loop, so verifying a 441 MB file uses only 64 KB of heap — safe for the large model files.
- **Embedded constants**: Same trade-off as `cargo` and `go mod` checksum databases. If HuggingFace updates a model (rare), the hash becomes stale, but the clear error message tells the user what happened.

### Files changed

| File | Changes |
|------|---------|
| `crates/photon/src/cli/models.rs` | Added `blake3` field to `ModelVariant`, 4 checksum constants, `verify_blake3()` function, updated `download_file()` signature and all 3 call sites, added 3 unit tests |

### Checksums embedded

| File | Size | BLAKE3 |
|------|------|--------|
| `siglip-base-patch16/visual.onnx` | 355 MB | `05cd313b67db70acd8e800cd4c16105c3ebc4c385fe6002108d24ea806a248be` |
| `siglip-base-patch16-384/visual.onnx` | 356 MB | `9a4dcfd0c21b8e4d143652d1e566da52222605b564979723383f6012b53dd0df` |
| `text_model.onnx` | 421 MB | `fe62b4096a9e5c3ce735b771472c9e3faac6ddeceebab5794a0a5ce17ee171dd` |
| `tokenizer.json` | 2.3 MB | `cf171f3552992f467891b9d59be5bde1256ffe1344c62030d4bf0f87df583906` |

Checksums computed with `b3sum` against files downloaded from HuggingFace (`Xenova/siglip-base-patch16-224` and `Xenova/siglip-base-patch16-384` repos).

### Key implementation details

1. **`ModelVariant.blake3`** — new `&'static str` field on the vision model variant struct, holding the expected hex digest
2. **`TEXT_ENCODER_BLAKE3` / `TOKENIZER_BLAKE3`** — standalone constants for the shared (non-variant) model files
3. **`download_file()` signature change** — now accepts `expected_blake3: Option<&str>` as the 4th parameter; verification runs after `file.flush()` completes
4. **`verify_blake3(path, expected)`** — standalone function that:
   - Computes the BLAKE3 hash using `Hasher::content_hash()` (streaming, 64 KB buffer)
   - On match: logs at `debug` level with first 16 chars of hash
   - On mismatch: removes the file, returns error with expected vs actual hashes and "try downloading again" guidance

### Error message on mismatch

```
Checksum mismatch for /Users/foo/.photon/models/siglip-base-patch16/visual.onnx:
  expected: 05cd313b67db70ac...
  actual:   a1b2c3d4e5f6a7b8...
Corrupt file removed — try downloading again.
```

### Tests added

| Test | Validates |
|------|-----------|
| `verify_blake3_correct_hash` | Correct hash passes, file preserved |
| `verify_blake3_wrong_hash_removes_file` | Wrong hash returns error, file is deleted, error message contains "Checksum mismatch" and "Corrupt file removed" |
| `verify_blake3_missing_file` | Nonexistent file returns error gracefully |

### Acceptance criteria status

| Criteria | Status |
|----------|--------|
| All model files verified after download | Done — all 4 files verified via BLAKE3 |
| Corrupt/truncated files detected and removed with clear error | Done — `verify_blake3()` removes file and returns descriptive error |
| Existing `download_file` signature updated (non-breaking) | Done — only called internally, `Option<&str>` parameter added |
| No new dependencies | Done — reuses existing `blake3` via `Hasher::content_hash` |
