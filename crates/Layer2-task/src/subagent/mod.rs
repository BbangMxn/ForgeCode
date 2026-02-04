//! Sub-agent system for task orchestration
//!
//! Sub-agents are specialized agents that can be spawned to handle
//! specific tasks with isolated context and tool access.
//!
//! ## Features
//!
//! - **Context Window Management**: Token tracking with auto-summarization
//! - **Discovery Sharing**: Knowledge transfer between agents
//! - **Tool Access Control**: Per-agent tool filtering
//! - **Log Integration**: Task log access for debugging
//! - **Handoff System**: Clean session transitions (Amp-style)

pub mod config;
pub mod context;
pub mod handoff;
pub mod manager;
pub mod types;

pub use config::{
    EffectiveTokenBudget, ModelSelection, PermissionMode, SubAgentConfig, TokenBudgetConfig,
    TokenBudgetSource,
};
pub use context::{
    CompressionCheckpoint, CompressionStats, ContextMessage, ContextStore, ContextToolResult,
    ContextWindowConfig, ContextWindowStatus, Discovery, DiscoveryId, FileAction, PreRotAction,
    PreRotConfig, PreRotLevel, PreRotStatus, RecoverableCompressionConfig, StructuredSummary,
    SubAgentContext, SummaryDecision, SummaryFact, SummaryFileRef, SummaryToolUsage, TokenReport,
};
pub use handoff::{
    ChangeType, CodeSnippet, EnvironmentContext, FileChange, HandoffManager, HandoffPackage,
    HandoffReason, HandoffRecommendation, HandoffRecord, HandoffStats, HandoffTriggerConfig,
    HandoffUrgency, QualityMetrics,
};
pub use manager::{QueuePriority, QueueStats, SubAgentManager, SubAgentManagerConfig};
pub use types::{SubAgent, SubAgentId, SubAgentState, SubAgentType};
