//! SQLite Storage for runtime data
//!
//! 런타임 데이터 저장:
//! - Sessions: 대화 세션
//! - Messages: 메시지 기록
//! - Token Usage: 토큰 사용량 추적
//! - Tool Executions: 도구 실행 로그
//!
//! 설정 데이터는 JSON (storage/json/)에서 관리
//!
//! ## Migration System
//!
//! Database schema is versioned. Migrations run automatically on startup.
//! - Version 1: Initial schema (sessions, messages, token_usage, tool_executions)
//! - Version 2: Add context_tokens and thinking_tokens columns

use crate::{Error, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing::{debug, info, warn};

/// Current schema version
const CURRENT_SCHEMA_VERSION: i32 = 2;

/// Storage service for persisting runtime data
pub struct Storage {
    conn: Arc<Mutex<Connection>>,
}

impl Storage {
    /// Create a new storage instance
    pub fn new(data_dir: &PathBuf) -> Result<Self> {
        std::fs::create_dir_all(data_dir)
            .map_err(|e| Error::Storage(format!("Failed to create data directory: {}", e)))?;

        let db_path = data_dir.join("forgecode.db");
        let conn = Connection::open(&db_path)
            .map_err(|e| Error::Storage(format!("Failed to open database: {}", e)))?;

        // Enable WAL mode for better concurrent performance
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
            .map_err(|e| Error::Storage(format!("Failed to set pragmas: {}", e)))?;

        let storage = Self {
            conn: Arc::new(Mutex::new(conn)),
        };

        storage.initialize_schema()?;
        storage.run_migrations()?;

        Ok(storage)
    }

    /// Create an in-memory storage (for testing)
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()
            .map_err(|e| Error::Storage(format!("Failed to create in-memory database: {}", e)))?;

        let storage = Self {
            conn: Arc::new(Mutex::new(conn)),
        };

        storage.initialize_schema()?;
        storage.run_migrations()?;

        Ok(storage)
    }

    /// Get current schema version from database
    pub fn get_schema_version(&self) -> Result<i32> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| Error::Internal("Lock poisoned".to_string()))?;

        conn.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |row| row.get(0),
        )
        .map_err(|e| Error::Storage(format!("Failed to get schema version: {}", e)))
    }

    /// Initialize database schema (base tables)
    fn initialize_schema(&self) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| Error::Internal("Lock poisoned".to_string()))?;

        conn.execute_batch(
            r#"
            -- Schema version tracking
            CREATE TABLE IF NOT EXISTS schema_version (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            -- Sessions table
            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                title TEXT,
                working_directory TEXT,
                provider TEXT,
                model TEXT,
                total_input_tokens INTEGER DEFAULT 0,
                total_output_tokens INTEGER DEFAULT 0,
                total_cost_cents INTEGER DEFAULT 0,
                message_count INTEGER DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            -- Messages table
            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                role TEXT NOT NULL CHECK(role IN ('user', 'assistant', 'system', 'tool')),
                content TEXT NOT NULL,
                tool_calls TEXT,
                tool_results TEXT,
                input_tokens INTEGER DEFAULT 0,
                output_tokens INTEGER DEFAULT 0,
                finish_reason TEXT,
                created_at TEXT NOT NULL,
                FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_messages_session
                ON messages(session_id, created_at);
            CREATE INDEX IF NOT EXISTS idx_sessions_updated
                ON sessions(updated_at DESC);

            -- Token usage history
            CREATE TABLE IF NOT EXISTS token_usage (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT,
                provider TEXT NOT NULL,
                model TEXT NOT NULL,
                input_tokens INTEGER NOT NULL,
                output_tokens INTEGER NOT NULL,
                cache_read_tokens INTEGER DEFAULT 0,
                cache_write_tokens INTEGER DEFAULT 0,
                cost_cents INTEGER DEFAULT 0,
                recorded_at TEXT NOT NULL,
                FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE SET NULL
            );

            CREATE INDEX IF NOT EXISTS idx_token_usage_date
                ON token_usage(recorded_at);
            CREATE INDEX IF NOT EXISTS idx_token_usage_provider
                ON token_usage(provider, model);

            -- Tool execution history
            CREATE TABLE IF NOT EXISTS tool_executions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT,
                message_id TEXT,
                tool_name TEXT NOT NULL,
                tool_call_id TEXT NOT NULL,
                input_json TEXT NOT NULL,
                output_text TEXT,
                status TEXT NOT NULL CHECK(status IN ('pending', 'running', 'success', 'error', 'timeout', 'cancelled')),
                error_message TEXT,
                duration_ms INTEGER,
                created_at TEXT NOT NULL,
                completed_at TEXT,
                FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE,
                FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_tool_executions_session
                ON tool_executions(session_id, created_at);

            -- Insert initial schema version if not exists
            INSERT OR IGNORE INTO schema_version (version) VALUES (1);
            "#,
        )
        .map_err(|e| Error::Storage(format!("Failed to initialize schema: {}", e)))?;

        Ok(())
    }

    /// Run all pending migrations
    fn run_migrations(&self) -> Result<()> {
        let current_version = self.get_schema_version()?;

        if current_version >= CURRENT_SCHEMA_VERSION {
            debug!(
                "Database schema is up to date (version {})",
                current_version
            );
            return Ok(());
        }

        info!(
            "Running database migrations from version {} to {}",
            current_version, CURRENT_SCHEMA_VERSION
        );

        let conn = self
            .conn
            .lock()
            .map_err(|_| Error::Internal("Lock poisoned".to_string()))?;

        // Run migrations sequentially
        for version in (current_version + 1)..=CURRENT_SCHEMA_VERSION {
            match version {
                2 => self.migrate_v2(&conn)?,
                _ => {
                    warn!("Unknown migration version: {}", version);
                }
            }

            // Record migration
            conn.execute(
                "INSERT OR REPLACE INTO schema_version (version) VALUES (?1)",
                params![version],
            )
            .map_err(|e| Error::Storage(format!("Failed to record migration: {}", e)))?;

            info!("Applied migration to version {}", version);
        }

        Ok(())
    }

    /// Migration to version 2: Add context and thinking token tracking
    fn migrate_v2(&self, conn: &Connection) -> Result<()> {
        // Add context_tokens to sessions for tracking context window usage
        let _ = conn.execute(
            "ALTER TABLE sessions ADD COLUMN context_tokens INTEGER DEFAULT 0",
            [],
        );

        // Add thinking_tokens to token_usage for extended thinking tracking
        let _ = conn.execute(
            "ALTER TABLE token_usage ADD COLUMN thinking_tokens INTEGER DEFAULT 0",
            [],
        );

        // Add metadata JSON column to messages for extensibility
        let _ = conn.execute("ALTER TABLE messages ADD COLUMN metadata TEXT", []);

        Ok(())
    }

    // ========================================================================
    // Session Operations
    // ========================================================================

    /// Create a new session
    pub fn create_session(&self, session: &SessionRecord) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| Error::Internal("Lock poisoned".to_string()))?;
        let now = chrono::Utc::now().to_rfc3339();

        conn.execute(
            r#"
            INSERT INTO sessions (id, title, working_directory, provider, model, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
            "#,
            params![
                session.id,
                session.title,
                session.working_directory,
                session.provider,
                session.model,
                now,
            ],
        )
        .map_err(|e| Error::Storage(format!("Failed to create session: {}", e)))?;

        Ok(())
    }

    /// Update session metadata
    pub fn update_session(&self, session: &SessionRecord) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| Error::Internal("Lock poisoned".to_string()))?;
        let now = chrono::Utc::now().to_rfc3339();

        conn.execute(
            r#"
            UPDATE sessions SET
                title = COALESCE(?2, title),
                total_input_tokens = ?3,
                total_output_tokens = ?4,
                total_cost_cents = ?5,
                message_count = ?6,
                context_tokens = ?7,
                updated_at = ?8
            WHERE id = ?1
            "#,
            params![
                session.id,
                session.title,
                session.total_input_tokens,
                session.total_output_tokens,
                session.total_cost_cents,
                session.message_count,
                session.context_tokens,
                now,
            ],
        )
        .map_err(|e| Error::Storage(format!("Failed to update session: {}", e)))?;

        Ok(())
    }

    /// Get a session by ID
    pub fn get_session(&self, id: &str) -> Result<Option<SessionRecord>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| Error::Internal("Lock poisoned".to_string()))?;

        conn.query_row(
            r#"
            SELECT id, title, working_directory, provider, model,
                   total_input_tokens, total_output_tokens, total_cost_cents,
                   message_count, COALESCE(context_tokens, 0), created_at, updated_at
            FROM sessions WHERE id = ?1
            "#,
            params![id],
            |row| {
                Ok(SessionRecord {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    working_directory: row.get(2)?,
                    provider: row.get(3)?,
                    model: row.get(4)?,
                    total_input_tokens: row.get(5)?,
                    total_output_tokens: row.get(6)?,
                    total_cost_cents: row.get(7)?,
                    message_count: row.get(8)?,
                    context_tokens: row.get(9)?,
                    created_at: row.get(10)?,
                    updated_at: row.get(11)?,
                })
            },
        )
        .optional()
        .map_err(|e| Error::Storage(format!("Failed to get session: {}", e)))
    }

    /// Get all sessions, optionally limited
    pub fn get_sessions(&self, limit: Option<u32>) -> Result<Vec<SessionRecord>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| Error::Internal("Lock poisoned".to_string()))?;

        let query = match limit {
            Some(n) => format!(
                r#"
                SELECT id, title, working_directory, provider, model,
                       total_input_tokens, total_output_tokens, total_cost_cents,
                       message_count, COALESCE(context_tokens, 0), created_at, updated_at
                FROM sessions ORDER BY updated_at DESC LIMIT {}
                "#,
                n
            ),
            None => r#"
                SELECT id, title, working_directory, provider, model,
                       total_input_tokens, total_output_tokens, total_cost_cents,
                       message_count, COALESCE(context_tokens, 0), created_at, updated_at
                FROM sessions ORDER BY updated_at DESC
            "#
            .to_string(),
        };

        let mut stmt = conn
            .prepare(&query)
            .map_err(|e| Error::Storage(format!("Failed to prepare query: {}", e)))?;

        let sessions = stmt
            .query_map([], |row| {
                Ok(SessionRecord {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    working_directory: row.get(2)?,
                    provider: row.get(3)?,
                    model: row.get(4)?,
                    total_input_tokens: row.get(5)?,
                    total_output_tokens: row.get(6)?,
                    total_cost_cents: row.get(7)?,
                    message_count: row.get(8)?,
                    context_tokens: row.get(9)?,
                    created_at: row.get(10)?,
                    updated_at: row.get(11)?,
                })
            })
            .map_err(|e| Error::Storage(format!("Failed to query sessions: {}", e)))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(sessions)
    }

    /// Delete a session and all related data
    pub fn delete_session(&self, id: &str) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| Error::Internal("Lock poisoned".to_string()))?;

        conn.execute("DELETE FROM sessions WHERE id = ?1", params![id])
            .map_err(|e| Error::Storage(format!("Failed to delete session: {}", e)))?;

        Ok(())
    }

    // ========================================================================
    // Message Operations
    // ========================================================================

    /// Save a message
    pub fn save_message(&self, message: &MessageRecord) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| Error::Internal("Lock poisoned".to_string()))?;

        conn.execute(
            r#"
            INSERT INTO messages (id, session_id, role, content, tool_calls, tool_results,
                                  input_tokens, output_tokens, finish_reason, metadata, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            "#,
            params![
                message.id,
                message.session_id,
                message.role,
                message.content,
                message.tool_calls,
                message.tool_results,
                message.input_tokens,
                message.output_tokens,
                message.finish_reason,
                message.metadata,
                message.created_at,
            ],
        )
        .map_err(|e| Error::Storage(format!("Failed to save message: {}", e)))?;

        // Update session stats
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            r#"
            UPDATE sessions SET
                message_count = message_count + 1,
                total_input_tokens = total_input_tokens + ?2,
                total_output_tokens = total_output_tokens + ?3,
                updated_at = ?4
            WHERE id = ?1
            "#,
            params![
                message.session_id,
                message.input_tokens,
                message.output_tokens,
                now,
            ],
        )
        .ok();

        Ok(())
    }

    /// Get messages for a session
    pub fn get_messages(&self, session_id: &str) -> Result<Vec<MessageRecord>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| Error::Internal("Lock poisoned".to_string()))?;

        let mut stmt = conn
            .prepare(
                r#"
                SELECT id, session_id, role, content, tool_calls, tool_results,
                       input_tokens, output_tokens, finish_reason, metadata, created_at
                FROM messages
                WHERE session_id = ?1
                ORDER BY created_at ASC
                "#,
            )
            .map_err(|e| Error::Storage(format!("Failed to prepare query: {}", e)))?;

        let messages = stmt
            .query_map(params![session_id], |row| {
                Ok(MessageRecord {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    role: row.get(2)?,
                    content: row.get(3)?,
                    tool_calls: row.get(4)?,
                    tool_results: row.get(5)?,
                    input_tokens: row.get(6)?,
                    output_tokens: row.get(7)?,
                    finish_reason: row.get(8)?,
                    metadata: row.get(9)?,
                    created_at: row.get(10)?,
                })
            })
            .map_err(|e| Error::Storage(format!("Failed to query messages: {}", e)))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(messages)
    }

    /// Get recent messages (for context window)
    pub fn get_recent_messages(&self, session_id: &str, limit: u32) -> Result<Vec<MessageRecord>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| Error::Internal("Lock poisoned".to_string()))?;

        let mut stmt = conn
            .prepare(
                r#"
                SELECT id, session_id, role, content, tool_calls, tool_results,
                       input_tokens, output_tokens, finish_reason, metadata, created_at
                FROM messages
                WHERE session_id = ?1
                ORDER BY created_at DESC
                LIMIT ?2
                "#,
            )
            .map_err(|e| Error::Storage(format!("Failed to prepare query: {}", e)))?;

        let mut messages: Vec<MessageRecord> = stmt
            .query_map(params![session_id, limit], |row| {
                Ok(MessageRecord {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    role: row.get(2)?,
                    content: row.get(3)?,
                    tool_calls: row.get(4)?,
                    tool_results: row.get(5)?,
                    input_tokens: row.get(6)?,
                    output_tokens: row.get(7)?,
                    finish_reason: row.get(8)?,
                    metadata: row.get(9)?,
                    created_at: row.get(10)?,
                })
            })
            .map_err(|e| Error::Storage(format!("Failed to query messages: {}", e)))?
            .filter_map(|r| r.ok())
            .collect();

        messages.reverse();
        Ok(messages)
    }

    // ========================================================================
    // Token Usage Operations
    // ========================================================================

    /// Record token usage
    pub fn record_usage(&self, usage: &TokenUsageRecord) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| Error::Internal("Lock poisoned".to_string()))?;
        let now = chrono::Utc::now().to_rfc3339();

        conn.execute(
            r#"
            INSERT INTO token_usage (session_id, provider, model, input_tokens, output_tokens,
                                     cache_read_tokens, cache_write_tokens, thinking_tokens, cost_cents, recorded_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            "#,
            params![
                usage.session_id,
                usage.provider,
                usage.model,
                usage.input_tokens,
                usage.output_tokens,
                usage.cache_read_tokens,
                usage.cache_write_tokens,
                usage.thinking_tokens,
                usage.cost_cents,
                now,
            ],
        )
        .map_err(|e| Error::Storage(format!("Failed to record usage: {}", e)))?;

        Ok(())
    }

    /// Get usage summary
    pub fn get_usage_summary(&self, since: Option<&str>) -> Result<UsageSummary> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| Error::Internal("Lock poisoned".to_string()))?;

        let (query, use_param) = match since {
            Some(_) => (
                r#"
                SELECT COALESCE(SUM(input_tokens), 0), COALESCE(SUM(output_tokens), 0),
                       COALESCE(SUM(cost_cents), 0), COUNT(*)
                FROM token_usage WHERE recorded_at >= ?1
                "#,
                true,
            ),
            None => (
                r#"
                SELECT COALESCE(SUM(input_tokens), 0), COALESCE(SUM(output_tokens), 0),
                       COALESCE(SUM(cost_cents), 0), COUNT(*)
                FROM token_usage
                "#,
                false,
            ),
        };

        let summary = if use_param {
            conn.query_row(query, params![since.unwrap()], |row| {
                Ok(UsageSummary {
                    total_input_tokens: row.get(0)?,
                    total_output_tokens: row.get(1)?,
                    total_cost_cents: row.get(2)?,
                    request_count: row.get(3)?,
                })
            })
        } else {
            conn.query_row(query, [], |row| {
                Ok(UsageSummary {
                    total_input_tokens: row.get(0)?,
                    total_output_tokens: row.get(1)?,
                    total_cost_cents: row.get(2)?,
                    request_count: row.get(3)?,
                })
            })
        }
        .map_err(|e| Error::Storage(format!("Failed to get usage summary: {}", e)))?;

        Ok(summary)
    }

    // ========================================================================
    // Tool Execution Operations
    // ========================================================================

    /// Record tool execution start
    pub fn start_tool_execution(&self, execution: &ToolExecutionRecord) -> Result<i64> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| Error::Internal("Lock poisoned".to_string()))?;
        let now = chrono::Utc::now().to_rfc3339();

        conn.execute(
            r#"
            INSERT INTO tool_executions (session_id, message_id, tool_name, tool_call_id,
                                         input_json, status, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, 'running', ?6)
            "#,
            params![
                execution.session_id,
                execution.message_id,
                execution.tool_name,
                execution.tool_call_id,
                execution.input_json,
                now,
            ],
        )
        .map_err(|e| Error::Storage(format!("Failed to start tool execution: {}", e)))?;

        Ok(conn.last_insert_rowid())
    }

    /// Complete tool execution
    pub fn complete_tool_execution(
        &self,
        id: i64,
        output: Option<&str>,
        status: &str,
        error: Option<&str>,
        duration_ms: Option<i64>,
    ) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| Error::Internal("Lock poisoned".to_string()))?;
        let now = chrono::Utc::now().to_rfc3339();

        conn.execute(
            r#"
            UPDATE tool_executions SET
                output_text = ?2,
                status = ?3,
                error_message = ?4,
                duration_ms = ?5,
                completed_at = ?6
            WHERE id = ?1
            "#,
            params![id, output, status, error, duration_ms, now],
        )
        .map_err(|e| Error::Storage(format!("Failed to complete tool execution: {}", e)))?;

        Ok(())
    }

    /// Get tool executions for a session
    pub fn get_tool_executions(&self, session_id: &str) -> Result<Vec<ToolExecutionRecord>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| Error::Internal("Lock poisoned".to_string()))?;

        let mut stmt = conn
            .prepare(
                r#"
                SELECT id, session_id, message_id, tool_name, tool_call_id, input_json,
                       output_text, status, error_message, duration_ms, created_at, completed_at
                FROM tool_executions
                WHERE session_id = ?1
                ORDER BY created_at DESC
                "#,
            )
            .map_err(|e| Error::Storage(format!("Failed to prepare query: {}", e)))?;

        let executions = stmt
            .query_map(params![session_id], |row| {
                Ok(ToolExecutionRecord {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    message_id: row.get(2)?,
                    tool_name: row.get(3)?,
                    tool_call_id: row.get(4)?,
                    input_json: row.get(5)?,
                    output_text: row.get(6)?,
                    status: row.get(7)?,
                    error_message: row.get(8)?,
                    duration_ms: row.get(9)?,
                    created_at: row.get(10)?,
                    completed_at: row.get(11)?,
                })
            })
            .map_err(|e| Error::Storage(format!("Failed to query tool executions: {}", e)))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(executions)
    }

    /// Get recent tool executions across all sessions
    pub fn get_recent_tool_executions(&self, limit: u32) -> Result<Vec<ToolExecutionRecord>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| Error::Internal("Lock poisoned".to_string()))?;

        let mut stmt = conn
            .prepare(
                r#"
                SELECT id, session_id, message_id, tool_name, tool_call_id, input_json,
                       output_text, status, error_message, duration_ms, created_at, completed_at
                FROM tool_executions
                ORDER BY created_at DESC
                LIMIT ?1
                "#,
            )
            .map_err(|e| Error::Storage(format!("Failed to prepare query: {}", e)))?;

        let executions = stmt
            .query_map(params![limit], |row| {
                Ok(ToolExecutionRecord {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    message_id: row.get(2)?,
                    tool_name: row.get(3)?,
                    tool_call_id: row.get(4)?,
                    input_json: row.get(5)?,
                    output_text: row.get(6)?,
                    status: row.get(7)?,
                    error_message: row.get(8)?,
                    duration_ms: row.get(9)?,
                    created_at: row.get(10)?,
                    completed_at: row.get(11)?,
                })
            })
            .map_err(|e| Error::Storage(format!("Failed to query tool executions: {}", e)))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(executions)
    }

    /// Get failed tool executions (for error analysis)
    pub fn get_failed_tool_executions(&self, limit: u32) -> Result<Vec<ToolExecutionRecord>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| Error::Internal("Lock poisoned".to_string()))?;

        let mut stmt = conn
            .prepare(
                r#"
                SELECT id, session_id, message_id, tool_name, tool_call_id, input_json,
                       output_text, status, error_message, duration_ms, created_at, completed_at
                FROM tool_executions
                WHERE status IN ('error', 'timeout', 'cancelled')
                ORDER BY created_at DESC
                LIMIT ?1
                "#,
            )
            .map_err(|e| Error::Storage(format!("Failed to prepare query: {}", e)))?;

        let executions = stmt
            .query_map(params![limit], |row| {
                Ok(ToolExecutionRecord {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    message_id: row.get(2)?,
                    tool_name: row.get(3)?,
                    tool_call_id: row.get(4)?,
                    input_json: row.get(5)?,
                    output_text: row.get(6)?,
                    status: row.get(7)?,
                    error_message: row.get(8)?,
                    duration_ms: row.get(9)?,
                    created_at: row.get(10)?,
                    completed_at: row.get(11)?,
                })
            })
            .map_err(|e| Error::Storage(format!("Failed to query failed executions: {}", e)))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(executions)
    }

    /// Get token usage history
    pub fn get_token_usage_history(&self, limit: u32) -> Result<Vec<TokenUsageRecord>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| Error::Internal("Lock poisoned".to_string()))?;

        let mut stmt = conn
            .prepare(
                r#"
                SELECT session_id, provider, model, input_tokens, output_tokens,
                       cache_read_tokens, cache_write_tokens, COALESCE(thinking_tokens, 0), cost_cents
                FROM token_usage
                ORDER BY recorded_at DESC
                LIMIT ?1
                "#,
            )
            .map_err(|e| Error::Storage(format!("Failed to prepare query: {}", e)))?;

        let records = stmt
            .query_map(params![limit], |row| {
                Ok(TokenUsageRecord {
                    session_id: row.get(0)?,
                    provider: row.get(1)?,
                    model: row.get(2)?,
                    input_tokens: row.get(3)?,
                    output_tokens: row.get(4)?,
                    cache_read_tokens: row.get(5)?,
                    cache_write_tokens: row.get(6)?,
                    thinking_tokens: row.get(7)?,
                    cost_cents: row.get(8)?,
                })
            })
            .map_err(|e| Error::Storage(format!("Failed to query token usage: {}", e)))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(records)
    }

    /// Get usage by provider/model
    pub fn get_usage_by_provider(&self) -> Result<Vec<(String, String, UsageSummary)>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| Error::Internal("Lock poisoned".to_string()))?;

        let mut stmt = conn
            .prepare(
                r#"
                SELECT provider, model,
                       COALESCE(SUM(input_tokens), 0), COALESCE(SUM(output_tokens), 0),
                       COALESCE(SUM(cost_cents), 0), COUNT(*)
                FROM token_usage
                GROUP BY provider, model
                ORDER BY SUM(cost_cents) DESC
                "#,
            )
            .map_err(|e| Error::Storage(format!("Failed to prepare query: {}", e)))?;

        let results = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    UsageSummary {
                        total_input_tokens: row.get(2)?,
                        total_output_tokens: row.get(3)?,
                        total_cost_cents: row.get(4)?,
                        request_count: row.get(5)?,
                    },
                ))
            })
            .map_err(|e| Error::Storage(format!("Failed to query usage by provider: {}", e)))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(results)
    }
}

// ============================================================================
// Record Types
// ============================================================================

/// Session record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRecord {
    pub id: String,
    pub title: Option<String>,
    pub working_directory: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub total_cost_cents: i64,
    pub message_count: i32,
    /// Context window token usage (for tracking context limits)
    pub context_tokens: i64,
    pub created_at: String,
    pub updated_at: String,
}

impl Default for SessionRecord {
    fn default() -> Self {
        Self {
            id: String::new(),
            title: None,
            working_directory: None,
            provider: None,
            model: None,
            total_input_tokens: 0,
            total_output_tokens: 0,
            total_cost_cents: 0,
            message_count: 0,
            context_tokens: 0,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }
}

/// Message record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageRecord {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub tool_calls: Option<String>,
    pub tool_results: Option<String>,
    pub input_tokens: i32,
    pub output_tokens: i32,
    pub finish_reason: Option<String>,
    /// Additional metadata (JSON, for extensibility)
    pub metadata: Option<String>,
    pub created_at: String,
}

impl Default for MessageRecord {
    fn default() -> Self {
        Self {
            id: String::new(),
            session_id: String::new(),
            role: String::new(),
            content: String::new(),
            tool_calls: None,
            tool_results: None,
            input_tokens: 0,
            output_tokens: 0,
            finish_reason: None,
            metadata: None,
            created_at: String::new(),
        }
    }
}

/// Token usage record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsageRecord {
    pub session_id: Option<String>,
    pub provider: String,
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    /// Thinking/reasoning tokens (for extended thinking models)
    pub thinking_tokens: i64,
    pub cost_cents: i64,
}

/// Usage summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageSummary {
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub total_cost_cents: i64,
    pub request_count: i64,
}

/// Tool execution record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecutionRecord {
    pub id: Option<i64>,
    pub session_id: Option<String>,
    pub message_id: Option<String>,
    pub tool_name: String,
    pub tool_call_id: String,
    pub input_json: String,
    pub output_text: Option<String>,
    pub status: String,
    pub error_message: Option<String>,
    pub duration_ms: Option<i64>,
    pub created_at: Option<String>,
    pub completed_at: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_in_memory_storage() {
        let storage = Storage::in_memory().expect("Failed to create storage");

        let session = SessionRecord {
            id: "test-session".to_string(),
            title: Some("Test".to_string()),
            ..Default::default()
        };

        storage
            .create_session(&session)
            .expect("Failed to create session");

        let retrieved = storage
            .get_session("test-session")
            .expect("Failed to get session")
            .expect("Session not found");

        assert_eq!(retrieved.title, Some("Test".to_string()));
    }

    #[test]
    fn test_message_operations() {
        let storage = Storage::in_memory().expect("Failed to create storage");

        let session = SessionRecord {
            id: "test-session".to_string(),
            ..Default::default()
        };
        storage
            .create_session(&session)
            .expect("Failed to create session");

        let message = MessageRecord {
            id: "msg-1".to_string(),
            session_id: "test-session".to_string(),
            role: "user".to_string(),
            content: "Hello!".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            ..Default::default()
        };

        storage
            .save_message(&message)
            .expect("Failed to save message");

        let messages = storage
            .get_messages("test-session")
            .expect("Failed to get messages");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "Hello!");
    }
}
