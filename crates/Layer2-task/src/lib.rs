//! # forge-task
//!
//! Task management and execution system for ForgeCode.
//! Handles task lifecycle, queuing, and execution through various executors.

pub mod executor;
pub mod manager;
pub mod state;
pub mod task;

pub use executor::{ContainerExecutor, Executor, LocalExecutor};
pub use manager::TaskManager;
pub use state::TaskState;
pub use task::{Task, TaskId, TaskResult};
