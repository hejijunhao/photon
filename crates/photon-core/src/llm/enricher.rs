//! LLM enrichment engine for concurrent image description generation.
//!
//! The enricher takes already-processed images and generates descriptions
//! in parallel using bounded concurrency (semaphore). Results are delivered
//! via a callback as they complete, enabling real-time JSONL streaming.

use super::provider::{ImageInput, LlmProvider, LlmRequest};
use super::retry;
use crate::types::{EnrichmentPatch, ProcessedImage};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Semaphore;

/// Configuration for the enrichment engine.
#[derive(Debug, Clone)]
pub struct EnrichOptions {
    /// Maximum concurrent LLM calls
    pub parallel: usize,
    /// Per-request timeout in milliseconds
    pub timeout_ms: u64,
    /// Maximum retries per image
    pub retry_attempts: u32,
    /// Base backoff delay in milliseconds
    pub retry_delay_ms: u64,
}

impl Default for EnrichOptions {
    fn default() -> Self {
        Self {
            parallel: 4,
            timeout_ms: 60_000,
            retry_attempts: 3,
            retry_delay_ms: 1000,
        }
    }
}

/// Result of enriching a single image.
#[derive(Debug)]
pub enum EnrichResult {
    Success(EnrichmentPatch),
    Failure(PathBuf, String),
}

/// Concurrent LLM enrichment engine.
pub struct Enricher {
    provider: Arc<dyn LlmProvider>,
    options: EnrichOptions,
}

impl Enricher {
    pub fn new(provider: Box<dyn LlmProvider>, options: EnrichOptions) -> Self {
        Self {
            provider: Arc::from(provider),
            options,
        }
    }

    /// Enrich a batch of processed images with LLM descriptions.
    ///
    /// Spawns one tokio task per image, bounded by a semaphore. Calls
    /// `on_result` for each completed enrichment so the CLI can stream
    /// JSONL lines in real time.
    ///
    /// Returns `(succeeded, failed)` counts.
    pub async fn enrich_batch<F>(
        &self,
        images: &[ProcessedImage],
        on_result: F,
    ) -> (usize, usize)
    where
        F: Fn(EnrichResult) + Send + Sync + 'static,
    {
        let semaphore = Arc::new(Semaphore::new(self.options.parallel));
        let on_result = Arc::new(on_result);
        let mut handles = Vec::with_capacity(images.len());

        for image in images {
            let permit = semaphore.clone().acquire_owned().await;
            if permit.is_err() {
                break; // Semaphore closed
            }
            let permit = permit.unwrap();

            let provider = self.provider.clone();
            let options = self.options.clone();
            let on_result = on_result.clone();
            let image = image.clone();

            let handle = tokio::spawn(async move {
                let result =
                    enrich_single(&provider, &image, &options).await;
                let success = matches!(&result, EnrichResult::Success(_));
                on_result(result);
                drop(permit);
                success
            });

            handles.push(handle);
        }

        // Wait for all tasks and count results
        let mut succeeded = 0usize;
        let mut failed = 0usize;

        for handle in handles {
            match handle.await {
                Ok(true) => succeeded += 1,
                Ok(false) => failed += 1,
                Err(e) => {
                    tracing::error!("Enrichment task panicked: {e}");
                    failed += 1;
                }
            }
        }

        (succeeded, failed)
    }
}

/// Enrich a single image with retry logic.
async fn enrich_single(
    provider: &Arc<dyn LlmProvider>,
    image: &ProcessedImage,
    options: &EnrichOptions,
) -> EnrichResult {
    // Read image from disk and encode as base64
    let image_bytes = match tokio::fs::read(&image.file_path).await {
        Ok(bytes) => bytes,
        Err(e) => {
            return EnrichResult::Failure(
                image.file_path.clone(),
                format!("Failed to read image: {e}"),
            );
        }
    };

    let image_input = ImageInput::from_bytes(&image_bytes, &image.format);
    let request = LlmRequest::describe_image(image_input, &image.tags);

    // Retry loop
    let mut last_error = String::new();
    for attempt in 0..=options.retry_attempts {
        if attempt > 0 {
            let delay = retry::backoff_duration(attempt - 1, options.retry_delay_ms);
            tracing::debug!(
                "Retry {attempt}/{} for {:?} after {delay:?}",
                options.retry_attempts,
                image.file_path
            );
            tokio::time::sleep(delay).await;
        }

        match tokio::time::timeout(
            std::time::Duration::from_millis(options.timeout_ms),
            provider.generate(&request),
        )
        .await
        {
            Ok(Ok(response)) => {
                return EnrichResult::Success(EnrichmentPatch {
                    content_hash: image.content_hash.clone(),
                    description: response.text,
                    llm_model: response.model,
                    llm_latency_ms: response.latency_ms,
                    llm_tokens: response.tokens_used,
                });
            }
            Ok(Err(e)) => {
                last_error = e.to_string();
                if !retry::is_retryable(&e) {
                    break;
                }
            }
            Err(_) => {
                last_error = format!("Timeout after {}ms", options.timeout_ms);
                // Timeouts are retryable
            }
        }
    }

    EnrichResult::Failure(image.file_path.clone(), last_error)
}
