//! Task executors
//!
//! Provides multiple execution backends:
//! - `LocalExecutor` - Simple process execution with log streaming
//! - `PtyExecutor` - Full PTY support for interactive commands
//! - `ContainerExecutor` - Docker-based isolated execution
//! - `SandboxExecutor` - Platform-native sandboxed execution (Seatbelt/Landlock)

pub mod container;
pub mod local;
pub mod pty;
pub mod sandbox;
pub mod r#trait;

pub use container::ContainerExecutor;
pub use local::{LocalExecutor, LocalExecutorConfig, TimeoutPolicy, TimeoutState};
pub use pty::{PtyEnvSecurityConfig, PtyExecutor, PtyExecutorConfig, PtySizeConfig};
pub use r#trait::Executor;
pub use sandbox::{SandboxConfig, SandboxExecutor, SandboxPolicy, SandboxResult, SandboxType};
