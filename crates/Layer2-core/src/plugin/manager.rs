//! Plugin Manager - 플러그인 라이프사이클 관리
//!
//! ## 개선된 기능
//!
//! - DynamicToolRegistry/DynamicSkillRegistry 사용으로 동적 Tool/Skill 등록
//! - PluginStore를 통한 플러그인 영속화
//! - 플러그인 발견 및 자동 로드

use super::events::EventBus;
use super::registry::PluginRegistry;
use super::traits::{Plugin, PluginContext, PluginStatus};
use crate::registry::{DynamicToolRegistry, DynamicSkillRegistry};
use forge_foundation::{Error, Result};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

/// 플러그인 매니저 설정
#[derive(Debug, Clone)]
pub struct PluginManagerConfig {
    /// 플러그인 검색 경로
    pub plugin_paths: Vec<PathBuf>,

    /// 자동 로드 활성화
    pub auto_load: bool,

    /// 오류 시 계속 진행
    pub continue_on_error: bool,
}

impl Default for PluginManagerConfig {
    fn default() -> Self {
        Self {
            plugin_paths: vec![],
            auto_load: true,
            continue_on_error: true,
        }
    }
}

/// 플러그인 매니저 - 전체 플러그인 시스템 관리
pub struct PluginManager {
    /// 플러그인 레지스트리
    registry: Arc<PluginRegistry>,

    /// Tool 레지스트리 (동적 등록 지원)
    tool_registry: Arc<DynamicToolRegistry>,

    /// Skill 레지스트리 (동적 등록 지원)
    skill_registry: Arc<DynamicSkillRegistry>,

    /// 이벤트 버스
    event_bus: Arc<EventBus>,

    /// 작업 디렉토리
    working_dir: PathBuf,

    /// 설정
    config: PluginManagerConfig,
}

impl PluginManager {
    /// 새 매니저 생성
    pub fn new(working_dir: PathBuf) -> Self {
        Self {
            registry: Arc::new(PluginRegistry::new()),
            tool_registry: Arc::new(DynamicToolRegistry::new()),
            skill_registry: Arc::new(DynamicSkillRegistry::new()),
            event_bus: Arc::new(EventBus::new()),
            working_dir,
            config: PluginManagerConfig::default(),
        }
    }

    /// 설정으로 생성
    pub fn with_config(working_dir: PathBuf, config: PluginManagerConfig) -> Self {
        Self {
            registry: Arc::new(PluginRegistry::new()),
            tool_registry: Arc::new(DynamicToolRegistry::new()),
            skill_registry: Arc::new(DynamicSkillRegistry::new()),
            event_bus: Arc::new(EventBus::new()),
            working_dir,
            config,
        }
    }

    /// 기존 레지스트리들과 함께 생성
    pub fn with_registries(
        working_dir: PathBuf,
        tool_registry: Arc<DynamicToolRegistry>,
        skill_registry: Arc<DynamicSkillRegistry>,
    ) -> Self {
        Self {
            registry: Arc::new(PluginRegistry::new()),
            tool_registry,
            skill_registry,
            event_bus: Arc::new(EventBus::new()),
            working_dir,
            config: PluginManagerConfig::default(),
        }
    }

    // ========================================================================
    // 플러그인 로드/언로드
    // ========================================================================

    /// 플러그인 로드
    pub async fn load(&self, plugin: Arc<dyn Plugin>) -> Result<()> {
        let manifest = plugin.manifest();
        let id = manifest.id.clone();

        info!("Loading plugin: {} (v{})", id, manifest.version);

        // 의존성 검사
        for dep in &manifest.dependencies {
            if !dep.optional && !self.registry.contains(&dep.name).await {
                if self.config.continue_on_error {
                    warn!(
                        "Plugin {} missing dependency: {}. Continuing anyway.",
                        id, dep.name
                    );
                } else {
                    return Err(Error::Plugin(format!(
                        "Plugin {} requires dependency: {}",
                        id, dep.name
                    )));
                }
            }
        }

        // 레지스트리에 등록
        if !self.registry.register(Arc::clone(&plugin)).await {
            return Err(Error::Plugin(format!(
                "Plugin {} is already loaded",
                id
            )));
        }

        // 플러그인 컨텍스트 생성
        let ctx = PluginContext::new(Arc::clone(&self.event_bus), self.working_dir.clone());

        // on_load 호출
        if let Err(e) = plugin.on_load(&ctx).await {
            error!("Plugin {} failed to load: {}", id, e);
            self.registry.unregister(&id).await;
            return Err(e);
        }

        // 플러그인이 등록한 Tool/Skill 수집
        let tools = ctx.take_tools().await;
        let skills = ctx.take_skills().await;

        // Tool 등록 - DynamicToolRegistry로 동적 등록 가능
        for tool in tools {
            let tool_name = tool.name().to_string();
            if let Err(e) = self.tool_registry.register(tool).await {
                warn!("Failed to register tool {} from plugin {}: {}", tool_name, id, e);
            } else {
                debug!("Registered tool from plugin {}: {}", id, tool_name);
            }
        }

        // Skill 등록 - DynamicSkillRegistry로 동적 등록 가능
        for skill in skills {
            let skill_name = skill.definition().name.clone();
            if let Err(e) = self.skill_registry.register(skill).await {
                warn!("Failed to register skill {} from plugin {}: {}", skill_name, id, e);
            } else {
                debug!("Registered skill from plugin {}: {}", id, skill_name);
            }
        }

        // 상태 업데이트
        self.registry.set_status(&id, PluginStatus::Active).await;

        // 이벤트 발행
        self.event_bus
            .publish(super::events::PluginEvent::new(
                super::events::EventType::PluginLoaded,
                serde_json::json!({ "plugin_id": id }),
                "plugin_manager",
            ))
            .await;

        info!("Plugin {} loaded successfully", id);
        Ok(())
    }

    /// 플러그인 언로드
    pub async fn unload(&self, id: &str) -> Result<()> {
        info!("Unloading plugin: {}", id);

        let plugin = self.registry.get(id).await.ok_or_else(|| {
            Error::NotFound(format!("Plugin {} not found", id))
        })?;

        let manifest = plugin.manifest();

        // 플러그인 컨텍스트 생성
        let ctx = PluginContext::new(Arc::clone(&self.event_bus), self.working_dir.clone());

        // on_unload 호출
        if let Err(e) = plugin.on_unload(&ctx).await {
            warn!("Plugin {} on_unload failed: {}", id, e);
            // 계속 진행
        }

        // 플러그인이 등록한 Tool 제거
        for tool_name in &manifest.provides.tools {
            if self.tool_registry.unregister(tool_name).await.is_some() {
                debug!("Unregistered tool: {}", tool_name);
            }
        }

        // 플러그인이 등록한 Skill 제거
        for skill_name in &manifest.provides.skills {
            if self.skill_registry.unregister(skill_name).await.is_some() {
                debug!("Unregistered skill: {}", skill_name);
            }
        }

        // 레지스트리에서 제거
        self.registry.unregister(id).await;

        // 이벤트 발행
        self.event_bus
            .publish(super::events::PluginEvent::new(
                super::events::EventType::PluginUnloaded,
                serde_json::json!({ "plugin_id": id }),
                "plugin_manager",
            ))
            .await;

        info!("Plugin {} unloaded", id);
        Ok(())
    }

    /// 플러그인 리로드
    pub async fn reload(&self, id: &str) -> Result<()> {
        let plugin = self.registry.get(id).await.ok_or_else(|| {
            Error::NotFound(format!("Plugin {} not found", id))
        })?;

        self.unload(id).await?;
        self.load(plugin).await?;

        Ok(())
    }

    // ========================================================================
    // 플러그인 활성화/비활성화
    // ========================================================================

    /// 플러그인 활성화
    pub async fn activate(&self, id: &str) -> Result<()> {
        let plugin = self.registry.get(id).await.ok_or_else(|| {
            Error::NotFound(format!("Plugin {} not found", id))
        })?;

        let ctx = PluginContext::new(Arc::clone(&self.event_bus), self.working_dir.clone());
        plugin.on_activate(&ctx).await?;

        self.registry.set_enabled(id, true).await;
        info!("Plugin {} activated", id);

        Ok(())
    }

    /// 플러그인 비활성화
    pub async fn deactivate(&self, id: &str) -> Result<()> {
        let plugin = self.registry.get(id).await.ok_or_else(|| {
            Error::NotFound(format!("Plugin {} not found", id))
        })?;

        let ctx = PluginContext::new(Arc::clone(&self.event_bus), self.working_dir.clone());
        plugin.on_deactivate(&ctx).await?;

        self.registry.set_enabled(id, false).await;
        info!("Plugin {} deactivated", id);

        Ok(())
    }

    // ========================================================================
    // 접근자
    // ========================================================================

    /// 플러그인 레지스트리 접근
    pub fn registry(&self) -> &Arc<PluginRegistry> {
        &self.registry
    }

    /// Tool 레지스트리 접근
    pub fn tool_registry(&self) -> &Arc<DynamicToolRegistry> {
        &self.tool_registry
    }

    /// Skill 레지스트리 접근
    pub fn skill_registry(&self) -> &Arc<DynamicSkillRegistry> {
        &self.skill_registry
    }

    /// 이벤트 버스 접근
    pub fn event_bus(&self) -> &Arc<EventBus> {
        &self.event_bus
    }

    // ========================================================================
    // 시스템 프롬프트 수정
    // ========================================================================

    /// 모든 플러그인의 시스템 프롬프트 수정 적용
    pub async fn apply_system_prompt_modifiers(&self, base_prompt: &str) -> String {
        let mut prompt = base_prompt.to_string();

        for plugin in self.registry.list_enabled().await {
            if let Some(modified) = plugin.modify_system_prompt(&prompt) {
                prompt = modified;
            }
        }

        prompt
    }

    // ========================================================================
    // 유틸리티
    // ========================================================================

    /// 로드된 플러그인 수
    pub async fn plugin_count(&self) -> usize {
        self.registry.len().await
    }

    /// 플러그인 요약 정보
    pub async fn summary(&self) -> PluginSummary {
        let plugins = self.registry.list().await;
        let enabled = self.registry.list_enabled().await.len();

        PluginSummary {
            total: plugins.len(),
            enabled,
            disabled: plugins.len() - enabled,
            tool_count: self.tool_registry.len().await,
            skill_count: self.skill_registry.len().await,
        }
    }
}

/// 플러그인 시스템 요약
#[derive(Debug, Clone)]
pub struct PluginSummary {
    pub total: usize,
    pub enabled: usize,
    pub disabled: usize,
    pub tool_count: usize,
    pub skill_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::manifest::{PluginManifest, PluginProvides};
    use async_trait::async_trait;
    use std::any::Any;

    struct TestPlugin;

    #[async_trait]
    impl Plugin for TestPlugin {
        fn manifest(&self) -> PluginManifest {
            PluginManifest::new("test.plugin", "Test Plugin")
                .with_provides(PluginProvides::new().with_tool("test_tool"))
        }

        async fn on_load(&self, _ctx: &PluginContext) -> Result<()> {
            Ok(())
        }

        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    #[tokio::test]
    async fn test_load_plugin() {
        let manager = PluginManager::new(PathBuf::from("/tmp"));
        let plugin = Arc::new(TestPlugin);

        manager.load(plugin).await.unwrap();

        assert_eq!(manager.plugin_count().await, 1);
    }

    #[tokio::test]
    async fn test_unload_plugin() {
        let manager = PluginManager::new(PathBuf::from("/tmp"));
        let plugin = Arc::new(TestPlugin);

        manager.load(plugin).await.unwrap();
        manager.unload("test.plugin").await.unwrap();

        assert_eq!(manager.plugin_count().await, 0);
    }
}
