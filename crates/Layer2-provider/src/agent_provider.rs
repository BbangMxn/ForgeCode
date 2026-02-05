//! Agent Provider Abstraction
//!
//! Claude Agent SDK, OpenAI Codex, 로컬 모델 등 다양한 AI 에이전트 프로바이더를
//! 통합하는 추상화 레이어입니다.
//!
//! ## 설계 원칙
//!
//! 1. **Provider Agnostic**: 어떤 프로바이더든 동일한 인터페이스로 사용
//! 2. **Tool Mapping**: 각 프로바이더의 도구 형식을 통합 형식으로 변환
//! 3. **Session Portability**: 세션을 프로바이더 간 이전 가능
//! 4. **Graceful Fallback**: 한 프로바이더 실패 시 다른 프로바이더로 폴백

use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::pin::Pin;

// ============================================================================
// Core Types
// ============================================================================

/// Agent 프로바이더 타입
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentProviderType {
    /// ForgeCode 자체 Agent (직접 LLM 호출)
    Native,

    /// Claude Agent SDK
    ClaudeAgentSdk,

    /// OpenAI Codex CLI/API
    OpenAiCodex,

    /// Google Gemini CLI
    GeminiCli,

    /// 커스텀 MCP 서버
    McpServer,
}

/// Agent 쿼리 옵션
#[derive(Debug, Clone, Default)]
pub struct AgentQueryOptions {
    /// 허용된 도구 목록
    pub allowed_tools: Vec<String>,

    /// 권한 모드
    pub permission_mode: PermissionMode,

    /// 최대 턴 수
    pub max_turns: Option<u32>,

    /// 작업 디렉토리
    pub working_dir: Option<String>,

    /// 시스템 프롬프트
    pub system_prompt: Option<String>,

    /// 세션 재개 ID
    pub resume_session: Option<String>,

    /// MCP 서버 설정
    pub mcp_servers: HashMap<String, McpServerConfig>,

    /// 서브에이전트 정의
    pub subagents: HashMap<String, SubagentDefinition>,

    /// 추가 메타데이터
    pub metadata: HashMap<String, serde_json::Value>,
}

/// 권한 모드
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PermissionMode {
    /// 모든 권한 우회
    BypassAll,

    /// 편집만 자동 승인
    AcceptEdits,

    /// 기본 (확인 필요)
    #[default]
    Default,

    /// 모든 작업 거부
    DenyAll,
}

/// MCP 서버 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// 실행 명령어
    pub command: String,

    /// 인자
    pub args: Vec<String>,

    /// 환경 변수
    pub env: HashMap<String, String>,
}

/// 서브에이전트 정의
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentDefinition {
    /// 설명
    pub description: String,

    /// 시스템 프롬프트
    pub prompt: String,

    /// 허용 도구
    pub tools: Vec<String>,

    /// 모델 (옵션)
    pub model: Option<String>,
}

// ============================================================================
// Agent Events
// ============================================================================

/// Agent에서 발생하는 이벤트 (통합 형식)
#[derive(Debug, Clone)]
pub enum AgentStreamEvent {
    /// 세션 시작
    SessionStart {
        session_id: String,
        provider: AgentProviderType,
    },

    /// 텍스트 응답
    Text(String),

    /// 생각 과정 (reasoning)
    Thinking(String),

    /// 도구 호출 시작
    ToolCallStart {
        tool_use_id: String,
        tool_name: String,
        arguments: serde_json::Value,
    },

    /// 도구 호출 완료
    ToolCallComplete {
        tool_use_id: String,
        tool_name: String,
        result: String,
        success: bool,
        duration_ms: u64,
    },

    /// 서브에이전트 시작
    SubagentStart {
        agent_name: String,
        parent_tool_use_id: String,
    },

    /// 서브에이전트 완료
    SubagentComplete { agent_name: String, result: String },

    /// 토큰 사용량
    Usage {
        input_tokens: u32,
        output_tokens: u32,
    },

    /// 완료
    Done { result: String, total_turns: u32 },

    /// 에러
    Error(String),
}

/// Agent 스트림 타입
pub type AgentStream = Pin<Box<dyn Stream<Item = AgentStreamEvent> + Send>>;

// ============================================================================
// Agent Provider Trait
// ============================================================================

/// Agent 프로바이더 트레이트
///
/// 모든 AI 에이전트 프로바이더가 구현해야 하는 인터페이스입니다.
#[async_trait]
pub trait AgentProvider: Send + Sync {
    /// 프로바이더 타입
    fn provider_type(&self) -> AgentProviderType;

    /// 프로바이더 이름
    fn name(&self) -> &str;

    /// 지원하는 도구 목록
    fn supported_tools(&self) -> Vec<String>;

    /// 프로바이더가 사용 가능한지 확인
    async fn is_available(&self) -> bool;

    /// Agent 쿼리 실행
    async fn query(
        &self,
        prompt: &str,
        options: AgentQueryOptions,
    ) -> Result<AgentStream, AgentProviderError>;

    /// 세션 재개
    async fn resume_session(
        &self,
        session_id: &str,
        prompt: &str,
        options: AgentQueryOptions,
    ) -> Result<AgentStream, AgentProviderError> {
        let mut opts = options;
        opts.resume_session = Some(session_id.to_string());
        self.query(prompt, opts).await
    }

    /// 세션 정보 조회
    async fn get_session_info(&self, session_id: &str) -> Result<SessionInfo, AgentProviderError>;

    /// 모델 목록
    async fn list_models(&self) -> Vec<String>;

    /// 현재 모델
    fn current_model(&self) -> &str;
}

/// 세션 정보
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub provider: AgentProviderType,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub message_count: usize,
    pub token_usage: TokenUsage,
    pub tools_used: Vec<String>,
}

/// 토큰 사용량
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
}

/// Agent 프로바이더 에러
#[derive(Debug, thiserror::Error)]
pub enum AgentProviderError {
    #[error("Provider not available: {0}")]
    NotAvailable(String),

    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("Session not found: {0}")]
    SessionNotFound(String),

    #[error("Tool not supported: {0}")]
    ToolNotSupported(String),

    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Provider error: {0}")]
    ProviderError(String),
}

// ============================================================================
// Native Provider (ForgeCode 자체 Agent)
// ============================================================================

/// ForgeCode Native Provider
///
/// ForgeCode 자체 Agent를 사용하는 프로바이더입니다.
/// Layer3의 Agent를 직접 사용합니다.
pub struct NativeAgentProvider {
    /// 모델 이름
    model: String,

    /// 설정
    config: NativeProviderConfig,
}

/// Native Provider 설정
#[derive(Debug, Clone, Default)]
pub struct NativeProviderConfig {
    /// LLM Provider 타입 (anthropic, openai, etc.)
    pub llm_provider: String,

    /// API 키
    pub api_key: Option<String>,

    /// 기본 URL
    pub base_url: Option<String>,
}

impl NativeAgentProvider {
    pub fn new(model: &str, config: NativeProviderConfig) -> Self {
        Self {
            model: model.to_string(),
            config,
        }
    }
}

#[async_trait]
impl AgentProvider for NativeAgentProvider {
    fn provider_type(&self) -> AgentProviderType {
        AgentProviderType::Native
    }

    fn name(&self) -> &str {
        "ForgeCode Native"
    }

    fn supported_tools(&self) -> Vec<String> {
        vec![
            "Read".to_string(),
            "Write".to_string(),
            "Edit".to_string(),
            "Bash".to_string(),
            "Glob".to_string(),
            "Grep".to_string(),
            "WebSearch".to_string(),
            "WebFetch".to_string(),
            "Task".to_string(),
        ]
    }

    async fn is_available(&self) -> bool {
        // API 키 확인
        self.config.api_key.is_some()
            || std::env::var("ANTHROPIC_API_KEY").is_ok()
            || std::env::var("OPENAI_API_KEY").is_ok()
    }

    async fn query(
        &self,
        _prompt: &str,
        _options: AgentQueryOptions,
    ) -> Result<AgentStream, AgentProviderError> {
        // TODO: Layer3 Agent와 연결
        // 현재는 placeholder
        Err(AgentProviderError::NotAvailable(
            "Native provider not yet implemented".to_string(),
        ))
    }

    async fn get_session_info(&self, session_id: &str) -> Result<SessionInfo, AgentProviderError> {
        Err(AgentProviderError::SessionNotFound(session_id.to_string()))
    }

    async fn list_models(&self) -> Vec<String> {
        match self.config.llm_provider.as_str() {
            "anthropic" => vec![
                "claude-sonnet-4-5".to_string(),
                "claude-sonnet-4".to_string(),
                "claude-opus-4".to_string(),
            ],
            "openai" => vec![
                "gpt-4o".to_string(),
                "gpt-4-turbo".to_string(),
                "o1".to_string(),
            ],
            _ => vec![self.model.clone()],
        }
    }

    fn current_model(&self) -> &str {
        &self.model
    }
}

// ============================================================================
// Claude Agent SDK Provider (Placeholder)
// ============================================================================

/// Claude Agent SDK Provider
///
/// Claude Agent SDK를 사용하는 프로바이더입니다.
/// SDK가 도구 실행을 직접 처리합니다.
#[derive(Debug)]
pub struct ClaudeAgentSdkProvider {
    /// API 키
    api_key: String,

    /// 모델
    model: String,
}

impl ClaudeAgentSdkProvider {
    pub fn new(api_key: &str, model: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            model: model.to_string(),
        }
    }

    pub fn from_env() -> Option<Self> {
        let api_key = std::env::var("ANTHROPIC_API_KEY").ok()?;
        Some(Self::new(&api_key, "claude-sonnet-4-5"))
    }
}

#[async_trait]
impl AgentProvider for ClaudeAgentSdkProvider {
    fn provider_type(&self) -> AgentProviderType {
        AgentProviderType::ClaudeAgentSdk
    }

    fn name(&self) -> &str {
        "Claude Agent SDK"
    }

    fn supported_tools(&self) -> Vec<String> {
        // Claude SDK 지원 도구
        vec![
            "Read".to_string(),
            "Write".to_string(),
            "Edit".to_string(),
            "Bash".to_string(),
            "Glob".to_string(),
            "Grep".to_string(),
            "WebSearch".to_string(),
            "WebFetch".to_string(),
            "Task".to_string(),
            "AskUserQuestion".to_string(),
        ]
    }

    async fn is_available(&self) -> bool {
        // TODO: Claude Code 설치 확인
        !self.api_key.is_empty()
    }

    async fn query(
        &self,
        _prompt: &str,
        _options: AgentQueryOptions,
    ) -> Result<AgentStream, AgentProviderError> {
        // TODO: Claude Agent SDK 연동
        // claude_agent_sdk::query() 호출
        Err(AgentProviderError::NotAvailable(
            "Claude Agent SDK not yet integrated".to_string(),
        ))
    }

    async fn get_session_info(&self, session_id: &str) -> Result<SessionInfo, AgentProviderError> {
        Err(AgentProviderError::SessionNotFound(session_id.to_string()))
    }

    async fn list_models(&self) -> Vec<String> {
        vec![
            "claude-sonnet-4-5".to_string(),
            "claude-sonnet-4".to_string(),
            "claude-opus-4".to_string(),
        ]
    }

    fn current_model(&self) -> &str {
        &self.model
    }
}

// ============================================================================
// OpenAI Codex Provider (Placeholder)
// ============================================================================

/// OpenAI Codex Provider
///
/// OpenAI Codex API를 사용하는 프로바이더입니다.
#[derive(Debug)]
pub struct CodexProvider {
    /// API 키
    api_key: String,

    /// 모델
    model: String,
}

impl CodexProvider {
    pub fn new(api_key: &str, model: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            model: model.to_string(),
        }
    }

    pub fn from_env() -> Option<Self> {
        let api_key = std::env::var("OPENAI_API_KEY").ok()?;
        Some(Self::new(&api_key, "gpt-5-codex"))
    }
}

#[async_trait]
impl AgentProvider for CodexProvider {
    fn provider_type(&self) -> AgentProviderType {
        AgentProviderType::OpenAiCodex
    }

    fn name(&self) -> &str {
        "OpenAI Codex"
    }

    fn supported_tools(&self) -> Vec<String> {
        // Codex 지원 도구 (OpenAI 명명 규칙)
        vec![
            "read_file".to_string(),
            "write_file".to_string(),
            "edit_file".to_string(),
            "shell".to_string(),
            "list_files".to_string(),
            "search".to_string(),
            "web_search".to_string(),
        ]
    }

    async fn is_available(&self) -> bool {
        // TODO: Codex CLI 설치 확인
        !self.api_key.is_empty()
    }

    async fn query(
        &self,
        _prompt: &str,
        _options: AgentQueryOptions,
    ) -> Result<AgentStream, AgentProviderError> {
        // TODO: Codex API 연동
        Err(AgentProviderError::NotAvailable(
            "Codex provider not yet integrated".to_string(),
        ))
    }

    async fn get_session_info(&self, session_id: &str) -> Result<SessionInfo, AgentProviderError> {
        Err(AgentProviderError::SessionNotFound(session_id.to_string()))
    }

    async fn list_models(&self) -> Vec<String> {
        vec![
            "gpt-5-codex".to_string(),
            "gpt-5".to_string(),
            "gpt-4o".to_string(),
        ]
    }

    fn current_model(&self) -> &str {
        &self.model
    }
}

// ============================================================================
// Agent Provider Registry
// ============================================================================

/// Agent Provider Registry
///
/// 여러 프로바이더를 등록하고 관리합니다.
pub struct AgentProviderRegistry {
    providers: HashMap<String, Box<dyn AgentProvider>>,
    default_provider: Option<String>,
}

impl AgentProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
            default_provider: None,
        }
    }

    /// 프로바이더 등록
    pub fn register(&mut self, name: &str, provider: Box<dyn AgentProvider>) {
        if self.default_provider.is_none() {
            self.default_provider = Some(name.to_string());
        }
        self.providers.insert(name.to_string(), provider);
    }

    /// 기본 프로바이더 설정
    pub fn set_default(&mut self, name: &str) -> bool {
        if self.providers.contains_key(name) {
            self.default_provider = Some(name.to_string());
            true
        } else {
            false
        }
    }

    /// 프로바이더 가져오기
    pub fn get(&self, name: &str) -> Option<&dyn AgentProvider> {
        self.providers.get(name).map(|p| p.as_ref())
    }

    /// 기본 프로바이더 가져오기
    pub fn default_provider(&self) -> Option<&dyn AgentProvider> {
        self.default_provider
            .as_ref()
            .and_then(|name| self.get(name))
    }

    /// 사용 가능한 프로바이더 목록
    pub async fn available_providers(&self) -> Vec<&str> {
        let mut available = Vec::new();
        for (name, provider) in &self.providers {
            if provider.is_available().await {
                available.push(name.as_str());
            }
        }
        available
    }

    /// 모든 프로바이더 이름
    pub fn all_providers(&self) -> Vec<&str> {
        self.providers.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for AgentProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tool Mapping
// ============================================================================

/// ForgeCode 도구 이름을 다른 프로바이더 형식으로 변환
pub fn map_tool_name(forge_name: &str, target: AgentProviderType) -> String {
    match target {
        AgentProviderType::Native
        | AgentProviderType::ClaudeAgentSdk
        | AgentProviderType::GeminiCli => {
            // Claude SDK와 동일한 이름 사용
            forge_name.to_string()
        }
        AgentProviderType::OpenAiCodex => {
            // Codex는 snake_case 사용
            match forge_name {
                "Read" => "read_file".to_string(),
                "Write" => "write_file".to_string(),
                "Edit" => "edit_file".to_string(),
                "Bash" => "shell".to_string(),
                "Glob" => "list_files".to_string(),
                "Grep" => "search".to_string(),
                "WebSearch" => "web_search".to_string(),
                "WebFetch" => "fetch_url".to_string(),
                "Task" => "spawn_agent".to_string(),
                _ => forge_name.to_lowercase(),
            }
        }
        AgentProviderType::McpServer => {
            // MCP는 원래 이름 유지
            forge_name.to_string()
        }
    }
}

/// 다른 프로바이더의 도구 이름을 ForgeCode 형식으로 변환
pub fn normalize_tool_name(name: &str, source: AgentProviderType) -> String {
    match source {
        AgentProviderType::OpenAiCodex => match name {
            "read_file" => "Read".to_string(),
            "write_file" => "Write".to_string(),
            "edit_file" => "Edit".to_string(),
            "shell" => "Bash".to_string(),
            "list_files" => "Glob".to_string(),
            "search" => "Grep".to_string(),
            "web_search" => "WebSearch".to_string(),
            "fetch_url" => "WebFetch".to_string(),
            "spawn_agent" => "Task".to_string(),
            _ => name.to_string(),
        },
        _ => name.to_string(),
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_mapping_to_codex() {
        assert_eq!(
            map_tool_name("Read", AgentProviderType::OpenAiCodex),
            "read_file"
        );
        assert_eq!(
            map_tool_name("Bash", AgentProviderType::OpenAiCodex),
            "shell"
        );
    }

    #[test]
    fn test_tool_mapping_to_claude() {
        assert_eq!(
            map_tool_name("Read", AgentProviderType::ClaudeAgentSdk),
            "Read"
        );
    }

    #[test]
    fn test_normalize_tool_name() {
        assert_eq!(
            normalize_tool_name("read_file", AgentProviderType::OpenAiCodex),
            "Read"
        );
        assert_eq!(
            normalize_tool_name("shell", AgentProviderType::OpenAiCodex),
            "Bash"
        );
    }

    #[tokio::test]
    async fn test_provider_registry() {
        let mut registry = AgentProviderRegistry::new();

        let native = NativeAgentProvider::new("claude-sonnet-4", NativeProviderConfig::default());
        registry.register("native", Box::new(native));

        assert!(registry.get("native").is_some());
        assert!(registry.get("unknown").is_none());
    }
}
