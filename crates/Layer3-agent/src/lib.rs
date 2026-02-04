//! # forge-agent
//!
//! Agent system for ForgeCode - Claude Code / OpenCode / Gemini CLI 스타일의
//! 단순하고 효율적인 Agent Loop 구현입니다.
//!
//! ## 핵심 원칙
//!
//! 1. **Simple is Better** - 복잡한 4단계 loop 대신 단순한 while(tool_call) loop
//! 2. **Single Thread** - Multi-agent swarm 대신 single loop + sub-agent dispatch
//! 3. **Flat History** - 단순한 MessageHistory
//! 4. **Sequential Tools** - 병렬 Tool 실행은 sub-agent로 위임
//!
//! ## 핵심 컴포넌트
//!
//! - **Agent**: 메인 에이전트 루프
//! - **Hook**: 확장 가능한 훅 시스템 (before/after agent, tool)
//! - **Compressor**: 자동 컨텍스트 압축 (92% threshold)
//! - **Steering**: 실시간 중단/재개/방향전환
//!
//! ## 사용 예
//!
//! ```ignore
//! use forge_agent::{Agent, AgentConfig, AgentEvent};
//! use forge_agent::hook::LoggingHook;
//!
//! // Agent 생성
//! let agent = Agent::with_config(ctx, AgentConfig::default())
//!     .with_hook(LoggingHook::new())
//!     .with_max_iterations(50);
//!
//! // Steering handle로 외부에서 제어 가능
//! let handle = agent.steering_handle();
//!
//! // 실행
//! let (tx, mut rx) = tokio::sync::mpsc::channel(100);
//! let response = agent.run(session_id, &mut history, "Hello", tx).await?;
//!
//! // 외부에서 중단
//! handle.stop("User requested").await?;
//! ```

// Core modules
pub mod agent;
pub mod context;
pub mod history;
pub mod recovery;
pub mod session;

// New simplified system
pub mod compressor;
pub mod hook;
pub mod provider_bridge;
pub mod steering;

// Legacy modules (kept for compatibility, may be removed later)
pub mod optimizer;

// Legacy modular agent system (to be deprecated)
// These are kept for now but the new simplified system is preferred
pub mod bench;
pub mod runtime;
pub mod strategy;
pub mod variant;

// ============================================================================
// Primary Exports (New System)
// ============================================================================

pub use agent::{Agent, AgentConfig, AgentEvent};
pub use context::{AgentContext, ProviderInfo};
pub use history::MessageHistory;
pub use session::{Session, SessionManager};

// Hook system
pub use hook::{
    AgentHook, HookManager, HookResult, LoggingHook, TokenTrackingHook, ToolResult, TurnInfo,
};

// Compressor
pub use compressor::{
    CompressionResult, CompressionStats, CompressorConfig, ContextCompressor, LlmCompressor,
    SmartCompressor,
};

// Steering
pub use steering::{
    AgentState, AgentStatus, Steerable, SteeringChecker, SteeringCommand, SteeringError,
    SteeringHandle, SteeringQueue,
};

// Provider bridge (connects Layer3 Agent to Layer2 AgentProvider)
pub use provider_bridge::{build_provider_registry, ForgeNativeProvider};

// Recovery
pub use recovery::{
    EditConflictRecovery, ErrorRecovery, FileNotFoundRecovery, PermissionDeniedRecovery,
    RateLimitRecovery, RecoveryAction, RecoveryContext, RecoveryStrategy, TimeoutRecovery,
    ToolError,
};

// Optimizer (legacy, for context management)
pub use optimizer::{
    estimate_tokens, ContextCompactor, ContextMessage, ContextOptimizer, ContextOptimizerConfig,
    MessageImportance, OptimizationResult, OptimizerStats,
};

// ============================================================================
// Legacy Exports (Deprecated - use new system instead)
// ============================================================================

// Runtime exports (deprecated)
pub use runtime::{
    AgentCapability, AgentMetadata, AgentPhase, AgentRuntime, AgentRuntimeExt, ExecuteOutput,
    LifecycleEvent, LifecycleObserver, PlanOutput, ReflectOutput, RuntimeConfig, RuntimeContext,
    RuntimeHooks, ThinkOutput,
};

// Strategy exports (deprecated)
pub use strategy::{
    AdaptiveExecution, ChainOfThought, ExecutionStrategy, HierarchicalPlanning, MemoryStrategy,
    ParallelExecution, PlanningStrategy, RAGMemory, ReActReasoning, ReasoningStrategy,
    SequentialExecution, SimplePlanning, SlidingWindowMemory, SummarizingMemory, TreeOfThought,
};

// ============================================================================
// Prelude
// ============================================================================

/// Convenient imports for common usage
pub mod prelude {
    pub use crate::agent::{Agent, AgentConfig, AgentEvent};
    pub use crate::compressor::{CompressorConfig, ContextCompressor};
    pub use crate::context::AgentContext;
    pub use crate::history::MessageHistory;
    pub use crate::hook::{AgentHook, HookManager, HookResult, LoggingHook};
    pub use crate::provider_bridge::{build_provider_registry, ForgeNativeProvider};
    pub use crate::steering::{SteeringHandle, SteeringQueue};
}
