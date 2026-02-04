//! Plugin Registry - 플러그인 저장소

use super::traits::{Plugin, PluginStatus};
use super::manifest::PluginManifest;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// 플러그인 정보
pub struct PluginInfo {
    /// 플러그인 인스턴스
    pub plugin: Arc<dyn Plugin>,

    /// 현재 상태
    pub status: PluginStatus,

    /// 활성화 여부
    pub enabled: bool,

    /// 로드 순서
    pub load_order: usize,
}

/// 플러그인 레지스트리 - 모든 플러그인 관리
pub struct PluginRegistry {
    /// 플러그인 저장소 (ID -> PluginInfo)
    plugins: RwLock<HashMap<String, PluginInfo>>,

    /// 로드 카운터
    load_counter: RwLock<usize>,
}

impl PluginRegistry {
    /// 새 레지스트리 생성
    pub fn new() -> Self {
        Self {
            plugins: RwLock::new(HashMap::new()),
            load_counter: RwLock::new(0),
        }
    }

    /// 플러그인 등록
    pub async fn register(&self, plugin: Arc<dyn Plugin>) -> bool {
        let manifest = plugin.manifest();
        let id = manifest.id.clone();

        let mut plugins = self.plugins.write().await;

        if plugins.contains_key(&id) {
            warn!("Plugin {} is already registered", id);
            return false;
        }

        let mut counter = self.load_counter.write().await;
        *counter += 1;
        let load_order = *counter;

        plugins.insert(
            id.clone(),
            PluginInfo {
                plugin,
                status: PluginStatus::Loaded,
                enabled: true,
                load_order,
            },
        );

        info!("Registered plugin: {} (v{})", id, manifest.version);
        true
    }

    /// 플러그인 등록 해제
    pub async fn unregister(&self, id: &str) -> Option<Arc<dyn Plugin>> {
        let mut plugins = self.plugins.write().await;

        if let Some(info) = plugins.remove(id) {
            info!("Unregistered plugin: {}", id);
            Some(info.plugin)
        } else {
            None
        }
    }

    /// 플러그인 조회
    pub async fn get(&self, id: &str) -> Option<Arc<dyn Plugin>> {
        let plugins = self.plugins.read().await;
        plugins.get(id).map(|info| Arc::clone(&info.plugin))
    }

    /// 플러그인 매니페스트 조회
    pub async fn get_manifest(&self, id: &str) -> Option<PluginManifest> {
        let plugins = self.plugins.read().await;
        plugins.get(id).map(|info| info.plugin.manifest())
    }

    /// 플러그인 상태 조회
    pub async fn get_status(&self, id: &str) -> Option<PluginStatus> {
        let plugins = self.plugins.read().await;
        plugins.get(id).map(|info| info.status)
    }

    /// 플러그인 상태 설정
    pub async fn set_status(&self, id: &str, status: PluginStatus) -> bool {
        let mut plugins = self.plugins.write().await;
        if let Some(info) = plugins.get_mut(id) {
            info.status = status;
            debug!("Set plugin {} status to {}", id, status);
            true
        } else {
            false
        }
    }

    /// 플러그인 활성화/비활성화
    pub async fn set_enabled(&self, id: &str, enabled: bool) -> bool {
        let mut plugins = self.plugins.write().await;
        if let Some(info) = plugins.get_mut(id) {
            info.enabled = enabled;
            info.status = if enabled {
                PluginStatus::Active
            } else {
                PluginStatus::Inactive
            };
            debug!("Set plugin {} enabled = {}", id, enabled);
            true
        } else {
            false
        }
    }

    /// 모든 플러그인 목록
    pub async fn list(&self) -> Vec<PluginManifest> {
        let plugins = self.plugins.read().await;
        plugins.values().map(|info| info.plugin.manifest()).collect()
    }

    /// 활성화된 플러그인 목록 (로드 순서대로)
    pub async fn list_enabled(&self) -> Vec<Arc<dyn Plugin>> {
        let plugins = self.plugins.read().await;
        let mut enabled: Vec<_> = plugins
            .values()
            .filter(|info| info.enabled)
            .collect();

        enabled.sort_by_key(|info| info.load_order);
        enabled.iter().map(|info| Arc::clone(&info.plugin)).collect()
    }

    /// 특정 기능을 제공하는 플러그인 찾기
    pub async fn find_providers(&self, tool_name: &str) -> Vec<Arc<dyn Plugin>> {
        let plugins = self.plugins.read().await;
        plugins
            .values()
            .filter(|info| {
                info.enabled && info.plugin.manifest().provides.tools.contains(&tool_name.to_string())
            })
            .map(|info| Arc::clone(&info.plugin))
            .collect()
    }

    /// 특정 Skill을 제공하는 플러그인 찾기
    pub async fn find_skill_providers(&self, skill_name: &str) -> Vec<Arc<dyn Plugin>> {
        let plugins = self.plugins.read().await;
        plugins
            .values()
            .filter(|info| {
                info.enabled && info.plugin.manifest().provides.skills.contains(&skill_name.to_string())
            })
            .map(|info| Arc::clone(&info.plugin))
            .collect()
    }

    /// 플러그인 존재 여부 확인
    pub async fn contains(&self, id: &str) -> bool {
        let plugins = self.plugins.read().await;
        plugins.contains_key(id)
    }

    /// 플러그인 수
    pub async fn len(&self) -> usize {
        let plugins = self.plugins.read().await;
        plugins.len()
    }

    /// 비어있는지 확인
    pub async fn is_empty(&self) -> bool {
        let plugins = self.plugins.read().await;
        plugins.is_empty()
    }

    /// 모든 플러그인 클리어
    pub async fn clear(&self) {
        let mut plugins = self.plugins.write().await;
        plugins.clear();
        *self.load_counter.write().await = 0;
    }

    /// 의존성 검사
    pub async fn check_dependencies(&self, id: &str) -> Vec<String> {
        let plugins = self.plugins.read().await;

        if let Some(info) = plugins.get(id) {
            let manifest = info.plugin.manifest();
            let mut missing = Vec::new();

            for dep in &manifest.dependencies {
                if !dep.optional && !plugins.contains_key(&dep.name) {
                    missing.push(dep.name.clone());
                }
            }

            missing
        } else {
            vec![]
        }
    }

    /// 로드 순서에 따라 정렬된 플러그인 ID 목록
    pub async fn load_order(&self) -> Vec<String> {
        let plugins = self.plugins.read().await;
        let mut ordered: Vec<_> = plugins.iter().collect();
        ordered.sort_by_key(|(_, info)| info.load_order);
        ordered.into_iter().map(|(id, _)| id.clone()).collect()
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::manifest::PluginProvides;
    use async_trait::async_trait;
    use forge_foundation::Result;
    use std::any::Any;

    struct TestPlugin {
        id: String,
    }

    #[async_trait]
    impl Plugin for TestPlugin {
        fn manifest(&self) -> PluginManifest {
            PluginManifest::new(&self.id, "Test")
                .with_provides(PluginProvides::new().with_tool("test_tool"))
        }

        async fn on_load(&self, _ctx: &super::super::traits::PluginContext) -> Result<()> {
            Ok(())
        }

        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    #[tokio::test]
    async fn test_register_plugin() {
        let registry = PluginRegistry::new();
        let plugin = Arc::new(TestPlugin { id: "test.plugin".into() });

        assert!(registry.register(plugin).await);
        assert_eq!(registry.len().await, 1);
    }

    #[tokio::test]
    async fn test_duplicate_registration() {
        let registry = PluginRegistry::new();
        let plugin1 = Arc::new(TestPlugin { id: "test.plugin".into() });
        let plugin2 = Arc::new(TestPlugin { id: "test.plugin".into() });

        assert!(registry.register(plugin1).await);
        assert!(!registry.register(plugin2).await); // Should fail
        assert_eq!(registry.len().await, 1);
    }

    #[tokio::test]
    async fn test_find_providers() {
        let registry = PluginRegistry::new();
        let plugin = Arc::new(TestPlugin { id: "test.plugin".into() });

        registry.register(plugin).await;

        let providers = registry.find_providers("test_tool").await;
        assert_eq!(providers.len(), 1);

        let providers = registry.find_providers("nonexistent").await;
        assert!(providers.is_empty());
    }

    #[tokio::test]
    async fn test_enable_disable() {
        let registry = PluginRegistry::new();
        let plugin = Arc::new(TestPlugin { id: "test.plugin".into() });

        registry.register(plugin).await;

        // 비활성화
        registry.set_enabled("test.plugin", false).await;
        assert_eq!(registry.list_enabled().await.len(), 0);

        // 활성화
        registry.set_enabled("test.plugin", true).await;
        assert_eq!(registry.list_enabled().await.len(), 1);
    }
}
