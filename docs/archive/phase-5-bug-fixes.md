# Phase 5 Bug Fixes

> Post-Phase-5 code review bug fixes, 2026-02-11

All 7 bugs from `docs/executing/phase-5-bugs.md` resolved. 50 tests passing (including 2 new regression tests), zero clippy warnings.

---

## Bug 1: Enricher success/failure counting

**File:** `crates/photon-core/src/llm/enricher.rs`

**Problem:** `enrich_batch` counted `Ok(())` from `handle.await` as "succeeded", but the spawned task always returned `Ok(())` regardless of LLM outcome. Actual success/failure was communicated via the `on_result` callback, so the `(succeeded, failed)` tuple was really "tasks that didn't panic / tasks that panicked".

**Fix:** Changed the spawned task to return a `bool` indicating LLM success:

```rust
// Before: task returns ()
let handle = tokio::spawn(async move {
    let result = enrich_single(&provider, &image, &options).await;
    on_result(result);
    drop(permit);
});
// handle.await → Ok(()) always

// After: task returns bool
let handle = tokio::spawn(async move {
    let result = enrich_single(&provider, &image, &options).await;
    let success = matches!(&result, EnrichResult::Success(_));
    on_result(result);
    drop(permit);
    success
});
// handle.await → Ok(true) or Ok(false)
```

Counting now matches on `Ok(true)` / `Ok(false)` / `Err(panic)`.

---

## Bug 2: Batch JSON stdout + `--llm` drops core records

**File:** `crates/photon/src/cli/process.rs`

**Problem:** In batch mode with `--format json --llm <provider>` outputting to stdout, core records were collected into `results` but the stdout block guarded by `!llm_enabled` skipped printing them. Only enrichment patches were emitted — silent data loss.

**Fix:** Removed the `!llm_enabled` guard and added an inner branch: when LLM is enabled, core records are wrapped in `OutputRecord::Core` before printing as a JSON array. This ensures core data appears on stdout before enrichment patches.

```rust
// Before: condition excluded LLM case entirely
} else if !results.is_empty() && args.output.is_none()
    && !llm_enabled && matches!(args.format, OutputFormat::Json) {
    println!("{}", serde_json::to_string_pretty(&results)?);
}

// After: handles both LLM and non-LLM cases
} else if !results.is_empty() && args.output.is_none()
    && matches!(args.format, OutputFormat::Json) {
    if llm_enabled {
        let core_records: Vec<OutputRecord> = results.iter()
            .map(|r| OutputRecord::Core(Box::new(r.clone())))
            .collect();
        println!("{}", serde_json::to_string_pretty(&core_records)?);
    } else {
        println!("{}", serde_json::to_string_pretty(&results)?);
    }
}
```

---

## Bug 3: String-based retry classification

**Files:** `crates/photon-core/src/error.rs`, `crates/photon-core/src/llm/retry.rs`, all provider files

**Problem:** `is_retryable` used `message.contains("429")`, `message.contains("500")` etc. — substring matching that produced false positives (e.g., "Processed 500 tokens" matched as server error).

**Fix:** Added `status_code: Option<u16>` field to `PipelineError::Llm`. Providers now capture the HTTP status code structurally at the point where `resp.status()` is available. The retry classifier matches on the typed status code first, falling back to message matching only for non-HTTP errors (connection failures).

```rust
// error.rs — new field
Llm {
    path: Option<PathBuf>,
    message: String,
    status_code: Option<u16>,  // NEW
}

// retry.rs — structured classification
PipelineError::Llm { status_code, message, .. } => {
    if let Some(code) = status_code {
        return *code == 429 || (500..=599).contains(code);
    }
    // Fallback for non-HTTP errors only
    message.contains("timed out") || message.contains("connect") || message.contains("connection")
}
```

**Providers updated:** Each provider now sets `status_code: Some(status.as_u16())` on HTTP error responses and `status_code: None` on network/parse errors.

**New tests:** `test_message_with_500_in_body_not_retryable_without_status` (regression test for false positive), `test_connection_error_retryable_without_status`.

---

## Bug 4: Empty OpenAI `choices` produces blank description

**File:** `crates/photon-core/src/llm/openai.rs`

**Problem:** If OpenAI returned `{"choices": []}`, `.first().and_then(...).unwrap_or_default()` silently produced an empty string, stored as a "successful" `EnrichmentPatch` with a blank description.

**Fix:** Replaced `unwrap_or_default()` with `ok_or_else(|| PipelineError::Llm { ... })?` so an empty choices array is surfaced as an error and can be retried or reported.

```rust
// Before
let text = chat_resp.choices.first()
    .and_then(|c| c.message.content.clone())
    .unwrap_or_default();

// After
let text = chat_resp.choices.first()
    .and_then(|c| c.message.content.clone())
    .ok_or_else(|| PipelineError::Llm {
        path: None,
        message: "OpenAI returned empty choices array — no content generated".to_string(),
        status_code: None,
    })?;
```

---

## Bug 5: Blocking `std::fs::read` in async context

**File:** `crates/photon-core/src/llm/enricher.rs`

**Problem:** `enrich_single` called `std::fs::read(&image.file_path)` inside a `tokio::spawn` task. This blocks a tokio worker thread for the duration of the read. With large images (up to 100MB per config limits) and 8 concurrent enrichments, this could stall the executor.

**Fix:** Replaced with `tokio::fs::read(...).await`, which internally uses `spawn_blocking` to offload the read to tokio's dedicated blocking thread pool.

```rust
// Before: blocks async worker thread
let image_bytes = match std::fs::read(&image.file_path) { ... };

// After: offloads to blocking thread pool
let image_bytes = match tokio::fs::read(&image.file_path).await { ... };
```

---

## Bug 6: `HyperbolicProvider::timeout()` dead code

**File:** `crates/photon-core/src/llm/hyperbolic.rs`

**Problem:** `HyperbolicProvider::timeout()` returned a hardcoded 60s, but `generate()` delegated to `self.inner.generate()` — the inner `OpenAiProvider` used its own `timeout()`. The outer timeout was never called and could silently diverge from the actual timeout in use.

**Fix:** Delegated `timeout()` to the inner provider, consistent with all other trait methods:

```rust
// Before: independent hardcoded value (dead code)
fn timeout(&self) -> Duration { Duration::from_secs(60) }

// After: delegates to inner provider
fn timeout(&self) -> Duration { self.inner.timeout() }
```

---

## Bug 7: `PathBuf::new()` sentinel in LLM errors

**Files:** `crates/photon-core/src/error.rs`, all provider files, `retry.rs` tests

**Problem:** All `PipelineError::Llm` errors in provider code used `path: PathBuf::new()` because providers don't have image path context. This produced error messages like `"LLM error for : message"` with an empty path. While the enricher re-wraps with the correct path for per-image errors, provider-level errors (factory failures, config issues) would surface with blank paths.

**Fix:** Changed `path: PathBuf` to `path: Option<PathBuf>` in the `Llm` variant. Providers now use `path: None`, and the Display format no longer includes the path (it reads `"LLM error: {message}"`). The path remains available as typed data for programmatic access when needed.

```rust
// Before
#[error("LLM error for {path}: {message}")]
Llm { path: PathBuf, message: String, status_code: Option<u16> }
// Providers: path: PathBuf::new()  → shows "LLM error for : ..."

// After
#[error("LLM error: {message}")]
Llm { path: Option<PathBuf>, message: String, status_code: Option<u16> }
// Providers: path: None  → shows "LLM error: ..."
```

Cleaned up unused `use std::path::PathBuf` imports from `openai.rs`, `anthropic.rs`, `ollama.rs`, and `provider.rs`.
