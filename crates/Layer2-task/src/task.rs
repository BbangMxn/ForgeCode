//! Task definition and types

use crate::state::TaskState;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use uuid::Uuid;

/// Unique identifier for a task
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(pub Uuid);

impl TaskId {
    /// Generate a new random TaskId
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for TaskId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.0.to_string()[..8])
    }
}

/// Execution mode for a task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecutionMode {
    /// Execute locally on the host (standard process)
    Local,

    /// Execute with PTY support (for interactive commands)
    /// Use this for commands that need terminal emulation (vim, htop, etc.)
    Pty,

    /// Execute in a Docker container
    Container {
        /// Container image to use
        image: String,

        /// Working directory inside container
        workdir: Option<String>,

        /// Environment variables
        env: Vec<(String, String)>,

        /// Volumes to mount (host:container)
        volumes: Vec<(String, String)>,
    },
}

impl Default for ExecutionMode {
    fn default() -> Self {
        Self::Local
    }
}

/// A task to be executed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Unique task identifier
    pub id: TaskId,

    /// Session this task belongs to
    pub session_id: String,

    /// Tool name that created this task
    pub tool_name: String,

    /// Command or action to execute
    pub command: String,

    /// Input parameters
    pub input: serde_json::Value,

    /// Current state
    pub state: TaskState,

    /// Execution mode
    pub execution_mode: ExecutionMode,

    /// Timeout duration
    pub timeout: Duration,

    /// When the task was created
    pub created_at: DateTime<Utc>,

    /// When the task started executing
    pub started_at: Option<DateTime<Utc>>,

    /// When the task completed
    pub completed_at: Option<DateTime<Utc>>,

    /// Container ID if running in container
    pub container_id: Option<String>,
}

impl Task {
    /// Create a new task
    pub fn new(
        session_id: impl Into<String>,
        tool_name: impl Into<String>,
        command: impl Into<String>,
        input: serde_json::Value,
    ) -> Self {
        Self {
            id: TaskId::new(),
            session_id: session_id.into(),
            tool_name: tool_name.into(),
            command: command.into(),
            input,
            state: TaskState::Pending,
            execution_mode: ExecutionMode::default(),
            timeout: Duration::from_secs(120),
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            container_id: None,
        }
    }

    /// Set execution mode
    pub fn with_execution_mode(mut self, mode: ExecutionMode) -> Self {
        self.execution_mode = mode;
        self
    }

    /// Set timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Mark task as running
    pub fn start(&mut self) {
        self.state = TaskState::Running;
        self.started_at = Some(Utc::now());
    }

    /// Mark task as completed successfully
    pub fn complete(&mut self, result: TaskResult) {
        self.state = TaskState::Completed(result);
        self.completed_at = Some(Utc::now());
    }

    /// Mark task as failed
    pub fn fail(&mut self, error: String) {
        self.state = TaskState::Failed(error);
        self.completed_at = Some(Utc::now());
    }

    /// Mark task as timed out
    pub fn timeout(&mut self) {
        self.state = TaskState::Timeout;
        self.completed_at = Some(Utc::now());
    }

    /// Mark task as cancelled
    pub fn cancel(&mut self) {
        self.state = TaskState::Cancelled;
        self.completed_at = Some(Utc::now());
    }

    /// Check if task is still active (pending or running)
    pub fn is_active(&self) -> bool {
        matches!(self.state, TaskState::Pending | TaskState::Running)
    }

    /// Get execution duration if task has started
    pub fn duration(&self) -> Option<Duration> {
        let start = self.started_at?;
        let end = self.completed_at.unwrap_or_else(Utc::now);
        Some((end - start).to_std().unwrap_or_default())
    }
}

/// Result of task execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    /// Output content
    pub output: String,

    /// Exit code (if applicable)
    pub exit_code: Option<i32>,

    /// Additional metadata
    pub metadata: Option<serde_json::Value>,
}

impl TaskResult {
    /// Create a success result
    pub fn success(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
            exit_code: Some(0),
            metadata: None,
        }
    }

    /// Create a result with specific exit code
    pub fn with_exit_code(output: impl Into<String>, exit_code: i32) -> Self {
        Self {
            output: output.into(),
            exit_code: Some(exit_code),
            metadata: None,
        }
    }

    /// Add metadata to result
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }
}
