//! Core Types - 공용 타입 정의
//!
//! 모든 레이어에서 공통으로 사용하는 타입들

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
