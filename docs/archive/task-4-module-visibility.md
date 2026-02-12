# Task 4: Tighten Module Visibility in lib.rs

**Status:** Complete
**Date:** 2026-02-13

## Summary

Changed internal modules in `photon-core/src/lib.rs` from `pub mod` to `pub(crate) mod`, preventing downstream crates from reaching into implementation details. Added missing re-exports to `lib.rs` so the CLI crate accesses all types through the public API surface.

## Changes

### lib.rs visibility

| Module | Before | After | Rationale |
|--------|--------|-------|-----------|
| `config` | `pub mod` | `pub mod` | Has consumer types (`AnthropicConfig`, `LlmConfig`, etc.) |
| `error` | `pub mod` | `pub mod` | Has consumer types (`PipelineError` variants) |
| `types` | `pub mod` | `pub mod` | Has consumer types (`OutputRecord`, `ProcessedImage`, etc.) |
| `embedding` | `pub mod` | `pub(crate) mod` | Internal; `EmbeddingEngine` re-exported |
| `llm` | `pub mod` | `pub(crate) mod` | Internal; key types re-exported |
| `math` | `pub mod` | `pub(crate) mod` | Pure internal utility |
| `output` | `pub mod` | `pub(crate) mod` | Internal; `OutputFormat`/`OutputWriter` re-exported |
| `pipeline` | `pub mod` | `pub(crate) mod` | Internal; key types re-exported |
| `tagging` | `pub mod` | `pub(crate) mod` | Pure internal |

### New re-exports added to lib.rs

```rust
pub use llm::{EnrichOptions, EnrichResult, Enricher, LlmProviderFactory};
pub use pipeline::{DiscoveredFile, FileDiscovery, Hasher, ImageProcessor, ProcessOptions};
```

### Submodule visibility tightened

All submodules within `embedding/`, `llm/`, `pipeline/`, `tagging/` changed from `pub mod` to `pub(crate) mod`.

### Unused re-exports removed from mod.rs files

- `llm/mod.rs`: removed `ImageInput`, `LlmProvider`, `LlmRequest`, `LlmResponse` (used internally via direct paths)
- `pipeline/mod.rs`: removed `DecodedImage`, `ImageDecoder`, `MetadataExtractor`, `ThumbnailGenerator`, `Validator` (used by processor.rs via `super::` paths)
- `tagging/mod.rs`: removed `Pool`, `RelevanceConfig`, `RelevanceTracker` (used by scorer.rs via `super::` paths)

### Dead code removed

| Item | File | Reason |
|------|------|--------|
| `SIGLIP_IMAGE_SIZE` constant | `embedding/preprocess.rs` | Never referenced; image sizes configured dynamically via `EmbeddingConfig` |
| `to_json()` function | `output.rs` | Never called; `OutputWriter` used instead |
| `to_jsonl()` function | `output.rs` | Never called; `OutputWriter` used instead |
| `encode()` method | `tagging/text_encoder.rs` | Never called; callers use `encode_batch()` directly |

### Test-only items gated with `#[cfg(test)]`

- `ThumbnailGenerator::generate_bytes()` — only called in `test_thumbnail_bytes()`
- `LabelBank::from_raw()` — only called in scorer test helpers
- `NeighborExpander::find_siblings()` — only called in neighbor tests

### CLI imports updated

All 12 `photon_core::{pipeline,llm,embedding,output}::` paths in the CLI crate replaced with re-export paths (e.g., `photon_core::Enricher` instead of `photon_core::llm::Enricher`).

## Files modified

- `crates/photon-core/src/lib.rs` — visibility + re-exports
- `crates/photon-core/src/embedding/mod.rs` — submodule visibility
- `crates/photon-core/src/embedding/preprocess.rs` — remove dead constant
- `crates/photon-core/src/llm/mod.rs` — submodule visibility + prune re-exports
- `crates/photon-core/src/output.rs` — remove dead functions
- `crates/photon-core/src/pipeline/mod.rs` — submodule visibility + prune re-exports
- `crates/photon-core/src/pipeline/thumbnail.rs` — `#[cfg(test)]` gate
- `crates/photon-core/src/tagging/mod.rs` — submodule visibility + prune re-exports
- `crates/photon-core/src/tagging/label_bank.rs` — `#[cfg(test)]` gate
- `crates/photon-core/src/tagging/neighbors.rs` — `#[cfg(test)]` gate
- `crates/photon-core/src/tagging/text_encoder.rs` — remove dead method
- `crates/photon-core/tests/integration.rs` — use re-exports
- `crates/photon/src/cli/models.rs` — use re-exports
- `crates/photon/src/cli/process/mod.rs` — use re-exports
- `crates/photon/src/cli/process/batch.rs` — use re-exports
- `crates/photon/src/cli/process/setup.rs` — use re-exports
- `crates/photon/src/cli/process/enrichment.rs` — use re-exports
- `crates/photon/src/cli/interactive/process.rs` — use re-exports

## Verification

- 185 tests passing (37 CLI + 138 core + 10 integration)
- Zero clippy warnings (`-D warnings`)
- Zero formatting violations
