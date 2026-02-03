//! Retry logic with exponential backoff

use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, warn};

/// Configuration for retry behavior
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_retries: u32,

    /// Initial delay between retries (milliseconds)
    pub initial_delay_ms: u64,

    /// Multiplier for exponential backoff
    pub backoff_multiplier: f64,

    /// Maximum delay between retries (milliseconds)
    pub max_delay_ms: u64,

    /// Whether to add jitter to prevent thundering herd
    pub jitter: bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay_ms: 1000,
            backoff_multiplier: 2.0,
            max_delay_ms: 30000,
            jitter: true,
        }
    }
}

impl RetryConfig {
    /// Create a config with no retries
    pub fn no_retry() -> Self {
        Self {
            max_retries: 0,
            ..Default::default()
        }
    }

    /// Calculate delay for a given attempt (0-indexed)
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let base_delay =
            self.initial_delay_ms as f64 * self.backoff_multiplier.powi(attempt as i32);

        let capped_delay = base_delay.min(self.max_delay_ms as f64);

        let final_delay = if self.jitter {
            // Add 20% jitter (0.8 to 1.2)
            let jitter_factor = 0.8 + rand_jitter() * 0.4;
            capped_delay * jitter_factor
        } else {
            capped_delay
        };

        Duration::from_millis(final_delay as u64)
    }
}

/// Simple pseudo-random jitter (0.0 to 1.0)
fn rand_jitter() -> f64 {
    use std::time::SystemTime;
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    (nanos % 1000) as f64 / 1000.0
}

/// Error classification for retry decisions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryClassification {
    /// Should retry (transient error)
    Retry,

    /// Should not retry (permanent error)
    NoRetry,

    /// Rate limited - use provided delay if available
    RateLimited { retry_after_ms: Option<u64> },
}

/// Trait for errors that can be classified for retry
pub trait RetryableError {
    fn classify(&self) -> RetryClassification;
}

/// Execute an async operation with retry logic
pub async fn with_retry<T, E, F, Fut>(
    config: &RetryConfig,
    operation_name: &str,
    mut operation: F,
) -> Result<T, E>
where
    E: RetryableError + std::fmt::Display,
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
{
    let mut attempt = 0;

    loop {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                let classification = e.classify();

                match classification {
                    RetryClassification::NoRetry => {
                        debug!(
                            "{}: non-retryable error on attempt {}: {}",
                            operation_name,
                            attempt + 1,
                            e
                        );
                        return Err(e);
                    }
                    RetryClassification::Retry | RetryClassification::RateLimited { .. } => {
                        if attempt >= config.max_retries {
                            warn!(
                                "{}: max retries ({}) exceeded: {}",
                                operation_name, config.max_retries, e
                            );
                            return Err(e);
                        }

                        let delay = match classification {
                            RetryClassification::RateLimited {
                                retry_after_ms: Some(ms),
                            } => Duration::from_millis(ms),
                            _ => config.delay_for_attempt(attempt),
                        };

                        warn!(
                            "{}: attempt {} failed, retrying in {:?}: {}",
                            operation_name,
                            attempt + 1,
                            delay,
                            e
                        );

                        sleep(delay).await;
                        attempt += 1;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delay_calculation() {
        let config = RetryConfig {
            initial_delay_ms: 1000,
            backoff_multiplier: 2.0,
            max_delay_ms: 30000,
            jitter: false,
            ..Default::default()
        };

        assert_eq!(config.delay_for_attempt(0), Duration::from_millis(1000));
        assert_eq!(config.delay_for_attempt(1), Duration::from_millis(2000));
        assert_eq!(config.delay_for_attempt(2), Duration::from_millis(4000));
        assert_eq!(config.delay_for_attempt(5), Duration::from_millis(30000)); // capped
    }
}
