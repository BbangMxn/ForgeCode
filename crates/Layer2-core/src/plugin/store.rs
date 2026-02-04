//! Plugin Store - 플러그인 설치 정보 관리
//!
//! installed.json을 통해 설치된 플러그인 목록을 관리합니다.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;
use tracing::{debug, info, warn};

use forge_foundation::Result;

// ============================================================================
// InstalledPlugin - 설치된 플러그인 정보
// ============================================================================

/// 설치된 플러그인 정보
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledPlugin {
    /// 플러그인 ID
    pub id: String,

    /// 플러그인 이름
    pub name: String,

    /// 버전
    pub version: String,

    /// 설치 시간
    pub installed_at: DateTime<Utc>,

    /// 설치 소스 (예: "github:forgecode/plugins/git-enhanced")
    pub source: String,

    /// 활성화 여부
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// 설치 경로
    pub path: PathBuf,

    /// 설명
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// 작성자
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
}

fn default_true() -> bool {
    true
}

impl InstalledPlugin {
    /// 새 InstalledPlugin 생성
    pub fn new(id: impl Into<String>, version: impl Into<String>, source: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        let id = id.into();
        Self {
            name: id.clone(),
            id,
            version: version.into(),
            installed_at: Utc::now(),
            source: source.into(),
            enabled: true,
            path: path.into(),
            description: None,
            author: None,
        }
    }

    /// 이름 설정
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// 설명 설정
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// 작성자 설정
    pub fn with_author(mut self, author: impl Into<String>) -> Self {
        self.author = Some(author.into());
        self
    }
}

// ============================================================================
// InstalledPluginsFile - installed.json 구조
// ============================================================================

/// installed.json 파일 구조
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPluginsFile {
    /// 파일 버전
    #[serde(default = "default_version")]
    pub version: String,

    /// 설치된 플러그인 목록
    #[serde(default)]
    pub plugins: Vec<InstalledPlugin>,
}

fn default_version() -> String {
    "1.0".to_string()
}

impl Default for InstalledPluginsFile {
    fn default() -> Self {
        Self {
            version: default_version(),
            plugins: Vec::new(),
        }
    }
}

// ============================================================================
// PluginStore - 플러그인 저장소
// ============================================================================

/// 플러그인 저장소 - installed.json 관리
pub struct PluginStore {
    /// 기본 디렉토리 (~/.forgecode/plugins)
    base_dir: PathBuf,

    /// installed.json 캐시
    cache: tokio::sync::RwLock<InstalledPluginsFile>,
}

impl PluginStore {
    /// 새 저장소 생성
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
            cache: tokio::sync::RwLock::new(InstalledPluginsFile::default()),
        }
    }

    /// 사용자 홈 디렉토리 기반으로 생성
    pub fn user_store() -> Option<Self> {
        dirs::home_dir().map(|home| Self::new(home.join(".forgecode").join("plugins")))
    }

    /// 기본 디렉토리 경로
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    /// installed.json 경로
    fn installed_file(&self) -> PathBuf {
        self.base_dir.join("installed.json")
    }

    /// 플러그인 디렉토리 경로
    pub fn plugin_dir(&self, id: &str) -> PathBuf {
        self.base_dir.join(id)
    }

    // ========================================================================
    // 로드 / 저장
    // ========================================================================

    /// installed.json 로드
    pub async fn load(&self) -> Result<()> {
        let path = self.installed_file();

        if !path.exists() {
            debug!("installed.json not found at {:?}, using empty", path);
            return Ok(());
        }

        let content = fs::read_to_string(&path).await?;

        let file: InstalledPluginsFile = serde_json::from_str(&content)?;

        *self.cache.write().await = file;

        info!("Loaded {} installed plugins", self.cache.read().await.plugins.len());
        Ok(())
    }

    /// installed.json 저장
    pub async fn save(&self) -> Result<()> {
        // 디렉토리 생성
        if !self.base_dir.exists() {
            fs::create_dir_all(&self.base_dir).await?;
        }

        let path = self.installed_file();
        let cache = self.cache.read().await;
        let content = serde_json::to_string_pretty(&*cache)?;

        fs::write(&path, content).await?;

        debug!("Saved installed.json with {} plugins", cache.plugins.len());
        Ok(())
    }

    // ========================================================================
    // 플러그인 관리
    // ========================================================================

    /// 설치된 플러그인 목록
    pub async fn list(&self) -> Vec<InstalledPlugin> {
        self.cache.read().await.plugins.clone()
    }

    /// 활성화된 플러그인만
    pub async fn list_enabled(&self) -> Vec<InstalledPlugin> {
        self.cache
            .read()
            .await
            .plugins
            .iter()
            .filter(|p| p.enabled)
            .cloned()
            .collect()
    }

    /// ID로 플러그인 조회
    pub async fn get(&self, id: &str) -> Option<InstalledPlugin> {
        self.cache
            .read()
            .await
            .plugins
            .iter()
            .find(|p| p.id == id)
            .cloned()
    }

    /// 플러그인 존재 여부
    pub async fn contains(&self, id: &str) -> bool {
        self.cache.read().await.plugins.iter().any(|p| p.id == id)
    }

    /// 플러그인 설치 기록
    pub async fn record_install(&self, plugin: InstalledPlugin) -> Result<()> {
        {
            let mut cache = self.cache.write().await;

            // 이미 존재하면 업데이트
            if let Some(existing) = cache.plugins.iter_mut().find(|p| p.id == plugin.id) {
                info!("Updating plugin: {} -> v{}", plugin.id, plugin.version);
                *existing = plugin;
            } else {
                info!("Installing plugin: {} v{}", plugin.id, plugin.version);
                cache.plugins.push(plugin);
            }
        }

        self.save().await
    }

    /// 플러그인 제거 기록
    pub async fn record_uninstall(&self, id: &str) -> Result<Option<InstalledPlugin>> {
        let removed = {
            let mut cache = self.cache.write().await;
            let index = cache.plugins.iter().position(|p| p.id == id);

            if let Some(idx) = index {
                let removed = cache.plugins.remove(idx);
                info!("Uninstalled plugin: {}", id);
                Some(removed)
            } else {
                warn!("Plugin not found for uninstall: {}", id);
                None
            }
        };

        if removed.is_some() {
            self.save().await?;
        }

        Ok(removed)
    }

    /// 플러그인 활성화 상태 변경
    pub async fn set_enabled(&self, id: &str, enabled: bool) -> Result<bool> {
        let updated = {
            let mut cache = self.cache.write().await;

            if let Some(plugin) = cache.plugins.iter_mut().find(|p| p.id == id) {
                plugin.enabled = enabled;
                info!("Plugin {} {}", id, if enabled { "enabled" } else { "disabled" });
                true
            } else {
                false
            }
        };

        if updated {
            self.save().await?;
        }

        Ok(updated)
    }

    /// 플러그인 수
    pub async fn len(&self) -> usize {
        self.cache.read().await.plugins.len()
    }

    /// 비어있는지 확인
    pub async fn is_empty(&self) -> bool {
        self.cache.read().await.plugins.is_empty()
    }

    // ========================================================================
    // 유틸리티
    // ========================================================================

    /// 플러그인 ID로 디렉토리 존재 여부 확인
    pub async fn plugin_dir_exists(&self, id: &str) -> bool {
        self.plugin_dir(id).exists()
    }

    /// 플러그인 디렉토리 생성
    pub async fn create_plugin_dir(&self, id: &str) -> Result<PathBuf> {
        let dir = self.plugin_dir(id);
        fs::create_dir_all(&dir).await?;
        Ok(dir)
    }

    /// 플러그인 디렉토리 삭제
    pub async fn remove_plugin_dir(&self, id: &str) -> Result<()> {
        let dir = self.plugin_dir(id);
        if dir.exists() {
            fs::remove_dir_all(&dir).await?;
        }
        Ok(())
    }

    /// ID별 플러그인 맵 생성
    pub async fn as_map(&self) -> HashMap<String, InstalledPlugin> {
        self.cache
            .read()
            .await
            .plugins
            .iter()
            .map(|p| (p.id.clone(), p.clone()))
            .collect()
    }
}

// ============================================================================
// 테스트
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn test_store() -> (PluginStore, TempDir) {
        let temp = TempDir::new().unwrap();
        let store = PluginStore::new(temp.path().join("plugins"));
        (store, temp)
    }

    #[tokio::test]
    async fn test_empty_store() {
        let (store, _temp) = test_store().await;
        assert!(store.is_empty().await);
        assert!(store.list().await.is_empty());
    }

    #[tokio::test]
    async fn test_record_install() {
        let (store, _temp) = test_store().await;

        let plugin = InstalledPlugin::new(
            "test.plugin",
            "1.0.0",
            "github:test/plugin",
            store.plugin_dir("test.plugin"),
        );

        store.record_install(plugin.clone()).await.unwrap();

        assert!(!store.is_empty().await);
        assert!(store.contains("test.plugin").await);

        let retrieved = store.get("test.plugin").await.unwrap();
        assert_eq!(retrieved.version, "1.0.0");
    }

    #[tokio::test]
    async fn test_record_uninstall() {
        let (store, _temp) = test_store().await;

        let plugin = InstalledPlugin::new(
            "test.plugin",
            "1.0.0",
            "local",
            store.plugin_dir("test.plugin"),
        );

        store.record_install(plugin).await.unwrap();
        assert!(store.contains("test.plugin").await);

        let removed = store.record_uninstall("test.plugin").await.unwrap();
        assert!(removed.is_some());
        assert!(!store.contains("test.plugin").await);
    }

    #[tokio::test]
    async fn test_set_enabled() {
        let (store, _temp) = test_store().await;

        let plugin = InstalledPlugin::new(
            "test.plugin",
            "1.0.0",
            "local",
            store.plugin_dir("test.plugin"),
        );

        store.record_install(plugin).await.unwrap();

        // 기본값은 enabled
        assert_eq!(store.list_enabled().await.len(), 1);

        // 비활성화
        store.set_enabled("test.plugin", false).await.unwrap();
        assert!(store.list_enabled().await.is_empty());

        // 다시 활성화
        store.set_enabled("test.plugin", true).await.unwrap();
        assert_eq!(store.list_enabled().await.len(), 1);
    }

    #[tokio::test]
    async fn test_save_and_load() {
        let (store, _temp) = test_store().await;

        let plugin = InstalledPlugin::new(
            "test.plugin",
            "1.0.0",
            "github:test/plugin",
            store.plugin_dir("test.plugin"),
        )
        .with_name("Test Plugin")
        .with_description("A test plugin")
        .with_author("Test Author");

        store.record_install(plugin).await.unwrap();

        // 새 store 인스턴스로 로드
        let store2 = PluginStore::new(store.base_dir());
        store2.load().await.unwrap();

        let retrieved = store2.get("test.plugin").await.unwrap();
        assert_eq!(retrieved.name, "Test Plugin");
        assert_eq!(retrieved.description, Some("A test plugin".to_string()));
        assert_eq!(retrieved.author, Some("Test Author".to_string()));
    }

    #[tokio::test]
    async fn test_update_existing() {
        let (store, _temp) = test_store().await;

        let plugin1 = InstalledPlugin::new(
            "test.plugin",
            "1.0.0",
            "local",
            store.plugin_dir("test.plugin"),
        );

        store.record_install(plugin1).await.unwrap();

        // 업데이트
        let plugin2 = InstalledPlugin::new(
            "test.plugin",
            "2.0.0",
            "local",
            store.plugin_dir("test.plugin"),
        );

        store.record_install(plugin2).await.unwrap();

        // 하나만 있어야 함
        assert_eq!(store.len().await, 1);

        // 버전이 업데이트 됨
        let retrieved = store.get("test.plugin").await.unwrap();
        assert_eq!(retrieved.version, "2.0.0");
    }
}
