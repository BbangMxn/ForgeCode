//! Session Manager - 세션 저장/로드 기능
//!
//! OpenCode/Claude Code 스타일의 세션 관리:
//! - 대화 히스토리 영구 저장
//! - `--continue` 플래그로 이전 세션 이어가기
//! - `--session <name>` 으로 특정 세션 로드
//! - 세션 목록 조회

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// 저장된 메시지
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedMessage {
    pub role: String, // "user", "assistant", "system"
    pub content: String,
    pub timestamp: DateTime<Local>,
    #[serde(default)]
    pub tool_calls: Vec<SavedToolCall>,
}

/// 저장된 도구 호출
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedToolCall {
    pub name: String,
    pub success: bool,
    pub duration_ms: u64,
    #[serde(default)]
    pub output_preview: String,
}

/// 세션 메타데이터
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub id: String,
    pub name: Option<String>,
    pub created_at: DateTime<Local>,
    pub updated_at: DateTime<Local>,
    pub message_count: usize,
    pub provider: String,
    pub model: String,
    pub working_dir: String,
}

/// 세션 데이터
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionData {
    pub metadata: SessionMetadata,
    pub messages: Vec<SavedMessage>,
}

/// 세션 매니저
pub struct SessionManager {
    /// 세션 저장 디렉토리
    sessions_dir: PathBuf,
    /// 현재 세션 ID
    current_session_id: Option<String>,
}

impl SessionManager {
    /// 새 세션 매니저 생성
    pub fn new() -> Self {
        let sessions_dir = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".forgecode")
            .join("sessions");

        // 디렉토리 생성
        let _ = fs::create_dir_all(&sessions_dir);

        Self {
            sessions_dir,
            current_session_id: None,
        }
    }

    /// 커스텀 경로로 생성
    pub fn with_path(path: PathBuf) -> Self {
        let _ = fs::create_dir_all(&path);
        Self {
            sessions_dir: path,
            current_session_id: None,
        }
    }

    /// 세션 디렉토리 경로
    pub fn sessions_dir(&self) -> &PathBuf {
        &self.sessions_dir
    }

    /// 세션 파일 경로
    fn session_path(&self, session_id: &str) -> PathBuf {
        self.sessions_dir.join(format!("{}.json", session_id))
    }

    /// 새 세션 생성
    pub fn create_session(&mut self, provider: &str, model: &str, working_dir: &str) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Local::now();

        let metadata = SessionMetadata {
            id: id.clone(),
            name: None,
            created_at: now,
            updated_at: now,
            message_count: 0,
            provider: provider.to_string(),
            model: model.to_string(),
            working_dir: working_dir.to_string(),
        };

        let session = SessionData {
            metadata,
            messages: Vec::new(),
        };

        // 저장
        let path = self.session_path(&id);
        if let Ok(json) = serde_json::to_string_pretty(&session) {
            let _ = fs::write(&path, json);
        }

        self.current_session_id = Some(id.clone());
        id
    }

    /// 현재 세션 ID
    pub fn current_session(&self) -> Option<&str> {
        self.current_session_id.as_deref()
    }

    /// 세션 로드
    pub fn load_session(&mut self, session_id: &str) -> Option<SessionData> {
        let path = self.session_path(session_id);
        let content = fs::read_to_string(&path).ok()?;
        let session: SessionData = serde_json::from_str(&content).ok()?;
        self.current_session_id = Some(session_id.to_string());
        Some(session)
    }

    /// 이름으로 세션 찾기
    pub fn find_session_by_name(&self, name: &str) -> Option<SessionData> {
        for entry in fs::read_dir(&self.sessions_dir).ok()? {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false) {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(session) = serde_json::from_str::<SessionData>(&content) {
                        if session.metadata.name.as_deref() == Some(name) {
                            return Some(session);
                        }
                    }
                }
            }
        }
        None
    }

    /// 가장 최근 세션 로드
    pub fn load_latest_session(&mut self) -> Option<SessionData> {
        let sessions = self.list_sessions();
        if let Some(latest) = sessions.first() {
            return self.load_session(&latest.id);
        }
        None
    }

    /// 세션에 메시지 추가
    pub fn add_message(&self, session_id: &str, message: SavedMessage) {
        let path = self.session_path(session_id);
        
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(mut session) = serde_json::from_str::<SessionData>(&content) {
                session.messages.push(message);
                session.metadata.message_count = session.messages.len();
                session.metadata.updated_at = Local::now();

                if let Ok(json) = serde_json::to_string_pretty(&session) {
                    let _ = fs::write(&path, json);
                }
            }
        }
    }

    /// 세션 저장
    pub fn save_session(&self, session: &SessionData) {
        let path = self.session_path(&session.metadata.id);
        if let Ok(json) = serde_json::to_string_pretty(session) {
            let _ = fs::write(&path, json);
        }
    }

    /// 세션 이름 설정
    pub fn rename_session(&self, session_id: &str, name: &str) -> bool {
        let path = self.session_path(session_id);
        
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(mut session) = serde_json::from_str::<SessionData>(&content) {
                session.metadata.name = Some(name.to_string());
                session.metadata.updated_at = Local::now();

                if let Ok(json) = serde_json::to_string_pretty(&session) {
                    return fs::write(&path, json).is_ok();
                }
            }
        }
        false
    }

    /// 세션 삭제
    pub fn delete_session(&self, session_id: &str) -> bool {
        let path = self.session_path(session_id);
        fs::remove_file(path).is_ok()
    }

    /// 모든 세션 목록 (최신순)
    pub fn list_sessions(&self) -> Vec<SessionMetadata> {
        let mut sessions = Vec::new();

        if let Ok(entries) = fs::read_dir(&self.sessions_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "json").unwrap_or(false) {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(session) = serde_json::from_str::<SessionData>(&content) {
                            sessions.push(session.metadata);
                        }
                    }
                }
            }
        }

        // 최신순 정렬
        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        sessions
    }

    /// 세션 검색 (메시지 내용으로)
    pub fn search_sessions(&self, query: &str) -> Vec<SessionMetadata> {
        let query_lower = query.to_lowercase();
        let mut results = Vec::new();

        if let Ok(entries) = fs::read_dir(&self.sessions_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "json").unwrap_or(false) {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(session) = serde_json::from_str::<SessionData>(&content) {
                            // 이름 또는 메시지 내용에서 검색
                            let matches = session
                                .metadata
                                .name
                                .as_ref()
                                .map(|n| n.to_lowercase().contains(&query_lower))
                                .unwrap_or(false)
                                || session.messages.iter().any(|m| {
                                    m.content.to_lowercase().contains(&query_lower)
                                });

                            if matches {
                                results.push(session.metadata);
                            }
                        }
                    }
                }
            }
        }

        // 최신순 정렬
        results.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        results
    }

    /// 오래된 세션 정리 (days일 이상 된 세션 삭제)
    pub fn cleanup_old_sessions(&self, days: i64) -> usize {
        let cutoff = Local::now() - chrono::Duration::days(days);
        let mut deleted = 0;

        if let Ok(entries) = fs::read_dir(&self.sessions_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "json").unwrap_or(false) {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(session) = serde_json::from_str::<SessionData>(&content) {
                            if session.metadata.updated_at < cutoff {
                                if fs::remove_file(&path).is_ok() {
                                    deleted += 1;
                                }
                            }
                        }
                    }
                }
            }
        }

        deleted
    }

    /// 세션 내보내기 (마크다운)
    pub fn export_session_markdown(&self, session_id: &str) -> Option<String> {
        let session = self.load_session_without_set(session_id)?;
        let mut md = String::new();

        // 헤더
        md.push_str(&format!("# Session: {}\n\n", 
            session.metadata.name.as_deref().unwrap_or(&session.metadata.id[..8])));
        md.push_str(&format!("- **Created:** {}\n", session.metadata.created_at.format("%Y-%m-%d %H:%M")));
        md.push_str(&format!("- **Model:** {} ({})\n", session.metadata.model, session.metadata.provider));
        md.push_str(&format!("- **Working Dir:** {}\n\n", session.metadata.working_dir));
        md.push_str("---\n\n");

        // 메시지
        for msg in &session.messages {
            let role_icon = match msg.role.as_str() {
                "user" => "👤",
                "assistant" => "🤖",
                _ => "ℹ️",
            };
            
            md.push_str(&format!("## {} {} ({})\n\n", 
                role_icon, 
                msg.role.to_uppercase(),
                msg.timestamp.format("%H:%M")));
            
            md.push_str(&msg.content);
            md.push_str("\n\n");

            // 도구 호출
            for tool in &msg.tool_calls {
                let status = if tool.success { "✓" } else { "✗" };
                md.push_str(&format!("> 🔧 `{}` {} ({:.1}s)\n", 
                    tool.name, status, tool.duration_ms as f64 / 1000.0));
            }
            
            if !msg.tool_calls.is_empty() {
                md.push_str("\n");
            }

            md.push_str("---\n\n");
        }

        Some(md)
    }

    /// 세션 로드 (current_session 설정 없이)
    fn load_session_without_set(&self, session_id: &str) -> Option<SessionData> {
        let path = self.session_path(session_id);
        let content = fs::read_to_string(&path).ok()?;
        serde_json::from_str(&content).ok()
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;

    fn temp_session_manager() -> SessionManager {
        let path = temp_dir().join(format!("forgecode_test_{}", uuid::Uuid::new_v4()));
        SessionManager::with_path(path)
    }

    #[test]
    fn test_create_and_load_session() {
        let mut mgr = temp_session_manager();
        
        let id = mgr.create_session("ollama", "qwen3:8b", "/home/user");
        assert!(!id.is_empty());

        let session = mgr.load_session(&id).unwrap();
        assert_eq!(session.metadata.provider, "ollama");
        assert_eq!(session.metadata.model, "qwen3:8b");
    }

    #[test]
    fn test_add_message() {
        let mut mgr = temp_session_manager();
        let id = mgr.create_session("ollama", "qwen3:8b", "/home/user");

        let msg = SavedMessage {
            role: "user".to_string(),
            content: "Hello!".to_string(),
            timestamp: Local::now(),
            tool_calls: vec![],
        };

        mgr.add_message(&id, msg);

        let session = mgr.load_session(&id).unwrap();
        assert_eq!(session.messages.len(), 1);
        assert_eq!(session.messages[0].content, "Hello!");
    }

    #[test]
    fn test_list_sessions() {
        let mut mgr = temp_session_manager();
        mgr.create_session("ollama", "model1", "/");
        mgr.create_session("openai", "model2", "/");

        let sessions = mgr.list_sessions();
        assert_eq!(sessions.len(), 2);
    }

    #[test]
    fn test_rename_session() {
        let mut mgr = temp_session_manager();
        let id = mgr.create_session("ollama", "qwen3:8b", "/");

        assert!(mgr.rename_session(&id, "My Session"));

        let session = mgr.load_session(&id).unwrap();
        assert_eq!(session.metadata.name, Some("My Session".to_string()));
    }
}
