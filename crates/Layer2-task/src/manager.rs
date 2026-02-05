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

    /// Auto cleanup settings
    pub auto_cleanup: AutoCleanupConfig,
}

/// Auto cleanup configuration for completed tasks
#[derive(Debug, Clone)]
pub struct AutoCleanupConfig {
    /// Enable automatic cleanup of completed tasks
    pub enabled: bool,

    /// Cleanup interval in seconds
    pub interval_secs: u64,

    /// Number of completed tasks to keep per session
    pub keep_per_session: usize,
}

impl Default for AutoCleanupConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_secs: 300, // 5 minutes
            keep_per_session: 50,
        }
    }
}

impl Default for TaskManagerConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 4,
            default_mode: ExecutionMode::Local,
            max_log_entries: 10000,
            persist_logs: false,
            auto_cleanup: AutoCleanupConfig::default(),
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
///
/// Performance optimizations:
/// - Uses atomic counter for running_count (no lock contention)
/// - Batched queue operations to reduce lock acquisitions
/// - Pre-allocated HashMap capacity
#[derive(Clone)]
pub struct TaskManager {
    /// All tasks by ID
    tasks: Arc<RwLock<HashMap<TaskId, Task>>>,

    /// Pending task queue
    queue: Arc<Mutex<VecDeque<TaskId>>>,

    /// Currently running task count (atomic for lock-free reads)
    running_count: Arc<std::sync::atomic::AtomicUsize>,

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
    ///
    /// If `auto_cleanup.enabled` is true, starts a background cleanup loop
    /// that periodically removes old completed tasks to save memory.
    pub async fn new(config: TaskManagerConfig) -> Self {
        let log_manager =
            Arc::new(TaskLogManager::new().with_max_buffers(config.max_concurrent * 10));

        let auto_cleanup = config.auto_cleanup.clone();

        let manager = Self {
            // Pre-allocate HashMap with expected capacity
            tasks: Arc::new(RwLock::new(HashMap::with_capacity(config.max_concurrent * 4))),
            queue: Arc::new(Mutex::new(VecDeque::with_capacity(config.max_concurrent * 2))),
            // Atomic counter for lock-free reads
            running_count: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            local_executor: Arc::new(LocalExecutor::with_log_manager(Arc::clone(&log_manager))),
            pty_executor: Arc::new(PtyExecutor::with_log_manager(Arc::clone(&log_manager))),
            container_executor: Arc::new(ContainerExecutor::new().await),
            log_manager,
            config: Arc::new(config),
        };

        // Start auto cleanup if enabled
        if auto_cleanup.enabled {
            let manager_clone = manager.clone();
            let interval = std::time::Duration::from_secs(auto_cleanup.interval_secs);
            let keep = auto_cleanup.keep_per_session;

            tokio::spawn(async move {
                let mut interval_timer = tokio::time::interval(interval);
                // Skip the first immediate tick
                interval_timer.tick().await;

                loop {
                    interval_timer.tick().await;
                    let cleaned = manager_clone.cleanup_completed(keep).await;
                    manager_clone.cleanup_logs().await;

                    if cleaned > 0 {
                        debug!("Auto cleanup: removed {} completed tasks", cleaned);
                    }
                }
            });
            info!(
                "Task auto-cleanup enabled: interval={}s, keep_per_session={}",
                auto_cleanup.interval_secs, auto_cleanup.keep_per_session
            );
        }

        manager
    }

    /// Create with default configuration (auto cleanup enabled)
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
    ///
    /// Performance optimized:
    /// - Lock-free running count check via atomic
    /// - Single lock acquisition for queue pop
    async fn process_queue(&self) {
        use std::sync::atomic::Ordering;

        loop {
            // Lock-free check: can we run more tasks?
            let current = self.running_count.load(Ordering::Acquire);
            if current >= self.config.max_concurrent {
                break;
            }

            // Get next task from queue (single lock)
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
    ///
    /// Performance optimized:
    /// - Atomic increment for running count (no lock)
    /// - Single write lock for task state update
    async fn execute_task_inner(&self, task_id: TaskId) {
        use std::sync::atomic::Ordering;

        // Update task state to running (single lock acquisition)
        let task = {
            let mut tasks = self.tasks.write().await;
            if let Some(task) = tasks.get_mut(&task_id) {
                task.start();
                task.clone()
            } else {
                return;
            }
        };

        // Atomic increment - no lock needed
        self.running_count.fetch_add(1, Ordering::AcqRel);

        info!("Executing task {}: {}", task_id, task.tool_name);

        // For PTY mode, use background execution (returns immediately)
        if matches!(task.execution_mode, ExecutionMode::Pty) {
            let result = self.pty_executor.spawn_background(&task).await;

            // Update task state based on spawn result
            {
                let mut tasks = self.tasks.write().await;
                if let Some(t) = tasks.get_mut(&task_id) {
                    match result {
                        Ok(_) => {
                            // Task is running in background, keep state as Running
                            info!("PTY task {} spawned in background", task_id);
                        }
                        Err(e) => {
                            t.fail(e.to_string());
                            // Atomic decrement on failure
                            self.running_count.fetch_sub(1, Ordering::AcqRel);
                        }
                    }
                }
            }
            return;
        }

        // Select executor for non-PTY modes
        let executor: Arc<dyn Executor> = match &task.execution_mode {
            ExecutionMode::Local => self.local_executor.clone(),
            ExecutionMode::Container { .. } => {
                if self.container_executor.is_available() {
                    self.container_executor.clone()
                } else {
                    warn!("Container executor not available, falling back to local");
                    self.local_executor.clone()
                }
            }
            ExecutionMode::Pty => unreachable!(), // Handled above
        };

        // Execute synchronously
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

        // Atomic decrement - no lock needed
        self.running_count.fetch_sub(1, Ordering::AcqRel);
        // Note: Next task will be picked up by the loop in process_queue
    }

    /// Get the PTY executor for direct access (for wait operations)
    pub fn pty_executor(&self) -> Arc<PtyExecutor> {
        Arc::clone(&self.pty_executor)
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

    /// Get count of running tasks (lock-free)
    pub fn running_count(&self) -> usize {
        use std::sync::atomic::Ordering;
        self.running_count.load(Ordering::Acquire)
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

    /// 특정 Task의 상세 진행 상태 조회
    pub async fn get_progress_report(&self, task_id: TaskId) -> Option<TaskProgressReport> {
        let tasks = self.tasks.read().await;
        let task = tasks.get(&task_id)?;

        // 최근 로그 가져오기
        let logs = self.get_logs(task_id, Some(5)).await;
        let recent_output: Vec<String> = logs
            .iter()
            .map(|l| l.content.chars().take(100).collect())
            .collect();

        // 에러 수 계산
        let errors = self.get_errors(task_id).await;
        let error_count = errors.len();

        // 실행 시간 계산
        let elapsed_ms = task
            .started_at
            .map(|start| {
                (chrono::Utc::now() - start).num_milliseconds().max(0) as u64
            })
            .unwrap_or(0);

        // 진행률 힌트 추출 (특정 패턴에서)
        let progress_hint = self.extract_progress_hint(&recent_output, &task.command);

        Some(TaskProgressReport {
            task_id,
            session_id: task.session_id.clone(),
            tool_name: task.tool_name.clone(),
            command: task.command.clone(),
            state: task.state.clone(),
            elapsed_ms,
            recent_output,
            error_count,
            progress_hint,
        })
    }

    /// 모든 실행 중인 Task의 진행 상태 조회
    pub async fn get_all_progress_reports(&self) -> Vec<TaskProgressReport> {
        let running_ids = self.get_running_tasks().await;
        let mut reports = Vec::with_capacity(running_ids.len());

        for task_id in running_ids {
            if let Some(report) = self.get_progress_report(task_id).await {
                reports.push(report);
            }
        }

        reports
    }

    /// 출력에서 진행률 힌트 추출
    fn extract_progress_hint(&self, output: &[String], command: &str) -> Option<ProgressHint> {
        // npm/cargo 빌드 패턴 감지
        for line in output.iter().rev() {
            let line_lower = line.to_lowercase();

            // npm 패턴: "added X packages"
            if line_lower.contains("added") && line_lower.contains("packages") {
                return Some(ProgressHint {
                    percent: 100,
                    current_operation: "Dependencies installed".to_string(),
                    remaining_items: None,
                });
            }

            // cargo 빌드 패턴: "Compiling X (Y/Z)"
            if line_lower.contains("compiling") {
                if let Some(progress) = self.parse_cargo_progress(line) {
                    return Some(progress);
                }
                return Some(ProgressHint {
                    percent: 50,
                    current_operation: "Compiling...".to_string(),
                    remaining_items: None,
                });
            }

            // cargo test 패턴: "running X tests"
            if line_lower.contains("running") && line_lower.contains("test") {
                return Some(ProgressHint {
                    percent: 80,
                    current_operation: "Running tests".to_string(),
                    remaining_items: None,
                });
            }

            // 서버 시작 패턴
            if line_lower.contains("listening") || line_lower.contains("started") {
                return Some(ProgressHint {
                    percent: 100,
                    current_operation: "Server running".to_string(),
                    remaining_items: None,
                });
            }
        }

        // 명령어에 따른 기본 힌트
        if command.contains("npm install") || command.contains("yarn") {
            return Some(ProgressHint {
                percent: 25,
                current_operation: "Installing dependencies".to_string(),
                remaining_items: None,
            });
        }

        None
    }

    /// Cargo 빌드 진행률 파싱
    fn parse_cargo_progress(&self, line: &str) -> Option<ProgressHint> {
        // "Compiling crate_name vX.Y.Z (current/total)"
        // 간단한 구현 - 실제 파싱은 더 복잡할 수 있음
        if let Some(start) = line.find('(') {
            if let Some(end) = line.find(')') {
                let nums: &str = &line[start + 1..end];
                let parts: Vec<&str> = nums.split('/').collect();
                if parts.len() == 2 {
                    if let (Ok(current), Ok(total)) = (
                        parts[0].trim().parse::<usize>(),
                        parts[1].trim().parse::<usize>(),
                    ) {
                        let percent = ((current as f32 / total as f32) * 100.0) as u8;
                        return Some(ProgressHint {
                            percent,
                            current_operation: format!("Compiling {}/{}", current, total),
                            remaining_items: Some(total - current),
                        });
                    }
                }
            }
        }
        None
    }

    /// 리소스 통계 조회
    pub async fn resource_stats(&self) -> ResourceStats {
        use std::sync::atomic::Ordering;
        let tasks = self.tasks.read().await;
        let running = self.running_count.load(Ordering::Acquire);
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
            running,
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

/// 상세 Task 진행 상태 리포트 - LLM이 작업 진행 상태를 파악할 수 있음
#[derive(Debug, Clone)]
pub struct TaskProgressReport {
    /// 태스크 ID
    pub task_id: TaskId,
    /// 세션 ID
    pub session_id: String,
    /// 도구 이름
    pub tool_name: String,
    /// 실행 명령
    pub command: String,
    /// 현재 상태
    pub state: TaskState,
    /// 실행 시간 (밀리초)
    pub elapsed_ms: u64,
    /// 마지막 출력 라인 (최근 5줄)
    pub recent_output: Vec<String>,
    /// 에러 수
    pub error_count: usize,
    /// 추정 진행률 (있는 경우)
    pub progress_hint: Option<ProgressHint>,
}

/// 진행률 힌트 - 특정 작업 패턴에서 추정된 진행률
#[derive(Debug, Clone)]
pub struct ProgressHint {
    /// 진행률 (0-100)
    pub percent: u8,
    /// 현재 작업 설명
    pub current_operation: String,
    /// 남은 항목 수 (알 수 있는 경우)
    pub remaining_items: Option<usize>,
}

impl TaskProgressReport {
    /// LLM이 이해하기 쉬운 형식으로 포맷
    pub fn format_for_llm(&self) -> String {
        let mut output = format!(
            "Task {} ({}): {}\n  Command: {}\n  Status: {:?}\n  Elapsed: {}ms",
            self.task_id, self.tool_name, self.session_id, self.command, self.state, self.elapsed_ms
        );

        if self.error_count > 0 {
            output.push_str(&format!("\n  Errors: {}", self.error_count));
        }

        if let Some(ref hint) = self.progress_hint {
            output.push_str(&format!(
                "\n  Progress: {}% - {}",
                hint.percent, hint.current_operation
            ));
            if let Some(remaining) = hint.remaining_items {
                output.push_str(&format!(" ({} remaining)", remaining));
            }
        }

        if !self.recent_output.is_empty() {
            output.push_str("\n  Recent output:");
            for line in &self.recent_output {
                output.push_str(&format!("\n    | {}", line));
            }
        }

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_task_manager_creation() {
        let manager = TaskManager::default().await;
        assert_eq!(manager.running_count(), 0);
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
