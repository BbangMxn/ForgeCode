//! # Plugin System
//!
//! ForgeCode 확장 플러그인 시스템
//!
//! ## 개요
//!
//! Plugin 시스템을 통해 ForgeCode의 기능을 동적으로 확장할 수 있습니다:
//! - 새로운 Tool 추가
//! - 새로운 Skill 추가
//! - 시스템 프롬프트 수정
//! - 이벤트 훅 등록
//!
//! ## 아키텍처
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                     PluginManager                           │
//! │  ┌───────────────────────────────────────────────────────┐ │
//! │  │                   PluginRegistry                       │ │
//! │  │  ┌────────────┬────────────┬────────────────────┐    │ │
//! │  │  │ Plugin A   │ Plugin B   │ Plugin C (wasm)    │    │ │
//! │  │  │ (native)   │ (native)   │                    │    │ │
//! │  │  └────────────┴────────────┴────────────────────┘    │ │
//! │  └───────────────────────────────────────────────────────┘ │
//! │                          │                                  │
//! │  ┌───────────────────────┼───────────────────────────────┐ │
//! │  │     PluginContext     │                               │ │
//! │  │  - ToolRegistry       │                               │ │
//! │  │  - SkillRegistry      │                               │ │
//! │  │  - EventBus           │                               │ │
//! │  │  - ConfigStore        │                               │ │
//! │  └───────────────────────────────────────────────────────┘ │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## 플러그인 타입
//!
//! 1. **Native Plugin**: Rust로 작성된 컴파일타임 플러그인
//! 2. **WASM Plugin**: WebAssembly 기반 런타임 플러그인 (향후)
//! 3. **Script Plugin**: JavaScript/Lua 스크립트 플러그인 (향후)
//!
//! ## 예시
//!
//! ```ignore
//! // 플러그인 정의
//! struct MyPlugin;
//!
//! impl Plugin for MyPlugin {
//!     fn manifest(&self) -> PluginManifest { ... }
//!     fn on_load(&self, ctx: &PluginContext) -> Result<()> {
//!         ctx.register_tool(Arc::new(MyTool::new()));
//!         ctx.register_skill(Arc::new(MySkill::new()));
//!         Ok(())
//!     }
//! }
//!
//! // 플러그인 등록
//! let mut manager = PluginManager::new();
//! manager.load(Arc::new(MyPlugin));
//! ```

mod traits;
mod registry;
mod manager;
mod manifest;
mod events;
mod store;
mod discovery;
mod installer;

pub use traits::{Plugin, PluginContext, PluginCapability};
pub use registry::PluginRegistry;
pub use manager::{PluginManager, PluginManagerConfig, PluginSummary};
pub use manifest::{PluginManifest, PluginVersion, PluginDependency, PluginProvides};
pub use events::{PluginEvent, PluginEventHandler, EventBus};
pub use store::{PluginStore, InstalledPlugin};
pub use discovery::{PluginDiscovery, DiscoveredPlugin, PluginScope};
pub use installer::{PluginInstaller, PluginSource};
