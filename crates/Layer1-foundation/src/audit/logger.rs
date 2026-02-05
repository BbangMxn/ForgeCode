//! Audit Logger - 감사 로그 기록 및 관리
//!
//! 감사 로그를 SQLite에 저장하고 조회하는 기능을 제공합니다.

use super::types::{AuditAction, AuditEntry, AuditId, AuditQuery, AuditResult, AuditStatistics};
use crate::event::{EventBus, EventCategory, EventListener, ForgeEvent};
use async_trait::async_trait;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, error, info};

// ============================================================================
// AuditLogger
// ============================================================================

/// 감사 로거 설정
#[derive(Debug, Clone)]
pub struct AuditLoggerConfig {
    /// 데이터베이스 경로
    pub db_path: PathBuf,

    /// 최대 보관 기간 (일)
    pub retention_days: u32,

    /// 자동 정리 활성화
    pub auto_cleanup: bool,

    /// 이벤트 버스 연동 활성화
    pub event_integration: bool,
}

impl Default for AuditLoggerConfig {
    fn default() -> Self {
        let db_path = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("forgecode")
            .join("audit.db");

        Self {
            db_path,
            retention_days: 90,
            auto_cleanup: true,
            event_integration: true,
        }
    }
}

/// 감사 로거
///
/// 시스템의 모든 중요 이벤트를 기록하고 조회합니다.
///
/// ## 사용법
///
/// ```ignore
/// use forge_foundation::audit::{AuditLogger, AuditEntry, AuditAction, AuditResult};
///
/// let logger = AuditLogger::new()?;
///
/// // 감사 로그 기록
/// let entry = AuditEntry::new(AuditAction::ToolSucceeded, "read")
///     .with_result(AuditResult::Success)
///     .with_target("/path/to/file");
///
/// logger.log(entry).await?;
///
/// // 조회
/// let query = AuditQuery::new()
///     .with_actions(vec![AuditAction::ToolSucceeded])
///     .with_limit(10);
///
/// let entries = logger.query(&query).await?;
/// ```
pub struct AuditLogger {
    /// SQLite 연결
    db: Mutex<Connection>,

    /// 설정
    config: AuditLoggerConfig,
}

impl AuditLogger {
    /// 기본 설정으로 감사 로거 생성
    pub fn new() -> crate::Result<Self> {
        Self::with_config(AuditLoggerConfig::default())
    }

    /// 커스텀 설정으로 감사 로거 생성
    pub fn with_config(config: AuditLoggerConfig) -> crate::Result<Self> {
        // 디렉토리 생성
        if let Some(parent) = config.db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(&config.db_path)?;

        let logger = Self {
            db: Mutex::new(conn),
            config,
        };

        // 테이블 초기화
        tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(logger.init_tables())
        })?;

        info!(
            db_path = %logger.config.db_path.display(),
            "Audit logger initialized"
        );

        Ok(logger)
    }

    /// 인메모리 로거 생성 (테스트용)
    pub fn in_memory() -> crate::Result<Self> {
        let conn = Connection::open_in_memory()?;

        let logger = Self {
            db: Mutex::new(conn),
            config: AuditLoggerConfig {
                db_path: PathBuf::from(":memory:"),
                ..Default::default()
            },
        };

        tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(logger.init_tables())
        })?;

        Ok(logger)
    }

    /// 테이블 초기화
    async fn init_tables(&self) -> crate::Result<()> {
        let db = self.db.lock().await;

        db.execute(
            r#"
            CREATE TABLE IF NOT EXISTS audit_log (
                id TEXT PRIMARY KEY,
                timestamp TEXT NOT NULL,
                action TEXT NOT NULL,
                result TEXT NOT NULL,
                session_id TEXT,
                actor TEXT NOT NULL,
                target TEXT,
                description TEXT NOT NULL,
                data TEXT NOT NULL,
                duration_ms INTEGER,
                error TEXT,
                risk_level INTEGER NOT NULL,
                tags TEXT NOT NULL
            )
            "#,
            [],
        )?;

        // 인덱스 생성
        db.execute(
            "CREATE INDEX IF NOT EXISTS idx_audit_timestamp ON audit_log(timestamp)",
            [],
        )?;
        db.execute(
            "CREATE INDEX IF NOT EXISTS idx_audit_action ON audit_log(action)",
            [],
        )?;
        db.execute(
            "CREATE INDEX IF NOT EXISTS idx_audit_session ON audit_log(session_id)",
            [],
        )?;
        db.execute(
            "CREATE INDEX IF NOT EXISTS idx_audit_risk ON audit_log(risk_level)",
            [],
        )?;

        Ok(())
    }

    /// 감사 로그 기록
    pub async fn log(&self, entry: AuditEntry) -> crate::Result<AuditId> {
        let db = self.db.lock().await;

        let id = entry.id.clone();
        let timestamp = entry.timestamp.to_rfc3339();
        let action = entry.action.as_str();
        let result = entry.result.as_str();
        let data = serde_json::to_string(&entry.data)?;
        let tags = serde_json::to_string(&entry.tags)?;

        db.execute(
            r#"
            INSERT INTO audit_log (
                id, timestamp, action, result, session_id, actor, target,
                description, data, duration_ms, error, risk_level, tags
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            "#,
            params![
                id.0,
                timestamp,
                action,
                result,
                entry.session_id,
                entry.actor,
                entry.target,
                entry.description,
                data,
                entry.duration_ms,
                entry.error,
                entry.risk_level,
                tags,
            ],
        )?;

        debug!(
            audit_id = %id,
            action = action,
            actor = %entry.actor,
            "Audit entry logged"
        );

        Ok(id)
    }

    /// ID로 감사 로그 조회
    pub async fn get(&self, id: &AuditId) -> crate::Result<Option<AuditEntry>> {
        let db = self.db.lock().await;

        let entry = db
            .query_row(
                "SELECT * FROM audit_log WHERE id = ?1",
                params![id.0],
                |row| Self::row_to_entry(row),
            )
            .optional()?;

        Ok(entry)
    }

    /// 쿼리로 감사 로그 조회
    pub async fn query(&self, query: &AuditQuery) -> crate::Result<Vec<AuditEntry>> {
        let db = self.db.lock().await;

        let mut sql = String::from("SELECT * FROM audit_log WHERE 1=1");
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        // 액션 필터
        if let Some(ref actions) = query.actions {
            let placeholders: Vec<String> = actions.iter().map(|_| "?".to_string()).collect();
            sql.push_str(&format!(" AND action IN ({})", placeholders.join(", ")));
            for action in actions {
                params_vec.push(Box::new(action.as_str().to_string()));
            }
        }

        // 결과 필터
        if let Some(ref results) = query.results {
            let placeholders: Vec<String> = results.iter().map(|_| "?".to_string()).collect();
            sql.push_str(&format!(" AND result IN ({})", placeholders.join(", ")));
            for result in results {
                params_vec.push(Box::new(result.as_str().to_string()));
            }
        }

        // 세션 필터
        if let Some(ref session_id) = query.session_id {
            sql.push_str(" AND session_id = ?");
            params_vec.push(Box::new(session_id.clone()));
        }

        // 액터 필터
        if let Some(ref actor) = query.actor {
            sql.push_str(" AND actor = ?");
            params_vec.push(Box::new(actor.clone()));
        }

        // 시간 범위 필터
        if let Some(ref from) = query.from {
            sql.push_str(" AND timestamp >= ?");
            params_vec.push(Box::new(from.to_rfc3339()));
        }
        if let Some(ref to) = query.to {
            sql.push_str(" AND timestamp <= ?");
            params_vec.push(Box::new(to.to_rfc3339()));
        }

        // 위험도 필터
        if let Some(min_risk) = query.min_risk_level {
            sql.push_str(" AND risk_level >= ?");
            params_vec.push(Box::new(min_risk as i32));
        }

        // 정렬
        sql.push_str(" ORDER BY timestamp DESC");

        // 페이지네이션
        if let Some(limit) = query.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }
        if let Some(offset) = query.offset {
            sql.push_str(&format!(" OFFSET {}", offset));
        }

        let params_refs: Vec<&dyn rusqlite::ToSql> =
            params_vec.iter().map(|p| p.as_ref()).collect();

        let mut stmt = db.prepare(&sql)?;
        let entries = stmt
            .query_map(params_refs.as_slice(), |row| Self::row_to_entry(row))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(entries)
    }

    /// 최근 감사 로그 조회
    pub async fn recent(&self, limit: usize) -> crate::Result<Vec<AuditEntry>> {
        self.query(&AuditQuery::new().with_limit(limit)).await
    }

    /// 고위험 감사 로그 조회
    pub async fn high_risk(&self, min_level: u8, limit: usize) -> crate::Result<Vec<AuditEntry>> {
        self.query(&AuditQuery::new().with_min_risk(min_level).with_limit(limit))
            .await
    }

    /// 통계 계산
    pub async fn statistics(&self) -> crate::Result<AuditStatistics> {
        let db = self.db.lock().await;

        let total_entries: u64 =
            db.query_row("SELECT COUNT(*) FROM audit_log", [], |row| row.get(0))?;

        let avg_risk: f64 = db
            .query_row("SELECT AVG(risk_level) FROM audit_log", [], |row| {
                row.get::<_, Option<f64>>(0)
            })?
            .unwrap_or(0.0);

        // 액션별 카운트
        let mut by_action = std::collections::HashMap::new();
        let mut stmt = db.prepare("SELECT action, COUNT(*) FROM audit_log GROUP BY action")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let action: String = row.get(0)?;
            let count: u64 = row.get(1)?;
            by_action.insert(action, count);
        }

        // 결과별 카운트
        let mut by_result = std::collections::HashMap::new();
        let mut stmt = db.prepare("SELECT result, COUNT(*) FROM audit_log GROUP BY result")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let result: String = row.get(0)?;
            let count: u64 = row.get(1)?;
            by_result.insert(result, count);
        }

        // 최고 위험도 엔트리
        let mut highest_risk_entries = Vec::new();
        let mut stmt = db.prepare(
            "SELECT id FROM audit_log ORDER BY risk_level DESC, timestamp DESC LIMIT 10",
        )?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            highest_risk_entries.push(AuditId(id));
        }

        Ok(AuditStatistics {
            total_entries,
            by_action,
            by_result,
            avg_risk_level: avg_risk,
            highest_risk_entries,
            period_start: None,
            period_end: None,
        })
    }

    /// 오래된 로그 정리
    pub async fn cleanup(&self, days: u32) -> crate::Result<u64> {
        let db = self.db.lock().await;

        let cutoff = chrono::Utc::now() - chrono::Duration::days(days as i64);
        let cutoff_str = cutoff.to_rfc3339();

        let deleted = db.execute(
            "DELETE FROM audit_log WHERE timestamp < ?1",
            params![cutoff_str],
        )?;

        if deleted > 0 {
            info!(
                deleted = deleted,
                days = days,
                "Cleaned up old audit entries"
            );
        }

        Ok(deleted as u64)
    }

    /// 행을 AuditEntry로 변환
    fn row_to_entry(row: &rusqlite::Row) -> rusqlite::Result<AuditEntry> {
        let id: String = row.get("id")?;
        let timestamp: String = row.get("timestamp")?;
        let action: String = row.get("action")?;
        let result: String = row.get("result")?;
        let session_id: Option<String> = row.get("session_id")?;
        let actor: String = row.get("actor")?;
        let target: Option<String> = row.get("target")?;
        let description: String = row.get("description")?;
        let data: String = row.get("data")?;
        let duration_ms: Option<u64> = row.get("duration_ms")?;
        let error: Option<String> = row.get("error")?;
        let risk_level: u8 = row.get("risk_level")?;
        let tags: String = row.get("tags")?;

        Ok(AuditEntry {
            id: AuditId(id),
            timestamp: chrono::DateTime::parse_from_rfc3339(&timestamp)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
            action: parse_action(&action),
            result: parse_result(&result),
            session_id,
            actor,
            target,
            description,
            data: serde_json::from_str(&data).unwrap_or(Value::Null),
            duration_ms,
            error,
            risk_level,
            tags: serde_json::from_str(&tags).unwrap_or_default(),
        })
    }
}

// ============================================================================
// AuditEventListener - EventBus 연동
// ============================================================================

/// 이벤트 버스와 연동되는 감사 리스너
pub struct AuditEventListener {
    logger: Arc<AuditLogger>,
}

impl AuditEventListener {
    pub fn new(logger: Arc<AuditLogger>) -> Self {
        Self { logger }
    }

    /// EventBus에 리스너 등록
    pub async fn register(logger: Arc<AuditLogger>, event_bus: &EventBus) {
        let listener = Arc::new(Self::new(logger));
        event_bus.subscribe(listener).await;
    }
}

#[async_trait]
impl EventListener for AuditEventListener {
    fn name(&self) -> &str {
        "audit_logger"
    }

    fn categories(&self) -> Option<Vec<EventCategory>> {
        // 모든 중요 카테고리 구독
        Some(vec![
            EventCategory::Permission,
            EventCategory::Tool,
            EventCategory::Session,
            EventCategory::Error,
        ])
    }

    async fn on_event(&self, event: &ForgeEvent) {
        // 이벤트를 감사 로그로 변환
        let entry = match event.category {
            EventCategory::Permission => {
                let action = match event.event_type.as_str() {
                    "permission.requested" => AuditAction::PermissionRequested,
                    "permission.granted" => AuditAction::PermissionGranted,
                    "permission.denied" => AuditAction::PermissionDenied,
                    _ => return,
                };

                let actor = event
                    .data
                    .get("tool")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");

                AuditEntry::new(action, actor)
                    .with_data(event.data.clone())
                    .with_result(if action == AuditAction::PermissionDenied {
                        AuditResult::Denied
                    } else {
                        AuditResult::Success
                    })
            }
            EventCategory::Tool => {
                let (action, result) = match event.event_type.as_str() {
                    "tool.started" => (AuditAction::ToolStarted, AuditResult::Pending),
                    "tool.completed" => (AuditAction::ToolSucceeded, AuditResult::Success),
                    "tool.failed" => (AuditAction::ToolFailed, AuditResult::Failure),
                    _ => return,
                };

                let actor = event
                    .data
                    .get("tool")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");

                let duration = event.data.get("duration_ms").and_then(|v| v.as_u64());

                let mut entry = AuditEntry::new(action, actor)
                    .with_result(result)
                    .with_data(event.data.clone());

                if let Some(d) = duration {
                    entry = entry.with_duration(d);
                }

                entry
            }
            EventCategory::Session => {
                let action = match event.event_type.as_str() {
                    "session.started" => AuditAction::SessionStarted,
                    "session.ended" => AuditAction::SessionEnded,
                    _ => return,
                };

                AuditEntry::new(action, "session")
                    .with_result(AuditResult::Success)
                    .with_data(event.data.clone())
            }
            EventCategory::Error => {
                let actor = event.source.clone();
                let error_msg = event
                    .data
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown error");

                AuditEntry::new(AuditAction::ErrorOccurred, actor)
                    .with_result(AuditResult::Failure)
                    .with_error(error_msg)
                    .with_data(event.data.clone())
            }
            _ => return,
        };

        // 세션 ID 추가
        let entry = if let Some(ref sid) = event.session_id {
            entry.with_session(sid)
        } else {
            entry
        };

        // 로그 기록
        if let Err(e) = self.logger.log(entry).await {
            error!(error = %e, "Failed to log audit entry from event");
        }
    }
}

// ============================================================================
// 헬퍼 함수
// ============================================================================

fn parse_action(s: &str) -> AuditAction {
    match s {
        "permission_requested" => AuditAction::PermissionRequested,
        "permission_granted" => AuditAction::PermissionGranted,
        "permission_denied" => AuditAction::PermissionDenied,
        "tool_started" => AuditAction::ToolStarted,
        "tool_succeeded" => AuditAction::ToolSucceeded,
        "tool_failed" => AuditAction::ToolFailed,
        "file_read" => AuditAction::FileRead,
        "file_write" => AuditAction::FileWrite,
        "file_delete" => AuditAction::FileDelete,
        "command_executed" => AuditAction::CommandExecuted,
        "command_blocked" => AuditAction::CommandBlocked,
        "session_started" => AuditAction::SessionStarted,
        "session_ended" => AuditAction::SessionEnded,
        "config_changed" => AuditAction::ConfigChanged,
        "error_occurred" => AuditAction::ErrorOccurred,
        _ => AuditAction::Custom,
    }
}

fn parse_result(s: &str) -> AuditResult {
    match s {
        "success" => AuditResult::Success,
        "failure" => AuditResult::Failure,
        "denied" => AuditResult::Denied,
        "timeout" => AuditResult::Timeout,
        "cancelled" => AuditResult::Cancelled,
        _ => AuditResult::Pending,
    }
}

// ============================================================================
// 테스트
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_audit_logger_basic() {
        let logger = AuditLogger::in_memory().unwrap();

        let entry = AuditEntry::new(AuditAction::ToolSucceeded, "read")
            .with_result(AuditResult::Success)
            .with_target("/test/file.txt")
            .with_duration(100);

        let id = logger.log(entry).await.unwrap();

        let retrieved = logger.get(&id).await.unwrap();
        assert!(retrieved.is_some());

        let entry = retrieved.unwrap();
        assert_eq!(entry.actor, "read");
        assert_eq!(entry.action, AuditAction::ToolSucceeded);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_audit_query() {
        let logger = AuditLogger::in_memory().unwrap();

        // 여러 엔트리 추가
        for i in 0..5 {
            let entry = AuditEntry::new(AuditAction::ToolSucceeded, format!("tool-{}", i))
                .with_result(AuditResult::Success);
            logger.log(entry).await.unwrap();
        }

        // 쿼리
        let entries = logger.recent(3).await.unwrap();
        assert_eq!(entries.len(), 3);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_audit_statistics() {
        let logger = AuditLogger::in_memory().unwrap();

        logger
            .log(
                AuditEntry::new(AuditAction::ToolSucceeded, "read")
                    .with_result(AuditResult::Success),
            )
            .await
            .unwrap();
        logger
            .log(
                AuditEntry::new(AuditAction::ToolFailed, "write").with_result(AuditResult::Failure),
            )
            .await
            .unwrap();

        let stats = logger.statistics().await.unwrap();
        assert_eq!(stats.total_entries, 2);
    }
}
