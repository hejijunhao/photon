//! Bounded channels for backpressure in the processing pipeline.

use tokio::sync::mpsc;

use crate::config::PipelineConfig;

/// Create a bounded channel pair with the configured buffer size.
///
/// When the buffer is full, the sender will block, providing backpressure
/// to prevent memory exhaustion during batch processing.
pub fn bounded_channel<T>(config: &PipelineConfig) -> (mpsc::Sender<T>, mpsc::Receiver<T>) {
    mpsc::channel(config.buffer_size)
}

/// A pipeline stage that processes items with backpressure.
///
/// This is a utility for building multi-stage processing pipelines
/// where each stage pulls from an input channel and pushes to an output channel.
pub struct PipelineStage<I, O> {
    input: mpsc::Receiver<I>,
    output: mpsc::Sender<O>,
}

impl<I, O> PipelineStage<I, O> {
    /// Create a new pipeline stage.
    pub fn new(input: mpsc::Receiver<I>, output: mpsc::Sender<O>) -> Self {
        Self { input, output }
    }

    /// Run the stage with a processing function.
    ///
    /// The function `f` is called for each input item. If it returns `Some(output)`,
    /// the output is sent to the next stage. If it returns `None`, the item is dropped.
    pub async fn run<F, Fut>(mut self, f: F)
    where
        F: Fn(I) -> Fut,
        Fut: std::future::Future<Output = Option<O>>,
    {
        while let Some(item) = self.input.recv().await {
            if let Some(result) = f(item).await {
                if self.output.send(result).await.is_err() {
                    // Downstream closed, stop processing
                    break;
                }
            }
        }
    }

    /// Run the stage with a fallible processing function.
    ///
    /// Similar to `run`, but the processing function can return a `Result`.
    /// Errors are logged and the item is skipped.
    pub async fn run_fallible<F, Fut, E>(mut self, f: F)
    where
        F: Fn(I) -> Fut,
        Fut: std::future::Future<Output = Result<O, E>>,
        E: std::fmt::Display,
    {
        while let Some(item) = self.input.recv().await {
            match f(item).await {
                Ok(result) => {
                    if self.output.send(result).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    tracing::error!("Pipeline stage error: {}", e);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PipelineConfig;

    #[tokio::test]
    async fn test_bounded_channel() {
        let config = PipelineConfig {
            buffer_size: 10,
            retry_attempts: 3,
            retry_delay_ms: 1000,
        };

        let (tx, mut rx) = bounded_channel::<i32>(&config);

        tx.send(42).await.unwrap();
        let received = rx.recv().await;

        assert_eq!(received, Some(42));
    }

    #[tokio::test]
    async fn test_pipeline_stage() {
        let (input_tx, input_rx) = mpsc::channel::<i32>(10);
        let (output_tx, mut output_rx) = mpsc::channel::<i32>(10);

        let stage = PipelineStage::new(input_rx, output_tx);

        // Spawn the stage
        tokio::spawn(async move {
            stage.run(|x| async move { Some(x * 2) }).await;
        });

        // Send input
        input_tx.send(5).await.unwrap();
        input_tx.send(10).await.unwrap();
        drop(input_tx); // Close input

        // Check output
        assert_eq!(output_rx.recv().await, Some(10));
        assert_eq!(output_rx.recv().await, Some(20));
    }
}
