//! Event Types - 시스템 전체에서 사용되는 이벤트 타입 정의
//!
//! ForgeCode의 모든 레이어에서 발생하는 이벤트를 정의합니다.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

// ============================================================================
// Event ID
// ============================================================================

/// 이벤트 고유 ID
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EventId(pub String);

impl EventId {
    /// 새 이벤트 ID 생성
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// 문자열에서 생성
    pub fn from_str(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl Default for EventId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for EventId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ============================================================================
// Event Category
// ============================================================================

/// 이벤트 카테고리
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventCategory {
    /// 시스템 이벤트 (시작, 종료, 설정 변경)
    System,
    /// 권한 관련 이벤트
    Permission,
    /// 도구 실행 이벤트
    Tool,
    /// 스킬 실행 이벤트
    Skill,
    /// 에이전트 이벤트
    Agent,
    /// LLM 프로바이더 이벤트
    Provider,
    /// 세션 이벤트
    Session,
    /// 에러 이벤트
    Error,
    /// 사용자 정의 이벤트
    Custom,
}

impl EventCategory {
    /// 카테고리 문자열 반환
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Permission => "permission",
            Self::Tool => "tool",
            Self::Skill => "skill",
            Self::Agent => "agent",
            Self::Provider => "provider",
            Self::Session => "session",
            Self::Error => "error",
            Self::Custom => "custom",
        }
    }
}

// ============================================================================
// Event Severity
// ============================================================================

/// 이벤트 심각도
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventSeverity {
    /// 디버그 정보
    Debug,
    /// 일반 정보
    Info,
    /// 경고
    Warning,
    /// 에러
    Error,
    /// 심각한 에러
    Critical,
}

impl EventSeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
            Self::Critical => "critical",
        }
    }
}

impl Default for EventSeverity {
    fn default() -> Self {
        Self::Info
    }
}

// ============================================================================
// ForgeEvent - 핵심 이벤트 타입
// ============================================================================

/// ForgeCode 시스템 이벤트
///
/// 모든 레이어에서 발생하는 이벤트의 공통 구조입니다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgeEvent {
    /// 이벤트 ID
    pub id: EventId,

    /// 이벤트 타입 (예: "tool.executed", "permission.granted")
    pub event_type: String,

    /// 이벤트 카테고리
    pub category: EventCategory,

    /// 심각도
    pub severity: EventSeverity,

    /// 이벤트 발생 시간
    pub timestamp: DateTime<Utc>,

    /// 이벤트 소스 (레이어/모듈)
    pub source: String,

    /// 세션 ID (있는 경우)
    pub session_id: Option<String>,

    /// 이벤트 데이터
    pub data: Value,

    /// 추가 메타데이터
    pub metadata: HashMap<String, Value>,
}

impl ForgeEvent {
    /// 새 이벤트 생성
    pub fn new(event_type: impl Into<String>, category: EventCategory) -> Self {
        Self {
            id: EventId::new(),
            event_type: event_type.into(),
            category,
            severity: EventSeverity::Info,
            timestamp: Utc::now(),
            source: String::new(),
            session_id: None,
            data: Value::Null,
            metadata: HashMap::new(),
        }
    }

    /// 심각도 설정
    pub fn with_severity(mut self, severity: EventSeverity) -> Self {
        self.severity = severity;
        self
    }

    /// 소스 설정
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = source.into();
        self
    }

    /// 세션 ID 설정
    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// 데이터 설정
    pub fn with_data(mut self, data: Value) -> Self {
        self.data = data;
        self
    }

    /// 메타데이터 추가
    pub fn with_metadata(mut self, key: impl Into<String>, value: Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }
}

// ============================================================================
// 사전 정의된 이벤트 타입들
// ============================================================================

/// 시스템 이벤트
pub mod system {
    use super::*;

    /// 시스템 시작 이벤트
    pub fn started(version: &str) -> ForgeEvent {
        ForgeEvent::new("system.started", EventCategory::System)
            .with_source("forge")
            .with_data(serde_json::json!({
                "version": version,
            }))
    }

    /// 시스템 종료 이벤트
    pub fn shutdown(reason: &str) -> ForgeEvent {
        ForgeEvent::new("system.shutdown", EventCategory::System)
            .with_source("forge")
            .with_data(serde_json::json!({
                "reason": reason,
            }))
    }

    /// 설정 변경 이벤트
    pub fn config_changed(config_type: &str, changes: Value) -> ForgeEvent {
        ForgeEvent::new("system.config_changed", EventCategory::System)
            .with_source("config")
            .with_data(serde_json::json!({
                "config_type": config_type,
                "changes": changes,
            }))
    }
}

/// 권한 이벤트
pub mod permission {
    use super::*;

    /// 권한 요청 이벤트
    pub fn requested(tool: &str, action: &str, description: &str) -> ForgeEvent {
        ForgeEvent::new("permission.requested", EventCategory::Permission)
            .with_source("permission")
            .with_data(serde_json::json!({
                "tool": tool,
                "action": action,
                "description": description,
            }))
    }

    /// 권한 승인 이벤트
    pub fn granted(tool: &str, action: &str, scope: &str) -> ForgeEvent {
        ForgeEvent::new("permission.granted", EventCategory::Permission)
            .with_source("permission")
            .with_data(serde_json::json!({
                "tool": tool,
                "action": action,
                "scope": scope,
            }))
    }

    /// 권한 거부 이벤트
    pub fn denied(tool: &str, action: &str, reason: &str) -> ForgeEvent {
        ForgeEvent::new("permission.denied", EventCategory::Permission)
            .with_severity(EventSeverity::Warning)
            .with_source("permission")
            .with_data(serde_json::json!({
                "tool": tool,
                "action": action,
                "reason": reason,
            }))
    }
}

/// 도구 이벤트
pub mod tool {
    use super::*;

    /// 도구 실행 시작 이벤트
    pub fn started(tool_name: &str, args: &Value) -> ForgeEvent {
        ForgeEvent::new("tool.started", EventCategory::Tool)
            .with_source("tool")
            .with_data(serde_json::json!({
                "tool": tool_name,
                "arguments": args,
            }))
    }

    /// 도구 실행 완료 이벤트
    pub fn completed(tool_name: &str, success: bool, duration_ms: u64) -> ForgeEvent {
        ForgeEvent::new("tool.completed", EventCategory::Tool)
            .with_source("tool")
            .with_data(serde_json::json!({
                "tool": tool_name,
                "success": success,
                "duration_ms": duration_ms,
            }))
    }

    /// 도구 실행 실패 이벤트
    pub fn failed(tool_name: &str, error: &str, duration_ms: u64) -> ForgeEvent {
        ForgeEvent::new("tool.failed", EventCategory::Tool)
            .with_severity(EventSeverity::Error)
            .with_source("tool")
            .with_data(serde_json::json!({
                "tool": tool_name,
                "error": error,
                "duration_ms": duration_ms,
            }))
    }
}

/// 세션 이벤트
pub mod session {
    use super::*;

    /// 세션 시작 이벤트
    pub fn started(session_id: &str) -> ForgeEvent {
        ForgeEvent::new("session.started", EventCategory::Session)
            .with_source("session")
            .with_session(session_id)
            .with_data(serde_json::json!({
                "session_id": session_id,
            }))
    }

    /// 세션 종료 이벤트
    pub fn ended(session_id: &str, duration_secs: u64, message_count: usize) -> ForgeEvent {
        ForgeEvent::new("session.ended", EventCategory::Session)
            .with_source("session")
            .with_session(session_id)
            .with_data(serde_json::json!({
                "session_id": session_id,
                "duration_secs": duration_secs,
                "message_count": message_count,
            }))
    }
}

/// 에러 이벤트
pub mod error {
    use super::*;

    /// 일반 에러 이벤트
    pub fn occurred(source: &str, error_type: &str, message: &str) -> ForgeEvent {
        ForgeEvent::new("error.occurred", EventCategory::Error)
            .with_severity(EventSeverity::Error)
            .with_source(source)
            .with_data(serde_json::json!({
                "error_type": error_type,
                "message": message,
            }))
    }

    /// 심각한 에러 이벤트
    pub fn critical(source: &str, error_type: &str, message: &str) -> ForgeEvent {
        ForgeEvent::new("error.critical", EventCategory::Error)
            .with_severity(EventSeverity::Critical)
            .with_source(source)
            .with_data(serde_json::json!({
                "error_type": error_type,
                "message": message,
            }))
    }
}

// ============================================================================
// 테스트
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_id() {
        let id1 = EventId::new();
        let id2 = EventId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_forge_event_creation() {
        let event = ForgeEvent::new("test.event", EventCategory::System)
            .with_severity(EventSeverity::Info)
            .with_source("test")
            .with_session("session-123")
            .with_data(serde_json::json!({"key": "value"}));

        assert_eq!(event.event_type, "test.event");
        assert_eq!(event.category, EventCategory::System);
        assert_eq!(event.source, "test");
        assert_eq!(event.session_id, Some("session-123".to_string()));
    }

    #[test]
    fn test_system_events() {
        let event = system::started("0.1.0");
        assert_eq!(event.event_type, "system.started");
        assert_eq!(event.category, EventCategory::System);
    }

    #[test]
    fn test_permission_events() {
        let event = permission::granted("read", "file.read", "session");
        assert_eq!(event.event_type, "permission.granted");
        assert_eq!(event.category, EventCategory::Permission);
    }

    #[test]
    fn test_tool_events() {
        let event = tool::completed("read", true, 150);
        assert_eq!(event.event_type, "tool.completed");
        assert_eq!(event.category, EventCategory::Tool);
    }
}
