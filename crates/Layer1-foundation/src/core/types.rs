//! Core Types - 공용 타입 정의
//!
//! 모든 레이어에서 공통으로 사용하는 타입들입니다.
//! 이 모듈의 타입들은 ForgeCode 전체에서 표준으로 사용됩니다.
//!
//! ## 핵심 타입
//!
//! - `ToolCall`: LLM이 요청한 도구 호출
//! - `ToolResult`: 도구 실행 결과
//! - `Message`: 대화 메시지
//! - `MessageRole`: 메시지 역할 (system, user, assistant, tool)
//! - `TokenUsage`: 토큰 사용량
//! - `StreamEvent`: 스트리밍 이벤트

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;

// ============================================================================
// Message Types - 메시지 관련 타입
// ============================================================================

/// 메시지 역할
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    /// 시스템 프롬프트
    System,
    /// 사용자 입력
    User,
    /// 어시스턴트 응답
    Assistant,
    /// 도구 결과
    Tool,
}

impl Default for MessageRole {
    fn default() -> Self {
        Self::User
    }
}

impl std::fmt::Display for MessageRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::System => write!(f, "system"),
            Self::User => write!(f, "user"),
            Self::Assistant => write!(f, "assistant"),
            Self::Tool => write!(f, "tool"),
        }
    }
}

/// 대화 메시지
///
/// LLM과의 대화에서 사용되는 메시지 타입입니다.
/// 모든 Layer에서 표준으로 사용됩니다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// 고유 메시지 ID
    pub id: Uuid,

    /// 메시지 역할
    pub role: MessageRole,

    /// 텍스트 내용
    pub content: String,

    /// 어시스턴트가 요청한 도구 호출들 (assistant 역할인 경우)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,

    /// 도구 실행 결과 (tool 역할인 경우)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_result: Option<ToolResultMessage>,
}

impl Message {
    /// 시스템 메시지 생성
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: MessageRole::System,
            content: content.into(),
            tool_calls: None,
            tool_result: None,
        }
    }

    /// 사용자 메시지 생성
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: MessageRole::User,
            content: content.into(),
            tool_calls: None,
            tool_result: None,
        }
    }

    /// 어시스턴트 메시지 생성
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: MessageRole::Assistant,
            content: content.into(),
            tool_calls: None,
            tool_result: None,
        }
    }

    /// 도구 호출이 포함된 어시스턴트 메시지 생성
    pub fn assistant_with_tools(content: impl Into<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: MessageRole::Assistant,
            content: content.into(),
            tool_calls: Some(tool_calls),
            tool_result: None,
        }
    }

    /// 도구 결과 메시지 생성
    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>, is_error: bool) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: MessageRole::Tool,
            content: String::new(),
            tool_calls: None,
            tool_result: Some(ToolResultMessage {
                tool_call_id: tool_call_id.into(),
                content: content.into(),
                is_error,
            }),
        }
    }

    /// 도구 호출이 있는지 확인
    pub fn has_tool_calls(&self) -> bool {
        self.tool_calls.as_ref().map_or(false, |calls| !calls.is_empty())
    }
}

// ============================================================================
// Tool Call Types - 도구 호출 관련 타입
// ============================================================================

/// LLM이 요청한 도구 호출
///
/// LLM이 응답에서 도구 사용을 요청할 때 이 구조체를 사용합니다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// 고유 호출 ID
    pub id: String,

    /// 도구 이름
    pub name: String,

    /// 도구 인자 (JSON)
    pub arguments: Value,
}

impl ToolCall {
    /// 새 도구 호출 생성
    pub fn new(id: impl Into<String>, name: impl Into<String>, arguments: Value) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            arguments,
        }
    }

    /// 인자에서 문자열 값 추출
    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.arguments.get(key).and_then(|v| v.as_str())
    }

    /// 인자에서 bool 값 추출
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.arguments.get(key).and_then(|v| v.as_bool())
    }

    /// 인자에서 i64 값 추출
    pub fn get_i64(&self, key: &str) -> Option<i64> {
        self.arguments.get(key).and_then(|v| v.as_i64())
    }
}

/// 도구 결과 메시지 (LLM 메시지용)
///
/// LLM에게 도구 실행 결과를 전달할 때 사용하는 타입입니다.
/// Tool trait의 실행 결과는 `ToolExecutionResult` (traits.rs)를 사용하세요.
///
/// ## 사용처
/// - Message 구조체의 tool_result 필드
/// - LLM API 응답 처리
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultMessage {
    /// 원본 도구 호출 ID (ToolCall.id와 매칭)
    pub tool_call_id: String,

    /// 결과 내용
    pub content: String,

    /// 에러 여부
    pub is_error: bool,
}

impl ToolResultMessage {
    /// 성공 결과 생성
    pub fn success(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            tool_call_id: tool_call_id.into(),
            content: content.into(),
            is_error: false,
        }
    }

    /// 에러 결과 생성
    pub fn error(tool_call_id: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            tool_call_id: tool_call_id.into(),
            content: error.into(),
            is_error: true,
        }
    }
}

// 하위 호환성을 위한 type alias (deprecated)
#[deprecated(
    since = "0.2.0",
    note = "Use ToolResultMessage for LLM messages or ToolResult (from traits) for tool execution"
)]
pub type ToolResultMsg = ToolResultMessage;

// ============================================================================
// Token Usage - 토큰 사용량
// ============================================================================

/// 토큰 사용량 정보
///
/// LLM API 호출에서 사용된 토큰 수를 추적합니다.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    /// 입력 토큰 수
    pub input_tokens: u32,

    /// 출력 토큰 수
    pub output_tokens: u32,

    /// 캐시에서 읽은 토큰 수
    #[serde(default)]
    pub cache_read_tokens: u32,

    /// 캐시 생성에 사용된 토큰 수
    #[serde(default)]
    pub cache_creation_tokens: u32,
}

impl TokenUsage {
    /// 새 토큰 사용량 생성
    pub fn new(input_tokens: u32, output_tokens: u32) -> Self {
        Self {
            input_tokens,
            output_tokens,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
        }
    }

    /// 총 토큰 수
    pub fn total(&self) -> u32 {
        self.input_tokens + self.output_tokens
    }

    /// 다른 사용량 추가
    pub fn add(&mut self, other: &TokenUsage) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.cache_read_tokens += other.cache_read_tokens;
        self.cache_creation_tokens += other.cache_creation_tokens;
    }

    /// 비용 추정 (USD)
    pub fn estimate_cost(&self, input_price_per_1m: f64, output_price_per_1m: f64) -> f64 {
        let input_cost = (self.input_tokens as f64 / 1_000_000.0) * input_price_per_1m;
        let output_cost = (self.output_tokens as f64 / 1_000_000.0) * output_price_per_1m;
        input_cost + output_cost
    }
}

impl std::ops::Add for TokenUsage {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            input_tokens: self.input_tokens + other.input_tokens,
            output_tokens: self.output_tokens + other.output_tokens,
            cache_read_tokens: self.cache_read_tokens + other.cache_read_tokens,
            cache_creation_tokens: self.cache_creation_tokens + other.cache_creation_tokens,
        }
    }
}

impl std::ops::AddAssign for TokenUsage {
    fn add_assign(&mut self, other: Self) {
        self.add(&other);
    }
}

// ============================================================================
// Stream Events - 스트리밍 이벤트
// ============================================================================

/// 스트리밍 응답 이벤트
///
/// LLM의 스트리밍 응답에서 발생하는 이벤트입니다.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// 텍스트 청크
    Text(String),

    /// 생각/추론 내용 (thinking 모델용)
    Thinking(String),

    /// 도구 호출 시작
    ToolCallStart {
        index: usize,
        id: String,
        name: String,
    },

    /// 도구 호출 인자 부분
    ToolCallDelta {
        index: usize,
        arguments_delta: String,
    },

    /// 도구 호출 완료
    ToolCall(ToolCall),

    /// 토큰 사용량 업데이트
    Usage(TokenUsage),

    /// 스트림 완료
    Done,

    /// 에러 발생
    Error(String),
}

// ============================================================================
// Tool Source - 도구 출처
// ============================================================================

/// 도구 출처 (어디서 왔는지)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolSource {
    /// 내장 도구 (Bash, Read, Write 등)
    Builtin { name: String },

    /// MCP 서버에서 제공하는 도구
    Mcp { server: String, tool: String },

    /// 사용자 정의 도구
    Custom { id: String },
}

impl ToolSource {
    pub fn builtin(name: impl Into<String>) -> Self {
        Self::Builtin { name: name.into() }
    }

    pub fn mcp(server: impl Into<String>, tool: impl Into<String>) -> Self {
        Self::Mcp {
            server: server.into(),
            tool: tool.into(),
        }
    }

    pub fn custom(id: impl Into<String>) -> Self {
        Self::Custom { id: id.into() }
    }

    /// 전체 식별자 (permission key로 사용)
    pub fn full_id(&self) -> String {
        match self {
            Self::Builtin { name } => format!("builtin:{}", name),
            Self::Mcp { server, tool } => format!("mcp:{}:{}", server, tool),
            Self::Custom { id } => format!("custom:{}", id),
        }
    }

    /// 표시용 이름
    pub fn display_name(&self) -> String {
        match self {
            Self::Builtin { name } => name.clone(),
            Self::Mcp { server, tool } => format!("{}/{}", server, tool),
            Self::Custom { id } => id.clone(),
        }
    }
}

impl std::fmt::Display for ToolSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.full_id())
    }
}

// ============================================================================
// Execution Context - 실행 컨텍스트 정보
// ============================================================================

/// 실행 환경 정보
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExecutionEnv {
    /// 작업 디렉토리
    pub working_dir: Option<String>,

    /// 환경 변수
    #[serde(default)]
    pub env_vars: HashMap<String, String>,

    /// 타임아웃 (초)
    pub timeout_secs: Option<u64>,

    /// 쉘 타입 (builtin 도구용)
    pub shell_type: Option<String>,
}

impl ExecutionEnv {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn working_dir(mut self, dir: impl Into<String>) -> Self {
        self.working_dir = Some(dir.into());
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env_vars.insert(key.into(), value.into());
        self
    }

    pub fn timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = Some(secs);
        self
    }

    pub fn shell(mut self, shell: impl Into<String>) -> Self {
        self.shell_type = Some(shell.into());
        self
    }
}

// ============================================================================
// Permission Rule - 권한 규칙
// ============================================================================

/// 권한 규칙 액션
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionRuleAction {
    /// 항상 허용
    Allow,
    /// 항상 확인
    Ask,
    /// 항상 거부
    Deny,
}

impl Default for PermissionRuleAction {
    fn default() -> Self {
        Self::Ask
    }
}

/// 권한 규칙 (Claude Code 스타일)
///
/// 예: "bash:rm *" -> Deny
/// 예: "mcp:notion:*" -> Allow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRule {
    /// 도구 패턴 (glob 지원)
    /// 예: "bash", "mcp:*", "mcp:notion:*"
    pub tool_pattern: String,

    /// 액션 패턴 (glob 지원)
    /// 예: "rm *", "/home/user/**", "*"
    #[serde(default)]
    pub action_pattern: Option<String>,

    /// 규칙 액션
    pub rule: PermissionRuleAction,

    /// 이유/설명
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl PermissionRule {
    pub fn allow(tool_pattern: impl Into<String>) -> Self {
        Self {
            tool_pattern: tool_pattern.into(),
            action_pattern: None,
            rule: PermissionRuleAction::Allow,
            reason: None,
        }
    }

    pub fn ask(tool_pattern: impl Into<String>) -> Self {
        Self {
            tool_pattern: tool_pattern.into(),
            action_pattern: None,
            rule: PermissionRuleAction::Ask,
            reason: None,
        }
    }

    pub fn deny(tool_pattern: impl Into<String>) -> Self {
        Self {
            tool_pattern: tool_pattern.into(),
            action_pattern: None,
            rule: PermissionRuleAction::Deny,
            reason: None,
        }
    }

    pub fn action_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.action_pattern = Some(pattern.into());
        self
    }

    pub fn reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    /// 도구와 액션이 이 규칙에 매칭되는지 확인
    pub fn matches(&self, tool_id: &str, action: Option<&str>) -> bool {
        // 도구 패턴 매칭
        if !Self::pattern_matches(&self.tool_pattern, tool_id) {
            return false;
        }

        // 액션 패턴 매칭 (있으면)
        if let (Some(pattern), Some(action)) = (&self.action_pattern, action) {
            if !Self::pattern_matches(pattern, action) {
                return false;
            }
        }

        true
    }

    fn pattern_matches(pattern: &str, value: &str) -> bool {
        // 와일드카드 처리
        if pattern == "*" {
            return true;
        }

        if pattern.ends_with("*") {
            let prefix = &pattern[..pattern.len() - 1];
            return value.starts_with(prefix);
        }

        if pattern.contains("**") {
            // ** 는 모든 하위 경로 매칭
            let parts: Vec<&str> = pattern.split("**").collect();
            if parts.len() == 2 {
                return value.starts_with(parts[0])
                    && (parts[1].is_empty() || value.ends_with(parts[1]));
            }
        }

        // Glob 매칭 시도
        glob::Pattern::new(pattern)
            .map(|p| p.matches(value))
            .unwrap_or(pattern == value)
    }
}

// ============================================================================
// Session Info - 세션 정보
// ============================================================================

/// 세션 정보
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    /// 세션 ID
    pub id: String,

    /// 프로젝트 경로
    pub project_path: Option<String>,

    /// 생성 시간 (Unix timestamp)
    pub created_at: u64,

    /// 마지막 활동 시간
    pub last_active_at: u64,

    /// 사용 중인 프로바이더
    pub provider: Option<String>,

    /// 사용 중인 모델
    pub model: Option<String>,
}

impl SessionInfo {
    pub fn new(id: impl Into<String>) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            id: id.into(),
            project_path: None,
            created_at: now,
            last_active_at: now,
            provider: None,
            model: None,
        }
    }

    pub fn project(mut self, path: impl Into<String>) -> Self {
        self.project_path = Some(path.into());
        self
    }

    pub fn provider(mut self, provider: impl Into<String>) -> Self {
        self.provider = Some(provider.into());
        self
    }

    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    pub fn touch(&mut self) {
        self.last_active_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }
}

// ============================================================================
// Model Selection - 모델 선택 정보
// ============================================================================

/// 모델 선택 힌트
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelHint {
    /// 가장 빠른 모델 (haiku 등)
    Fast,
    /// 균형잡힌 모델 (sonnet 등)
    Balanced,
    /// 가장 강력한 모델 (opus 등)
    Powerful,
}

impl Default for ModelHint {
    fn default() -> Self {
        Self::Balanced
    }
}

impl ModelHint {
    /// 힌트에 맞는 Claude 모델 이름
    pub fn claude_model(&self) -> &'static str {
        match self {
            ModelHint::Fast => "claude-3-5-haiku-latest",
            ModelHint::Balanced => "claude-sonnet-4-20250514",
            ModelHint::Powerful => "claude-opus-4-20250514",
        }
    }

    /// 힌트에 맞는 OpenAI 모델 이름
    pub fn openai_model(&self) -> &'static str {
        match self {
            ModelHint::Fast => "gpt-4o-mini",
            ModelHint::Balanced => "gpt-4o",
            ModelHint::Powerful => "gpt-4o",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_source() {
        let builtin = ToolSource::builtin("bash");
        assert_eq!(builtin.full_id(), "builtin:bash");

        let mcp = ToolSource::mcp("notion", "create-page");
        assert_eq!(mcp.full_id(), "mcp:notion:create-page");
    }

    #[test]
    fn test_permission_rule_matching() {
        let rule = PermissionRule::deny("bash").action_pattern("rm *");

        assert!(rule.matches("bash", Some("rm -rf /")));
        assert!(!rule.matches("bash", Some("ls -la")));
        assert!(!rule.matches("read", Some("rm something")));

        let mcp_rule = PermissionRule::allow("mcp:*");
        assert!(mcp_rule.matches("mcp:notion:create-page", None));
        assert!(mcp_rule.matches("mcp:chrome:navigate", None));
        assert!(!mcp_rule.matches("builtin:bash", None));
    }

    #[test]
    fn test_session_info() {
        let session = SessionInfo::new("test-session")
            .project("/home/user/project")
            .provider("anthropic")
            .model("claude-sonnet-4");

        assert_eq!(session.id, "test-session");
        assert_eq!(session.project_path, Some("/home/user/project".to_string()));
    }
}
