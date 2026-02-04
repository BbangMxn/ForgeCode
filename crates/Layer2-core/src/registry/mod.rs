//! # Dynamic Registry System
//!
//! 동적으로 교체/변경 가능한 Registry 시스템
//!
//! ## 개요
//!
//! 이 모듈은 Tool, Skill, Plugin을 런타임에 동적으로 추가/제거/교체할 수 있는
//! Thread-safe한 Registry 시스템을 제공합니다.
//!
//! ## 설계 원칙
//!
//! 1. **Interior Mutability**: RwLock을 사용하여 Arc 내부에서도 변경 가능
//! 2. **Hot-reload**: 런타임에 Plugin/Skill 교체 지원
//! 3. **Event-driven**: 변경 시 이벤트 발행으로 리스너에게 통보
//! 4. **Version Control**: 변경 이력 추적 및 롤백 지원
//!
//! ## 아키텍처
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                  DynamicRegistry<T>                          │
//! │  ┌─────────────────────────────────────────────────────────┐│
//! │  │  RwLock<HashMap<String, RegistryEntry<T>>>              ││
//! │  │  ┌──────────┬──────────┬──────────┬──────────┐         ││
//! │  │  │ Entry 1  │ Entry 2  │ Entry 3  │ ...      │         ││
//! │  │  │ (v1.0.0) │ (v2.1.0) │ (v1.0.0) │          │         ││
//! │  │  └──────────┴──────────┴──────────┴──────────┘         ││
//! │  └─────────────────────────────────────────────────────────┘│
//! │                          │                                   │
//! │  ┌───────────────────────┼───────────────────────────────┐  │
//! │  │      ChangeNotifier   │                               │  │
//! │  │  - on_register        │                               │  │
//! │  │  - on_unregister      │                               │  │
//! │  │  - on_replace         │                               │  │
//! │  └───────────────────────────────────────────────────────┘  │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## 사용 예시
//!
//! ```ignore
//! use forge_core::registry::{DynamicToolRegistry, DynamicSkillRegistry};
//!
//! // Tool Registry
//! let tools = DynamicToolRegistry::with_builtins();
//!
//! // 런타임에 Tool 추가
//! tools.register(Arc::new(MyTool::new())).await;
//!
//! // 런타임에 Tool 교체
//! tools.replace("my_tool", Arc::new(MyToolV2::new())).await;
//!
//! // 변경 구독
//! let mut rx = tools.subscribe();
//! while let Ok(event) = rx.recv().await {
//!     match event {
//!         RegistryEvent::Registered(name) => println!("Added: {}", name),
//!         RegistryEvent::Replaced(name) => println!("Replaced: {}", name),
//!         _ => {}
//!     }
//! }
//! ```

mod traits;
mod dynamic;
mod entry;
mod snapshot;

pub use traits::{Registerable, RegistryEvent, RegistryEventHandler};
pub use dynamic::{DynamicRegistry, DynamicToolRegistry, DynamicSkillRegistry, RegistryStats};
pub use entry::{RegistryEntry, EntryMetadata, EntryState};
pub use snapshot::{
    RegistrySnapshot, SnapshotInfo, SnapshotManager,
    HotReloadState, HotReloadResult, HotReloadConfig,
};
