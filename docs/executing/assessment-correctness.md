# Assessment Correctness Fixes

> Source: `docs/plans/code-assessment.md` — remaining MEDIUM issues after v0.5.3
> Scope: 10 issues across 4 phases, all functional code changes
> Prerequisite: v0.5.3 (all HIGH-severity bugs already fixed)

---

## Overview

4 phases, ordered by user impact. Each phase is self-contained: implement, test, verify, commit.

| Phase | Items | Files Changed | Risk |
|-------|-------|---------------|------|
| **1** | M2+M3: `--skip-existing` + JSON format | 1 file | **Medium** — changes output behavior |
| **2** | M5, M11, M8, M12: Config & validation | 4 files | **Low** — additive fixes |
| **3** | M1, M6: Tagging subsystem | 2 files | **Low** — internal API |
| **4** | M9, M10: Pipeline accuracy | 2 files | **Low** — output field corrections |

---

## Phase 1 — Fix `--skip-existing` with JSON Format (M2+M3)

**Priority:** Highest — silently reprocesses everything or produces invalid output
**File:** `crates/photon/src/cli/process/batch.rs`

### Problem

Two paired bugs when using `--skip-existing` with `--format json` (not JSONL):

1. **M2 (line 243):** `load_existing_hashes()` reads line-by-line and tries `serde_json::from_str` per line. JSON array files (`[{...}, {...}]`) span multiple lines — line-by-line parsing fails silently, zero hashes loaded, everything reprocessed.

2. **M3 (line 152-153):** When skip-existing is active, the file is opened with `append(true)`. Appending a second JSON array to an existing JSON array file produces `[...][...]` — not valid JSON.

### Implementation

**Step 1 — Fix `load_existing_hashes()` (M2)**

Replace the line-by-line-only parsing with a two-pass approach. At `batch.rs:242`, replace:

```rust
// BEFORE (line 242-258):
let content = std::fs::read_to_string(path)?;
for line in content.lines() {
    let line = line.trim();
    if line.is_empty() { continue; }
    if let Ok(record) = serde_json::from_str::<OutputRecord>(line) { ... }
    if let Ok(image) = serde_json::from_str::<ProcessedImage>(line) { ... }
}
```

```rust
// AFTER:
let content = std::fs::read_to_string(path)?;

// Try JSON array first (handles --format json output)
if let Ok(records) = serde_json::from_str::<Vec<OutputRecord>>(&content) {
    for record in records {
        if let OutputRecord::Core(img) = record {
            hashes.insert(img.content_hash);
        }
    }
    return Ok(hashes);
}
if let Ok(images) = serde_json::from_str::<Vec<ProcessedImage>>(&content) {
    for image in images {
        hashes.insert(image.content_hash);
    }
    return Ok(hashes);
}

// Fall back to line-by-line JSONL parsing
for line in content.lines() {
    let line = line.trim();
    if line.is_empty() { continue; }
    if let Ok(record) = serde_json::from_str::<OutputRecord>(line) {
        if let OutputRecord::Core(img) = record {
            hashes.insert(img.content_hash);
        }
        continue;
    }
    if let Ok(image) = serde_json::from_str::<ProcessedImage>(line) {
        hashes.insert(image.content_hash);
    }
}
```

**Step 2 — Fix JSON append (M3)**

At `batch.rs:148-169`, the non-streaming JSON path opens with `append(true)` when skip-existing is active. Instead, read existing records, merge, and overwrite.

Replace lines 151-156:

```rust
// BEFORE:
let file = if args.skip_existing && output_path.exists() {
    std::fs::OpenOptions::new().append(true).open(output_path)?
} else {
    File::create(output_path)?
};
```

```rust
// AFTER:
// For JSON format with skip-existing, we must merge — appending arrays is invalid JSON.
// Read existing records, then overwrite the file with the combined set.
let mut existing_records: Vec<OutputRecord> = Vec::new();
if args.skip_existing && output_path.exists() {
    let content = std::fs::read_to_string(output_path)?;
    if let Ok(records) = serde_json::from_str::<Vec<OutputRecord>>(&content) {
        existing_records = records;
    }
}
let file = File::create(output_path)?;
```

Then when building `all_records` (line 161), prepend existing records:

```rust
let mut all_records: Vec<OutputRecord> = existing_records;
all_records.extend(results.iter().map(|r| OutputRecord::Core(Box::new(r.clone()))));
```

Note: The JSONL streaming path (line 50-54) is correct — JSONL append is valid. Only the JSON (non-streaming) path needs this fix.

### Tests

Add to the test module in `batch.rs` (or a new `tests/` file):

1. `test_load_existing_hashes_json_array` — write a JSON array file, verify hashes load
2. `test_load_existing_hashes_jsonl` — write JSONL, verify hashes load (regression)
3. `test_load_existing_hashes_empty_file` — empty file returns empty set
4. `test_load_existing_hashes_mixed_records` — JSON array with Core + Enrichment records

### Verification

```bash
cargo test -p photon
cargo clippy --workspace -- -D warnings
```

---

## Phase 2 — Config & Validation Fixes (M5, M11, M8, M12)

### 2A — Auto-derive `image_size` from model name (M5)

**File:** `crates/photon-core/src/config/validate.rs`
**Risk:** Silent wrong embeddings if user sets model to 384 variant without updating image_size

The function `EmbeddingConfig::image_size_for_model()` exists at `config/types.rs:132` but is never called. Add a validation/correction step.

**Option A (recommended) — Auto-correct during validation:**

Add to `validate()` in `config/validate.rs`, after the existing range checks:

```rust
// Auto-derive image_size from model name to prevent desync
let expected_size = EmbeddingConfig::image_size_for_model(&self.embedding.model);
if self.embedding.image_size != expected_size {
    tracing::warn!(
        "Overriding embedding.image_size {} → {} to match model '{}'",
        self.embedding.image_size, expected_size, self.embedding.model
    );
    self.embedding.image_size = expected_size;
}
```

This requires `validate()` to take `&mut self` instead of `&self`. Check whether this is acceptable — if not, do the correction in `Config::load_from()` after deserializing.

**Option B — Remove `image_size` from config entirely:**

Remove the `image_size` field from `EmbeddingConfig`, always derive from model name:

```rust
impl EmbeddingConfig {
    pub fn image_size(&self) -> u32 {
        Self::image_size_for_model(&self.model)
    }
}
```

Then replace all `config.embedding.image_size` reads with `config.embedding.image_size()`. This is cleaner but touches more files.

### 2B — Improve TIFF magic bytes check (M11)

**File:** `crates/photon-core/src/pipeline/validate.rs`, lines 121-123

Replace:

```rust
// BEFORE:
if (header[0] == b'I' && header[1] == b'I') || (header[0] == b'M' && header[1] == b'M') {
    return true;
}
```

```rust
// AFTER — require TIFF version number at bytes 2-3:
if bytes_read >= 4 {
    let is_tiff_le = header[0] == b'I' && header[1] == b'I' && header[2] == 0x2A && header[3] == 0x00;
    let is_tiff_be = header[0] == b'M' && header[1] == b'M' && header[2] == 0x00 && header[3] == 0x2A;
    if is_tiff_le || is_tiff_be {
        return true;
    }
}
```

This eliminates false positives from non-image files that happen to start with `II` or `MM`.

Add test: `test_tiff_magic_bytes` — verify LE/BE TIFF headers pass, bare `II`/`MM` without version bytes fail.

### 2C — Remove dead `enabled` field from LLM configs (M12)

**Files:**
- `crates/photon-core/src/config/types.rs` — remove `pub enabled: bool` from `OllamaConfig`, `HyperbolicConfig`, `AnthropicConfig`, `OpenAiConfig` (lines 328, 351, 378, 401)
- Remove from `Default` impls
- Search for any reads of `.enabled` in the CLI interactive code and remove

Since `LlmProviderFactory::create()` never checks `enabled`, and provider selection is purely via CLI `--llm` flag, this field only misleads config file users.

**Backwards compatibility:** Users with `enabled = true` in their TOML will get a deserialization warning (unknown field). Add `#[serde(deny_unknown_fields)]` is too aggressive — instead, just remove the field. Serde's default behavior ignores unknown fields, so existing configs won't break.

### 2D — Fix download_vision menu display (M8)

**File:** `crates/photon/src/cli/models.rs`

The non-interactive `Download` command (line ~206) prints a 3-option model menu but immediately downloads option `[0]` without waiting for input. Either:

- **Option A:** Remove the printed menu, just log "Downloading Base (224) vision encoder..."
- **Option B:** Accept a `--variant` CLI flag (`224`, `384`, `both`) to let users choose

Option A is simpler and correct for the non-interactive path (the interactive path in `interactive/models.rs` already uses `dialoguer::Select`).

### Verification

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
```

---

## Phase 3 — Tagging Subsystem Fixes (M1, M6)

### 3A — Implement Warm→Cold demotion (M1)

**File:** `crates/photon-core/src/tagging/relevance.rs`

**Problem:** `warm_demotion_checks` config field (line 72, default 50) is dead — `sweep()` never demotes Warm→Cold. Once a term enters Warm (via neighbor expansion's `promote_to_warm()`), it stays forever, causing unbounded Warm pool growth.

**Implementation:**

Add a `warm_checks` counter to `TermStats` (after line 42):

```rust
pub struct TermStats {
    pub hit_count: u32,
    pub score_sum: f32,
    pub last_hit_ts: u64,
    pub pool: Pool,
    /// Consecutive warm sweep checks with no hits (for Warm→Cold demotion)
    pub warm_checks_without_hit: u32,
}
```

Initialize to 0 in `TermStats::new()` / default.

In `sweep()`, update the `Pool::Warm` arm (lines 183-191):

```rust
// BEFORE:
Pool::Warm => {
    if stat.hit_count > 0
        && stat.avg_confidence() >= self.config.promotion_threshold
    {
        stat.pool = Pool::Active;
        newly_promoted.push(i);
    }
}
```

```rust
// AFTER:
Pool::Warm => {
    if stat.hit_count > 0
        && stat.avg_confidence() >= self.config.promotion_threshold
    {
        stat.pool = Pool::Active;
        stat.warm_checks_without_hit = 0;
        newly_promoted.push(i);
    } else {
        stat.warm_checks_without_hit += 1;
        if stat.warm_checks_without_hit >= self.config.warm_demotion_checks {
            stat.pool = Pool::Cold;
            stat.warm_checks_without_hit = 0;
        }
    }
}
```

Reset `warm_checks_without_hit` when a hit is recorded — add to `record_hit()`:

```rust
pub fn record_hit(&mut self, term_index: usize, confidence: f32) {
    let stat = &mut self.stats[term_index];
    stat.hit_count += 1;
    stat.score_sum += confidence;
    stat.last_hit_ts = /* timestamp */;
    stat.warm_checks_without_hit = 0;  // ← add this
}
```

**Tests:**

1. `test_warm_to_cold_demotion` — promote term to Warm, sweep N times with no hits, verify it demotes to Cold
2. `test_warm_hit_resets_demotion_counter` — term in Warm gets a hit, counter resets, doesn't demote
3. `test_warm_promotion_resets_counter` — term promotes to Active, counter resets

### 3B — Replace `LabelBank::append()` assert with Result (M6)

**File:** `crates/photon-core/src/tagging/label_bank.rs`, lines 55-62

Replace:

```rust
// BEFORE:
pub fn append(&mut self, other: &LabelBank) {
    assert_eq!(
        self.embedding_dim, other.embedding_dim,
        "Cannot append label banks with different embedding dimensions"
    );
    self.matrix.extend_from_slice(&other.matrix);
    self.term_count += other.term_count;
}
```

```rust
// AFTER:
pub fn append(&mut self, other: &LabelBank) -> Result<(), PipelineError> {
    if self.embedding_dim != other.embedding_dim {
        return Err(PipelineError::Model {
            message: format!(
                "Cannot append label banks: dimension mismatch ({} vs {})",
                self.embedding_dim, other.embedding_dim
            ),
        });
    }
    self.matrix.extend_from_slice(&other.matrix);
    self.term_count += other.term_count;
    Ok(())
}
```

Update the caller in `progressive.rs:152` to propagate:

```rust
running_bank.append(&chunk_bank)?;
```

**Note:** Check if `PipelineError::Model` variant exists. If not, use the most appropriate existing variant or add one. The CLAUDE.md mentions per-stage error variants — `Tagging` may be more appropriate.

### Verification

```bash
cargo test -p photon-core
cargo clippy --workspace -- -D warnings
```

---

## Phase 4 — Pipeline Accuracy (M9, M10)

### 4A — Content-based image format detection (M9)

**File:** `crates/photon-core/src/pipeline/decode.rs`, lines 89-94

**Problem:** `ImageFormat::from_path()` detects by extension only. A `photo.png` that's actually JPEG gets mislabeled in output.

Replace:

```rust
// BEFORE:
let format = ImageFormat::from_path(path).map_err(|e| PipelineError::Decode {
    path: path.to_path_buf(),
    message: format!("Unknown format: {}", e),
})?;
let image = image::open(path).map_err(|e| PipelineError::Decode { ... })?;
```

```rust
// AFTER:
let reader = image::ImageReader::open(path).map_err(|e| PipelineError::Decode {
    path: path.to_path_buf(),
    message: e.to_string(),
})?;
let reader = reader.with_guessed_format().map_err(|e| PipelineError::Decode {
    path: path.to_path_buf(),
    message: format!("Cannot detect image format: {}", e),
})?;
let format = reader.format().unwrap_or_else(|| {
    // Fall back to extension-based detection
    ImageFormat::from_path(path).unwrap_or(ImageFormat::Jpeg)
});
let image = reader.decode().map_err(|e| PipelineError::Decode {
    path: path.to_path_buf(),
    message: e.to_string(),
})?;
```

This uses the `image` crate's built-in content sniffing (`with_guessed_format()` reads magic bytes), falling back to extension when content detection fails.

**Test:** `test_format_detected_by_content` — copy `test.png` to `test_misnamed.jpg`, process it, verify format field says "png" not "jpeg".

### 4B — Include all EXIF fields in presence check (M10)

**File:** `crates/photon-core/src/pipeline/metadata.rs`, lines 36-45

**Problem:** Images with only `iso`, `aperture`, `shutter_speed`, `focal_length`, or `orientation` have their EXIF silently dropped.

Replace:

```rust
// BEFORE:
if data.captured_at.is_some()
    || data.camera_make.is_some()
    || data.camera_model.is_some()
    || data.gps_latitude.is_some()
{
    Some(data)
} else {
    None
}
```

```rust
// AFTER:
if data.captured_at.is_some()
    || data.camera_make.is_some()
    || data.camera_model.is_some()
    || data.gps_latitude.is_some()
    || data.gps_longitude.is_some()
    || data.iso.is_some()
    || data.aperture.is_some()
    || data.shutter_speed.is_some()
    || data.focal_length.is_some()
    || data.orientation.is_some()
{
    Some(data)
} else {
    None
}
```

This ensures any non-empty EXIF data is preserved in the output.

### Verification

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
```

---

## Commit Strategy

One commit per phase:

1. `fix: --skip-existing JSON array parsing and merge (M2+M3)`
2. `fix: config validation — image_size desync, TIFF magic bytes, dead fields (M5/M8/M11/M12)`
3. `fix: tagging — Warm→Cold demotion, LabelBank panic→Result (M1/M6)`
4. `fix: pipeline — content-based format detection, EXIF field coverage (M9/M10)`

After all phases: update `docs/plans/code-assessment.md` to mark issues resolved, bump changelog.
