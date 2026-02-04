//! # Agent Variants (DEPRECATED)
//!
//! **⚠️ DEPRECATED**: 이 모듈은 더 이상 사용되지 않습니다.
//!
//! Claude Code / OpenCode / Gemini CLI 분석 결과:
//! - ReAct, Reflexion, TreeSearch 등 복잡한 Agent 변형은 실제로 사용되지 않음
//! - 단순한 single-threaded loop가 더 효율적이고 디버깅하기 쉬움
//!
//! 새로운 시스템 사용법:
//!
//! ```ignore
//! use forge_agent::{Agent, AgentConfig};
//!
//! let agent = Agent::with_config(ctx, AgentConfig::default());
//! ```
//!
//! ## 이전 설명 (참고용)
//!
//! 미리 정의된 Agent 변형들입니다.
//!
//! 각 변형은 특정 전략 조합으로 구성되어 있으며,
//! 다양한 사용 사례에 최적화되어 있습니다.
//!
//! ## 사용 가능한 변형
//!
//! - **ClassicAgent**: 기본 Agent (단순 추론 + 순차 실행)
//! - **ReActAgent**: ReAct 패턴 (Reasoning + Acting 반복)
//! - **ReflexionAgent**: Reflexion 패턴 (자기 반성 포함)
//! - **TreeSearchAgent**: Tree-of-Thought 탐색
//! - **ComposableAgent**: 사용자 정의 전략 조합

mod builder;
mod classic;
mod react;
mod reflexion;
mod registry;
mod tree_search;

pub use builder::AgentBuilder;
pub use classic::ClassicAgent;
pub use react::ReActAgent;
pub use reflexion::ReflexionAgent;
pub use registry::{AgentRegistry, AgentVariantInfo, VariantCategory};
pub use tree_search::TreeSearchAgent;

/// 기본 Agent Registry 생성 (모든 내장 변형 등록)
pub fn create_default_registry() -> AgentRegistry {
    let mut registry = AgentRegistry::new();

    // 내장 변형 등록
    registry.register_builtin::<ClassicAgent>();
    registry.register_builtin::<ReActAgent>();
    registry.register_builtin::<ReflexionAgent>();
    registry.register_builtin::<TreeSearchAgent>();

    registry
}
