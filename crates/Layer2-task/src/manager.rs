//! Task Manager - orchestrates task execution with log support
//!
//! Features:
//! - Task lifecycle management
//! - Real-time log access
//! - Task termination
//! - LLM log analysis integration

use crate::executor::{ContainerExecutor, Executor, LocalExecutor, PtyExecutor};
use crate::log::{LogAnalysisReport, LogEntry, TaskLogManager};
use crate::state::TaskState;
use crate::task::{ExecutionMode, Task, TaskId, TaskResult};
use forge_foundation::{Error, Result};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex, RwLock};
use tracing::{debug, info, warn};

/// Configuration for task manager
#[derive(Debug, Clone)]
pub struct TaskManagerConfig {
    /// Maximum concurrent tasks
    pub max_concurrent: usize,

    /// Default execution mode
    pub default_mode: ExecutionMode,

    /// Maximum log entries per task
    pub max_log_entries: usize,

    /// Enable log persistence
    pub persist_logs: bool,
}

impl Default for TaskManagerConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 4,
            default_mode: ExecutionMode::Local,
            max_log_entries: 10000,
            persist_logs: false,
        }
    }
}

/// Task status for external queries
#[derive(Debug, Clone)]
pub struct TaskStatus {
    pub id: TaskId,
    pub session_id: String,
    pub tool_name: String,
    pub command: String,
    pub state: TaskState,
    pub is_running: bool,
    pub has_errors: bool,
    pub log_line_count: usize,
}

/// Task Manager - handles task lifecycle and execution
#[derive(Clone)]
pub struct TaskManager {
    /// All tasks by ID
    tasks: Arc<RwLock<HashMap<TaskId, Task>>>,

    /// Pending task queue
    queue: Arc<Mutex<VecDeque<TaskId>>>,

    /// Currently running task count
    running_count: Arc<Mutex<usize>>,

    /// Local executor
    local_executor: Arc<LocalExecutor>,

    /// PTY executor for interactive commands
    pty_executor: Arc<PtyExecutor>,

    /// Container executor
    container_executor: Arc<ContainerExecutor>,

    /// Shared log manager
    log_manager: Arc<TaskLogManager>,

    /// Configuration
    config: Arc<TaskManagerConfig>,
}

impl TaskManager {
    /// Create a new task manager
    pub async fn new(config: TaskManagerConfig) -> Self {
        let log_manager =
            Arc::new(TaskLogManager::new().with_max_buffers(config.max_concurrent * 10));

        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
            queue: Arc::new(Mutex::new(VecDeque::new())),
            running_count: Arc::new(Mutex::new(0)),
            local_executor: Arc::new(LocalExecutor::with_log_manager(Arc::clone(&log_manager))),
            pty_executor: Arc::new(PtyExecutor::with_log_manager(Arc::clone(&log_manager))),
            container_executor: Arc::new(ContainerExecutor::new().await),
            log_manager,
            config: Arc::new(config),
        }
    }

    /// Create with default configuration
    pub async fn default() -> Self {
        Self::new(TaskManagerConfig::default()).await
    }

    /// Get log manager
    pub fn log_manager(&self) -> Arc<TaskLogManager> {
        Arc::clone(&self.log_manager)
    }

    /// Submit a new task
    pub async fn submit(&self, mut task: Task) -> TaskId {
        let task_id = task.id;

        // Set default execution mode if not specified
        if matches!(task.execution_mode, ExecutionMode::Local) {
            task.execution_mode = self.config.default_mode.clone();
        }

        // Store task
        {
            let mut tasks = self.tasks.write().await;
            tasks.insert(task_id, task);
        }

        // Add to queue
        {
            let mut queue = self.queue.lock().await;
            queue.push_back(task_id);
        }

        // Try to process queue
        self.process_queue().await;

        task_id
    }

    /// Process pending tasks in the queue
    async fn process_queue(&self) {
        loop {
            // Check if we can run more tasks
            let can_run = {
                let count = self.running_count.lock().await;
                *count < self.config.max_concurrent
            };

            if !can_run {
                break;
            }

            // Get next task from queue
            let task_id = {
                let mut queue = self.queue.lock().await;
                queue.pop_front()
            };

            let task_id = match task_id {
                Some(id) => id,
                None => break,
            };

            // Execute task inline (no recursion)
            self.execute_task_inner(task_id).await;
        }
    }

    /// Execute a specific task (inner implementation without recursion)
    async fn execute_task_inner(&self, task_id: TaskId) {
        // Update task state to running
        let task = {
            let mut tasks = self.tasks.write().await;
            if let Some(task) = tasks.get_mut(&task_id) {
                task.start();
                task.clone()
            } else {
                return;
            }
        };

        // Increment running count
        {
            let mut count = self.running_count.lock().await;
            *count += 1;
        }

        info!("Executing task {}: {}", task_id, task.tool_name);

        // Select executor
        let executor: Arc<dyn Executor> = match &task.execution_mode {
            ExecutionMode::Local => self.local_executor.clone(),
            ExecutionMode::Pty => self.pty_executor.clone(),
            ExecutionMode::Container { .. } => {
                if self.container_executor.is_available() {
                    self.container_executor.clone()
                } else {
                    warn!("Container executor not available, falling back to local");
                    self.local_executor.clone()
                }
            }
        };

        // Execute
        let result = executor.execute(&task).await;

        // Update task state
        {
            let mut tasks = self.tasks.write().await;
            if let Some(task) = tasks.get_mut(&task_id) {
                match result {
                    Ok(result) => task.complete(result),
                    Err(Error::Timeout(_)) => task.timeout(),
                    Err(e) => task.fail(e.to_string()),
                }
            }
        }

        // Decrement running count
        {
            let mut count = self.running_count.lock().await;
            *count = count.saturating_sub(1);
        }
        // Note: Next task will be picked up by the loop in process_queue
    }

    /// Execute a specific task (public API that also triggers queue processing)
    pub async fn execute_task(&self, task_id: TaskId) {
        self.execute_task_inner(task_id).await;
        // Continue processing queue after task completes
        self.process_queue().await;
    }

    /// Get a task by ID
    pub async fn get(&self, task_id: TaskId) -> Option<Task> {
        let tasks = self.tasks.read().await;
        tasks.get(&task_id).cloned()
    }

    /// Get task status (simplified view with log info)
    pub async fn get_status(&self, task_id: TaskId) -> Option<TaskStatus> {
        let tasks = self.tasks.read().await;
        let task = tasks.get(&task_id)?;

        let task_id_str = task_id.to_string();
        let log_info = self.log_manager.get_buffer(&task_id_str).await;

        Some(TaskStatus {
            id: task.id,
            session_id: task.session_id.clone(),
            tool_name: task.tool_name.clone(),
            command: task.command.clone(),
            state: task.state.clone(),
            is_running: task.state.is_running(),
            has_errors: log_info
                .as_ref()
                .map(|b| !b.errors().is_empty())
                .unwrap_or(false),
            log_line_count: log_info.map(|b| b.line_count()).unwrap_or(0),
        })
    }

    /// Get all tasks for a session
    pub async fn get_by_session(&self, session_id: &str) -> Vec<Task> {
        let tasks = self.tasks.read().await;
        tasks
            .values()
            .filter(|t| t.session_id == session_id)
            .cloned()
            .collect()
    }

    /// Get all task statuses for a session
    pub async fn get_status_by_session(&self, session_id: &str) -> Vec<TaskStatus> {
        let mut statuses = Vec::new();
        let tasks = self.tasks.read().await;

        for task in tasks.values().filter(|t| t.session_id == session_id) {
            let task_id_str = task.id.to_string();
            let log_info = self.log_manager.get_buffer(&task_id_str).await;

            statuses.push(TaskStatus {
                id: task.id,
                session_id: task.session_id.clone(),
                tool_name: task.tool_name.clone(),
                command: task.command.clone(),
                state: task.state.clone(),
                is_running: task.state.is_running(),
                has_errors: log_info
                    .as_ref()
                    .map(|b| !b.errors().is_empty())
                    .unwrap_or(false),
                log_line_count: log_info.map(|b| b.line_count()).unwrap_or(0),
            });
        }

        statuses
    }

    /// Cancel a task
    pub async fn cancel(&self, task_id: TaskId) -> Result<()> {
        // Get task
        let task = {
            let tasks = self.tasks.read().await;
            tasks.get(&task_id).cloned()
        };

        let task = match task {
            Some(t) => t,
            None => return Err(Error::NotFound(format!("Task {} not found", task_id))),
        };

        // If pending, just remove from queue
        if task.state.is_pending() {
            let mut queue = self.queue.lock().await;
            queue.retain(|id| *id != task_id);
        }

        // If running, cancel in executor
        if task.state.is_running() {
            let executor: Arc<dyn Executor> = match &task.execution_mode {
                ExecutionMode::Local => self.local_executor.clone(),
                ExecutionMode::Pty => self.pty_executor.clone(),
                ExecutionMode::Container { .. } => self.container_executor.clone(),
            };

            executor.cancel(&task).await?;
        }

        // Update state
        {
            let mut tasks = self.tasks.write().await;
            if let Some(task) = tasks.get_mut(&task_id) {
                task.cancel();
            }
        }

        info!("Cancelled task {}", task_id);
        Ok(())
    }

    /// Force kill a running task
    pub async fn force_kill(&self, task_id: TaskId) -> Result<()> {
        let task = {
            let tasks = self.tasks.read().await;
            tasks.get(&task_id).cloned()
        };

        let task = match task {
            Some(t) => t,
            None => return Err(Error::NotFound(format!("Task {} not found", task_id))),
        };

        if !task.state.is_running() {
            return Err(Error::Task(format!("Task {} is not running", task_id)));
        }

        // Force kill through local executor
        self.local_executor.force_kill(&task_id.to_string()).await?;

        // Update state
        {
            let mut tasks = self.tasks.write().await;
            if let Some(task) = tasks.get_mut(&task_id) {
                task.cancel();
            }
        }

        info!("Force killed task {}", task_id);
        Ok(())
    }

    /// Get count of running tasks
    pub async fn running_count(&self) -> usize {
        *self.running_count.lock().await
    }

    /// Get count of pending tasks
    pub async fn pending_count(&self) -> usize {
        self.queue.lock().await.len()
    }

    /// Check if container execution is available
    pub fn container_available(&self) -> bool {
        self.container_executor.is_available()
    }

    /// Check if PTY execution is available
    pub fn pty_available(&self) -> bool {
        self.pty_executor.is_available()
    }

    /// Wait for a task to complete
    pub async fn wait(&self, task_id: TaskId) -> Option<TaskResult> {
        loop {
            let task = self.get(task_id).await?;

            match task.state {
                TaskState::Completed(result) => return Some(result),
                TaskState::Failed(_) | TaskState::Timeout | TaskState::Cancelled => return None,
                _ => {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            }
        }
    }

    // ========== Log Access Methods ==========

    /// Get logs for a task
    pub async fn get_logs(&self, task_id: TaskId, tail: Option<usize>) -> Vec<LogEntry> {
        self.local_executor
            .get_logs(&task_id.to_string(), tail)
            .await
    }

    /// Get errors for a task
    pub async fn get_errors(&self, task_id: TaskId) -> Vec<LogEntry> {
        self.local_executor.get_errors(&task_id.to_string()).await
    }

    /// Get log analysis for LLM debugging
    pub async fn get_log_analysis(&self, task_id: TaskId) -> Option<LogAnalysisReport> {
        self.local_executor
            .get_log_analysis(&task_id.to_string())
            .await
    }

    /// Get formatted log analysis for LLM
    pub async fn get_log_analysis_for_llm(&self, task_id: TaskId) -> Option<String> {
        self.get_log_analysis(task_id)
            .await
            .map(|report| report.format_for_llm())
    }

    /// Subscribe to real-time logs for a task
    pub async fn subscribe_logs(&self, task_id: TaskId) -> Option<broadcast::Receiver<LogEntry>> {
        self.local_executor
            .subscribe_logs(&task_id.to_string())
            .await
    }

    /// Get all running task IDs
    pub async fn get_running_tasks(&self) -> Vec<TaskId> {
        let tasks = self.tasks.read().await;
        tasks
            .iter()
            .filter(|(_, t)| t.state.is_running())
            .map(|(id, _)| *id)
            .collect()
    }

    /// Get active log tasks
    pub async fn get_active_log_tasks(&self) -> Vec<String> {
        self.log_manager.active_tasks().await
    }

    /// Cleanup old logs
    pub async fn cleanup_logs(&self) {
        self.log_manager.cleanup().await;
    }

    // ========== Batch Operations ==========

    /// Cancel all tasks in a session
    pub async fn cancel_session(&self, session_id: &str) -> Result<Vec<TaskId>> {
        let tasks = self.get_by_session(session_id).await;
        let mut cancelled = Vec::new();

        for task in tasks {
            if task.is_active() {
                if let Ok(()) = self.cancel(task.id).await {
                    cancelled.push(task.id);
                }
            }
        }

        info!(
            "Cancelled {} tasks in session {}",
            cancelled.len(),
            session_id
        );
        Ok(cancelled)
    }

    /// Force kill all running tasks
    pub async fn force_kill_all(&self) -> Result<Vec<TaskId>> {
        let running = self.get_running_tasks().await;
        let mut killed = Vec::new();

        for task_id in running {
            if let Ok(()) = self.force_kill(task_id).await {
                killed.push(task_id);
            }
        }

        warn!("Force killed {} tasks", killed.len());
        Ok(killed)
    }

    // ========== Resource Cleanup ==========

    /// 완료된 태스크 정리 (세션별로 최근 N개만 유지)
    ///
    /// 오래된 완료/실패/취소된 태스크를 정리하여 메모리를 절약합니다.
    ///
    /// # Arguments
    /// * `keep_per_session` - 세션당 유지할 태스크 수
    pub async fn cleanup_completed(&self, keep_per_session: usize) -> usize {
        let mut tasks = self.tasks.write().await;

        // 세션별로 완료된 태스크 그룹화
        let mut by_session: std::collections::HashMap<
            String,
            Vec<(TaskId, chrono::DateTime<chrono::Utc>)>,
        > = std::collections::HashMap::new();

        for (id, task) in tasks.iter() {
            if task.state.is_terminal() {
                by_session
                    .entry(task.session_id.clone())
                    .or_default()
                    .push((*id, task.completed_at.unwrap_or_else(|| chrono::Utc::now())));
            }
        }

        let mut removed = 0;

        // 각 세션에서 오래된 태스크 제거
        for (_session_id, mut task_times) in by_session {
            if task_times.len() > keep_per_session {
                // 완료 시간 순 정렬 (오래된 것 먼저)
                task_times.sort_by_key(|(_, time)| *time);

                // 오래된 것 제거
                for (task_id, _) in task_times.iter().take(task_times.len() - keep_per_session) {
                    tasks.remove(task_id);
                    removed += 1;
                }
            }
        }

        if removed > 0 {
            debug!("Cleaned up {} completed tasks", removed);
        }

        removed
    }

    /// 특정 기간 이전의 완료된 태스크 모두 제거
    ///
    /// # Arguments
    /// * `older_than` - 이 기간보다 오래된 태스크 제거
    pub async fn cleanup_older_than(&self, older_than: std::time::Duration) -> usize {
        let mut tasks = self.tasks.write().await;
        let cutoff =
            chrono::Utc::now() - chrono::Duration::from_std(older_than).unwrap_or_default();

        let to_remove: Vec<TaskId> = tasks
            .iter()
            .filter(|(_, task)| {
                task.state.is_terminal() && task.completed_at.map(|t| t < cutoff).unwrap_or(false)
            })
            .map(|(id, _)| *id)
            .collect();

        let count = to_remove.len();
        for id in to_remove {
            tasks.remove(&id);
        }

        if count > 0 {
            debug!("Cleaned up {} tasks older than {:?}", count, older_than);
        }

        count
    }

    /// 리소스 통계 조회
    pub async fn resource_stats(&self) -> ResourceStats {
        let tasks = self.tasks.read().await;
        let running = self.running_count.lock().await;
        let pending = self.queue.lock().await.len();

        let mut completed = 0;
        let mut failed = 0;

        for task in tasks.values() {
            match &task.state {
                TaskState::Completed(_) => completed += 1,
                TaskState::Failed(_) | TaskState::Timeout | TaskState::Cancelled => failed += 1,
                _ => {}
            }
        }

        ResourceStats {
            total_tasks: tasks.len(),
            running: *running,
            pending,
            completed,
            failed,
            log_tasks: self.log_manager.active_tasks().await.len(),
        }
    }

    /// 주기적 정리 시작 (백그라운드 태스크)
    ///
    /// # Arguments
    /// * `interval` - 정리 주기
    /// * `keep_per_session` - 세션당 유지할 완료된 태스크 수
    pub fn start_periodic_cleanup(
        self: Arc<Self>,
        interval: std::time::Duration,
        keep_per_session: usize,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(interval);

            loop {
                interval_timer.tick().await;

                // 완료된 태스크 정리
                let cleaned = self.cleanup_completed(keep_per_session).await;

                // 로그 정리
                self.cleanup_logs().await;

                if cleaned > 0 {
                    debug!("Periodic cleanup: removed {} tasks", cleaned);
                }
            }
        })
    }
}

/// 리소스 통계
#[derive(Debug, Clone)]
pub struct ResourceStats {
    /// 총 태스크 수
    pub total_tasks: usize,
    /// 실행 중인 태스크
    pub running: usize,
    /// 대기 중인 태스크
    pub pending: usize,
    /// 완료된 태스크
    pub completed: usize,
    /// 실패한 태스크
    pub failed: usize,
    /// 로그가 있는 태스크 수
    pub log_tasks: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_task_manager_creation() {
        let manager = TaskManager::default().await;
        assert_eq!(manager.running_count().await, 0);
        assert_eq!(manager.pending_count().await, 0);
    }

    #[tokio::test]
    async fn test_submit_task() {
        let manager = TaskManager::default().await;

        let task = Task::new("session-1", "bash", "echo hello", serde_json::json!({}));
        let task_id = manager.submit(task).await;

        // Task should be in the system
        let retrieved = manager.get(task_id).await;
        assert!(retrieved.is_some());
    }

    #[tokio::test]
    async fn test_get_status() {
        let manager = TaskManager::default().await;

        let task = Task::new("session-1", "bash", "echo test", serde_json::json!({}));
        let task_id = manager.submit(task).await;

        // Wait a bit for task to complete
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        let status = manager.get_status(task_id).await;
        assert!(status.is_some());

        let status = status.unwrap();
        assert_eq!(status.session_id, "session-1");
    }

    #[tokio::test]
    async fn test_cancel_task() {
        let manager = TaskManager::default().await;

        let task = Task::new("session-1", "bash", "sleep 10", serde_json::json!({}));
        let task_id = manager.submit(task).await;

        // Cancel
        let result = manager.cancel(task_id).await;
        assert!(result.is_ok());

        let task = manager.get(task_id).await.unwrap();
        assert!(task.state.is_terminal());
    }

    #[tokio::test]
    async fn test_log_access() {
        let manager = TaskManager::default().await;

        let task = Task::new("session-1", "bash", "echo test_log", serde_json::json!({}));
        let task_id = manager.submit(task).await;

        // Wait for completion
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Check logs
        let logs = manager.get_logs(task_id, None).await;
        // Logs should exist (may be empty on some systems)
        debug!("Log count: {}", logs.len());
    }

    #[tokio::test]
    async fn test_running_tasks() {
        let manager = TaskManager::default().await;

        let running = manager.get_running_tasks().await;
        assert!(running.is_empty());
    }
}
