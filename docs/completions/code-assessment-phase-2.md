# Code Assessment Fix — Phase 2: Text Encoder Unwrap

> Completed: 2026-02-12
> Source plan: `docs/executing/code-assessment-fixes.md`

---

## Summary

Replaced a panic-on-empty `.unwrap()` with a proper `Result` return via `.ok_or_else()` in the single-text encoding convenience method.

## Bug Details

**File:** `crates/photon-core/src/tagging/text_encoder.rs:154-157`
**Function:** `SigLipTextEncoder::encode()`
**Severity:** LOW — would only trigger if ONNX returned an empty tensor for a valid single input, which is unlikely but not impossible under resource pressure or model corruption

**Root cause:** `encode()` delegates to `encode_batch(&[text])` and then calls `.unwrap()` on the iterator's first element. If the batch result were somehow empty (e.g., ONNX session returns zero rows), this would panic instead of propagating a typed error.

## Changes

**File:** `crates/photon-core/src/tagging/text_encoder.rs` (1 line changed)

Before:
```rust
Ok(batch.into_iter().next().unwrap())
```

After:
```rust
batch.into_iter().next().ok_or_else(|| PipelineError::Model {
    message: "Text encoder returned empty result for single input".to_string(),
})
```

The `Ok(...)` wrapper was removed because `.ok_or_else()` already returns `Result`.

## Verification

- `cargo test -p photon-core` — 123 tests pass
- `cargo clippy --workspace -- -D warnings` — zero warnings
- No files outside `text_encoder.rs` were modified
