//! Audit Log Types - 감사 로그 타입 정의
//!
//! 권한 요청, 도구 실행, 에러 등의 감사 기록을 위한 타입들입니다.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ============================================================================
// Audit Entry ID
// ============================================================================

/// 감사 로그 엔트리 ID
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AuditId(pub String);

impl AuditId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

impl Default for AuditId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for AuditId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ============================================================================
// Audit Action Type
// ============================================================================

/// 감사 대상 액션 타입
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditAction {
    // 권한 관련
    /// 권한 요청
    PermissionRequested,
    /// 권한 승인
    PermissionGranted,
    /// 권한 거부
    PermissionDenied,

    // 도구 관련
    /// 도구 실행 시작
    ToolStarted,
    /// 도구 실행 성공
    ToolSucceeded,
    /// 도구 실행 실패
    ToolFailed,

    // 파일 관련
    /// 파일 읽기
    FileRead,
    /// 파일 쓰기
    FileWrite,
    /// 파일 삭제
    FileDelete,

    // 명령어 관련
    /// 명령어 실행
    CommandExecuted,
    /// 명령어 차단
    CommandBlocked,

    // 세션 관련
    /// 세션 시작
    SessionStarted,
    /// 세션 종료
    SessionEnded,

    // 설정 관련
    /// 설정 변경
    ConfigChanged,

    // 에러
    /// 에러 발생
    ErrorOccurred,

    // 기타
    /// 사용자 정의 액션
    Custom,
}

impl AuditAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PermissionRequested => "permission_requested",
            Self::PermissionGranted => "permission_granted",
            Self::PermissionDenied => "permission_denied",
            Self::ToolStarted => "tool_started",
            Self::ToolSucceeded => "tool_succeeded",
            Self::ToolFailed => "tool_failed",
            Self::FileRead => "file_read",
            Self::FileWrite => "file_write",
            Self::FileDelete => "file_delete",
            Self::CommandExecuted => "command_executed",
            Self::CommandBlocked => "command_blocked",
            Self::SessionStarted => "session_started",
            Self::SessionEnded => "session_ended",
            Self::ConfigChanged => "config_changed",
            Self::ErrorOccurred => "error_occurred",
            Self::Custom => "custom",
        }
    }

    /// 위험도 레벨 (0-10)
    pub fn risk_level(&self) -> u8 {
        match self {
            Self::PermissionDenied => 3,
            Self::PermissionGranted => 2,
            Self::PermissionRequested => 1,
            Self::ToolFailed => 4,
            Self::ToolSucceeded => 1,
            Self::ToolStarted => 0,
            Self::FileRead => 1,
            Self::FileWrite => 5,
            Self::FileDelete => 7,
            Self::CommandExecuted => 6,
            Self::CommandBlocked => 8,
            Self::SessionStarted => 0,
            Self::SessionEnded => 0,
            Self::ConfigChanged => 3,
            Self::ErrorOccurred => 5,
            Self::Custom => 2,
        }
    }
}

// ============================================================================
// Audit Result
// ============================================================================

/// 감사 대상 작업의 결과
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditResult {
    /// 성공
    Success,
    /// 실패
    Failure,
    /// 거부됨
    Denied,
    /// 시간 초과
    Timeout,
    /// 취소됨
    Cancelled,
    /// 보류 중
    Pending,
}

impl AuditResult {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
            Self::Denied => "denied",
            Self::Timeout => "timeout",
            Self::Cancelled => "cancelled",
            Self::Pending => "pending",
        }
    }

    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success)
    }
}

// ============================================================================
// Audit Entry
// ============================================================================

/// 감사 로그 엔트리
///
/// 시스템에서 발생한 중요 이벤트의 감사 기록입니다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// 고유 ID
    pub id: AuditId,

    /// 발생 시간
    pub timestamp: DateTime<Utc>,

    /// 액션 타입
    pub action: AuditAction,

    /// 결과
    pub result: AuditResult,

    /// 세션 ID
    pub session_id: Option<String>,

    /// 액터 (도구, 사용자 등)
    pub actor: String,

    /// 대상 (파일 경로, 명령어 등)
    pub target: Option<String>,

    /// 설명
    pub description: String,

    /// 추가 데이터
    pub data: Value,

    /// 실행 시간 (밀리초)
    pub duration_ms: Option<u64>,

    /// 에러 메시지 (실패 시)
    pub error: Option<String>,

    /// 위험도 레벨 (0-10)
    pub risk_level: u8,

    /// 태그
    pub tags: Vec<String>,
}

impl AuditEntry {
    /// 새 감사 엔트리 생성
    pub fn new(action: AuditAction, actor: impl Into<String>) -> Self {
        Self {
            id: AuditId::new(),
            timestamp: Utc::now(),
            action,
            result: AuditResult::Pending,
            session_id: None,
            actor: actor.into(),
            target: None,
            description: String::new(),
            data: Value::Null,
            duration_ms: None,
            error: None,
            risk_level: action.risk_level(),
            tags: Vec::new(),
        }
    }

    /// 결과 설정
    pub fn with_result(mut self, result: AuditResult) -> Self {
        self.result = result;
        self
    }

    /// 세션 ID 설정
    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// 대상 설정
    pub fn with_target(mut self, target: impl Into<String>) -> Self {
        self.target = Some(target.into());
        self
    }

    /// 설명 설정
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// 데이터 설정
    pub fn with_data(mut self, data: Value) -> Self {
        self.data = data;
        self
    }

    /// 실행 시간 설정
    pub fn with_duration(mut self, duration_ms: u64) -> Self {
        self.duration_ms = Some(duration_ms);
        self
    }

    /// 에러 설정
    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.error = Some(error.into());
        self.result = AuditResult::Failure;
        self
    }

    /// 태그 추가
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// 위험도 오버라이드
    pub fn with_risk_level(mut self, level: u8) -> Self {
        self.risk_level = level.min(10);
        self
    }
}

// ============================================================================
// Audit Query
// ============================================================================

/// 감사 로그 조회 쿼리
#[derive(Debug, Clone, Default)]
pub struct AuditQuery {
    /// 액션 필터
    pub actions: Option<Vec<AuditAction>>,

    /// 결과 필터
    pub results: Option<Vec<AuditResult>>,

    /// 세션 ID 필터
    pub session_id: Option<String>,

    /// 액터 필터
    pub actor: Option<String>,

    /// 시작 시간
    pub from: Option<DateTime<Utc>>,

    /// 종료 시간
    pub to: Option<DateTime<Utc>>,

    /// 최소 위험도
    pub min_risk_level: Option<u8>,

    /// 태그 필터
    pub tags: Option<Vec<String>>,

    /// 최대 결과 수
    pub limit: Option<usize>,

    /// 오프셋
    pub offset: Option<usize>,
}

impl AuditQuery {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_actions(mut self, actions: Vec<AuditAction>) -> Self {
        self.actions = Some(actions);
        self
    }

    pub fn with_results(mut self, results: Vec<AuditResult>) -> Self {
        self.results = Some(results);
        self
    }

    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    pub fn with_actor(mut self, actor: impl Into<String>) -> Self {
        self.actor = Some(actor.into());
        self
    }

    pub fn with_time_range(mut self, from: DateTime<Utc>, to: DateTime<Utc>) -> Self {
        self.from = Some(from);
        self.to = Some(to);
        self
    }

    pub fn with_min_risk(mut self, level: u8) -> Self {
        self.min_risk_level = Some(level);
        self
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    pub fn with_offset(mut self, offset: usize) -> Self {
        self.offset = Some(offset);
        self
    }

    /// 엔트리가 쿼리와 매칭되는지 확인
    pub fn matches(&self, entry: &AuditEntry) -> bool {
        if let Some(ref actions) = self.actions {
            if !actions.contains(&entry.action) {
                return false;
            }
        }

        if let Some(ref results) = self.results {
            if !results.contains(&entry.result) {
                return false;
            }
        }

        if let Some(ref session_id) = self.session_id {
            if entry.session_id.as_ref() != Some(session_id) {
                return false;
            }
        }

        if let Some(ref actor) = self.actor {
            if &entry.actor != actor {
                return false;
            }
        }

        if let Some(ref from) = self.from {
            if entry.timestamp < *from {
                return false;
            }
        }

        if let Some(ref to) = self.to {
            if entry.timestamp > *to {
                return false;
            }
        }

        if let Some(min_risk) = self.min_risk_level {
            if entry.risk_level < min_risk {
                return false;
            }
        }

        if let Some(ref tags) = self.tags {
            if !tags.iter().any(|t| entry.tags.contains(t)) {
                return false;
            }
        }

        true
    }
}

// ============================================================================
// Audit Statistics
// ============================================================================

/// 감사 로그 통계
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuditStatistics {
    /// 총 엔트리 수
    pub total_entries: u64,

    /// 액션별 카운트
    pub by_action: std::collections::HashMap<String, u64>,

    /// 결과별 카운트
    pub by_result: std::collections::HashMap<String, u64>,

    /// 평균 위험도
    pub avg_risk_level: f64,

    /// 최고 위험도 엔트리
    pub highest_risk_entries: Vec<AuditId>,

    /// 기간
    pub period_start: Option<DateTime<Utc>>,
    pub period_end: Option<DateTime<Utc>>,
}

// ============================================================================
// 테스트
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_entry_creation() {
        let entry = AuditEntry::new(AuditAction::ToolSucceeded, "read")
            .with_result(AuditResult::Success)
            .with_target("/path/to/file")
            .with_description("Read file successfully")
            .with_duration(150);

        assert_eq!(entry.action, AuditAction::ToolSucceeded);
        assert_eq!(entry.result, AuditResult::Success);
        assert_eq!(entry.actor, "read");
        assert_eq!(entry.target, Some("/path/to/file".to_string()));
        assert_eq!(entry.duration_ms, Some(150));
    }

    #[test]
    fn test_audit_query() {
        let entry = AuditEntry::new(AuditAction::ToolSucceeded, "read")
            .with_result(AuditResult::Success)
            .with_session("session-123");

        let query = AuditQuery::new()
            .with_actions(vec![AuditAction::ToolSucceeded])
            .with_session("session-123");

        assert!(query.matches(&entry));

        let query2 = AuditQuery::new().with_actions(vec![AuditAction::ToolFailed]);

        assert!(!query2.matches(&entry));
    }

    #[test]
    fn test_risk_levels() {
        assert_eq!(AuditAction::FileDelete.risk_level(), 7);
        assert_eq!(AuditAction::CommandBlocked.risk_level(), 8);
        assert_eq!(AuditAction::FileRead.risk_level(), 1);
    }
}
