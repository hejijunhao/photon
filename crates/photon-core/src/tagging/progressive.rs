//! Progressive vocabulary encoding for fast first-run startup.
//!
//! Encodes a seed set of high-value terms synchronously (~30s), starts image
//! processing immediately, then encodes remaining terms in background chunks —
//! swapping in progressively larger scorers as they become available.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use crate::config::TaggingConfig;
use crate::error::PipelineError;

use super::label_bank::LabelBank;
use super::scorer::TagScorer;
use super::text_encoder::SigLipTextEncoder;
use super::vocabulary::Vocabulary;

/// All state needed for progressive encoding, bundled to avoid too-many-arguments.
struct ProgressiveContext {
    full_vocabulary: Vocabulary,
    text_encoder: Arc<SigLipTextEncoder>,
    config: TaggingConfig,
    scorer_slot: Arc<RwLock<TagScorer>>,
    seed_indices: Vec<usize>,
    seed_bank: LabelBank,
    cache_path: PathBuf,
    vocab_hash: String,
    chunk_size: usize,
}

/// Orchestrates background vocabulary encoding with progressive scorer updates.
pub struct ProgressiveEncoder;

impl ProgressiveEncoder {
    /// Encode seed terms synchronously and return an initial TagScorer.
    ///
    /// Also spawns a background tokio task that encodes remaining terms
    /// and progressively swaps in larger scorers via the provided `RwLock`.
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        full_vocabulary: Vocabulary,
        text_encoder: Arc<SigLipTextEncoder>,
        config: TaggingConfig,
        scorer_slot: Arc<RwLock<TagScorer>>,
        seed_indices: Vec<usize>,
        cache_path: PathBuf,
        vocab_hash: String,
        chunk_size: usize,
    ) -> Result<(), PipelineError> {
        // 1. Create seed vocabulary + label bank (SYNCHRONOUS)
        let seed_vocab = full_vocabulary.subset(&seed_indices);
        let seed_bank = LabelBank::encode_all(&seed_vocab, &text_encoder, 64)?;

        tracing::info!(
            "Seed vocabulary ready: {} terms encoded",
            seed_indices.len(),
        );

        // Clone seed bank for the background task BEFORE moving into scorer.
        // This is a small clone (~6MB for ~2K seed terms) that avoids a later
        // read-lock + clone from the scorer in background_encode().
        let seed_bank_for_background = seed_bank.clone();
        let seed_scorer = TagScorer::new(seed_vocab, seed_bank, config.clone());

        // 2. Determine remaining terms to encode
        let seed_set: HashSet<usize> = seed_indices.iter().copied().collect();
        let mut remaining: Vec<usize> = (0..full_vocabulary.len())
            .filter(|i| !seed_set.contains(i))
            .collect();
        remaining.sort();

        if remaining.is_empty() {
            // All terms were in the seed — save cache and return
            if let Err(e) = seed_scorer.label_bank().save(&cache_path, &vocab_hash) {
                tracing::error!("Failed to save label bank cache: {e}");
            }
            let mut lock = scorer_slot
                .write()
                .expect("TagScorer lock poisoned during seed installation");
            *lock = seed_scorer;
            return Ok(());
        }

        // Install seed scorer BEFORE spawning background task.
        {
            let mut lock = scorer_slot
                .write()
                .expect("TagScorer lock poisoned during seed installation");
            *lock = seed_scorer;
        }

        // 3. Spawn background encoding task
        let total_terms = full_vocabulary.len();
        let ctx = ProgressiveContext {
            full_vocabulary,
            text_encoder,
            config,
            scorer_slot: Arc::clone(&scorer_slot),
            seed_indices,
            seed_bank: seed_bank_for_background,
            cache_path,
            vocab_hash,
            chunk_size,
        };

        tokio::spawn(async move {
            Self::background_encode(ctx, remaining, total_terms).await;
        });

        Ok(())
    }

    async fn background_encode(
        ctx: ProgressiveContext,
        remaining_indices: Vec<usize>,
        total_terms: usize,
    ) {
        // Accumulate all encoded indices + the running label bank matrix.
        // We APPEND new embeddings rather than re-encoding everything —
        // this keeps total encoding work at O(N) not O(N * num_swaps).
        let mut encoded_indices = ctx.seed_indices;
        // Use the seed bank passed directly from start() — avoids a read-lock +
        // clone from the scorer that was previously needed here.
        let mut running_bank = ctx.seed_bank;

        let mut failed_chunks = 0usize;
        let chunks: Vec<&[usize]> = remaining_indices.chunks(ctx.chunk_size).collect();
        let total_chunks = chunks.len();

        for (chunk_idx, chunk) in chunks.into_iter().enumerate() {
            // Encode ONLY this chunk's terms in a blocking task
            let chunk_indices: Vec<usize> = chunk.to_vec();
            let chunk_vocab = ctx.full_vocabulary.subset(&chunk_indices);
            let encoder = Arc::clone(&ctx.text_encoder);

            let chunk_bank = tokio::task::spawn_blocking(move || {
                LabelBank::encode_all(&chunk_vocab, &encoder, 64)
            })
            .await;

            let chunk_bank = match chunk_bank {
                Ok(Ok(bank)) => bank,
                Ok(Err(e)) => {
                    tracing::error!("Background encoding chunk failed: {e}");
                    failed_chunks += 1;
                    continue;
                }
                Err(e) => {
                    tracing::error!("Background encoding task panicked: {e}");
                    failed_chunks += 1;
                    continue;
                }
            };

            // Append the new chunk's embeddings to the running bank
            if let Err(e) = running_bank.append(&chunk_bank) {
                tracing::error!("Failed to append chunk to label bank: {e}");
                failed_chunks += 1;
                continue;
            }
            encoded_indices.extend_from_slice(&chunk_indices);

            // Build a new scorer from the accumulated data.
            // Note: subset() preserves the order given in encoded_indices,
            // which matches the running_bank's row order.
            //
            // Move running_bank into the scorer (zero cost) instead of cloning.
            // This reduces peak memory: the old scorer is dropped by the assignment
            // before we clone back, so we never hold old + current + clone simultaneously.
            let combined_vocab = ctx.full_vocabulary.subset(&encoded_indices);
            let bank = std::mem::replace(&mut running_bank, LabelBank::empty());
            let new_scorer = TagScorer::new(combined_vocab, bank, ctx.config.clone());

            // Atomic swap — write lock held only for the duration of a field swap
            // plus clone-back for the next iteration.
            {
                let mut lock = ctx
                    .scorer_slot
                    .write()
                    .expect("TagScorer lock poisoned during background encoding swap");
                *lock = new_scorer; // old scorer dropped here

                // Clone back only if there are more chunks — the last iteration
                // doesn't need running_bank for append, and we save from the
                // scorer lock post-loop instead.
                if chunk_idx + 1 < total_chunks {
                    running_bank = lock.label_bank().clone();
                }
            }

            tracing::info!(
                "Progressive encoding: {}/{} terms encoded",
                encoded_indices.len(),
                total_terms,
            );
        }

        if failed_chunks > 0 {
            tracing::warn!(
                "Progressive encoding: {failed_chunks}/{total_chunks} chunks failed — \
                 vocabulary is incomplete ({} of {} terms encoded). Skipping cache save.",
                encoded_indices.len(),
                total_terms,
            );
        } else {
            // All terms encoded — save complete cache to disk.
            // First ensure the parent directory exists.
            if let Some(parent) = ctx.cache_path.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    tracing::error!("Failed to create taxonomy dir {:?}: {e}", parent);
                    return;
                }
            }

            // Save from the scorer lock — running_bank was moved into the scorer
            // on the last iteration, so we read directly from the installed scorer.
            let scorer = ctx
                .scorer_slot
                .read()
                .expect("TagScorer lock poisoned during cache save");
            if let Err(e) = scorer.label_bank().save(&ctx.cache_path, &ctx.vocab_hash) {
                tracing::error!("Failed to save complete label bank cache: {e}");
            } else {
                tracing::info!(
                    "Progressive encoding complete. Full vocabulary ({} terms) cached to {:?}",
                    total_terms,
                    ctx.cache_path,
                );
            }
        }
    }
}
