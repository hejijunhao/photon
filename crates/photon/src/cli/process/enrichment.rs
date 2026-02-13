//! LLM enrichment helpers for collecting and streaming enrichment patches.

use photon_core::{EnrichResult, Enricher, OutputRecord, ProcessedImage};

/// Run LLM enrichment via a spawned task, collecting patches via channel.
///
/// Returns both the enrichment patches and the original results (moved back
/// from the spawned task), allowing callers to consume results by move
/// instead of cloning.
pub async fn run_enrichment_collect(
    enricher: Enricher,
    results: Vec<ProcessedImage>,
) -> anyhow::Result<(Vec<OutputRecord>, Vec<ProcessedImage>)> {
    tracing::info!("Starting LLM enrichment for {} images...", results.len());

    let (tx, rx) = std::sync::mpsc::sync_channel::<OutputRecord>(64);

    let enricher_handle = {
        let tx = tx;
        tokio::spawn(async move {
            let stats = enricher
                .enrich_batch(&results, move |enrich_result| match enrich_result {
                    EnrichResult::Success(patch) => {
                        let _ = tx.send(OutputRecord::Enrichment(patch));
                    }
                    EnrichResult::Failure(path, msg) => {
                        tracing::error!("Enrichment failed: {path:?} - {msg}");
                    }
                })
                .await;
            (stats, results)
        })
    };

    let ((enriched, enrich_failed), results) = enricher_handle.await?;
    let records: Vec<OutputRecord> = rx.try_iter().collect();
    log_enrichment_stats(enriched, enrich_failed);
    Ok((records, results))
}

/// Run LLM enrichment, printing patches to stdout as they arrive.
///
/// Used for stdout-targeted enrichment with real-time streaming.
pub async fn run_enrichment_stdout(
    enricher: Enricher,
    results: &[ProcessedImage],
    pretty: bool,
) -> anyhow::Result<()> {
    tracing::info!("Starting LLM enrichment for {} images...", results.len());

    let (enriched, enrich_failed) = enricher
        .enrich_batch(results, move |enrich_result| match enrich_result {
            EnrichResult::Success(patch) => {
                let record = OutputRecord::Enrichment(patch);
                let json = if pretty {
                    serde_json::to_string_pretty(&record)
                } else {
                    serde_json::to_string(&record)
                };
                if let Ok(json) = json {
                    println!("{json}");
                }
            }
            EnrichResult::Failure(path, msg) => {
                tracing::error!("Enrichment failed: {path:?} - {msg}");
            }
        })
        .await;
    log_enrichment_stats(enriched, enrich_failed);
    Ok(())
}

fn log_enrichment_stats(succeeded: usize, failed: usize) {
    if failed > 0 {
        tracing::warn!("LLM enrichment: {} succeeded, {} failed", succeeded, failed);
    } else {
        tracing::info!("LLM enrichment: {} succeeded", succeeded);
    }
}
