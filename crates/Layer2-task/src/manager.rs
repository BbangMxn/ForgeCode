//! Task Manager - orchestrates task execution

use crate::executor::{ContainerExecutor, Executor, LocalExecutor};
use crate::state::TaskState;
use crate::task::{ExecutionMode, Task, TaskId, TaskResult};
use forge_foundation::{Error, Result};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tracing::{info, warn};

/// Configuration for task manager
pub struct TaskManagerConfig {
    /// Maximum concurrent tasks
    pub max_concurrent: usize,

    /// Default execution mode
    pub default_mode: ExecutionMode,
}

impl Default for TaskManagerConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 4,
            default_mode: ExecutionMode::Local,
        }
    }
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

    /// Container executor
    container_executor: Arc<ContainerExecutor>,

    /// Configuration
    config: Arc<TaskManagerConfig>,
}

impl TaskManager {
    /// Create a new task manager
    pub async fn new(config: TaskManagerConfig) -> Self {
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
            queue: Arc::new(Mutex::new(VecDeque::new())),
            running_count: Arc::new(Mutex::new(0)),
            local_executor: Arc::new(LocalExecutor::new()),
            container_executor: Arc::new(ContainerExecutor::new().await),
            config: Arc::new(config),
        }
    }

    /// Create with default configuration
    pub async fn default() -> Self {
        Self::new(TaskManagerConfig::default()).await
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

    /// Get all tasks for a session
    pub async fn get_by_session(&self, session_id: &str) -> Vec<Task> {
        let tasks = self.tasks.read().await;
        tasks
            .values()
            .filter(|t| t.session_id == session_id)
            .cloned()
            .collect()
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
}
