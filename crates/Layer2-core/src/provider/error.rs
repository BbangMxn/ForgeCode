//! Provider 에러 타입

use thiserror::Error;

/// Provider 에러
#[derive(Error, Debug, Clone)]
pub enum ProviderError {
    /// API 키 없음
    #[error("API key not configured for {0}")]
    ApiKeyMissing(String),

    /// 모델 사용 불가
    #[error("Model not available: {0}")]
    ModelNotAvailable(String),

    /// 요청 실패
    #[error("Request failed: {0}")]
    RequestFailed(String),

    /// 응답 파싱 실패
    #[error("Failed to parse response: {0}")]
    ParseError(String),

    /// 스트림 에러
    #[error("Stream error: {0}")]
    StreamError(String),

    /// Rate limit
    #[error("Rate limited: {0}")]
    RateLimited(String),

    /// 인증 실패
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    /// 컨텐츠 필터링
    #[error("Content filtered: {0}")]
    ContentFiltered(String),

    /// 컨텍스트 초과
    #[error("Context length exceeded: {0}")]
    ContextLengthExceeded(String),

    /// 타임아웃
    #[error("Request timeout")]
    Timeout,

    /// 서버 에러
    #[error("Server error: {0}")]
    ServerError(String),

    /// 알 수 없는 에러
    #[error("Unknown error: {0}")]
    Unknown(String),
}

impl ProviderError {
    /// HTTP 상태 코드에서 에러 생성
    pub fn from_http_status(status: u16, body: &str) -> Self {
        match status {
            401 => Self::AuthenticationFailed(body.to_string()),
            403 => Self::AuthenticationFailed(format!("Forbidden: {}", body)),
            429 => Self::RateLimited(body.to_string()),
            400 => {
                if body.contains("context_length") || body.contains("max_tokens") {
                    Self::ContextLengthExceeded(body.to_string())
                } else {
                    Self::RequestFailed(body.to_string())
                }
            }
            500..=599 => Self::ServerError(format!("Status {}: {}", status, body)),
            _ => Self::Unknown(format!("Status {}: {}", status, body)),
        }
    }

    /// 재시도 가능 여부
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::RateLimited(_) | Self::Timeout | Self::ServerError(_) | Self::StreamError(_)
        )
    }

    /// 사용자에게 표시할 수 있는 에러인지
    pub fn is_user_facing(&self) -> bool {
        matches!(
            self,
            Self::ApiKeyMissing(_)
                | Self::ModelNotAvailable(_)
                | Self::RateLimited(_)
                | Self::ContentFiltered(_)
                | Self::ContextLengthExceeded(_)
        )
    }
}

impl From<reqwest::Error> for ProviderError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            Self::Timeout
        } else if err.is_connect() {
            Self::RequestFailed(format!("Connection failed: {}", err))
        } else {
            Self::RequestFailed(err.to_string())
        }
    }
}
