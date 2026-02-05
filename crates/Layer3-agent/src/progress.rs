//! Progress Tracking System
//!
//! Claude Code 스타일의 진행 상황 추적 시스템.
//! 세션 간 상태를 유지하여 새 컨텍스트 윈도우에서도
//! 이전 작업을 이어갈 수 있습니다.
//!
//! ## 핵심 파일
//!
//! - `claude-progress.txt` - 사람이 읽기 쉬운 진행 로그
//! - `feature_list.json` - 기능 목록 및 상태
//! - `init.sh` - 환경 설정 스크립트
//!
//! ## Session Start Routine
//!
//! ```ignore
//! 1. pwd → 작업 디렉토리 확인
//! 2. git log → 최근 커밋 확인
//! 3. progress file → 진행 상황 확인
//! 4. feature list → 다음 작업 선택
//! 5. init.sh → 개발 서버 시작
//! 6. basic test → 기본 기능 확인
//! 7. work on ONE feature
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;
use tracing::{debug, info, warn};

/// 진행 상황 항목
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressEntry {
    /// 타임스탬프
    pub timestamp: DateTime<Utc>,
    
    /// 세션 ID
    pub session_id: String,
    
    /// 액션 타입
    pub action: ProgressAction,
    
    /// 설명
    pub description: String,
    
    /// 관련 파일들
    #[serde(default)]
    pub files: Vec<String>,
    
    /// Git 커밋 해시 (있는 경우)
    pub commit_hash: Option<String>,
}

/// 진행 액션 타입
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProgressAction {
    /// 세션 시작
    SessionStart,
    /// 세션 종료
    SessionEnd,
    /// 기능 시작
    FeatureStart { feature_id: String },
    /// 기능 완료
    FeatureComplete { feature_id: String },
    /// 파일 생성
    FileCreated,
    /// 파일 수정
    FileModified,
    /// 테스트 통과
    TestPassed,
    /// 테스트 실패
    TestFailed,
    /// 버그 수정
    BugFixed,
    /// Git 커밋
    GitCommit,
    /// 기타
    Note,
}

impl std::fmt::Display for ProgressAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProgressAction::SessionStart => write!(f, "🚀 SESSION START"),
            ProgressAction::SessionEnd => write!(f, "🏁 SESSION END"),
            ProgressAction::FeatureStart { feature_id } => write!(f, "📝 START: {}", feature_id),
            ProgressAction::FeatureComplete { feature_id } => write!(f, "✅ COMPLETE: {}", feature_id),
            ProgressAction::FileCreated => write!(f, "📄 FILE CREATED"),
            ProgressAction::FileModified => write!(f, "✏️ FILE MODIFIED"),
            ProgressAction::TestPassed => write!(f, "✓ TEST PASSED"),
            ProgressAction::TestFailed => write!(f, "✗ TEST FAILED"),
            ProgressAction::BugFixed => write!(f, "🐛 BUG FIXED"),
            ProgressAction::GitCommit => write!(f, "📦 GIT COMMIT"),
            ProgressAction::Note => write!(f, "📌 NOTE"),
        }
    }
}

/// 진행 상황 추적기
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressTracker {
    /// 프로젝트 이름
    pub project_name: String,
    
    /// 현재 세션 ID
    pub current_session: String,
    
    /// 진행 항목들
    pub entries: Vec<ProgressEntry>,
    
    /// 파일 경로
    #[serde(skip)]
    file_path: Option<PathBuf>,
}

impl ProgressTracker {
    /// 새 추적기 생성
    pub fn new(project_name: impl Into<String>) -> Self {
        Self {
            project_name: project_name.into(),
            current_session: uuid::Uuid::new_v4().to_string(),
            entries: Vec::new(),
            file_path: None,
        }
    }
    
    /// 파일에서 로드
    pub async fn load(path: impl AsRef<Path>) -> Result<Self, std::io::Error> {
        let path = path.as_ref();
        
        if !path.exists() {
            // 파일이 없으면 새로 생성
            let name = path.parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("project");
            
            let mut tracker = Self::new(name);
            tracker.file_path = Some(path.to_path_buf());
            return Ok(tracker);
        }
        
        let content = fs::read_to_string(path).await?;
        let mut tracker: ProgressTracker = serde_json::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        tracker.file_path = Some(path.to_path_buf());
        
        // 새 세션 시작
        tracker.current_session = uuid::Uuid::new_v4().to_string();
        
        Ok(tracker)
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
    
    /// Markdown 형식으로 저장 (사람이 읽기 쉬운 형태)
    pub async fn save_as_markdown(&self, path: impl AsRef<Path>) -> Result<(), std::io::Error> {
        let content = self.to_markdown();
        fs::write(path, content).await
    }
    
    /// 파일 경로 설정
    pub fn with_file(mut self, path: impl AsRef<Path>) -> Self {
        self.file_path = Some(path.as_ref().to_path_buf());
        self
    }
    
    // ========================================================================
    // 항목 추가
    // ========================================================================
    
    /// 항목 추가
    pub fn add(&mut self, action: ProgressAction, description: impl Into<String>) {
        let entry = ProgressEntry {
            timestamp: Utc::now(),
            session_id: self.current_session.clone(),
            action,
            description: description.into(),
            files: Vec::new(),
            commit_hash: None,
        };
        self.entries.push(entry);
    }
    
    /// 파일과 함께 항목 추가
    pub fn add_with_files(&mut self, action: ProgressAction, description: impl Into<String>, files: Vec<String>) {
        let entry = ProgressEntry {
            timestamp: Utc::now(),
            session_id: self.current_session.clone(),
            action,
            description: description.into(),
            files,
            commit_hash: None,
        };
        self.entries.push(entry);
    }
    
    /// Git 커밋 기록
    pub fn add_commit(&mut self, description: impl Into<String>, commit_hash: impl Into<String>) {
        let entry = ProgressEntry {
            timestamp: Utc::now(),
            session_id: self.current_session.clone(),
            action: ProgressAction::GitCommit,
            description: description.into(),
            files: Vec::new(),
            commit_hash: Some(commit_hash.into()),
        };
        self.entries.push(entry);
    }
    
    // ========================================================================
    // 조회
    // ========================================================================
    
    /// 최근 N개 항목
    pub fn recent(&self, n: usize) -> Vec<&ProgressEntry> {
        self.entries.iter().rev().take(n).collect()
    }
    
    /// 현재 세션 항목
    pub fn current_session_entries(&self) -> Vec<&ProgressEntry> {
        self.entries.iter()
            .filter(|e| e.session_id == self.current_session)
            .collect()
    }
    
    /// 완료된 기능들
    pub fn completed_features(&self) -> Vec<String> {
        self.entries.iter()
            .filter_map(|e| {
                if let ProgressAction::FeatureComplete { feature_id } = &e.action {
                    Some(feature_id.clone())
                } else {
                    None
                }
            })
            .collect()
    }
    
    // ========================================================================
    // 출력 형식
    // ========================================================================
    
    /// Markdown 형식으로 변환
    pub fn to_markdown(&self) -> String {
        let mut lines = Vec::new();
        
        lines.push(format!("# {} - Progress Log", self.project_name));
        lines.push(String::new());
        lines.push(format!("Last updated: {}", Utc::now().format("%Y-%m-%d %H:%M:%S UTC")));
        lines.push(String::new());
        
        // 요약
        let completed = self.completed_features();
        lines.push("## Summary".to_string());
        lines.push(format!("- Total sessions: {}", self.session_count()));
        lines.push(format!("- Features completed: {}", completed.len()));
        lines.push(format!("- Total entries: {}", self.entries.len()));
        lines.push(String::new());
        
        // 최근 활동
        lines.push("## Recent Activity".to_string());
        lines.push(String::new());
        
        for entry in self.recent(20) {
            let time = entry.timestamp.format("%Y-%m-%d %H:%M");
            let files = if entry.files.is_empty() {
                String::new()
            } else {
                format!(" ({})", entry.files.join(", "))
            };
            let commit = entry.commit_hash.as_ref()
                .map(|h| format!(" [{}]", &h[..7.min(h.len())]))
                .unwrap_or_default();
            
            lines.push(format!("- `{}` {} {}{}{}",
                time, entry.action, entry.description, files, commit));
        }
        
        lines.join("\n")
    }
    
    /// 세션 시작 시 읽을 컨텍스트 생성
    pub fn as_session_context(&self) -> String {
        let mut lines = Vec::new();
        
        lines.push("[Previous Session Context]".to_string());
        lines.push(String::new());
        
        // 완료된 기능
        let completed = self.completed_features();
        if !completed.is_empty() {
            lines.push("Completed features:".to_string());
            for f in completed.iter().take(10) {
                lines.push(format!("  ✓ {}", f));
            }
            if completed.len() > 10 {
                lines.push(format!("  ... and {} more", completed.len() - 10));
            }
            lines.push(String::new());
        }
        
        // 최근 활동
        lines.push("Recent activity:".to_string());
        for entry in self.recent(5) {
            lines.push(format!("  - {} {}", entry.action, entry.description));
        }
        
        lines.join("\n")
    }
    
    /// 세션 수
    fn session_count(&self) -> usize {
        let mut sessions: Vec<_> = self.entries.iter()
            .map(|e| e.session_id.as_str())
            .collect();
        sessions.sort();
        sessions.dedup();
        sessions.len()
    }
}

// ============================================================================
// Feature List
// ============================================================================

/// 기능 항목
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feature {
    /// 고유 ID
    pub id: String,
    
    /// 카테고리
    pub category: String,
    
    /// 설명
    pub description: String,
    
    /// 테스트 단계
    pub steps: Vec<String>,
    
    /// 통과 여부
    pub passes: bool,
    
    /// 우선순위 (낮을수록 높음)
    #[serde(default)]
    pub priority: u32,
    
    /// 메모
    #[serde(default)]
    pub notes: Option<String>,
}

/// 기능 목록 관리자
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureList {
    /// 기능 항목들
    pub features: Vec<Feature>,
    
    /// 파일 경로
    #[serde(skip)]
    file_path: Option<PathBuf>,
}

impl FeatureList {
    /// 새 목록 생성
    pub fn new() -> Self {
        Self {
            features: Vec::new(),
            file_path: None,
        }
    }
    
    /// 파일에서 로드
    pub async fn load(path: impl AsRef<Path>) -> Result<Self, std::io::Error> {
        let path = path.as_ref();
        
        if !path.exists() {
            let mut list = Self::new();
            list.file_path = Some(path.to_path_buf());
            return Ok(list);
        }
        
        let content = fs::read_to_string(path).await?;
        let mut list: FeatureList = serde_json::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        list.file_path = Some(path.to_path_buf());
        Ok(list)
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
    
    /// 기능 추가
    pub fn add(&mut self, category: impl Into<String>, description: impl Into<String>, steps: Vec<String>) -> String {
        let id = format!("feature_{}", self.features.len() + 1);
        let feature = Feature {
            id: id.clone(),
            category: category.into(),
            description: description.into(),
            steps,
            passes: false,
            priority: self.features.len() as u32,
            notes: None,
        };
        self.features.push(feature);
        id
    }
    
    /// 기능 통과 표시
    pub fn mark_passing(&mut self, id: &str) -> bool {
        if let Some(feature) = self.features.iter_mut().find(|f| f.id == id) {
            feature.passes = true;
            true
        } else {
            false
        }
    }
    
    /// 다음 미완료 기능
    pub fn next_incomplete(&self) -> Option<&Feature> {
        self.features.iter()
            .filter(|f| !f.passes)
            .min_by_key(|f| f.priority)
    }
    
    /// 통계
    pub fn stats(&self) -> (usize, usize) {
        let passing = self.features.iter().filter(|f| f.passes).count();
        (passing, self.features.len())
    }
    
    /// 진행률
    pub fn progress(&self) -> f64 {
        if self.features.is_empty() {
            return 1.0;
        }
        let (passing, total) = self.stats();
        passing as f64 / total as f64
    }
}

impl Default for FeatureList {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_progress_tracker() {
        let mut tracker = ProgressTracker::new("test-project");
        
        tracker.add(ProgressAction::SessionStart, "Starting work");
        tracker.add(ProgressAction::FeatureStart { feature_id: "login".to_string() }, "Working on login");
        tracker.add(ProgressAction::FeatureComplete { feature_id: "login".to_string() }, "Login done");
        
        assert_eq!(tracker.entries.len(), 3);
        assert_eq!(tracker.completed_features().len(), 1);
    }
    
    #[test]
    fn test_feature_list() {
        let mut list = FeatureList::new();
        
        list.add("auth", "User can login", vec!["Enter credentials".to_string()]);
        list.add("auth", "User can logout", vec!["Click logout".to_string()]);
        
        assert_eq!(list.features.len(), 2);
        assert!(list.next_incomplete().is_some());
        
        list.mark_passing(&list.features[0].id.clone());
        assert_eq!(list.stats(), (1, 2));
    }
}
