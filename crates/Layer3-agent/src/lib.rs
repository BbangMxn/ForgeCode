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

// Performance optimization
pub mod parallel;

// Context Store (2025 Deep Agent pattern)
pub mod context_store;

// Smart Context Management (2025 Claude Opus 4.5 style - 65% token savings)
pub mod smart_context;

// Agent Sub-skills (2025 Cursor 2.4 style)
pub mod subskill;

// Long-running agent support (Claude Code style)
pub mod todo;
pub mod progress;

// Research-based enhancements (2025)
// Based on: AI Agentic Programming Survey, ReAct, SWE-agent, OpenDevin
pub mod feedback;   // Feedback loop system (auto-retry on failure)
pub mod react;      // ReAct pattern (Thought-Action-Observation)
pub mod memory;     // Agent memory system (RAG, semantic search)

// ============================================================================
// Legacy Modules (Deprecated)
// ============================================================================
//
// 다음 모듈들은 레거시 코드로, 새로운 시스템으로 대체되었습니다.
// 하위 호환성을 위해 유지되지만, 새 코드에서는 사용하지 마세요.
//
// Migration Guide:
// - optimizer -> forge_foundation::cache (Layer1) 또는 compressor (이 크레이트)
// - runtime, strategy, variant -> agent 모듈의 새 시스템 사용
// - bench -> 별도 벤치마크 도구로 이동 예정

/// Context optimization module (deprecated)
///
/// **Deprecated**: Use `forge_foundation::cache::ContextCompactor` for context
/// management or `crate::compressor` for LLM-based compression.
#[deprecated(
    since = "0.2.0",
    note = "Use forge_foundation::cache::ContextCompactor or crate::compressor instead"
)]
pub mod optimizer;

/// Legacy modular agent system (deprecated)
///
/// **Deprecated**: Use the new simplified agent system in `crate::agent`.
#[deprecated(since = "0.2.0", note = "Use crate::agent::Agent instead")]
#[allow(deprecated)]
pub mod bench;

#[deprecated(since = "0.2.0", note = "Use crate::agent::Agent instead")]
#[allow(deprecated)]
pub mod runtime;

#[deprecated(since = "0.2.0", note = "Use crate::agent::Agent instead")]
#[allow(deprecated)]
pub mod strategy;

#[deprecated(since = "0.2.0", note = "Use crate::agent::Agent instead")]
#[allow(deprecated)]
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
    SmartCompressor, TokenUsageInfo,
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
    ErrorRecovery, FileNotFoundRecovery, NetworkErrorRecovery,
    RateLimitRecovery, RecoveryAction, RecoveryContext, RecoveryStrategy, 
    RecoverableError, TimeoutRecovery,
};

// Long-running agent support (Claude Code style)
pub use todo::{TodoItem, TodoManager, TodoStats, TodoStatus, Priority};
pub use progress::{ProgressTracker, ProgressEntry, ProgressAction, Feature, FeatureList};

// Research-based enhancements (2025)
pub use feedback::{Feedback, FeedbackAnalyzer, FeedbackLoop, FeedbackType, RetryStrategy};
pub use react::{ReactExample, ReactPromptBuilder, ReactStep, ReactSummary, ReactTrace, ReactTracer};
pub use memory::{AgentMemory, LongTermMemory, MemoryEntry, MemoryMetadata, MemoryType, SemanticSearch, ShortTermMemory};

// ============================================================================
// Legacy Exports (Deprecated)
// ============================================================================

// Optimizer exports (deprecated)
// 주의: 이 타입들은 forge_foundation::cache로 마이그레이션하세요
#[allow(deprecated)]
pub use optimizer::{
    estimate_tokens, ContextCompactor as LegacyContextCompactor, ContextMessage,
    ContextOptimizer, ContextOptimizerConfig, MessageImportance, OptimizationResult, OptimizerStats,
};

// 하위 호환성을 위한 alias (deprecated)
#[deprecated(
    since = "0.2.0",
    note = "Use forge_foundation::cache::ContextCompactor instead"
)]
pub type ContextCompactor = optimizer::ContextCompactor;

// Runtime exports (deprecated)
#[allow(deprecated)]
pub use runtime::{
    AgentCapability, AgentMetadata, AgentPhase, AgentRuntime, AgentRuntimeExt, ExecuteOutput,
    LifecycleEvent, LifecycleObserver, PlanOutput, ReflectOutput, RuntimeConfig, RuntimeContext,
    RuntimeHooks, ThinkOutput,
};

// Strategy exports (deprecated)
#[allow(deprecated)]
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
    // Long-running agent support
    pub use crate::todo::{TodoManager, TodoItem, Priority};
    pub use crate::progress::{ProgressTracker, FeatureList};
    // Research-based enhancements
    pub use crate::feedback::{FeedbackLoop, FeedbackAnalyzer, RetryStrategy};
    pub use crate::react::{ReactTracer, ReactPromptBuilder};
    pub use crate::memory::AgentMemory;
}
