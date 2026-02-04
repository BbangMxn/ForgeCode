//! # Agent Runtime System (DEPRECATED)
//!
//! **⚠️ DEPRECATED**: 이 모듈은 더 이상 사용되지 않습니다.
//! 새로운 단순화된 Agent Loop를 사용하세요:
//!
//! ```ignore
//! use forge_agent::{Agent, AgentConfig};
//! use forge_agent::hook::LoggingHook;
//!
//! let agent = Agent::with_config(ctx, AgentConfig::default())
//!     .with_hook(LoggingHook::new());
//! ```
//!
//! ## 이전 설명 (참고용)
//!
//! 모듈형 Agent 아키텍처의 핵심 런타임 시스템입니다.
//!
//! ## 설계 원칙
//!
//! macOS의 System Extension 모델처럼, Agent를 교체 가능한 컴포넌트로 구성합니다:
//!
//! - **AgentRuntime**: 모든 Agent가 구현해야 하는 공통 인터페이스
//! - **AgentPhase**: Agent 실행의 각 단계 (Think → Plan → Execute → Reflect)
//! - **AgentCapability**: Agent가 지원하는 기능 플래그
//!
//! ## 사용 예
//!
//! ```ignore
//! // 커스텀 Agent 구현
//! struct MyAgent {
//!     reasoning: Box<dyn ReasoningStrategy>,
//!     planning: Box<dyn PlanningStrategy>,
//! }
//!
//! #[async_trait]
//! impl AgentRuntime for MyAgent {
//!     async fn think(&self, ctx: &mut RuntimeContext) -> Result<ThinkOutput> { ... }
//!     async fn plan(&self, ctx: &mut RuntimeContext) -> Result<PlanOutput> { ... }
//!     async fn execute(&self, ctx: &mut RuntimeContext) -> Result<ExecuteOutput> { ... }
//! }
//! ```

mod context;
mod lifecycle;
mod output;
mod traits;

pub use context::{ContextSnapshot, RuntimeContext, TurnInfo};
pub use lifecycle::{AgentPhase, LifecycleEvent, LifecycleObserver, PhaseResult, PhaseTransition};
pub use output::{
    ActionItem, ActionType, ExecuteOutput, PlanOutput, ReflectOutput, ThinkOutput, ToolRequest,
};
pub use traits::{
    AgentCapability, AgentMetadata, AgentRuntime, AgentRuntimeExt, RuntimeConfig, RuntimeHooks,
};
