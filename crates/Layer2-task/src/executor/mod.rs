//! Task executors
//!
//! Provides multiple execution backends:
//! - `LocalExecutor` - Simple process execution with log streaming
//! - `PtyExecutor` - Full PTY support for interactive commands
//! - `ContainerExecutor` - Docker-based isolated execution
//! - `SandboxExecutor` - Platform-native sandboxed execution (Seatbelt/Landlock)
//!
//! ## Security
//! - `ShellPolicy` - Command-level permission control for shell commands
//! - `TaskShellPolicy` - Per-task custom permission policies
//!
//! ## Resource Monitoring
//! - `ResourceMonitor` - CPU/Memory usage tracking for processes
//! - `ProcessResourceLimits` - Per-process resource limits

pub mod container;
pub mod local;
pub mod pty;
pub mod resource_monitor;
pub mod sandbox;
pub mod shell_policy;
pub mod r#trait;

pub use container::ContainerExecutor;
pub use local::{LocalExecutor, LocalExecutorConfig, TimeoutPolicy, TimeoutState};
pub use pty::{PtyEnvSecurityConfig, PtyExecutor, PtyExecutorConfig, PtySizeConfig};
pub use r#trait::Executor;
pub use sandbox::{SandboxConfig, SandboxExecutor, SandboxPolicy, SandboxResult, SandboxType};
pub use shell_policy::{PolicyResult, RiskLevel, ShellPolicy, TaskShellPolicy};
pub use resource_monitor::{
    LimitExceededAction, ProcessResourceLimits, ProcessResourceTracker,
    ResourceMonitor, ResourceSnapshot, ResourceViolation, ViolationType,
};
