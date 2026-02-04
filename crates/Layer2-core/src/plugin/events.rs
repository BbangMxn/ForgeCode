//! Plugin Events - 이벤트 시스템

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::debug;

// ============================================================================
// PluginEvent - 플러그인 이벤트 타입
// ============================================================================

/// 플러그인 이벤트
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginEvent {
    /// 이벤트 타입
    pub event_type: EventType,

    /// 이벤트 데이터
    pub data: Value,

    /// 타임스탬프
    pub timestamp: chrono::DateTime<chrono::Utc>,

    /// 소스 (이벤트 발생 위치)
    pub source: String,
}

impl PluginEvent {
    /// 새 이벤트 생성
    pub fn new(event_type: EventType, data: Value, source: impl Into<String>) -> Self {
        Self {
            event_type,
            data,
            timestamp: chrono::Utc::now(),
            source: source.into(),
        }
    }

    /// 간단한 이벤트 생성
    pub fn simple(event_type: EventType) -> Self {
        Self::new(event_type, Value::Null, "system")
    }
}

/// 이벤트 타입
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    // 세션 이벤트
    SessionStart,
    SessionEnd,

    // 메시지 이벤트
    MessageReceived,
    MessageSent,

    // Tool 이벤트
    ToolCallStart,
    ToolCallEnd,
    ToolCallError,

    // Skill 이벤트
    SkillInvoked,
    SkillCompleted,
    SkillError,

    // Agent 이벤트
    AgentThinking,
    AgentResponse,

    // 파일 이벤트
    FileRead,
    FileWrite,
    FileDelete,

    // Git 이벤트
    GitCommit,
    GitPush,
    GitPull,

    // 시스템 이벤트
    PluginLoaded,
    PluginUnloaded,
    ConfigChanged,

    // 사용자 정의 이벤트
    Custom,
}

impl std::fmt::Display for EventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SessionStart => write!(f, "session_start"),
            Self::SessionEnd => write!(f, "session_end"),
            Self::MessageReceived => write!(f, "message_received"),
            Self::MessageSent => write!(f, "message_sent"),
            Self::ToolCallStart => write!(f, "tool_call_start"),
            Self::ToolCallEnd => write!(f, "tool_call_end"),
            Self::ToolCallError => write!(f, "tool_call_error"),
            Self::SkillInvoked => write!(f, "skill_invoked"),
            Self::SkillCompleted => write!(f, "skill_completed"),
            Self::SkillError => write!(f, "skill_error"),
            Self::AgentThinking => write!(f, "agent_thinking"),
            Self::AgentResponse => write!(f, "agent_response"),
            Self::FileRead => write!(f, "file_read"),
            Self::FileWrite => write!(f, "file_write"),
            Self::FileDelete => write!(f, "file_delete"),
            Self::GitCommit => write!(f, "git_commit"),
            Self::GitPush => write!(f, "git_push"),
            Self::GitPull => write!(f, "git_pull"),
            Self::PluginLoaded => write!(f, "plugin_loaded"),
            Self::PluginUnloaded => write!(f, "plugin_unloaded"),
            Self::ConfigChanged => write!(f, "config_changed"),
            Self::Custom => write!(f, "custom"),
        }
    }
}

// ============================================================================
// PluginEventHandler - 이벤트 핸들러 트레이트
// ============================================================================

/// 이벤트 핸들러 트레이트
#[async_trait]
pub trait PluginEventHandler: Send + Sync {
    /// 핸들러 이름
    fn name(&self) -> &str;

    /// 관심 있는 이벤트 타입들
    fn interested_events(&self) -> Vec<EventType>;

    /// 이벤트 처리
    async fn handle(&self, event: &PluginEvent);
}

// ============================================================================
// EventBus - 이벤트 버스 (발행/구독)
// ============================================================================

/// 이벤트 버스 - 이벤트 발행 및 구독 관리
pub struct EventBus {
    /// 브로드캐스트 채널 발신자
    sender: broadcast::Sender<PluginEvent>,

    /// 등록된 핸들러
    handlers: RwLock<HashMap<String, Arc<dyn PluginEventHandler>>>,

    /// 이벤트 히스토리 (최근 N개)
    history: RwLock<Vec<PluginEvent>>,

    /// 히스토리 최대 크기
    history_size: usize,
}

impl EventBus {
    /// 새 이벤트 버스 생성
    pub fn new() -> Self {
        Self::with_capacity(1024, 100)
    }

    /// 용량 지정하여 생성
    pub fn with_capacity(channel_capacity: usize, history_size: usize) -> Self {
        let (sender, _) = broadcast::channel(channel_capacity);
        Self {
            sender,
            handlers: RwLock::new(HashMap::new()),
            history: RwLock::new(Vec::with_capacity(history_size)),
            history_size,
        }
    }

    /// 이벤트 핸들러 등록
    pub async fn register_handler(&self, handler: Arc<dyn PluginEventHandler>) {
        let name = handler.name().to_string();
        let mut handlers = self.handlers.write().await;
        handlers.insert(name, handler);
    }

    /// 이벤트 핸들러 제거
    pub async fn unregister_handler(&self, name: &str) {
        let mut handlers = self.handlers.write().await;
        handlers.remove(name);
    }

    /// 이벤트 발행
    pub async fn publish(&self, event: PluginEvent) {
        debug!("Publishing event: {:?}", event.event_type);

        // 히스토리에 추가
        {
            let mut history = self.history.write().await;
            if history.len() >= self.history_size {
                history.remove(0);
            }
            history.push(event.clone());
        }

        // 브로드캐스트 (구독자가 없어도 OK)
        let _ = self.sender.send(event.clone());

        // 핸들러 호출
        let handlers = self.handlers.read().await;
        for handler in handlers.values() {
            if handler.interested_events().contains(&event.event_type) {
                handler.handle(&event).await;
            }
        }
    }

    /// 이벤트 구독 (스트림 반환)
    pub fn subscribe(&self) -> broadcast::Receiver<PluginEvent> {
        self.sender.subscribe()
    }

    /// 특정 타입의 이벤트만 구독
    ///
    /// 반환된 receiver를 사용하여 이벤트를 수신하고,
    /// 필터링은 호출자가 직접 수행해야 합니다.
    pub fn subscribe_filtered(&self, _filter: Vec<EventType>) -> broadcast::Receiver<PluginEvent> {
        // Note: 필터링은 호출자가 직접 수행
        // tokio_stream 의존성을 피하기 위해 단순화
        self.sender.subscribe()
    }

    /// 이벤트 히스토리 조회
    pub async fn history(&self) -> Vec<PluginEvent> {
        let history = self.history.read().await;
        history.clone()
    }

    /// 특정 타입의 이벤트 히스토리 조회
    pub async fn history_by_type(&self, event_type: EventType) -> Vec<PluginEvent> {
        let history = self.history.read().await;
        history
            .iter()
            .filter(|e| e.event_type == event_type)
            .cloned()
            .collect()
    }

    /// 히스토리 클리어
    pub async fn clear_history(&self) {
        let mut history = self.history.write().await;
        history.clear();
    }

    /// 등록된 핸들러 수
    pub async fn handler_count(&self) -> usize {
        let handlers = self.handlers.read().await;
        handlers.len()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// 헬퍼 함수
// ============================================================================

/// Tool 호출 시작 이벤트 생성
#[allow(dead_code)]
pub fn tool_call_start_event(tool_name: &str, args: &Value) -> PluginEvent {
    PluginEvent::new(
        EventType::ToolCallStart,
        serde_json::json!({
            "tool_name": tool_name,
            "arguments": args,
        }),
        "agent",
    )
}

/// Tool 호출 완료 이벤트 생성
#[allow(dead_code)]
pub fn tool_call_end_event(tool_name: &str, result: &str, success: bool) -> PluginEvent {
    PluginEvent::new(
        EventType::ToolCallEnd,
        serde_json::json!({
            "tool_name": tool_name,
            "result": result,
            "success": success,
        }),
        "agent",
    )
}

/// Skill 호출 이벤트 생성
#[allow(dead_code)]
pub fn skill_invoked_event(skill_name: &str, command: &str) -> PluginEvent {
    PluginEvent::new(
        EventType::SkillInvoked,
        serde_json::json!({
            "skill_name": skill_name,
            "command": command,
        }),
        "user",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestHandler {
        name: String,
    }

    #[async_trait]
    impl PluginEventHandler for TestHandler {
        fn name(&self) -> &str {
            &self.name
        }

        fn interested_events(&self) -> Vec<EventType> {
            vec![EventType::ToolCallStart, EventType::ToolCallEnd]
        }

        async fn handle(&self, _event: &PluginEvent) {
            // Test handler
        }
    }

    #[tokio::test]
    async fn test_event_bus() {
        let bus = EventBus::new();

        // 핸들러 등록
        let handler = Arc::new(TestHandler {
            name: "test".into(),
        });
        bus.register_handler(handler).await;

        assert_eq!(bus.handler_count().await, 1);

        // 이벤트 발행
        bus.publish(PluginEvent::simple(EventType::ToolCallStart))
            .await;

        // 히스토리 확인
        let history = bus.history().await;
        assert_eq!(history.len(), 1);
    }

    #[tokio::test]
    async fn test_event_subscribe() {
        let bus = EventBus::new();
        let mut receiver = bus.subscribe();

        // 백그라운드에서 이벤트 발행
        let bus_clone = bus.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            bus_clone
                .publish(PluginEvent::simple(EventType::SessionStart))
                .await;
        });

        // 이벤트 수신
        let event = receiver.recv().await.unwrap();
        assert_eq!(event.event_type, EventType::SessionStart);
    }
}

impl Clone for EventBus {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            handlers: RwLock::new(HashMap::new()), // 핸들러는 복제 안함
            history: RwLock::new(Vec::new()),
            history_size: self.history_size,
        }
    }
}
