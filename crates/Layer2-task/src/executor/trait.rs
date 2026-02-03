//! Executor trait

use crate::task::{Task, TaskResult};
use async_trait::async_trait;
use forge_foundation::Result;

/// Executor trait - implement to add new execution backends
#[async_trait]
pub trait Executor: Send + Sync {
    /// Execute a task
    async fn execute(&self, task: &Task) -> Result<TaskResult>;

    /// Cancel a running task
    async fn cancel(&self, task: &Task) -> Result<()>;

    /// Check if the executor is available
    fn is_available(&self) -> bool;

    /// Get executor name
    fn name(&self) -> &'static str;
}
