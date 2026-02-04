//! # Agent Strategy Components (DEPRECATED)
//!
//! **⚠️ DEPRECATED**: 이 모듈은 더 이상 사용되지 않습니다.
//! Claude Code / OpenCode / Gemini CLI 분석 결과, 복잡한 전략 패턴보다
//! 단순한 while(tool_call) loop가 더 효율적입니다.
//!
//! 새로운 시스템에서는 Hook을 통해 필요시 확장하세요:
//!
//! ```ignore
//! use forge_agent::hook::AgentHook;
//!
//! struct MyReasoningHook;
//!
//! #[async_trait]
//! impl AgentHook for MyReasoningHook {
//!     async fn before_turn(&self, history: &MessageHistory, turn: u32) -> Result<HookResult> {
//!         // Custom reasoning logic here
//!         Ok(HookResult::Continue)
//!     }
//! }
//! ```
//!
//! ## 이전 설명 (참고용)
//!
//! 모듈형 Agent의 교체 가능한 전략 컴포넌트들입니다.
//!
//! ## 전략 타입
//!
//! - **ReasoningStrategy**: 추론 방식 (Chain-of-Thought, Tree-of-Thought 등)
//! - **PlanningStrategy**: 계획 수립 방식 (Hierarchical, ReAct 등)
//! - **MemoryStrategy**: 메모리/컨텍스트 관리 방식
//! - **ExecutionStrategy**: 실행 방식 (Sequential, Parallel 등)
//!
//! ## 사용 예
//!
//! ```ignore
//! // 전략 조합으로 Agent 생성
//! let agent = AgentBuilder::new()
//!     .with_reasoning(TreeOfThought::new())
//!     .with_planning(HierarchicalPlanning::new())
//!     .with_memory(SlidingWindowMemory::new(100_000))
//!     .with_execution(ParallelExecution::new(4))
//!     .build();
//! ```

mod execution;
mod memory;
mod planning;
mod reasoning;

pub use execution::{
    AdaptiveExecution, ExecutionPlan, ExecutionResult, ExecutionStrategy, ParallelExecution,
    SequentialExecution,
};
pub use memory::{
    MemoryEntry, MemoryQuery, MemoryResult, MemoryStrategy, RAGMemory, SlidingWindowMemory,
    SummarizingMemory,
};
pub use planning::{
    HierarchicalPlanning, Plan, PlanStep, PlanningContext, PlanningStrategy, ReActPlanning,
    SimplePlanning,
};
pub use reasoning::{
    ChainOfThought, ReActReasoning, ReasoningContext, ReasoningOutput, ReasoningStrategy,
    SimpleReasoning, TreeOfThought,
};
