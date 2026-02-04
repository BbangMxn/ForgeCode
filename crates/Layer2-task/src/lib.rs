//! # forge-task
//!
//! Task management and execution system for ForgeCode.
//! Handles task lifecycle, queuing, and execution through various executors.
//!
//! ## Features
//!
//! - Task management and scheduling
//! - Multiple execution backends (Local, Container)
//! - Sub-agent orchestration for specialized tasks
//! - Background execution with output streaming
//! - Context isolation and knowledge sharing
//! - **Real-time log streaming and access**
//! - **Task termination and control**
//! - **LLM log analysis for debugging**

pub mod cluster;
pub mod container;
pub mod executor;
pub mod log;
pub mod manager;
pub mod state;
pub mod subagent;
pub mod task;

// Task system
pub use executor::{
    ContainerExecutor, Executor, LocalExecutor, PtyEnvSecurityConfig, PtyExecutor,
    PtyExecutorConfig, PtySizeConfig, SandboxConfig, SandboxExecutor, SandboxPolicy, SandboxResult,
    SandboxType,
};
pub use manager::{ResourceStats, TaskManager, TaskManagerConfig, TaskStatus};
pub use state::TaskState;
pub use task::{ExecutionMode, Task, TaskId, TaskResult};

// Log system
pub use log::{LogAnalysisReport, LogEntry, LogLevel, TaskLogBuffer, TaskLogManager};

// Container system
pub use container::{
    ContainerConfig, ContainerError, ContainerExecutor as ContainerExecutorTrait, ContainerResult,
    ContainerRuntime, ContainerTemplates, DockerExecutor, NetworkMode, ResourceLimits,
    SecurityProfile, VolumeMount,
};

// Cluster system
pub use cluster::{
    ApiValidationResult, ClusterConfig, ClusterError, ClusterExecutor, ClusterStats,
    HealthCheckResult, HealthChecker, LoadBalanceStrategy, RequestContext, ServerInfo,
    ServerStatus, TaskCluster,
};

// Sub-agent system
pub use subagent::{
    ContextMessage, ContextStore, ContextToolResult, ContextWindowConfig, ContextWindowStatus,
    Discovery, DiscoveryId, EffectiveTokenBudget, ModelSelection, PermissionMode, SubAgent,
    SubAgentConfig, SubAgentContext, SubAgentId, SubAgentManager, SubAgentState, SubAgentType,
    TokenBudgetConfig, TokenBudgetSource, TokenReport,
};
