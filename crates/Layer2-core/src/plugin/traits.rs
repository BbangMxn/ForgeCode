//! Plugin traits - 핵심 플러그인 인터페이스

use super::events::EventBus;
use super::manifest::PluginManifest;
use crate::skill::Skill;
use crate::tool::Tool;
use async_trait::async_trait;
use forge_foundation::Result;
use serde_json::Value;
use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

// ============================================================================
// PluginCapability - 플러그인 기능 열거
// ============================================================================

/// 플러그인이 제공할 수 있는 기능
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PluginCapability {
    /// 새로운 Tool 등록
    RegisterTools,

    /// 새로운 Skill 등록
    RegisterSkills,

    /// 시스템 프롬프트 수정
    ModifySystemPrompt,

    /// 이벤트 핸들링
    HandleEvents,

    /// 설정 관리
    ManageConfig,

    /// 상태 저장
    PersistState,
}

// ============================================================================
// PluginContext - 플러그인에 제공되는 컨텍스트
// ============================================================================

/// 플러그인 컨텍스트 - 플러그인이 ForgeCode와 상호작용하는 인터페이스
pub struct PluginContext {
    /// 등록할 Tool 목록
    registered_tools: RwLock<Vec<Arc<dyn Tool>>>,

    /// 등록할 Skill 목록
    registered_skills: RwLock<Vec<Arc<dyn Skill>>>,

    /// 이벤트 버스 (이벤트 발행/구독)
    event_bus: Arc<EventBus>,

    /// 플러그인 설정
    config: RwLock<HashMap<String, Value>>,

    /// 플러그인 상태 저장소
    state: RwLock<HashMap<String, Value>>,

    /// 작업 디렉토리
    working_dir: std::path::PathBuf,
}

impl PluginContext {
    /// 새 컨텍스트 생성
    pub fn new(event_bus: Arc<EventBus>, working_dir: std::path::PathBuf) -> Self {
        Self {
            registered_tools: RwLock::new(Vec::new()),
            registered_skills: RwLock::new(Vec::new()),
            event_bus,
            config: RwLock::new(HashMap::new()),
            state: RwLock::new(HashMap::new()),
            working_dir,
        }
    }

    // ========================================================================
    // Tool 등록
    // ========================================================================

    /// Tool 등록
    pub async fn register_tool(&self, tool: Arc<dyn Tool>) {
        let mut tools = self.registered_tools.write().await;
        tools.push(tool);
    }

    /// 등록된 Tool 목록 반환
    pub async fn take_tools(&self) -> Vec<Arc<dyn Tool>> {
        let mut tools = self.registered_tools.write().await;
        std::mem::take(&mut *tools)
    }

    // ========================================================================
    // Skill 등록
    // ========================================================================

    /// Skill 등록
    pub async fn register_skill(&self, skill: Arc<dyn Skill>) {
        let mut skills = self.registered_skills.write().await;
        skills.push(skill);
    }

    /// 등록된 Skill 목록 반환
    pub async fn take_skills(&self) -> Vec<Arc<dyn Skill>> {
        let mut skills = self.registered_skills.write().await;
        std::mem::take(&mut *skills)
    }

    // ========================================================================
    // 이벤트
    // ========================================================================

    /// 이벤트 버스 접근
    pub fn event_bus(&self) -> &Arc<EventBus> {
        &self.event_bus
    }

    // ========================================================================
    // 설정
    // ========================================================================

    /// 설정 값 가져오기
    pub async fn get_config(&self, key: &str) -> Option<Value> {
        let config = self.config.read().await;
        config.get(key).cloned()
    }

    /// 설정 값 설정
    pub async fn set_config(&self, key: impl Into<String>, value: Value) {
        let mut config = self.config.write().await;
        config.insert(key.into(), value);
    }

    /// 모든 설정 반환
    pub async fn all_config(&self) -> HashMap<String, Value> {
        let config = self.config.read().await;
        config.clone()
    }

    /// 설정 로드 (외부에서 주입)
    pub async fn load_config(&self, config: HashMap<String, Value>) {
        let mut current = self.config.write().await;
        *current = config;
    }

    // ========================================================================
    // 상태 저장
    // ========================================================================

    /// 상태 저장
    pub async fn save_state(&self, key: impl Into<String>, value: Value) {
        let mut state = self.state.write().await;
        state.insert(key.into(), value);
    }

    /// 상태 로드
    pub async fn load_state(&self, key: &str) -> Option<Value> {
        let state = self.state.read().await;
        state.get(key).cloned()
    }

    /// 전체 상태 반환 (영속화용)
    pub async fn all_state(&self) -> HashMap<String, Value> {
        let state = self.state.read().await;
        state.clone()
    }

    // ========================================================================
    // 유틸리티
    // ========================================================================

    /// 작업 디렉토리
    pub fn working_dir(&self) -> &std::path::Path {
        &self.working_dir
    }
}

// ============================================================================
// Plugin Trait - 모든 플러그인이 구현해야 하는 인터페이스
// ============================================================================

/// 플러그인 트레이트
///
/// 모든 ForgeCode 플러그인은 이 트레이트를 구현해야 합니다.
#[async_trait]
pub trait Plugin: Send + Sync {
    /// 플러그인 매니페스트 반환
    fn manifest(&self) -> PluginManifest;

    /// 플러그인이 제공하는 기능 목록
    fn capabilities(&self) -> Vec<PluginCapability> {
        vec![]
    }

    /// 플러그인 로드 시 호출
    ///
    /// 여기서 Tool, Skill 등을 등록합니다.
    async fn on_load(&self, ctx: &PluginContext) -> Result<()>;

    /// 플러그인 언로드 시 호출
    async fn on_unload(&self, _ctx: &PluginContext) -> Result<()> {
        Ok(())
    }

    /// 플러그인 활성화 시 호출
    async fn on_activate(&self, _ctx: &PluginContext) -> Result<()> {
        Ok(())
    }

    /// 플러그인 비활성화 시 호출
    async fn on_deactivate(&self, _ctx: &PluginContext) -> Result<()> {
        Ok(())
    }

    /// 시스템 프롬프트 수정 (해당하는 경우)
    fn modify_system_prompt(&self, _prompt: &str) -> Option<String> {
        None
    }

    /// 플러그인 상태 표시 (디버깅용)
    fn status(&self) -> PluginStatus {
        PluginStatus::Active
    }

    /// 타입 캐스팅을 위한 헬퍼 (다운캐스팅 지원)
    fn as_any(&self) -> &dyn Any;
}

/// 플러그인 상태
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginStatus {
    /// 로드됨 (아직 활성화 안됨)
    Loaded,

    /// 활성화됨
    Active,

    /// 비활성화됨
    Inactive,

    /// 오류 상태
    Error,
}

impl std::fmt::Display for PluginStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Loaded => write!(f, "loaded"),
            Self::Active => write!(f, "active"),
            Self::Inactive => write!(f, "inactive"),
            Self::Error => write!(f, "error"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::manifest::PluginProvides;

    struct TestPlugin;

    #[async_trait]
    impl Plugin for TestPlugin {
        fn manifest(&self) -> PluginManifest {
            PluginManifest::new("test.plugin", "Test Plugin")
                .with_provides(PluginProvides::new().with_tool("test_tool"))
        }

        fn capabilities(&self) -> Vec<PluginCapability> {
            vec![PluginCapability::RegisterTools]
        }

        async fn on_load(&self, _ctx: &PluginContext) -> Result<()> {
            Ok(())
        }

        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    #[tokio::test]
    async fn test_plugin_manifest() {
        let plugin = TestPlugin;
        let manifest = plugin.manifest();

        assert_eq!(manifest.id, "test.plugin");
        assert!(manifest.provides.tools.contains(&"test_tool".to_string()));
    }

    #[tokio::test]
    async fn test_plugin_context() {
        let event_bus = Arc::new(EventBus::new());
        let ctx = PluginContext::new(event_bus, std::path::PathBuf::from("/tmp"));

        ctx.set_config("key", serde_json::json!("value")).await;
        let value = ctx.get_config("key").await;

        assert_eq!(value, Some(serde_json::json!("value")));
    }
}
