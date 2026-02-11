# Pre-Phase 5 Fixes

> Code quality cleanup before starting Phase 5 (LLM integration).
> Addresses all issues found during the post-Phase-4a code review.

---

## Priority A — Should Fix Before Phase 5

### A1. Run `cargo fmt`

**Files affected:** `models.rs`, `text_encoder.rs`, `label_bank.rs`, `siglip.rs`, `vocabulary.rs`, `preprocess.rs`, `processor.rs`

**Fix:** Single command — `cargo fmt`.

**Verify:** `cargo fmt --check` exits 0.

---

### A2. Fix clippy warnings (4)

#### A2a. `&*shape` deref — `embedding/siglip.rs:118`

**Current:**
```rust
message: format!("Unexpected pooler_output shape: {:?}", &*shape),
```

**Fix:** Remove the unnecessary `&*`:
```rust
message: format!("Unexpected pooler_output shape: {:?}", shape),
```

#### A2b. `unwrap()` after `is_none()` check — `pipeline/processor.rs:201-204`

**Current:**
```rust
if options.skip_embedding || self.embedding_engine.is_none() {
    vec![]
} else {
    let engine = Arc::clone(self.embedding_engine.as_ref().unwrap());
    ...
}
```

**Fix:** Restructure with `if let`:
```rust
if options.skip_embedding {
    vec![]
} else if let Some(engine) = &self.embedding_engine {
    let engine = Arc::clone(engine);
    ...
} else {
    vec![]
}
```

#### A2c. Redundant closure — `tagging/text_encoder.rs:143`

**Current:**
```rust
.map(|chunk| l2_normalize(chunk))
```

**Fix:**
```rust
.map(l2_normalize)
```

#### A2d. Dead function — `photon/src/logging.rs:48`

`init_from_config` is never called. Two options:

- **Option 1 (preferred):** Wire it into `main.rs` — load config first, then init logging from config with CLI verbose override. This is the intended design (config-driven logging).
- **Option 2:** Remove the function entirely.

**Recommended:** Option 1. In `main.rs`, after parsing CLI args, load `Config` and call `init_from_config(config, cli.verbose)` instead of `init(cli.verbose, cli.json_logs)`. This means the config file's `[logging]` section actually does something.

**Verify:** `cargo clippy` produces 0 warnings.

---

### A3. NaN safety in tag scoring — `tagging/scorer.rs:80`

**Current:**
```rust
tags.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());
```

**Risk:** Panics if ONNX produces NaN values.

**Fix:** Use `f32::total_cmp` (stable since Rust 1.62):
```rust
tags.sort_by(|a, b| b.confidence.total_cmp(&a.confidence));
```

This sorts NaN values consistently instead of panicking.

---

### A4. Stream model downloads to disk — `photon/src/cli/models.rs:255-272`

**Current:** `download_file` calls `response.bytes().await?` which buffers the entire model (350-441MB) in RAM before writing.

**Fix:** Stream response body to disk in chunks:

```rust
async fn download_file(url: &str, dest: &Path) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .send()
        .await?
        .error_for_status()
        .map_err(|e| anyhow::anyhow!("Download failed: {e}"))?;

    let total_size = response.content_length();
    if let Some(size) = total_size {
        tracing::info!("  Size: {:.1} MB", size as f64 / (1024.0 * 1024.0));
    }

    let mut file = tokio::fs::File::create(dest).await?;
    let mut stream = response.bytes_stream();
    let mut downloaded: u64 = 0;

    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;

        // Progress logging every ~50MB
        if let Some(total) = total_size {
            if downloaded % (50 * 1024 * 1024) < chunk.len() as u64 {
                tracing::info!(
                    "  Progress: {:.0}%",
                    downloaded as f64 / total as f64 * 100.0
                );
            }
        }
    }

    file.flush().await?;
    Ok(())
}
```

**Dependency:** Add `futures-util` to `photon/Cargo.toml` (for `StreamExt`). Check if `reqwest` needs the `stream` feature enabled.

---

### A5. Consolidate duplicate `l2_normalize` — two copies exist

**Current locations:**
- `embedding/siglip.rs:128` — takes `mut Vec<f32>`, normalizes in-place
- `tagging/text_encoder.rs:162` — takes `&[f32]`, allocates new `Vec`

**Fix:**

1. Create `crates/photon-core/src/math.rs` with both signatures:
   ```rust
   //! Shared math utilities.

   /// L2-normalize a vector in place so its magnitude is 1.
   pub fn l2_normalize_in_place(v: &mut [f32]) {
       let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
       if norm > f32::EPSILON {
           for x in v.iter_mut() {
               *x /= norm;
           }
       }
   }

   /// L2-normalize a slice, returning a new vector with unit magnitude.
   pub fn l2_normalize(v: &[f32]) -> Vec<f32> {
       let mut result = v.to_vec();
       l2_normalize_in_place(&mut result);
       result
   }
   ```

2. Add `pub mod math;` to `lib.rs`.

3. Update `embedding/siglip.rs` to use `crate::math::l2_normalize_in_place`:
   ```rust
   let mut raw = /* extract from tensor */;
   crate::math::l2_normalize_in_place(&mut raw);
   Ok(raw)
   ```

4. Update `tagging/text_encoder.rs` to use `crate::math::l2_normalize`.

5. Remove the local `l2_normalize` functions from both files.

6. Move existing `l2_normalize` tests from `siglip.rs` to `math.rs`.

---

### A6. `Config::load_from` takes `&PathBuf` instead of `&Path` — `config.rs:59`

**Current:**
```rust
pub fn load_from(path: &PathBuf) -> Result<Self, ConfigError> {
```

**Fix:**
```rust
pub fn load_from(path: &Path) -> Result<Self, ConfigError> {
```

Add `use std::path::Path;` if not already imported (it isn't — only `PathBuf` is imported).

---

## Priority B — Nice to Fix (Non-Blocking)

### B1. `taxonomy_dir()` hardcodes path — `config.rs:96-99`

**Current:**
```rust
pub fn taxonomy_dir(&self) -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".photon").join("taxonomy")
}
```

**Fix:** Derive from `model_dir` parent or add a `taxonomy_dir` field to `TaggingConfig`:

```rust
pub fn taxonomy_dir(&self) -> PathBuf {
    // Co-locate with models directory: ~/.photon/taxonomy
    let model_dir = self.model_dir();
    model_dir.parent()
        .unwrap_or(&model_dir)
        .join("taxonomy")
}
```

This way if `model_dir` is `~/.photon/models`, taxonomy lands at `~/.photon/taxonomy` — same result as now, but respects config changes to `model_dir`.

---

### B2. Label bank cache invalidation — `tagging/label_bank.rs`

**Problem:** If vocabulary files change, the cached `label_bank.bin` is silently stale. The size check only catches term-count mismatches.

**Fix:** Save a metadata sidecar alongside the binary:

1. When saving `label_bank.bin`, also save `label_bank.meta` containing:
   ```
   vocab_hash=<blake3 hash of concatenated vocabulary content>
   term_count=68152
   embedding_dim=768
   ```

2. When loading, check that the stored `vocab_hash` matches a freshly computed hash of the current vocabulary files. If mismatched, rebuild.

**Implementation:** Add a `vocabulary_hash()` method to `Vocabulary` that hashes all term names in order. Compare during `load_tagging()`.

---

### B3. Remove dead `Photon` struct — `lib.rs:51-81`

**Current:** `Photon` struct is a Phase 1 stub. The CLI uses `ImageProcessor` directly.

**Fix:** Two options:
- **Option 1:** Remove `Photon` struct and its tests entirely. `ImageProcessor` is the real public API.
- **Option 2:** Make `Photon` the high-level facade wrapping `ImageProcessor`, as originally intended in the blueprint.

**Recommended:** Option 1 for now. It can be reintroduced later if a higher-level API is needed. Keeping dead code creates confusion.

---

### B4. Remove dead `Vocabulary::prompts_for()` — `vocabulary.rs:139-146`

**Current:** Returns 3 prompt templates but nothing calls it. `LabelBank::encode_all()` hardcodes `"a photo of a {term}"`.

**Fix:** Remove the method. If multi-prompt averaging is implemented later (Phase 4b+), it can be re-added then.

---

### B5. Remove double file-size validation

**Current:** Both `Validator::validate()` (line 38-45) and `ImageDecoder::decode()` (line 44-53) check `max_file_size_mb`.

**Fix:** Remove the size check from `ImageDecoder::decode()` since `Validator::validate()` always runs first in the pipeline (`processor.rs:153` before `processor.rs:159`).

---

### B6. Batch processing memory accumulation — `cli/process.rs:237`

**Problem:** For JSON array output, all `ProcessedImage` results are collected into `Vec` before writing. Each has a 768-float embedding (3KB) plus metadata.

**Fix:** For JSONL output, already streams (correct). For JSON array output with `--output` file, could write a streaming JSON array:
1. Write `[` at start
2. Write each result with comma separator
3. Write `]` at end

**Recommendation:** Defer to Phase 6 (polish). Current approach works for typical batch sizes. Only matters at 10K+ images.

---

## Execution Order

1. **A1** — `cargo fmt` (clears the noise so subsequent diffs are clean)
2. **A2a-c** — Clippy fixes (mechanical, 3 one-line changes)
3. **A5** — Consolidate `l2_normalize` (creates `math.rs`, updates 2 files)
4. **A3** — NaN safety in scorer (one-line fix)
5. **A6** — `&PathBuf` → `&Path` (one-line fix)
6. **A2d** — Wire `init_from_config` or remove it
7. **A4** — Streaming downloads (most code change, adds dependency)
8. **B1** — `taxonomy_dir()` config consistency
9. **B2** — Label bank cache invalidation
10. **B3** — Remove dead `Photon` struct
11. **B4** — Remove dead `prompts_for()`
12. **B5** — Remove double size validation
13. **B6** — Skip (defer to Phase 6)

**Gate:** After step 7, run `cargo fmt --check && cargo clippy && cargo test` — all must pass with 0 warnings before proceeding to Priority B items.

After all fixes: update `CLAUDE.md` test count if it changes, and add a changelog entry.
