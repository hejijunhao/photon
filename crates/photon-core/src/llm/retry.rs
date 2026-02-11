//! Retry utilities for transient LLM failures.
//!
//! Provides classification of retryable errors and exponential backoff.

use crate::error::PipelineError;
use std::time::Duration;

/// Determine whether a pipeline error is worth retrying.
///
/// Retryable errors: timeouts, rate limits (429), server errors (5xx).
/// Non-retryable: auth failures, bad requests, missing models.
pub fn is_retryable(error: &PipelineError) -> bool {
    match error {
        PipelineError::Timeout { .. } => true,
        PipelineError::Llm { message, .. } => {
            // Rate limit or server error
            message.contains("429")
                || message.contains("rate limit")
                || message.contains("500")
                || message.contains("502")
                || message.contains("503")
                || message.contains("504")
                || message.contains("timeout")
                || message.contains("connection")
        }
        _ => false,
    }
}

/// Calculate exponential backoff duration for a given attempt.
///
/// Uses `base_delay * 2^attempt` with a cap at 30 seconds.
pub fn backoff_duration(attempt: u32, base_delay_ms: u64) -> Duration {
    let delay = base_delay_ms.saturating_mul(2u64.saturating_pow(attempt));
    Duration::from_millis(delay.min(30_000))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_timeout_is_retryable() {
        let err = PipelineError::Timeout {
            path: PathBuf::from("test.jpg"),
            stage: "llm".to_string(),
            timeout_ms: 60000,
        };
        assert!(is_retryable(&err));
    }

    #[test]
    fn test_rate_limit_is_retryable() {
        let err = PipelineError::Llm {
            path: PathBuf::from("test.jpg"),
            message: "HTTP 429: rate limit exceeded".to_string(),
        };
        assert!(is_retryable(&err));
    }

    #[test]
    fn test_server_error_is_retryable() {
        let err = PipelineError::Llm {
            path: PathBuf::from("test.jpg"),
            message: "HTTP 503: service unavailable".to_string(),
        };
        assert!(is_retryable(&err));
    }

    #[test]
    fn test_auth_error_not_retryable() {
        let err = PipelineError::Llm {
            path: PathBuf::from("test.jpg"),
            message: "HTTP 401: unauthorized".to_string(),
        };
        assert!(!is_retryable(&err));
    }

    #[test]
    fn test_decode_error_not_retryable() {
        let err = PipelineError::Decode {
            path: PathBuf::from("test.jpg"),
            message: "invalid header".to_string(),
        };
        assert!(!is_retryable(&err));
    }

    #[test]
    fn test_backoff_exponential() {
        assert_eq!(backoff_duration(0, 1000), Duration::from_millis(1000));
        assert_eq!(backoff_duration(1, 1000), Duration::from_millis(2000));
        assert_eq!(backoff_duration(2, 1000), Duration::from_millis(4000));
        assert_eq!(backoff_duration(3, 1000), Duration::from_millis(8000));
    }

    #[test]
    fn test_backoff_capped_at_30s() {
        assert_eq!(backoff_duration(10, 1000), Duration::from_millis(30_000));
    }
}
