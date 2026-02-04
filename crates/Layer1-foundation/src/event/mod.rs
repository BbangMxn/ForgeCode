//! Event System - 이벤트 발행/구독 시스템
//!
//! ForgeCode의 모든 레이어에서 발생하는 이벤트를 관리합니다.
//!
//! ## 아키텍처
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                        EventBus                              │
//! │  ┌─────────────────────────────────────────────────────┐    │
//! │  │  publish(event) ──────────────────────────────────┐ │    │
//! │  └─────────────────────────────────────────────────────┘    │
//! │         │                                                   │
//! │         ▼                                                   │
//! │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
//! │  │  Listener 1  │  │  Listener 2  │  │  Listener N  │      │
//! │  │  (AuditLog)  │  │  (Metrics)   │  │  (UI)        │      │
//! │  └──────────────┘  └──────────────┘  └──────────────┘      │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## 사용법
//!
//! ```ignore
//! use forge_foundation::event::{
//!     EventBus, ForgeEvent, EventCategory, EventListener,
//!     global_event_bus, publish,
//! };
//!
//! // 1. 리스너 구현
//! struct MyListener;
//!
//! #[async_trait]
//! impl EventListener for MyListener {
//!     fn name(&self) -> &str { "my_listener" }
//!
//!     async fn on_event(&self, event: &ForgeEvent) {
//!         println!("Received: {}", event.event_type);
//!     }
//! }
//!
//! // 2. 리스너 등록
//! let bus = global_event_bus();
//! bus.subscribe(Arc::new(MyListener)).await;
//!
//! // 3. 이벤트 발행
//! publish(ForgeEvent::new("test.event", EventCategory::System)).await;
//!
//! // 4. 사전 정의된 이벤트 사용
//! use forge_foundation::event::types::{tool, permission};
//!
//! publish(tool::completed("read", true, 150)).await;
//! publish(permission::granted("bash", "execute", "session")).await;
//! ```

pub mod bus;
pub mod types;

// Re-exports
pub use bus::{
    // Global functions
    global_event_bus,
    init_global_event_bus,
    publish,
    // EventBus
    EventBus,
    EventBusConfig,
    EventFilter,
    EventListener,
    ListenerId,
};

pub use types::{
    // Event constructors
    error,
    permission,
    session,
    system,
    tool,
    // Core types
    EventCategory,
    EventId,
    EventSeverity,
    ForgeEvent,
};
