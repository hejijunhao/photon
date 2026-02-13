# Speed Improvement — Phase 4: I/O and Allocation Optimizations

> Completed: 2026-02-13
> Ref: `docs/executing/speed-improvement-plan.md`, Phase 4 (Tasks 4.1–4.3)
> Tests: 220 passing (40 CLI + 160 core + 20 integration), zero clippy warnings

---

## Summary

Three independent optimizations targeting I/O waste and memory allocation overhead. The `--skip-existing` pre-filter no longer reads or hashes any files — it matches by (path, size) from the output file in microseconds. Label bank persistence eliminates ~209MB temporary allocations on save and halves peak memory on load via `unsafe` byte reinterpretation. Progressive encoding reduces peak memory per scorer swap by using move semantics instead of cloning, and passes the seed bank directly to the background task to avoid a read-lock + clone.

Three files changed. No new dependencies. +6 net new tests.

---

## Task 4.1: Cheap `--skip-existing` pre-filter

**File changed:** `cli/process/batch.rs`

**Problem:** The pre-filter computed a full BLAKE3 hash for every discovered file — reading each file entirely through a 64KB buffer — just to check the hash against a `HashSet<String>`. For 10K images where 9K were already processed, this read and hashed ~90GB of image data unnecessarily.

**Fix:**

- **`load_existing_hashes()` → `load_existing_entries()`:** Changed return type from `HashSet<String>` to `HashMap<(PathBuf, u64), ()>`. Extracts `(file_path, file_size)` from each `ProcessedImage` record in the output file (JSON or JSONL).

- **Pre-filter uses (path, size) matching:** `existing_entries.contains_key(&(file.path.clone(), file.size))` replaces `Hasher::content_hash(&file.path)`. Zero I/O — `DiscoveredFile.size` is already available from WalkDir's metadata call during discovery.

- **Safe fallback:** If a file's content changes without changing size (extremely rare), the path+size match is a false positive and the file is skipped. In practice, any meaningful content change almost always changes file size. Users who need exact deduplication should clear the output file and reprocess.

---

## Task 4.2: Zero-copy label bank save/load

**File changed:** `tagging/label_bank.rs`

**Problem:** `save()` allocated a ~209MB intermediate `Vec<u8>` via `flat_map(f32::to_le_bytes).collect()`. `load()` held both a `Vec<u8>` (~209MB) and `Vec<f32>` (~209MB) simultaneously — ~418MB peak for a ~209MB file.

**Fix:**

- **Compile-time endianness assert:** `const _: () = assert!(cfg!(target_endian = "little"));` — the on-disk format uses native little-endian f32 layout. Fails at build time if targeting big-endian.

- **`save()` → zero-copy:** `unsafe { std::slice::from_raw_parts(matrix.as_ptr() as *const u8, ...) }` reinterprets the `&[f32]` as `&[u8]` and writes directly. No intermediate allocation.

- **`load()` → single allocation:** Allocates `Vec<f32>` directly, creates an `unsafe` mutable byte view over it, and uses `file.read_exact()` to fill. Peak memory: one `Vec<f32>` (~209MB) instead of two vecs (~418MB). File size validated via `fs::metadata()` before allocation.

**Safety argument:** `Vec<f32>` alignment is always >= 4 bytes (Rust allocator guarantee). Little-endian is asserted at compile time. `read_exact` fills the buffer completely before any f32 access.

---

## Task 4.3: Reduced label bank cloning in progressive encoding

**File changed:** `tagging/progressive.rs`

**Problem:** Progressive encoding cloned the growing label bank at every chunk swap — from ~6MB (seed) to ~209MB (full vocabulary), totaling >1GB of transient allocations across ~13 chunks. Three copies existed simultaneously at peak: old scorer's bank + `running_bank` + clone.

**Fix:**

- **Seed bank passthrough:** Clone seed bank (~6MB) in `start()` before moving into scorer. Pass clone to background task via `ProgressiveContext.seed_bank`. Eliminates the read-lock + clone from scorer at line 124.

- **Move semantics via `std::mem::replace`:** Instead of `TagScorer::new(vocab, running_bank.clone(), config)`, use `std::mem::replace(&mut running_bank, LabelBank::empty())` to move the bank out (zero cost), then clone back from the installed scorer only for non-last iterations. The `replace` with `LabelBank::empty()` keeps the variable initialized so the compiler accepts the loop.

- **Last-chunk optimization:** On the final chunk, the clone-back is skipped entirely. Post-loop cache save reads directly from `scorer_slot.read()` instead of the moved-out `running_bank`.

- **Peak memory reduction:** Per swap, peak drops from `N_old + 2 × N_current` (old scorer + running_bank + clone) to `max(N_old + N_current, 2 × N_current)` — the old scorer is dropped before the clone-back happens.

---

## Files Modified

| File | Changes |
|------|---------|
| `crates/photon/src/cli/process/batch.rs` | `load_existing_entries()` returns `HashMap<(PathBuf, u64), ()>`, pre-filter uses path+size lookup, 9 tests (7 updated + 2 new) |
| `crates/photon-core/src/tagging/label_bank.rs` | Compile-time endianness assert, zero-copy `save()` and `load()` via unsafe byte reinterpretation, 2 new tests |
| `crates/photon-core/src/tagging/progressive.rs` | `seed_bank` field in context, move semantics per-chunk swap via `std::mem::replace`, save from scorer lock post-loop |
