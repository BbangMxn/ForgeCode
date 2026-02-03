//! Task executors

pub mod container;
pub mod local;
pub mod r#trait;

pub use container::ContainerExecutor;
pub use local::LocalExecutor;
pub use r#trait::Executor;
