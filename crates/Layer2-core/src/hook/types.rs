//! Hook 타입 정의
//!
//! Claude Code 호환 Hook 타입 시스템

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

// ============================================================================
// HookEventType - 이벤트 타입
// ============================================================================

/// Hook 이벤트 타입 (Claude Code 호환)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HookEventType {
    /// Tool 실행 전 (블로킹 가능)
    PreToolUse,

    /// Tool 실행 후
    PostToolUse,

    /// 세션 시작
    #[serde(alias = "session_start")]
    SessionStart,

    /// 세션 종료
    #[serde(alias = "session_stop")]
    SessionStop,

    /// 프롬프트 제출
    #[serde(alias = "prompt_submit")]
    PromptSubmit,

    /// 에이전트 완료
    #[serde(alias = "agent_complete")]
    AgentComplete,

    /// 파일 변경
    #[serde(alias = "file_changed")]
    FileChanged,
}

impl std::fmt::Display for HookEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PreToolUse => write!(f, "PreToolUse"),
            Self::PostToolUse => write!(f, "PostToolUse"),
            Self::SessionStart => write!(f, "SessionStart"),
            Self::SessionStop => write!(f, "SessionStop"),
            Self::PromptSubmit => write!(f, "PromptSubmit"),
            Self::AgentComplete => write!(f, "AgentComplete"),
            Self::FileChanged => write!(f, "FileChanged"),
        }
    }
}

// ============================================================================
// HookEvent - 이벤트 데이터
// ============================================================================

/// Hook 이벤트 데이터
#[derive(Debug, Clone)]
pub struct HookEvent {
    /// 이벤트 타입
    pub event_type: HookEventType,

    /// Tool 이름 (Tool 관련 이벤트 시)
    pub tool_name: Option<String>,

    /// Tool 입력 파라미터 (JSON)
    pub tool_input: Option<Value>,

    /// Tool 결과 (PostToolUse 시)
    pub tool_output: Option<String>,

    /// 파일 경로 (FileChanged 시)
    pub file_path: Option<String>,

    /// 프롬프트 (PromptSubmit 시)
    pub prompt: Option<String>,

    /// 추가 메타데이터
    pub metadata: HashMap<String, Value>,
}

impl HookEvent {
    /// PreToolUse 이벤트 생성
    pub fn pre_tool_use(tool_name: impl Into<String>, input: Value) -> Self {
        Self {
            event_type: HookEventType::PreToolUse,
            tool_name: Some(tool_name.into()),
            tool_input: Some(input),
            tool_output: None,
            file_path: None,
            prompt: None,
            metadata: HashMap::new(),
        }
    }

    /// PostToolUse 이벤트 생성
    pub fn post_tool_use(
        tool_name: impl Into<String>,
        input: Value,
        output: impl Into<String>,
    ) -> Self {
        Self {
            event_type: HookEventType::PostToolUse,
            tool_name: Some(tool_name.into()),
            tool_input: Some(input),
            tool_output: Some(output.into()),
            file_path: None,
            prompt: None,
            metadata: HashMap::new(),
        }
    }

    /// SessionStart 이벤트 생성
    pub fn session_start() -> Self {
        Self {
            event_type: HookEventType::SessionStart,
            tool_name: None,
            tool_input: None,
            tool_output: None,
            file_path: None,
            prompt: None,
            metadata: HashMap::new(),
        }
    }

    /// SessionStop 이벤트 생성
    pub fn session_stop() -> Self {
        Self {
            event_type: HookEventType::SessionStop,
            tool_name: None,
            tool_input: None,
            tool_output: None,
            file_path: None,
            prompt: None,
            metadata: HashMap::new(),
        }
    }

    /// PromptSubmit 이벤트 생성
    pub fn prompt_submit(prompt: impl Into<String>) -> Self {
        Self {
            event_type: HookEventType::PromptSubmit,
            tool_name: None,
            tool_input: None,
            tool_output: None,
            file_path: None,
            prompt: Some(prompt.into()),
            metadata: HashMap::new(),
        }
    }

    /// FileChanged 이벤트 생성
    pub fn file_changed(path: impl Into<String>) -> Self {
        Self {
            event_type: HookEventType::FileChanged,
            tool_name: None,
            tool_input: None,
            tool_output: None,
            file_path: Some(path.into()),
            prompt: None,
            metadata: HashMap::new(),
        }
    }

    /// 메타데이터 추가
    pub fn with_metadata(mut self, key: impl Into<String>, value: Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }
}

// ============================================================================
// HookMatcher - 매칭 패턴
// ============================================================================

/// Hook 매처 (어떤 이벤트에 반응할지)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookMatcher {
    /// 매칭 패턴 (tool 이름, "*", 또는 glob 패턴)
    pub matcher: String,

    /// 실행할 Hook 액션들
    pub hooks: Vec<HookAction>,
}

impl HookMatcher {
    /// 새 매처 생성
    pub fn new(matcher: impl Into<String>) -> Self {
        Self {
            matcher: matcher.into(),
            hooks: Vec::new(),
        }
    }

    /// Hook 액션 추가
    pub fn with_action(mut self, action: HookAction) -> Self {
        self.hooks.push(action);
        self
    }

    /// 이벤트와 매칭되는지 확인
    pub fn matches(&self, event: &HookEvent) -> bool {
        if self.matcher == "*" {
            return true;
        }

        match &event.tool_name {
            Some(tool_name) => {
                // 정확한 매칭
                if self.matcher == *tool_name {
                    return true;
                }

                // glob 패턴 매칭 (간단한 * 처리)
                if self.matcher.contains('*') {
                    let pattern = self.matcher.replace('*', "");
                    if self.matcher.starts_with('*') && tool_name.ends_with(&pattern) {
                        return true;
                    }
                    if self.matcher.ends_with('*') && tool_name.starts_with(&pattern) {
                        return true;
                    }
                }

                false
            }
            None => self.matcher == "*",
        }
    }
}

// ============================================================================
// HookAction - 액션 정의
// ============================================================================

/// Hook 액션 (실행할 내용)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum HookAction {
    /// Shell 명령어 실행
    #[serde(rename = "command")]
    Command {
        /// 실행할 명령어
        command: String,

        /// 타임아웃 (초)
        #[serde(default = "default_timeout")]
        timeout: u64,

        /// 블로킹 여부 (PreToolUse에서 실패 시 Tool 실행 차단)
        #[serde(default)]
        blocking: bool,
    },

    /// LLM 프롬프트
    #[serde(rename = "prompt")]
    Prompt {
        /// 프롬프트 내용
        prompt: String,
    },

    /// Subagent 실행
    #[serde(rename = "agent")]
    Agent {
        /// Agent 타입 (Explore, Plan, 커스텀)
        agent: String,

        /// 프롬프트
        prompt: String,

        /// 최대 턴 수
        #[serde(default = "default_max_turns")]
        max_turns: u32,
    },

    /// 알림 (로그/콘솔 출력)
    #[serde(rename = "notify")]
    Notify {
        /// 메시지
        message: String,

        /// 레벨 (info, warn, error)
        #[serde(default = "default_level")]
        level: String,
    },
}

fn default_timeout() -> u64 {
    30
}

fn default_max_turns() -> u32 {
    10
}

fn default_level() -> String {
    "info".to_string()
}

impl HookAction {
    /// Command 액션 생성
    pub fn command(cmd: impl Into<String>) -> Self {
        Self::Command {
            command: cmd.into(),
            timeout: default_timeout(),
            blocking: false,
        }
    }

    /// Blocking command 생성
    pub fn blocking_command(cmd: impl Into<String>) -> Self {
        Self::Command {
            command: cmd.into(),
            timeout: default_timeout(),
            blocking: true,
        }
    }

    /// Prompt 액션 생성
    pub fn prompt(prompt: impl Into<String>) -> Self {
        Self::Prompt {
            prompt: prompt.into(),
        }
    }

    /// Agent 액션 생성
    pub fn agent(agent_type: impl Into<String>, prompt: impl Into<String>) -> Self {
        Self::Agent {
            agent: agent_type.into(),
            prompt: prompt.into(),
            max_turns: default_max_turns(),
        }
    }

    /// Notify 액션 생성
    pub fn notify(message: impl Into<String>) -> Self {
        Self::Notify {
            message: message.into(),
            level: default_level(),
        }
    }
}

// ============================================================================
// HookConfig - 전체 Hook 설정
// ============================================================================

/// Hook 설정 (hooks.json 형식)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HookConfig {
    /// PreToolUse 매처들
    #[serde(rename = "PreToolUse", default)]
    pub pre_tool_use: Vec<HookMatcher>,

    /// PostToolUse 매처들
    #[serde(rename = "PostToolUse", default)]
    pub post_tool_use: Vec<HookMatcher>,

    /// SessionStart 매처들
    #[serde(rename = "SessionStart", default)]
    pub session_start: Vec<HookMatcher>,

    /// SessionStop 매처들
    #[serde(rename = "SessionStop", default)]
    pub session_stop: Vec<HookMatcher>,

    /// PromptSubmit 매처들
    #[serde(rename = "PromptSubmit", default)]
    pub prompt_submit: Vec<HookMatcher>,

    /// AgentComplete 매처들
    #[serde(rename = "AgentComplete", default)]
    pub agent_complete: Vec<HookMatcher>,

    /// FileChanged 매처들
    #[serde(rename = "FileChanged", default)]
    pub file_changed: Vec<HookMatcher>,
}

impl HookConfig {
    /// 새 설정 생성
    pub fn new() -> Self {
        Self::default()
    }

    /// 이벤트 타입에 해당하는 매처들 가져오기
    pub fn matchers_for(&self, event_type: HookEventType) -> &[HookMatcher] {
        match event_type {
            HookEventType::PreToolUse => &self.pre_tool_use,
            HookEventType::PostToolUse => &self.post_tool_use,
            HookEventType::SessionStart => &self.session_start,
            HookEventType::SessionStop => &self.session_stop,
            HookEventType::PromptSubmit => &self.prompt_submit,
            HookEventType::AgentComplete => &self.agent_complete,
            HookEventType::FileChanged => &self.file_changed,
        }
    }

    /// 다른 설정과 병합
    pub fn merge(&mut self, other: HookConfig) {
        self.pre_tool_use.extend(other.pre_tool_use);
        self.post_tool_use.extend(other.post_tool_use);
        self.session_start.extend(other.session_start);
        self.session_stop.extend(other.session_stop);
        self.prompt_submit.extend(other.prompt_submit);
        self.agent_complete.extend(other.agent_complete);
        self.file_changed.extend(other.file_changed);
    }

    /// 전체 매처 수
    pub fn total_matchers(&self) -> usize {
        self.pre_tool_use.len()
            + self.post_tool_use.len()
            + self.session_start.len()
            + self.session_stop.len()
            + self.prompt_submit.len()
            + self.agent_complete.len()
            + self.file_changed.len()
    }

    /// 비어있는지 확인
    pub fn is_empty(&self) -> bool {
        self.total_matchers() == 0
    }
}

// ============================================================================
// HookResult - 실행 결과
// ============================================================================

/// Hook 실행 결과
#[derive(Debug, Clone)]
pub struct HookResult {
    /// 성공 여부
    pub success: bool,

    /// 결과 출력
    pub output: Option<String>,

    /// 에러 메시지
    pub error: Option<String>,

    /// 실행 시간 (밀리초)
    pub duration_ms: u64,

    /// 결과 상태
    pub outcome: HookOutcome,
}

impl HookResult {
    /// 성공 결과 생성
    pub fn success(output: impl Into<String>, duration_ms: u64) -> Self {
        Self {
            success: true,
            output: Some(output.into()),
            error: None,
            duration_ms,
            outcome: HookOutcome::Passed,
        }
    }

    /// 실패 결과 생성
    pub fn failure(error: impl Into<String>, duration_ms: u64) -> Self {
        Self {
            success: false,
            output: None,
            error: Some(error.into()),
            duration_ms,
            outcome: HookOutcome::Failed,
        }
    }

    /// 블로킹 결과 생성
    pub fn blocked(reason: BlockReason, duration_ms: u64) -> Self {
        Self {
            success: false,
            output: None,
            error: Some(format!("Blocked: {:?}", reason)),
            duration_ms,
            outcome: HookOutcome::Blocked(reason),
        }
    }
}

/// Hook 실행 결과 상태
#[derive(Debug, Clone)]
pub enum HookOutcome {
    /// 통과
    Passed,
    /// 실패 (비블로킹)
    Failed,
    /// 블로킹됨 (PreToolUse에서)
    Blocked(BlockReason),
    /// 스킵됨
    Skipped,
}

/// 블로킹 사유
#[derive(Debug, Clone)]
pub struct BlockReason {
    /// 이유
    pub reason: String,
    /// 상세 정보
    pub details: Option<String>,
}

impl BlockReason {
    /// 새 블로킹 사유 생성
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            details: None,
        }
    }

    /// 상세 정보 추가
    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }
}

// ============================================================================
// 테스트
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_event_type_display() {
        assert_eq!(HookEventType::PreToolUse.to_string(), "PreToolUse");
        assert_eq!(HookEventType::PostToolUse.to_string(), "PostToolUse");
    }

    #[test]
    fn test_hook_event_creation() {
        let event = HookEvent::pre_tool_use("Bash", serde_json::json!({"command": "ls"}));
        assert_eq!(event.event_type, HookEventType::PreToolUse);
        assert_eq!(event.tool_name, Some("Bash".to_string()));
    }

    #[test]
    fn test_hook_matcher() {
        let matcher = HookMatcher::new("Bash");
        let event = HookEvent::pre_tool_use("Bash", serde_json::json!({}));
        assert!(matcher.matches(&event));

        let event2 = HookEvent::pre_tool_use("Read", serde_json::json!({}));
        assert!(!matcher.matches(&event2));
    }

    #[test]
    fn test_wildcard_matcher() {
        let matcher = HookMatcher::new("*");
        let event = HookEvent::pre_tool_use("Bash", serde_json::json!({}));
        assert!(matcher.matches(&event));
    }

    #[test]
    fn test_hook_config_parse() {
        let json = r#"{
            "PreToolUse": [{
                "matcher": "Bash",
                "hooks": [{
                    "type": "command",
                    "command": "echo 'Hello'"
                }]
            }]
        }"#;

        let config: HookConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.pre_tool_use.len(), 1);
        assert_eq!(config.pre_tool_use[0].matcher, "Bash");
    }

    #[test]
    fn test_hook_action_command() {
        let action = HookAction::command("echo test");
        match action {
            HookAction::Command {
                command,
                timeout,
                blocking,
            } => {
                assert_eq!(command, "echo test");
                assert_eq!(timeout, 30);
                assert!(!blocking);
            }
            other => panic!("Expected Command action, got {:?}", other),
        }
    }

    #[test]
    fn test_hook_config_merge() {
        let mut config1 = HookConfig::new();
        config1.pre_tool_use.push(HookMatcher::new("Bash"));

        let mut config2 = HookConfig::new();
        config2.pre_tool_use.push(HookMatcher::new("Read"));
        config2.post_tool_use.push(HookMatcher::new("*"));

        config1.merge(config2);

        assert_eq!(config1.pre_tool_use.len(), 2);
        assert_eq!(config1.post_tool_use.len(), 1);
    }
}
