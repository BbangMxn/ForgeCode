//! TODO-Based Planning System
//!
//! Claude Code 스타일의 TODO 관리 시스템.
//! 도구 사용 후 현재 상태를 "리마인더"로 주입하여
//! 모델이 목표를 잃지 않도록 합니다.
//!
//! ## 핵심 원칙
//!
//! 1. **Reminder Injection**: 도구 사용 후 TODO 상태를 시스템 메시지로 주입
//! 2. **Incremental Work**: 한 번에 하나의 기능만 작업
//! 3. **Clean State**: 세션 종료 시 깔끔한 상태 유지
//!
//! ## 사용 예시
//!
//! ```ignore
//! let mut todo_manager = TodoManager::new();
//! 
//! // TODO 추가
//! todo_manager.add("Implement login", Priority::High);
//! todo_manager.add("Add tests", Priority::Medium);
//! 
//! // 현재 상태를 리마인더로 변환
//! let reminder = todo_manager.as_reminder();
//! // → "[System] Current TODO state:\n- [ ] Implement login (HIGH)\n..."
//! 
//! // 완료 표시
//! todo_manager.complete("todo_1");
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;
use tracing::{debug, info};

/// TODO 항목의 우선순위
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Priority {
    /// 긴급 - 즉시 처리
    Critical = 0,
    /// 높음 - 현재 세션에서 처리
    High = 1,
    /// 중간 - 가능하면 처리
    Medium = 2,
    /// 낮음 - 나중에 처리
    Low = 3,
}

impl std::fmt::Display for Priority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Priority::Critical => write!(f, "🔴 CRITICAL"),
            Priority::High => write!(f, "🟠 HIGH"),
            Priority::Medium => write!(f, "🟡 MEDIUM"),
            Priority::Low => write!(f, "🟢 LOW"),
        }
    }
}

/// TODO 항목의 상태
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TodoStatus {
    /// 대기 중
    Pending,
    /// 진행 중
    InProgress,
    /// 완료됨
    Completed,
    /// 차단됨 (의존성 대기)
    Blocked,
    /// 건너뜀
    Skipped,
}

impl std::fmt::Display for TodoStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TodoStatus::Pending => write!(f, "[ ]"),
            TodoStatus::InProgress => write!(f, "[→]"),
            TodoStatus::Completed => write!(f, "[✓]"),
            TodoStatus::Blocked => write!(f, "[⊘]"),
            TodoStatus::Skipped => write!(f, "[~]"),
        }
    }
}

/// 단일 TODO 항목
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    /// 고유 ID
    pub id: String,
    
    /// 내용
    pub content: String,
    
    /// 상태
    pub status: TodoStatus,
    
    /// 우선순위
    pub priority: Priority,
    
    /// 의존하는 다른 TODO ID들
    #[serde(default)]
    pub dependencies: Vec<String>,
    
    /// 메모/노트
    #[serde(default)]
    pub notes: Option<String>,
    
    /// 생성 시각
    pub created_at: DateTime<Utc>,
    
    /// 완료 시각
    pub completed_at: Option<DateTime<Utc>>,
    
    /// 예상 소요 시간 (분)
    #[serde(default)]
    pub estimated_minutes: Option<u32>,
}

impl TodoItem {
    /// 새 TODO 생성
    pub fn new(content: impl Into<String>, priority: Priority) -> Self {
        let content = content.into();
        let id = format!("todo_{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap_or("x"));
        
        Self {
            id,
            content,
            status: TodoStatus::Pending,
            priority,
            dependencies: Vec::new(),
            notes: None,
            created_at: Utc::now(),
            completed_at: None,
            estimated_minutes: None,
        }
    }
    
    /// ID로 생성
    pub fn with_id(id: impl Into<String>, content: impl Into<String>, priority: Priority) -> Self {
        Self {
            id: id.into(),
            content: content.into(),
            status: TodoStatus::Pending,
            priority,
            dependencies: Vec::new(),
            notes: None,
            created_at: Utc::now(),
            completed_at: None,
            estimated_minutes: None,
        }
    }
    
    /// 의존성 추가
    pub fn with_dependency(mut self, dep_id: impl Into<String>) -> Self {
        self.dependencies.push(dep_id.into());
        self
    }
    
    /// 노트 추가
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes = Some(note.into());
        self
    }
    
    /// 예상 시간 설정
    pub fn with_estimate(mut self, minutes: u32) -> Self {
        self.estimated_minutes = Some(minutes);
        self
    }
    
    /// 완료 표시
    pub fn complete(&mut self) {
        self.status = TodoStatus::Completed;
        self.completed_at = Some(Utc::now());
    }
    
    /// 진행 중 표시
    pub fn start(&mut self) {
        self.status = TodoStatus::InProgress;
    }
    
    /// 차단 표시
    pub fn block(&mut self) {
        self.status = TodoStatus::Blocked;
    }
    
    /// 활성 상태인지 확인
    pub fn is_active(&self) -> bool {
        matches!(self.status, TodoStatus::Pending | TodoStatus::InProgress)
    }
    
    /// 리마인더 문자열 생성
    pub fn to_reminder_line(&self) -> String {
        let deps = if self.dependencies.is_empty() {
            String::new()
        } else {
            format!(" (depends on: {})", self.dependencies.join(", "))
        };
        
        let notes = self.notes.as_ref()
            .map(|n| format!(" - {}", n))
            .unwrap_or_default();
        
        format!("{} {} [{}]{}{}",
            self.status,
            self.content,
            self.priority,
            deps,
            notes
        )
    }
}

/// TODO 관리자
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoManager {
    /// TODO 항목들
    items: Vec<TodoItem>,
    
    /// 현재 작업 중인 항목 ID
    current: Option<String>,
    
    /// 파일 경로 (옵션)
    #[serde(skip)]
    file_path: Option<PathBuf>,
}

impl Default for TodoManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TodoManager {
    /// 새 관리자 생성
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            current: None,
            file_path: None,
        }
    }
    
    /// 파일에서 로드
    pub async fn load(path: impl AsRef<Path>) -> Result<Self, std::io::Error> {
        let path = path.as_ref();
        let content = fs::read_to_string(path).await?;
        let mut manager: TodoManager = serde_json::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        manager.file_path = Some(path.to_path_buf());
        Ok(manager)
    }
    
    /// 파일에 저장
    pub async fn save(&self) -> Result<(), std::io::Error> {
        if let Some(ref path) = self.file_path {
            let content = serde_json::to_string_pretty(self)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            fs::write(path, content).await?;
        }
        Ok(())
    }
    
    /// 파일 경로 설정
    pub fn with_file(mut self, path: impl AsRef<Path>) -> Self {
        self.file_path = Some(path.as_ref().to_path_buf());
        self
    }
    
    /// TODO 추가
    pub fn add(&mut self, content: impl Into<String>, priority: Priority) -> String {
        let item = TodoItem::new(content, priority);
        let id = item.id.clone();
        self.items.push(item);
        debug!("Added TODO: {}", id);
        id
    }
    
    /// ID로 TODO 추가
    pub fn add_with_id(&mut self, id: impl Into<String>, content: impl Into<String>, priority: Priority) {
        let item = TodoItem::with_id(id, content, priority);
        self.items.push(item);
    }
    
    /// TODO 항목 추가
    pub fn add_item(&mut self, item: TodoItem) {
        self.items.push(item);
    }
    
    /// TODO 완료
    pub fn complete(&mut self, id: &str) -> bool {
        if let Some(item) = self.items.iter_mut().find(|i| i.id == id) {
            item.complete();
            info!("Completed TODO: {}", id);
            
            // 현재 작업 중이었다면 해제
            if self.current.as_ref() == Some(&id.to_string()) {
                self.current = None;
            }
            
            true
        } else {
            false
        }
    }
    
    /// TODO 시작
    pub fn start(&mut self, id: &str) -> bool {
        // 기존 진행 중인 항목을 대기로 변경
        if let Some(ref current_id) = self.current {
            if let Some(item) = self.items.iter_mut().find(|i| i.id == *current_id) {
                if item.status == TodoStatus::InProgress {
                    item.status = TodoStatus::Pending;
                }
            }
        }
        
        // 새 항목 시작
        if let Some(item) = self.items.iter_mut().find(|i| i.id == id) {
            item.start();
            self.current = Some(id.to_string());
            info!("Started TODO: {}", id);
            true
        } else {
            false
        }
    }
    
    /// 다음 우선순위 TODO 가져오기
    pub fn next(&self) -> Option<&TodoItem> {
        self.items.iter()
            .filter(|i| i.status == TodoStatus::Pending)
            .filter(|i| self.dependencies_met(i))
            .min_by_key(|i| i.priority)
    }
    
    /// 의존성이 충족되었는지 확인
    fn dependencies_met(&self, item: &TodoItem) -> bool {
        item.dependencies.iter().all(|dep_id| {
            self.items.iter()
                .find(|i| i.id == *dep_id)
                .map(|i| i.status == TodoStatus::Completed)
                .unwrap_or(true) // 없는 의존성은 충족된 것으로 간주
        })
    }
    
    /// 현재 작업 중인 항목 가져오기
    pub fn current(&self) -> Option<&TodoItem> {
        self.current.as_ref()
            .and_then(|id| self.items.iter().find(|i| i.id == *id))
    }
    
    /// 모든 항목 가져오기
    pub fn all(&self) -> &[TodoItem] {
        &self.items
    }
    
    /// 활성 항목 가져오기
    pub fn active(&self) -> Vec<&TodoItem> {
        self.items.iter().filter(|i| i.is_active()).collect()
    }
    
    /// 완료된 항목 가져오기
    pub fn completed(&self) -> Vec<&TodoItem> {
        self.items.iter()
            .filter(|i| i.status == TodoStatus::Completed)
            .collect()
    }
    
    /// 진행률 (0.0 ~ 1.0)
    pub fn progress(&self) -> f64 {
        if self.items.is_empty() {
            return 1.0;
        }
        let completed = self.items.iter()
            .filter(|i| i.status == TodoStatus::Completed)
            .count();
        completed as f64 / self.items.len() as f64
    }
    
    /// 통계
    pub fn stats(&self) -> TodoStats {
        TodoStats {
            total: self.items.len(),
            pending: self.items.iter().filter(|i| i.status == TodoStatus::Pending).count(),
            in_progress: self.items.iter().filter(|i| i.status == TodoStatus::InProgress).count(),
            completed: self.items.iter().filter(|i| i.status == TodoStatus::Completed).count(),
            blocked: self.items.iter().filter(|i| i.status == TodoStatus::Blocked).count(),
            skipped: self.items.iter().filter(|i| i.status == TodoStatus::Skipped).count(),
        }
    }
    
    // ========================================================================
    // Reminder Injection (핵심!)
    // ========================================================================
    
    /// 리마인더 문자열 생성 (도구 사용 후 주입용)
    ///
    /// Claude Code 스타일: 도구 사용 후 현재 TODO 상태를 시스템 메시지로 주입
    pub fn as_reminder(&self) -> String {
        let mut lines = Vec::new();
        
        lines.push("[System] Current TODO state:".to_string());
        lines.push(String::new());
        
        // 현재 작업 중인 항목
        if let Some(current) = self.current() {
            lines.push(format!("📌 CURRENT: {}", current.to_reminder_line()));
            lines.push(String::new());
        }
        
        // 우선순위별로 정렬된 활성 항목
        let mut active: Vec<_> = self.active().into_iter()
            .filter(|i| Some(&i.id) != self.current.as_ref())
            .collect();
        active.sort_by_key(|i| i.priority);
        
        if !active.is_empty() {
            lines.push("📋 REMAINING:".to_string());
            for item in active.iter().take(5) { // 최대 5개만 표시
                lines.push(format!("  {}", item.to_reminder_line()));
            }
            if active.len() > 5 {
                lines.push(format!("  ... and {} more", active.len() - 5));
            }
            lines.push(String::new());
        }
        
        // 진행률
        let stats = self.stats();
        lines.push(format!("📊 Progress: {}/{} ({:.0}%)",
            stats.completed, stats.total, self.progress() * 100.0));
        
        lines.join("\n")
    }
    
    /// 간단한 리마인더 (컨텍스트 절약용)
    pub fn as_brief_reminder(&self) -> String {
        let stats = self.stats();
        let current = self.current()
            .map(|c| format!(" | Current: {}", c.content))
            .unwrap_or_default();
        
        format!("[TODO: {}/{} done{}]", stats.completed, stats.total, current)
    }
}

/// TODO 통계
#[derive(Debug, Clone)]
pub struct TodoStats {
    pub total: usize,
    pub pending: usize,
    pub in_progress: usize,
    pub completed: usize,
    pub blocked: usize,
    pub skipped: usize,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_todo_creation() {
        let mut manager = TodoManager::new();
        
        let id1 = manager.add("Implement login", Priority::High);
        let id2 = manager.add("Add tests", Priority::Medium);
        
        assert_eq!(manager.all().len(), 2);
        assert!(manager.next().is_some());
        assert_eq!(manager.next().unwrap().priority, Priority::High);
    }
    
    #[test]
    fn test_todo_completion() {
        let mut manager = TodoManager::new();
        
        let id = manager.add("Task 1", Priority::High);
        manager.complete(&id);
        
        assert_eq!(manager.stats().completed, 1);
        assert!(manager.next().is_none());
    }
    
    #[test]
    fn test_dependencies() {
        let mut manager = TodoManager::new();
        
        let id1 = manager.add("Task 1", Priority::High);
        
        let item2 = TodoItem::new("Task 2", Priority::High)
            .with_dependency(&id1);
        manager.add_item(item2);
        
        // Task 2는 Task 1에 의존하므로 next()는 Task 1을 반환해야 함
        let next = manager.next().unwrap();
        assert_eq!(next.id, id1);
        
        // Task 1 완료 후
        manager.complete(&id1);
        let next = manager.next().unwrap();
        assert_eq!(next.content, "Task 2");
    }
    
    #[test]
    fn test_reminder() {
        let mut manager = TodoManager::new();
        
        manager.add("Task 1", Priority::High);
        manager.add("Task 2", Priority::Medium);
        
        let reminder = manager.as_reminder();
        assert!(reminder.contains("TODO state"));
        assert!(reminder.contains("Task 1"));
    }
}
