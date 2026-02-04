//! Event Bus - 이벤트 브로드캐스트 시스템
//!
//! 비동기 이벤트 발행/구독 시스템을 제공합니다.

use super::types::{EventCategory, ForgeEvent};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, trace};

// ============================================================================
// EventListener Trait
// ============================================================================

/// 이벤트 리스너 ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ListenerId(u64);

impl ListenerId {
    fn new(id: u64) -> Self {
        Self(id)
    }
}

impl std::fmt::Display for ListenerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "listener-{}", self.0)
    }
}

/// 이벤트 리스너 trait
///
/// 이벤트를 수신하고 처리하는 컴포넌트가 구현합니다.
#[async_trait]
pub trait EventListener: Send + Sync {
    /// 리스너 이름 (디버깅용)
    fn name(&self) -> &str;

    /// 관심 있는 이벤트 카테고리 (None이면 모든 이벤트)
    fn categories(&self) -> Option<Vec<EventCategory>> {
        None
    }

    /// 이벤트 처리
    async fn on_event(&self, event: &ForgeEvent);
}

// ============================================================================
// EventFilter
// ============================================================================

/// 이벤트 필터
#[derive(Debug, Clone, Default)]
pub struct EventFilter {
    /// 카테고리 필터
    pub categories: Option<Vec<EventCategory>>,

    /// 이벤트 타입 패턴 (prefix 매칭)
    pub event_types: Option<Vec<String>>,

    /// 소스 필터
    pub sources: Option<Vec<String>>,

    /// 최소 심각도
    pub min_severity: Option<super::types::EventSeverity>,
}

impl EventFilter {
    /// 새 필터 생성
    pub fn new() -> Self {
        Self::default()
    }

    /// 카테고리 필터 추가
    pub fn with_categories(mut self, categories: Vec<EventCategory>) -> Self {
        self.categories = Some(categories);
        self
    }

    /// 이벤트 타입 필터 추가
    pub fn with_event_types(mut self, types: Vec<String>) -> Self {
        self.event_types = Some(types);
        self
    }

    /// 소스 필터 추가
    pub fn with_sources(mut self, sources: Vec<String>) -> Self {
        self.sources = Some(sources);
        self
    }

    /// 이벤트가 필터를 통과하는지 확인
    pub fn matches(&self, event: &ForgeEvent) -> bool {
        // 카테고리 체크
        if let Some(ref cats) = self.categories {
            if !cats.contains(&event.category) {
                return false;
            }
        }

        // 이벤트 타입 체크 (prefix 매칭)
        if let Some(ref types) = self.event_types {
            let matches = types.iter().any(|t| event.event_type.starts_with(t));
            if !matches {
                return false;
            }
        }

        // 소스 체크
        if let Some(ref sources) = self.sources {
            if !sources.contains(&event.source) {
                return false;
            }
        }

        // 심각도 체크
        if let Some(min_sev) = self.min_severity {
            if event.severity < min_sev {
                return false;
            }
        }

        true
    }
}

// ============================================================================
// EventBus
// ============================================================================

/// 이벤트 버스 설정
#[derive(Debug, Clone)]
pub struct EventBusConfig {
    /// 브로드캐스트 채널 용량
    pub channel_capacity: usize,

    /// 이벤트 히스토리 보관 개수
    pub history_size: usize,

    /// 디버그 모드 (모든 이벤트 로깅)
    pub debug_mode: bool,
}

impl Default for EventBusConfig {
    fn default() -> Self {
        Self {
            channel_capacity: 1024,
            history_size: 100,
            debug_mode: false,
        }
    }
}

/// 등록된 리스너 정보
struct RegisteredListener {
    listener: Arc<dyn EventListener>,
    filter: Option<EventFilter>,
}

/// 이벤트 버스
///
/// 시스템 전체의 이벤트를 브로드캐스트합니다.
///
/// ## 사용법
///
/// ```ignore
/// use forge_foundation::event::{EventBus, ForgeEvent, EventCategory};
///
/// // 이벤트 버스 생성
/// let bus = EventBus::new();
///
/// // 리스너 등록
/// let id = bus.subscribe(my_listener).await;
///
/// // 이벤트 발행
/// bus.publish(ForgeEvent::new("test.event", EventCategory::System)).await;
///
/// // 리스너 해제
/// bus.unsubscribe(id).await;
/// ```
pub struct EventBus {
    /// 설정
    config: EventBusConfig,

    /// 브로드캐스트 채널 송신자
    sender: broadcast::Sender<ForgeEvent>,

    /// 등록된 리스너
    listeners: RwLock<HashMap<ListenerId, RegisteredListener>>,

    /// 리스너 ID 카운터
    listener_counter: AtomicU64,

    /// 이벤트 히스토리
    history: RwLock<Vec<ForgeEvent>>,

    /// 발행된 이벤트 수
    event_count: AtomicU64,
}

impl EventBus {
    /// 기본 설정으로 이벤트 버스 생성
    pub fn new() -> Self {
        Self::with_config(EventBusConfig::default())
    }

    /// 커스텀 설정으로 이벤트 버스 생성
    pub fn with_config(config: EventBusConfig) -> Self {
        let (sender, _) = broadcast::channel(config.channel_capacity);

        Self {
            config,
            sender,
            listeners: RwLock::new(HashMap::new()),
            listener_counter: AtomicU64::new(0),
            history: RwLock::new(Vec::new()),
            event_count: AtomicU64::new(0),
        }
    }

    /// 리스너 등록
    pub async fn subscribe(&self, listener: Arc<dyn EventListener>) -> ListenerId {
        self.subscribe_with_filter(listener, None).await
    }

    /// 필터와 함께 리스너 등록
    pub async fn subscribe_with_filter(
        &self,
        listener: Arc<dyn EventListener>,
        filter: Option<EventFilter>,
    ) -> ListenerId {
        let id = ListenerId::new(self.listener_counter.fetch_add(1, Ordering::SeqCst));

        debug!(
            listener_name = listener.name(),
            listener_id = %id,
            "Registering event listener"
        );

        let mut listeners = self.listeners.write().await;
        listeners.insert(id, RegisteredListener { listener, filter });

        id
    }

    /// 리스너 해제
    pub async fn unsubscribe(&self, id: ListenerId) -> bool {
        let mut listeners = self.listeners.write().await;
        let removed = listeners.remove(&id).is_some();

        if removed {
            debug!(listener_id = %id, "Unregistered event listener");
        }

        removed
    }

    /// 이벤트 발행
    pub async fn publish(&self, event: ForgeEvent) {
        let event_count = self.event_count.fetch_add(1, Ordering::SeqCst);

        if self.config.debug_mode {
            trace!(
                event_id = %event.id,
                event_type = %event.event_type,
                category = ?event.category,
                "Publishing event #{}", event_count + 1
            );
        }

        // 히스토리에 추가
        {
            let mut history = self.history.write().await;
            history.push(event.clone());

            // 히스토리 크기 제한
            if history.len() > self.config.history_size {
                history.remove(0);
            }
        }

        // 브로드캐스트 채널로 전송
        let _ = self.sender.send(event.clone());

        // 등록된 리스너들에게 전달
        let listeners = self.listeners.read().await;
        for (id, registered) in listeners.iter() {
            // 필터 체크
            let should_deliver = match &registered.filter {
                Some(filter) => filter.matches(&event),
                None => {
                    // 리스너의 카테고리 필터 체크
                    match registered.listener.categories() {
                        Some(cats) => cats.contains(&event.category),
                        None => true,
                    }
                }
            };

            if should_deliver {
                trace!(
                    listener_id = %id,
                    listener_name = registered.listener.name(),
                    event_type = %event.event_type,
                    "Delivering event to listener"
                );

                registered.listener.on_event(&event).await;
            }
        }
    }

    /// 브로드캐스트 수신자 생성 (스트림 방식)
    pub fn receiver(&self) -> broadcast::Receiver<ForgeEvent> {
        self.sender.subscribe()
    }

    /// 최근 이벤트 히스토리 조회
    pub async fn history(&self, limit: Option<usize>) -> Vec<ForgeEvent> {
        let history = self.history.read().await;
        let limit = limit.unwrap_or(history.len());
        history.iter().rev().take(limit).cloned().collect()
    }

    /// 필터로 히스토리 검색
    pub async fn search_history(&self, filter: &EventFilter) -> Vec<ForgeEvent> {
        let history = self.history.read().await;
        history
            .iter()
            .filter(|e| filter.matches(e))
            .cloned()
            .collect()
    }

    /// 등록된 리스너 수
    pub async fn listener_count(&self) -> usize {
        self.listeners.read().await.len()
    }

    /// 총 발행된 이벤트 수
    pub fn event_count(&self) -> u64 {
        self.event_count.load(Ordering::SeqCst)
    }

    /// 히스토리 클리어
    pub async fn clear_history(&self) {
        let mut history = self.history.write().await;
        history.clear();
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// 전역 EventBus
// ============================================================================

use std::sync::OnceLock;

static GLOBAL_EVENT_BUS: OnceLock<Arc<EventBus>> = OnceLock::new();

/// 전역 이벤트 버스 초기화
pub fn init_global_event_bus(config: EventBusConfig) -> Arc<EventBus> {
    GLOBAL_EVENT_BUS
        .get_or_init(|| Arc::new(EventBus::with_config(config)))
        .clone()
}

/// 전역 이벤트 버스 가져오기
pub fn global_event_bus() -> Arc<EventBus> {
    GLOBAL_EVENT_BUS
        .get_or_init(|| Arc::new(EventBus::new()))
        .clone()
}

/// 전역 이벤트 발행 (편의 함수)
pub async fn publish(event: ForgeEvent) {
    global_event_bus().publish(event).await;
}

// ============================================================================
// 테스트
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    struct TestListener {
        name: String,
        count: AtomicUsize,
    }

    impl TestListener {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
                count: AtomicUsize::new(0),
            }
        }

        fn call_count(&self) -> usize {
            self.count.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl EventListener for TestListener {
        fn name(&self) -> &str {
            &self.name
        }

        async fn on_event(&self, _event: &ForgeEvent) {
            self.count.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn test_event_bus_basic() {
        let bus = EventBus::new();

        let listener = Arc::new(TestListener::new("test"));
        let id = bus.subscribe(listener.clone()).await;

        assert_eq!(bus.listener_count().await, 1);

        // 이벤트 발행
        let event = ForgeEvent::new("test.event", EventCategory::System);
        bus.publish(event).await;

        assert_eq!(listener.call_count(), 1);

        // 리스너 해제
        bus.unsubscribe(id).await;
        assert_eq!(bus.listener_count().await, 0);
    }

    #[tokio::test]
    async fn test_event_filter() {
        let filter = EventFilter::new()
            .with_categories(vec![EventCategory::Tool])
            .with_event_types(vec!["tool.".to_string()]);

        let tool_event = ForgeEvent::new("tool.completed", EventCategory::Tool);
        let system_event = ForgeEvent::new("system.started", EventCategory::System);

        assert!(filter.matches(&tool_event));
        assert!(!filter.matches(&system_event));
    }

    #[tokio::test]
    async fn test_event_history() {
        let config = EventBusConfig {
            history_size: 5,
            ..Default::default()
        };
        let bus = EventBus::with_config(config);

        // 10개 이벤트 발행
        for i in 0..10 {
            let event = ForgeEvent::new(format!("test.event.{}", i), EventCategory::System);
            bus.publish(event).await;
        }

        // 히스토리는 최근 5개만 유지
        let history = bus.history(None).await;
        assert_eq!(history.len(), 5);
    }
}
