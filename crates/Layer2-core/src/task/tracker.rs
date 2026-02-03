//! Task Tracker - Task 상태 추적
//!
//! 모든 Task의 상태와 이력을 관리

use super::TaskContainerId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, SystemTime};

/// Task 상태
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// 시작 중
    Starting,
    /// 실행 중
    Running,
    /// 정상 종료
    Stopped,
    /// 강제 종료
    Killed,
    /// 오류 발생
    Error,
}

/// Task 정보
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInfo {
    /// Task ID
    pub id: String,

    /// 실행한 명령어
    pub command: Option<String>,

    /// 상태
    pub status: TaskStatus,

    /// 시작 시간
    pub started_at: SystemTime,

    /// 종료 시간
    pub stopped_at: Option<SystemTime>,

    /// 에러 메시지
    pub error: Option<String>,
}

impl TaskInfo {
    pub fn new(id: &TaskContainerId, command: Option<&str>) -> Self {
        Self {
            id: id.to_string(),
            command: command.map(|s| s.to_string()),
            status: TaskStatus::Starting,
            started_at: SystemTime::now(),
            stopped_at: None,
            error: None,
        }
    }

    /// 실행 시간
    pub fn duration(&self) -> Duration {
        let end = self.stopped_at.unwrap_or_else(SystemTime::now);
        end.duration_since(self.started_at).unwrap_or_default()
    }

    /// 실행 중 여부
    pub fn is_running(&self) -> bool {
        matches!(self.status, TaskStatus::Starting | TaskStatus::Running)
    }
}

/// Task 추적기
pub struct TaskTracker {
    tasks: RwLock<HashMap<String, TaskInfo>>,
}

impl TaskTracker {
    pub fn new() -> Self {
        Self {
            tasks: RwLock::new(HashMap::new()),
        }
    }

    /// Task 등록
    pub fn register(&self, id: &TaskContainerId, command: Option<&str>) {
        let info = TaskInfo::new(id, command);
        self.tasks.write().unwrap().insert(id.to_string(), info);
    }

    /// 상태 업데이트: 실행 중
    pub fn mark_running(&self, id: &TaskContainerId) {
        if let Some(info) = self.tasks.write().unwrap().get_mut(&id.to_string()) {
            info.status = TaskStatus::Running;
        }
    }

    /// 상태 업데이트: 정상 종료
    pub fn mark_stopped(&self, id: &TaskContainerId) {
        if let Some(info) = self.tasks.write().unwrap().get_mut(&id.to_string()) {
            info.status = TaskStatus::Stopped;
            info.stopped_at = Some(SystemTime::now());
        }
    }

    /// 상태 업데이트: 강제 종료
    pub fn mark_killed(&self, id: &TaskContainerId) {
        if let Some(info) = self.tasks.write().unwrap().get_mut(&id.to_string()) {
            info.status = TaskStatus::Killed;
            info.stopped_at = Some(SystemTime::now());
        }
    }

    /// 상태 업데이트: 오류
    pub fn mark_error(&self, id: &TaskContainerId, error: &str) {
        if let Some(info) = self.tasks.write().unwrap().get_mut(&id.to_string()) {
            info.status = TaskStatus::Error;
            info.stopped_at = Some(SystemTime::now());
            info.error = Some(error.to_string());
        }
    }

    /// Task 정보 조회
    pub fn get(&self, id: &TaskContainerId) -> Option<TaskInfo> {
        self.tasks.read().unwrap().get(&id.to_string()).cloned()
    }

    /// 모든 Task 목록
    pub fn list_all(&self) -> Vec<TaskInfo> {
        self.tasks.read().unwrap().values().cloned().collect()
    }

    /// 실행 중인 Task 목록
    pub fn list_running(&self) -> Vec<TaskInfo> {
        self.tasks
            .read()
            .unwrap()
            .values()
            .filter(|t| t.is_running())
            .cloned()
            .collect()
    }

    /// Task 제거 (이력에서)
    pub fn remove(&self, id: &TaskContainerId) {
        self.tasks.write().unwrap().remove(&id.to_string());
    }

    /// 종료된 Task 정리
    pub fn cleanup_stopped(&self) {
        self.tasks
            .write()
            .unwrap()
            .retain(|_, t| t.is_running());
    }
}

impl Default for TaskTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_info() {
        let id = TaskContainerId::new();
        let info = TaskInfo::new(&id, Some("npm start"));

        assert!(info.is_running());
        assert_eq!(info.command, Some("npm start".to_string()));
    }

    #[test]
    fn test_tracker() {
        let tracker = TaskTracker::new();
        let id = TaskContainerId::new();

        tracker.register(&id, Some("test command"));
        assert!(tracker.get(&id).is_some());

        tracker.mark_running(&id);
        assert_eq!(tracker.get(&id).unwrap().status, TaskStatus::Running);

        tracker.mark_stopped(&id);
        assert!(!tracker.get(&id).unwrap().is_running());
    }
}
