//! Provider-specific error types
//!
//! ProviderError는 LLM 제공자 관련 세부 에러를 관리합니다.
//! forge_foundation::Error와의 변환을 지원합니다.

use crate::retry::{RetryClassification, RetryableError};
use forge_foundation::Error as FoundationError;
use thiserror::Error;

/// Errors that can occur during provider operations
#[derive(Error, Debug, Clone)]
pub enum ProviderError {
    /// API key is missing or invalid
    #[error("Authentication failed: {0}")]
    Authentication(String),

    /// Rate limit exceeded
    #[error("Rate limit exceeded{}", .retry_after_ms.map(|ms| format!(", retry after {}ms", ms)).unwrap_or_default())]
    RateLimited { retry_after_ms: Option<u64> },

    /// Context length exceeded
    #[error("Context length exceeded: {0}")]
    ContextLengthExceeded(String),

    /// Content was filtered
    #[error("Content filtered: {0}")]
    ContentFiltered(String),

    /// Server error (5xx)
    #[error("Server error: {0}")]
    ServerError(String),

    /// Request failed (network, timeout, etc.)
    #[error("Request failed: {0}")]
    RequestFailed(String),

    /// Network error (connection failed, DNS, etc.)
    #[error("Network error: {0}")]
    Network(String),

    /// Invalid request (bad parameters)
    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    /// Invalid response from API
    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    /// Model not found or not available
    #[error("Model not available: {0}")]
    ModelNotAvailable(String),

    /// Model not found
    #[error("Model not found: {0}")]
    ModelNotFound(String),

    /// Quota exceeded
    #[error("Quota exceeded: {0}")]
    QuotaExceeded(String),

    /// Streaming error
    #[error("Stream error: {0}")]
    StreamError(String),

    /// JSON parsing error
    #[error("Parse error: {0}")]
    ParseError(String),

    /// Provider not configured
    #[error("Provider not configured: {0}")]
    NotConfigured(String),

    /// Unknown error
    #[error("Unknown error: {0}")]
    Unknown(String),
}

impl RetryableError for ProviderError {
    fn classify(&self) -> RetryClassification {
        match self {
            // Rate limited - definitely retry
            ProviderError::RateLimited { retry_after_ms } => RetryClassification::RateLimited {
                retry_after_ms: *retry_after_ms,
            },

            // Server errors - retry
            ProviderError::ServerError(_) => RetryClassification::Retry,

            // Request failures (network issues) - retry
            ProviderError::RequestFailed(_) => RetryClassification::Retry,

            // Stream errors might be transient - retry
            ProviderError::StreamError(_) => RetryClassification::Retry,

            // Network errors - retry
            ProviderError::Network(_) => RetryClassification::Retry,

            // Everything else - don't retry
            ProviderError::Authentication(_)
            | ProviderError::ContextLengthExceeded(_)
            | ProviderError::ContentFiltered(_)
            | ProviderError::InvalidRequest(_)
            | ProviderError::InvalidResponse(_)
            | ProviderError::ModelNotAvailable(_)
            | ProviderError::ModelNotFound(_)
            | ProviderError::QuotaExceeded(_)
            | ProviderError::ParseError(_)
            | ProviderError::NotConfigured(_)
            | ProviderError::Unknown(_) => RetryClassification::NoRetry,
        }
    }
}

impl ProviderError {
    /// Create from HTTP status code and body
    pub fn from_http_status(status: u16, body: &str) -> Self {
        match status {
            401 | 403 => ProviderError::Authentication(body.to_string()),
            429 => {
                // Try to extract retry-after from body
                let retry_after = extract_retry_after(body);
                ProviderError::RateLimited {
                    retry_after_ms: retry_after,
                }
            }
            400 => {
                if body.contains("context") || body.contains("too long") || body.contains("token") {
                    ProviderError::ContextLengthExceeded(body.to_string())
                } else {
                    ProviderError::InvalidRequest(body.to_string())
                }
            }
            404 => ProviderError::ModelNotAvailable(body.to_string()),
            500..=599 => ProviderError::ServerError(body.to_string()),
            _ => ProviderError::Unknown(format!("HTTP {}: {}", status, body)),
        }
    }
}

/// Try to extract retry-after value from error body (in milliseconds)
fn extract_retry_after(body: &str) -> Option<u64> {
    // Try to find retry_after in JSON
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(secs) = json
            .get("error")
            .and_then(|e| e.get("retry_after"))
            .and_then(|v| v.as_f64())
        {
            return Some((secs * 1000.0) as u64);
        }
    }

    // Try to find in plain text
    if let Some(idx) = body.find("retry") {
        let after = &body[idx..];
        // Look for a number
        let num_str: String = after
            .chars()
            .skip_while(|c| !c.is_ascii_digit())
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();

        if let Ok(secs) = num_str.parse::<f64>() {
            return Some((secs * 1000.0) as u64);
        }
    }

    None
}

// ============================================================================
// forge_foundation::Error 변환
// ============================================================================

impl From<ProviderError> for FoundationError {
    fn from(err: ProviderError) -> Self {
        match err {
            ProviderError::Authentication(msg) => FoundationError::Api {
                provider: "unknown".to_string(),
                message: format!("Authentication failed: {}", msg),
            },
            ProviderError::RateLimited { retry_after_ms } => FoundationError::RateLimited(
                retry_after_ms
                    .map(|ms| format!("Retry after {}ms", ms))
                    .unwrap_or_else(|| "Rate limited".to_string()),
            ),
            ProviderError::ContextLengthExceeded(msg) => FoundationError::Api {
                provider: "unknown".to_string(),
                message: format!("Context length exceeded: {}", msg),
            },
            ProviderError::ContentFiltered(msg) => FoundationError::Api {
                provider: "unknown".to_string(),
                message: format!("Content filtered: {}", msg),
            },
            ProviderError::ServerError(msg) => FoundationError::Api {
                provider: "unknown".to_string(),
                message: format!("Server error: {}", msg),
            },
            ProviderError::RequestFailed(msg) => FoundationError::Http(msg),
            ProviderError::Network(msg) => FoundationError::Http(format!("Network: {}", msg)),
            ProviderError::InvalidRequest(msg) => FoundationError::InvalidInput(msg),
            ProviderError::InvalidResponse(msg) => {
                FoundationError::Provider(format!("Invalid response: {}", msg))
            }
            ProviderError::ModelNotAvailable(msg) => FoundationError::ProviderNotFound(msg),
            ProviderError::ModelNotFound(msg) => FoundationError::ProviderNotFound(msg),
            ProviderError::QuotaExceeded(msg) => FoundationError::RateLimited(msg),
            ProviderError::StreamError(msg) => {
                FoundationError::Provider(format!("Stream error: {}", msg))
            }
            ProviderError::ParseError(msg) => {
                FoundationError::Provider(format!("Parse error: {}", msg))
            }
            ProviderError::NotConfigured(msg) => FoundationError::Config(msg),
            ProviderError::Unknown(msg) => FoundationError::Provider(msg),
        }
    }
}
