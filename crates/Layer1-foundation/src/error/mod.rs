//! Error types for ForgeCode
//!
//! 모든 에러를 중앙에서 관리

use thiserror::Error;

/// Result type alias
pub type Result<T> = std::result::Result<T, Error>;

/// ForgeCode 에러 타입
#[derive(Error, Debug)]
pub enum Error {
    // ========================================================================
    // 설정 관련
    // ========================================================================
    #[error("Configuration error: {0}")]
    Config(String),

    // ========================================================================
    // 권한 관련
    // ========================================================================
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Permission not found: {0}")]
    PermissionNotFound(String),

    // ========================================================================
    // 저장소 관련
    // ========================================================================
    #[error("Storage error: {0}")]
    Storage(String),

    // ========================================================================
    // Provider 관련
    // ========================================================================
    #[error("Provider error: {0}")]
    Provider(String),

    #[error("Provider not found: {0}")]
    ProviderNotFound(String),

    #[error("API error: {provider} - {message}")]
    Api { provider: String, message: String },

    #[error("Rate limited: {0}")]
    RateLimited(String),

    // ========================================================================
    // MCP 관련
    // ========================================================================
    #[error("MCP error: {0}")]
    Mcp(String),

    #[error("MCP server not found: {0}")]
    McpServerNotFound(String),

    #[error("MCP connection error: {0}")]
    McpConnection(String),

    // ========================================================================
    // Tool 관련
    // ========================================================================
    #[error("Tool error: {0}")]
    Tool(String),

    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    #[error("Tool execution failed: {tool} - {message}")]
    ToolExecution { tool: String, message: String },

    // ========================================================================
    // Task/Agent 관련
    // ========================================================================
    #[error("Task error: {0}")]
    Task(String),

    #[error("Agent error: {0}")]
    Agent(String),

    // ========================================================================
    // 실행 관련
    // ========================================================================
    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Cancelled")]
    Cancelled,

    // ========================================================================
    // 일반
    // ========================================================================
    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Validation error: {0}")]
    Validation(String),

    // ========================================================================
    // 외부 에러 변환
    // ========================================================================
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("HTTP error: {0}")]
    Http(String),

    // ========================================================================
    // 기타
    // ========================================================================
    #[error("Internal error: {0}")]
    Internal(String),
}

impl Error {
    /// 재시도 가능한 에러인지 확인
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Error::Timeout(_) | Error::RateLimited(_) | Error::McpConnection(_) | Error::Http(_)
        )
    }

    /// 사용자에게 보여줄 수 있는 에러인지 확인
    pub fn is_user_facing(&self) -> bool {
        matches!(
            self,
            Error::PermissionDenied(_)
                | Error::NotFound(_)
                | Error::InvalidInput(_)
                | Error::Validation(_)
                | Error::Cancelled
        )
    }

    /// API 에러 생성 헬퍼
    pub fn api(provider: impl Into<String>, message: impl Into<String>) -> Self {
        Error::Api {
            provider: provider.into(),
            message: message.into(),
        }
    }

    /// Tool 실행 에러 생성 헬퍼
    pub fn tool_execution(tool: impl Into<String>, message: impl Into<String>) -> Self {
        Error::ToolExecution {
            tool: tool.into(),
            message: message.into(),
        }
    }
}

// ============================================================================
// From 구현 (추가 변환)
// ============================================================================

impl From<String> for Error {
    fn from(s: String) -> Self {
        Error::Internal(s)
    }
}

impl From<&str> for Error {
    fn from(s: &str) -> Self {
        Error::Internal(s.to_string())
    }
}
