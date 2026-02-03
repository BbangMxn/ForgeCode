//! Task Executor - Task 실행 관리
//!
//! 컨테이너 생성, 실행, 종료를 관리

use super::{ContainerConfig, TaskContainer, TaskContainerId, TaskTracker};
use forge_foundation::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Task 실행자
pub struct TaskExecutor {
    /// 실행 중인 컨테이너들
    containers: Arc<RwLock<HashMap<TaskContainerId, TaskContainer>>>,

    /// Task 추적기
    tracker: TaskTracker,

    /// 최대 동시 실행 수
    max_concurrent: usize,
}

impl TaskExecutor {
    pub fn new() -> Self {
        Self {
            containers: Arc::new(RwLock::new(HashMap::new())),
            tracker: TaskTracker::new(),
            max_concurrent: 10,
        }
    }

    pub fn with_max_concurrent(mut self, max: usize) -> Self {
        self.max_concurrent = max;
        self
    }

    /// 새 Task 시작
    pub async fn spawn(&self, config: ContainerConfig) -> Result<TaskContainerId> {
        // 동시 실행 수 확인
        let count = self.containers.read().await.len();
        if count >= self.max_concurrent {
            return Err(forge_foundation::Error::internal(format!(
                "Maximum concurrent tasks ({}) reached",
                self.max_concurrent
            )));
        }

        // 컨테이너 생성 및 시작
        let mut container = TaskContainer::new(config.clone());
        let id = container.id().clone();

        container.start().await?;

        // 등록
        self.containers.write().await.insert(id.clone(), container);
        self.tracker.register(&id, config.initial_command.as_deref());

        Ok(id)
    }

    /// Task에 입력 전송
    pub async fn send_input(&self, id: &TaskContainerId, input: &str) -> Result<()> {
        let containers = self.containers.read().await;
        let container = containers.get(id).ok_or_else(|| {
            forge_foundation::Error::not_found(format!("Task not found: {}", id))
        })?;

        container.send_input(input).await
    }

    /// Task 출력 읽기
    pub async fn read_output(&self, id: &TaskContainerId) -> Result<Vec<String>> {
        let containers = self.containers.read().await;
        let container = containers.get(id).ok_or_else(|| {
            forge_foundation::Error::not_found(format!("Task not found: {}", id))
        })?;

        Ok(container.read_output().await)
    }

    /// 최근 출력 읽기
    pub async fn read_recent_output(
        &self,
        id: &TaskContainerId,
        lines: usize,
    ) -> Result<Vec<String>> {
        let containers = self.containers.read().await;
        let container = containers.get(id).ok_or_else(|| {
            forge_foundation::Error::not_found(format!("Task not found: {}", id))
        })?;

        Ok(container.read_recent_output(lines).await)
    }

    /// Task 종료
    pub async fn stop(&self, id: &TaskContainerId) -> Result<()> {
        let mut containers = self.containers.write().await;
        let container = containers.get_mut(id).ok_or_else(|| {
            forge_foundation::Error::not_found(format!("Task not found: {}", id))
        })?;

        container.stop().await?;
        self.tracker.mark_stopped(id);

        Ok(())
    }

    /// Task 강제 종료
    pub async fn kill(&self, id: &TaskContainerId) -> Result<()> {
        let mut containers = self.containers.write().await;
        let container = containers.get_mut(id).ok_or_else(|| {
            forge_foundation::Error::not_found(format!("Task not found: {}", id))
        })?;

        container.kill().await?;
        self.tracker.mark_killed(id);

        // 컨테이너 제거
        containers.remove(id);

        Ok(())
    }

    /// 모든 Task 종료
    pub async fn stop_all(&self) -> Result<()> {
        let ids: Vec<_> = self.containers.read().await.keys().cloned().collect();

        for id in ids {
            let _ = self.stop(&id).await;
        }

        Ok(())
    }

    /// Task 목록 조회
    pub fn list(&self) -> &TaskTracker {
        &self.tracker
    }

    /// Task 존재 확인
    pub async fn exists(&self, id: &TaskContainerId) -> bool {
        self.containers.read().await.contains_key(id)
    }

    /// 실행 중인 Task 수
    pub async fn running_count(&self) -> usize {
        self.containers.read().await.len()
    }
}

impl Default for TaskExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_executor_new() {
        let executor = TaskExecutor::new();
        assert_eq!(executor.running_count().await, 0);
    }
}
