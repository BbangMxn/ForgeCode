//! Retry logic for provider requests

use crate::provider::ProviderError;
use std::future::Future;
use std::time::Duration;
use tokio::time::sleep;

/// Retry 설정
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// 최대 재시도 횟수
    pub max_retries: u32,

    /// 초기 대기 시간 (밀리초)
    pub initial_delay_ms: u64,

    /// 최대 대기 시간 (밀리초)
    pub max_delay_ms: u64,

    /// 백오프 배수
    pub backoff_multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay_ms: 1000,
            max_delay_ms: 30000,
            backoff_multiplier: 2.0,
        }
    }
}

impl RetryConfig {
    /// 재시도 없음
    pub fn no_retry() -> Self {
        Self {
            max_retries: 0,
            ..Default::default()
        }
    }

    /// 공격적인 재시도 (rate limit 용)
    pub fn aggressive() -> Self {
        Self {
            max_retries: 5,
            initial_delay_ms: 2000,
            max_delay_ms: 60000,
            backoff_multiplier: 2.0,
        }
    }

    /// n번째 재시도의 대기 시간 계산
    fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let delay_ms = (self.initial_delay_ms as f64 * self.backoff_multiplier.powi(attempt as i32))
            as u64;
        Duration::from_millis(delay_ms.min(self.max_delay_ms))
    }
}

/// 재시도 로직으로 함수 실행
///
/// # Arguments
/// * `config` - 재시도 설정
/// * `operation_name` - 로깅용 작업 이름
/// * `f` - 실행할 비동기 함수
///
/// # Returns
/// 함수 결과 또는 최종 에러
pub async fn with_retry<F, Fut, T>(
    config: &RetryConfig,
    operation_name: &str,
    mut f: F,
) -> Result<T, ProviderError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, ProviderError>>,
{
    let mut last_error = None;

    for attempt in 0..=config.max_retries {
        match f().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                if !e.is_retryable() || attempt >= config.max_retries {
                    return Err(e);
                }

                let delay = config.delay_for_attempt(attempt);
                tracing::warn!(
                    operation = operation_name,
                    attempt = attempt + 1,
                    max_retries = config.max_retries,
                    delay_ms = delay.as_millis() as u64,
                    error = %e,
                    "Retrying after error"
                );

                last_error = Some(e);
                sleep(delay).await;
            }
        }
    }

    Err(last_error.unwrap_or_else(|| ProviderError::Unknown("No attempts made".to_string())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_retry_success_first_attempt() {
        let config = RetryConfig::default();
        let result = with_retry(&config, "test", || async { Ok::<_, ProviderError>(42) }).await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_retry_success_after_failure() {
        let config = RetryConfig {
            max_retries: 3,
            initial_delay_ms: 10,
            ..Default::default()
        };

        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result = with_retry(&config, "test", || {
            let c = counter_clone.clone();
            async move {
                let count = c.fetch_add(1, Ordering::SeqCst);
                if count < 2 {
                    Err(ProviderError::RateLimited("Too fast".to_string()))
                } else {
                    Ok(42)
                }
            }
        })
        .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_exhausted() {
        let config = RetryConfig {
            max_retries: 2,
            initial_delay_ms: 10,
            ..Default::default()
        };

        let result: Result<i32, _> = with_retry(&config, "test", || async {
            Err(ProviderError::RateLimited("Always fails".to_string()))
        })
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_non_retryable_error() {
        let config = RetryConfig::default();

        let result: Result<i32, _> = with_retry(&config, "test", || async {
            Err(ProviderError::AuthenticationFailed("Bad key".to_string()))
        })
        .await;

        assert!(result.is_err());
        // 인증 실패는 재시도하지 않으므로 즉시 반환
    }
}
