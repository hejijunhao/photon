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
    pub async fn enrich_batch<F>(&self, images: &[ProcessedImage], on_result: F) -> (usize, usize)
    where
        F: Fn(EnrichResult) + Send + Sync + 'static,
    {
        let semaphore = Arc::new(Semaphore::new(self.options.parallel));
        let on_result = Arc::new(on_result);
        let mut handles = Vec::with_capacity(images.len());

        for image in images {
            let permit = semaphore.clone().acquire_owned().await;
            if permit.is_err() {
                tracing::warn!("Enrichment semaphore closed unexpectedly — stopping batch");
                break;
            }
            let permit = permit.unwrap();

            let provider = self.provider.clone();
            let options = self.options.clone();
            let on_result = on_result.clone();
            let image = image.clone();

            let handle = tokio::spawn(async move {
                let result = enrich_single(&provider, &image, &options).await;
                let success = matches!(&result, EnrichResult::Success(_));
                drop(permit); // Release concurrency permit before callback
                on_result(result);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::PipelineError;
    use crate::llm::provider::{LlmProvider, LlmRequest, LlmResponse};
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    /// A configurable mock LLM provider for testing enricher behavior.
    ///
    /// Each call to `generate()` invokes the response factory with the current
    /// call index, allowing callers to return different results per attempt.
    struct MockProvider {
        /// Factory that produces a response for each call index.
        response_fn: Box<dyn Fn(u32) -> Result<LlmResponse, PipelineError> + Send + Sync>,
        /// Tracks how many times `generate` was called (shared for post-hoc assertions).
        call_count: Arc<AtomicU32>,
        /// Optional delay before returning.
        delay: Option<Duration>,
        /// Tracks concurrent in-flight calls (for semaphore testing).
        in_flight: Option<(Arc<AtomicU32>, Arc<AtomicU32>)>, // (in_flight, max_concurrent)
    }

    impl MockProvider {
        fn success(text: &str) -> Self {
            let text = text.to_string();
            Self {
                response_fn: Box::new(move |_| {
                    Ok(LlmResponse {
                        text: text.clone(),
                        model: "mock-v1".to_string(),
                        tokens_used: Some(42),
                        latency_ms: 10,
                    })
                }),
                call_count: Arc::new(AtomicU32::new(0)),
                delay: None,
                in_flight: None,
            }
        }

        fn failing(status_code: Option<u16>, message: &str) -> Self {
            let message = message.to_string();
            Self {
                response_fn: Box::new(move |_| {
                    Err(PipelineError::Llm {
                        message: message.clone(),
                        status_code,
                    })
                }),
                call_count: Arc::new(AtomicU32::new(0)),
                delay: None,
                in_flight: None,
            }
        }

        /// First call returns an error, subsequent calls succeed.
        fn fail_then_succeed(
            status_code: Option<u16>,
            error_msg: &str,
            success_text: &str,
        ) -> Self {
            let error_msg = error_msg.to_string();
            let success_text = success_text.to_string();
            Self {
                response_fn: Box::new(move |idx| {
                    if idx == 0 {
                        Err(PipelineError::Llm {
                            message: error_msg.clone(),
                            status_code,
                        })
                    } else {
                        Ok(LlmResponse {
                            text: success_text.clone(),
                            model: "mock-v1".to_string(),
                            tokens_used: Some(20),
                            latency_ms: 50,
                        })
                    }
                }),
                call_count: Arc::new(AtomicU32::new(0)),
                delay: None,
                in_flight: None,
            }
        }

        fn with_delay(mut self, delay: Duration) -> Self {
            self.delay = Some(delay);
            self
        }

        /// Get a shared handle to the call counter (clone before moving provider).
        fn call_count_handle(&self) -> Arc<AtomicU32> {
            self.call_count.clone()
        }
    }

    #[async_trait]
    impl LlmProvider for MockProvider {
        fn name(&self) -> &str {
            "mock"
        }

        async fn is_available(&self) -> bool {
            true
        }

        async fn generate(&self, _request: &LlmRequest) -> Result<LlmResponse, PipelineError> {
            let idx = self.call_count.fetch_add(1, Ordering::SeqCst);
            if let Some((ref in_flight, ref max_concurrent)) = self.in_flight {
                let current = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                max_concurrent.fetch_max(current, Ordering::SeqCst);
            }
            if let Some(delay) = self.delay {
                tokio::time::sleep(delay).await;
            }
            let result = (self.response_fn)(idx);
            if let Some((ref in_flight, _)) = self.in_flight {
                in_flight.fetch_sub(1, Ordering::SeqCst);
            }
            result
        }

        fn timeout(&self) -> Duration {
            Duration::from_secs(60)
        }
    }

    /// Create a minimal `ProcessedImage` pointing to a real fixture file.
    fn fixture_image(name: &str) -> ProcessedImage {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/images")
            .join(name);
        ProcessedImage {
            file_path: path,
            file_name: name.to_string(),
            content_hash: format!("hash_{name}"),
            width: 100,
            height: 100,
            format: "jpeg".to_string(),
            file_size: 1000,
            embedding: vec![],
            exif: None,
            tags: vec![],
            description: None,
            thumbnail: None,
            perceptual_hash: None,
        }
    }

    /// Collect all `EnrichResult`s into a vec via the callback.
    async fn run_enricher(
        provider: MockProvider,
        images: &[ProcessedImage],
        options: EnrichOptions,
    ) -> (Vec<EnrichResult>, (usize, usize)) {
        let results = Arc::new(std::sync::Mutex::new(Vec::new()));
        let results_clone = results.clone();
        let enricher = Enricher::new(Box::new(provider), options);
        let counts = enricher
            .enrich_batch(images, move |r| {
                results_clone.lock().unwrap().push(r);
            })
            .await;
        let results = Arc::try_unwrap(results).unwrap().into_inner().unwrap();
        (results, counts)
    }

    fn fast_options() -> EnrichOptions {
        EnrichOptions {
            parallel: 4,
            timeout_ms: 5000,
            retry_attempts: 0,
            retry_delay_ms: 10,
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_enricher_basic_success() {
        let provider = MockProvider::success("A beautiful beach scene.");
        let images = vec![fixture_image("beach.jpg")];
        let (results, (succeeded, failed)) = run_enricher(provider, &images, fast_options()).await;

        assert_eq!(succeeded, 1);
        assert_eq!(failed, 0);
        assert_eq!(results.len(), 1);
        match &results[0] {
            EnrichResult::Success(patch) => {
                assert_eq!(patch.description, "A beautiful beach scene.");
                assert_eq!(patch.content_hash, "hash_beach.jpg");
                assert_eq!(patch.llm_model, "mock-v1");
            }
            EnrichResult::Failure(path, msg) => {
                panic!("Expected success, got failure for {path:?}: {msg}");
            }
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_enricher_retry_on_transient_error() {
        // First call: 429 (retryable), second call: success
        let provider =
            MockProvider::fail_then_succeed(Some(429), "rate limited", "Recovered after retry.");
        // Allow 1 retry with minimal backoff
        let options = EnrichOptions {
            retry_attempts: 1,
            retry_delay_ms: 10,
            ..fast_options()
        };
        let images = vec![fixture_image("beach.jpg")];
        let (results, (succeeded, failed)) = run_enricher(provider, &images, options).await;

        assert_eq!(succeeded, 1);
        assert_eq!(failed, 0);
        match &results[0] {
            EnrichResult::Success(patch) => {
                assert_eq!(patch.description, "Recovered after retry.");
            }
            EnrichResult::Failure(_, msg) => panic!("Expected success after retry: {msg}"),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_enricher_no_retry_on_auth_error() {
        let provider = MockProvider::failing(Some(401), "unauthorized");
        let call_count = provider.call_count_handle();
        let options = EnrichOptions {
            retry_attempts: 3, // Would retry 3 times if retryable
            retry_delay_ms: 10,
            ..fast_options()
        };
        let images = vec![fixture_image("beach.jpg")];
        let (results, (succeeded, failed)) = run_enricher(provider, &images, options).await;

        assert_eq!(succeeded, 0);
        assert_eq!(failed, 1);
        // Verify provider was called exactly once (no retries on 401)
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
        match &results[0] {
            EnrichResult::Failure(_, msg) => {
                assert!(msg.contains("unauthorized"));
            }
            EnrichResult::Success(_) => panic!("Expected auth failure"),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_enricher_timeout() {
        // Provider sleeps longer than the enricher's per-request timeout
        let provider = MockProvider::success("too slow").with_delay(Duration::from_secs(5));
        let options = EnrichOptions {
            timeout_ms: 50, // 50ms timeout — provider sleeps 5s
            retry_attempts: 0,
            ..fast_options()
        };
        let images = vec![fixture_image("beach.jpg")];
        let (results, (succeeded, failed)) = run_enricher(provider, &images, options).await;

        assert_eq!(succeeded, 0);
        assert_eq!(failed, 1);
        match &results[0] {
            EnrichResult::Failure(_, msg) => {
                assert!(msg.contains("Timeout"), "Expected timeout, got: {msg}");
            }
            EnrichResult::Success(_) => panic!("Expected timeout failure"),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_enricher_batch_partial_failure() {
        // Provider succeeds for all calls, but one image has a nonexistent path
        // (file read fails before the provider is ever called)
        let provider = MockProvider::success("described");
        let images = vec![
            fixture_image("beach.jpg"),
            {
                let mut img = fixture_image("nonexistent.jpg");
                img.file_path = PathBuf::from("/tmp/definitely_does_not_exist.jpg");
                img
            },
            fixture_image("car.jpg"),
        ];
        let (results, (succeeded, failed)) = run_enricher(provider, &images, fast_options()).await;

        assert_eq!(succeeded, 2);
        assert_eq!(failed, 1);
        assert_eq!(results.len(), 3);

        let successes: Vec<_> = results
            .iter()
            .filter(|r| matches!(r, EnrichResult::Success(_)))
            .collect();
        let failures: Vec<_> = results
            .iter()
            .filter(|r| matches!(r, EnrichResult::Failure(..)))
            .collect();
        assert_eq!(successes.len(), 2);
        assert_eq!(failures.len(), 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_enricher_missing_image_file() {
        let provider = MockProvider::success("should not reach");
        let call_count = provider.call_count_handle();
        let mut image = fixture_image("ghost.jpg");
        image.file_path = PathBuf::from("/nonexistent/path/ghost.jpg");
        let images = vec![image];
        let (results, (succeeded, failed)) = run_enricher(provider, &images, fast_options()).await;

        assert_eq!(succeeded, 0);
        assert_eq!(failed, 1);
        // Verify provider was never called (file read fails first)
        assert_eq!(call_count.load(Ordering::SeqCst), 0);
        match &results[0] {
            EnrichResult::Failure(path, msg) => {
                assert_eq!(path, &PathBuf::from("/nonexistent/path/ghost.jpg"));
                assert!(msg.contains("Failed to read image"), "Got: {msg}");
            }
            EnrichResult::Success(_) => panic!("Expected file-not-found failure"),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_enricher_semaphore_bounds_concurrency() {
        // Track concurrent in-flight calls to verify semaphore enforcement.
        let in_flight = Arc::new(AtomicU32::new(0));
        let max_concurrent = Arc::new(AtomicU32::new(0));

        let provider = MockProvider {
            response_fn: Box::new(|_| {
                Ok(LlmResponse {
                    text: "described".to_string(),
                    model: "mock-v1".to_string(),
                    tokens_used: Some(10),
                    latency_ms: 5,
                })
            }),
            call_count: Arc::new(AtomicU32::new(0)),
            delay: Some(Duration::from_millis(200)), // Hold permit for 200ms
            in_flight: Some((in_flight.clone(), max_concurrent.clone())),
        };

        // 6 images, parallel=2 → at most 2 concurrent calls
        let images: Vec<_> = (0..6).map(|_| fixture_image("beach.jpg")).collect();
        let options = EnrichOptions {
            parallel: 2,
            timeout_ms: 5000,
            retry_attempts: 0,
            retry_delay_ms: 10,
        };
        let (_, (succeeded, failed)) = run_enricher(provider, &images, options).await;

        assert_eq!(succeeded, 6);
        assert_eq!(failed, 0);
        assert!(
            max_concurrent.load(Ordering::SeqCst) <= 2,
            "semaphore violated: max concurrent was {}",
            max_concurrent.load(Ordering::SeqCst)
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_enricher_exhausts_retries() {
        // Always fail with 429 (retryable) — should exhaust all retries.
        let provider = MockProvider::failing(Some(429), "rate limited");
        let call_count = provider.call_count_handle();
        let options = EnrichOptions {
            parallel: 4,
            timeout_ms: 5000,
            retry_attempts: 2,
            retry_delay_ms: 10,
        };
        let images = vec![fixture_image("beach.jpg")];
        let (results, (succeeded, failed)) = run_enricher(provider, &images, options).await;

        assert_eq!(succeeded, 0);
        assert_eq!(failed, 1);
        // 1 initial + 2 retries = 3 total calls
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
        match &results[0] {
            EnrichResult::Failure(_, msg) => {
                assert!(msg.contains("rate limited"), "Got: {msg}");
            }
            EnrichResult::Success(_) => panic!("Expected retry exhaustion failure"),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_enricher_empty_batch() {
        let provider = MockProvider::success("should not reach");
        let call_count = provider.call_count_handle();
        let results = Arc::new(std::sync::Mutex::new(Vec::new()));
        let results_clone = results.clone();
        let enricher = Enricher::new(Box::new(provider), fast_options());
        let (succeeded, failed) = enricher
            .enrich_batch(&[], move |r| {
                results_clone.lock().unwrap().push(r);
            })
            .await;

        assert_eq!(succeeded, 0);
        assert_eq!(failed, 0);
        assert_eq!(call_count.load(Ordering::SeqCst), 0);
        assert!(results.lock().unwrap().is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_enricher_retry_on_server_error() {
        // First call: 500 (retryable), second call: success
        let provider = MockProvider::fail_then_succeed(
            Some(500),
            "internal server error",
            "Recovered after 500.",
        );
        let call_count = provider.call_count_handle();
        let options = EnrichOptions {
            retry_attempts: 1,
            retry_delay_ms: 10,
            ..fast_options()
        };
        let images = vec![fixture_image("beach.jpg")];
        let (results, (succeeded, failed)) = run_enricher(provider, &images, options).await;

        assert_eq!(succeeded, 1);
        assert_eq!(failed, 0);
        // 1 initial (500) + 1 retry (success) = 2 total calls
        assert_eq!(call_count.load(Ordering::SeqCst), 2);
        match &results[0] {
            EnrichResult::Success(patch) => {
                assert_eq!(patch.description, "Recovered after 500.");
            }
            EnrichResult::Failure(_, msg) => panic!("Expected success after 500 retry: {msg}"),
        }
    }
}
