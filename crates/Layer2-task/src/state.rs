//! Task state machine

use crate::task::TaskResult;
use serde::{Deserialize, Serialize};

/// Possible states of a task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskState {
    /// Task is waiting to be executed
    Pending,

    /// Task is queued for execution
    Queued,

    /// Task is currently running
    Running,

    /// Task completed successfully
    Completed(TaskResult),

    /// Task failed with an error
    Failed(String),

    /// Task timed out
    Timeout,

    /// Task was cancelled
    Cancelled,
}

impl TaskState {
    /// Check if this is a terminal state (cannot transition further)
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TaskState::Completed(_)
                | TaskState::Failed(_)
                | TaskState::Timeout
                | TaskState::Cancelled
        )
    }

    /// Check if task is currently running
    pub fn is_running(&self) -> bool {
        matches!(self, TaskState::Running)
    }

    /// Check if task is pending (not yet started)
    pub fn is_pending(&self) -> bool {
        matches!(self, TaskState::Pending | TaskState::Queued)
    }

    /// Check if task completed successfully
    pub fn is_success(&self) -> bool {
        matches!(self, TaskState::Completed(_))
    }

    /// Get display name for the state
    pub fn display_name(&self) -> &'static str {
        match self {
            TaskState::Pending => "Pending",
            TaskState::Queued => "Queued",
            TaskState::Running => "Running",
            TaskState::Completed(_) => "Completed",
            TaskState::Failed(_) => "Failed",
            TaskState::Timeout => "Timeout",
            TaskState::Cancelled => "Cancelled",
        }
    }

    /// Get a symbol for the state (for TUI)
    pub fn symbol(&self) -> &'static str {
        match self {
            TaskState::Pending => "◯",
            TaskState::Queued => "◎",
            TaskState::Running => "⟳",
            TaskState::Completed(_) => "✓",
            TaskState::Failed(_) => "✗",
            TaskState::Timeout => "⏱",
            TaskState::Cancelled => "⊘",
        }
    }
}

impl std::fmt::Display for TaskState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}
