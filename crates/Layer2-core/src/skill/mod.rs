//! # Skill System
//!
//! Claude Code 스타일의 Skills 시스템 구현
//!
//! ## 개요
//!
//! Skills는 `/commit`, `/review-pr` 같은 슬래시 명령어로 호출되는 고수준 작업입니다.
//! Tool과 달리 Skill은:
//! - 사용자가 직접 호출 (LLM이 아닌)
//! - 복잡한 다단계 워크플로우 수행
//! - 여러 Tool을 조합하여 사용
//! - 전용 시스템 프롬프트 제공
//!
//! ## 아키텍처
//!
//! ```text
//! ┌─────────────────────────────────────────────────────┐
//! │                   SkillRegistry                      │
//! │  ┌─────────────┬─────────────┬─────────────────┐   │
//! │  │  /commit    │  /review-pr │  /explain       │   │
//! │  │  (builtin)  │  (builtin)  │  (plugin)       │   │
//! │  └─────────────┴─────────────┴─────────────────┘   │
//! │                        │                            │
//! │              SkillExecutor (Agent Loop)             │
//! │                        │                            │
//! │              ToolRegistry (for tool calls)          │
//! └─────────────────────────────────────────────────────┘
//! ```
//!
//! ## 예시
//!
//! ```ignore
//! // 스킬 등록
//! let mut registry = SkillRegistry::new();
//! registry.register(Arc::new(CommitSkill::new()));
//!
//! // 스킬 실행
//! let skill = registry.get("/commit").unwrap();
//! let result = skill.execute(ctx, "/commit -m 'fix bug'").await?;
//! ```

mod registry;
mod traits;
mod loader;
mod store;
mod installer;
mod marketplace;
mod manager;

pub use registry::SkillRegistry;
pub use traits::{
    Skill, SkillArgument, SkillContext, SkillDefinition, SkillInput, SkillOutput, SkillMetadata,
    GitInfo, SkillAction,
};

// File-based skill loader (Claude Code compatible)
pub use loader::{SkillLoader, FileBasedSkill, SkillConfig};

// Skill store and installer (easy replacement!)
pub use store::{SkillStore, InstalledSkill};
pub use installer::{SkillInstaller, SkillSource};

// Marketplace (community skills)
pub use marketplace::{SkillMarketplace, MarketplaceSkill, MarketplaceRegistry, MarketplaceStats};

// Unified skill manager (easy API!)
pub use manager::SkillManager;

// Built-in skills
pub mod builtin;
pub use builtin::{CommitSkill, ReviewPrSkill, ExplainSkill};
