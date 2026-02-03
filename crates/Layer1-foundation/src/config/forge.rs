//! Forge Config - 통합 설정
//!
//! 모든 설정을 통합 관리하는 ForgeConfig

use crate::permission::PermissionSettings;
use crate::registry::{McpConfig, ProviderConfig, ShellConfig};
use crate::storage::JsonStore;
use crate::Result;
use serde::{Deserialize, Serialize};

use super::LimitsConfig;

/// 설정 파일명
pub const FORGE_CONFIG_FILE: &str = "config.json";

// ============================================================================
// Forge Config (통합)
// ============================================================================

/// ForgeCode 통합 설정
///
/// 모든 설정을 하나로 관리하거나, 개별 파일로 분리 관리 가능
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForgeConfig {
    /// 버전 (마이그레이션용)
    #[serde(default = "default_version")]
    pub version: u32,

    /// 기본 프로바이더 이름
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_provider: Option<String>,

    /// 기본 모델 이름
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,

    /// 기본 Shell 타입
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_shell: Option<String>,

    /// 테마 (TUI용)
    #[serde(default)]
    pub theme: ThemeConfig,

    /// 에디터 설정
    #[serde(default)]
    pub editor: EditorConfig,

    /// 자동 저장 설정
    #[serde(default)]
    pub auto_save: AutoSaveConfig,

    /// 실험적 기능
    #[serde(default)]
    pub experimental: ExperimentalConfig,
}

impl ForgeConfig {
    pub fn new() -> Self {
        Self::default()
    }

    // ========================================================================
    // Load / Save
    // ========================================================================

    /// 글로벌 + 프로젝트 병합 로드
    pub fn load() -> Result<Self> {
        let mut config = Self::new();

        // 1. 글로벌 설정
        if let Ok(global) = JsonStore::global() {
            if let Some(global_config) = global.load_optional::<ForgeConfig>(FORGE_CONFIG_FILE)? {
                config.merge(global_config);
            }
        }

        // 2. 프로젝트 설정
        if let Ok(project) = JsonStore::current_project() {
            if let Some(project_config) =
                project.load_optional::<ForgeConfig>(FORGE_CONFIG_FILE)?
            {
                config.merge(project_config);
            }
        }

        Ok(config)
    }

    /// 글로벌 설정만 로드
    pub fn load_global() -> Result<Self> {
        let store = JsonStore::global()?;
        Ok(store.load_or_default(FORGE_CONFIG_FILE))
    }

    /// 프로젝트 설정만 로드
    pub fn load_project() -> Result<Self> {
        let store = JsonStore::current_project()?;
        Ok(store.load_or_default(FORGE_CONFIG_FILE))
    }

    /// 글로벌 설정 저장
    pub fn save_global(&self) -> Result<()> {
        let store = JsonStore::global()?;
        store.save(FORGE_CONFIG_FILE, self)
    }

    /// 프로젝트 설정 저장
    pub fn save_project(&self) -> Result<()> {
        let store = JsonStore::current_project()?;
        store.save(FORGE_CONFIG_FILE, self)
    }

    // ========================================================================
    // 개별 설정 로드 (위임)
    // ========================================================================

    /// Provider 설정 로드
    pub fn load_providers() -> Result<ProviderConfig> {
        ProviderConfig::load()
    }

    /// MCP 설정 로드
    pub fn load_mcp() -> Result<McpConfig> {
        McpConfig::load()
    }

    /// Shell 설정 로드
    pub fn load_shell() -> Result<ShellConfig> {
        ShellConfig::load()
    }

    /// Permission 설정 로드
    pub fn load_permissions() -> Result<PermissionSettings> {
        PermissionSettings::load()
    }

    /// Limits 설정 로드
    pub fn load_limits() -> Result<LimitsConfig> {
        LimitsConfig::load()
    }

    // ========================================================================
    // Merge
    // ========================================================================

    /// 다른 설정과 병합 (other가 우선)
    pub fn merge(&mut self, other: ForgeConfig) {
        if other.default_provider.is_some() {
            self.default_provider = other.default_provider;
        }
        if other.default_model.is_some() {
            self.default_model = other.default_model;
        }
        if other.default_shell.is_some() {
            self.default_shell = other.default_shell;
        }

        self.theme.merge(other.theme);
        self.editor.merge(other.editor);
        self.auto_save.merge(other.auto_save);
        self.experimental.merge(other.experimental);
    }

    // ========================================================================
    // Builder
    // ========================================================================

    pub fn default_provider(mut self, provider: impl Into<String>) -> Self {
        self.default_provider = Some(provider.into());
        self
    }

    pub fn default_model(mut self, model: impl Into<String>) -> Self {
        self.default_model = Some(model.into());
        self
    }

    pub fn default_shell(mut self, shell: impl Into<String>) -> Self {
        self.default_shell = Some(shell.into());
        self
    }
}

// ============================================================================
// Theme Config
// ============================================================================

/// 테마 설정 (TUI)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeConfig {
    /// 테마 이름
    #[serde(default = "default_theme")]
    pub name: String,

    /// 다크 모드 자동 감지
    #[serde(default = "default_true")]
    pub auto_dark_mode: bool,

    /// 커스텀 색상
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_colors: Option<CustomColors>,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            name: default_theme(),
            auto_dark_mode: true,
            custom_colors: None,
        }
    }
}

impl ThemeConfig {
    fn merge(&mut self, other: ThemeConfig) {
        if other.name != default_theme() {
            self.name = other.name;
        }
        self.auto_dark_mode = other.auto_dark_mode;
        if other.custom_colors.is_some() {
            self.custom_colors = other.custom_colors;
        }
    }
}

/// 커스텀 색상
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomColors {
    pub primary: Option<String>,
    pub secondary: Option<String>,
    pub accent: Option<String>,
    pub background: Option<String>,
    pub foreground: Option<String>,
}

// ============================================================================
// Editor Config
// ============================================================================

/// 에디터 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorConfig {
    /// 기본 에디터 명령어
    #[serde(default = "default_editor")]
    pub command: String,

    /// 탭 크기
    #[serde(default = "default_tab_size")]
    pub tab_size: u8,

    /// 탭 대신 스페이스 사용
    #[serde(default = "default_true")]
    pub use_spaces: bool,

    /// 줄 바꿈
    #[serde(default)]
    pub word_wrap: bool,
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            command: default_editor(),
            tab_size: default_tab_size(),
            use_spaces: true,
            word_wrap: false,
        }
    }
}

impl EditorConfig {
    fn merge(&mut self, other: EditorConfig) {
        if other.command != default_editor() {
            self.command = other.command;
        }
        if other.tab_size != default_tab_size() {
            self.tab_size = other.tab_size;
        }
        self.use_spaces = other.use_spaces;
        self.word_wrap = other.word_wrap;
    }
}

// ============================================================================
// Auto Save Config
// ============================================================================

/// 자동 저장 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoSaveConfig {
    /// 자동 저장 활성화
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// 저장 간격 (초)
    #[serde(default = "default_auto_save_interval")]
    pub interval_secs: u64,

    /// 세션 히스토리 자동 저장
    #[serde(default = "default_true")]
    pub save_history: bool,
}

impl Default for AutoSaveConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_secs: default_auto_save_interval(),
            save_history: true,
        }
    }
}

impl AutoSaveConfig {
    fn merge(&mut self, other: AutoSaveConfig) {
        self.enabled = other.enabled;
        if other.interval_secs != default_auto_save_interval() {
            self.interval_secs = other.interval_secs;
        }
        self.save_history = other.save_history;
    }
}

// ============================================================================
// Experimental Config
// ============================================================================

/// 실험적 기능 설정
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentalConfig {
    /// 병렬 도구 실행
    #[serde(default)]
    pub parallel_tools: bool,

    /// 스트리밍 응답
    #[serde(default = "default_true")]
    pub streaming: bool,

    /// 캐시 사용
    #[serde(default = "default_true")]
    pub use_cache: bool,

    /// MCP 도구 자동 발견
    #[serde(default)]
    pub mcp_auto_discover: bool,
}

impl ExperimentalConfig {
    fn merge(&mut self, other: ExperimentalConfig) {
        self.parallel_tools = other.parallel_tools;
        self.streaming = other.streaming;
        self.use_cache = other.use_cache;
        self.mcp_auto_discover = other.mcp_auto_discover;
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn default_version() -> u32 {
    1
}

fn default_theme() -> String {
    "default".to_string()
}

fn default_editor() -> String {
    std::env::var("EDITOR").unwrap_or_else(|_| {
        #[cfg(target_os = "windows")]
        {
            "notepad".to_string()
        }
        #[cfg(not(target_os = "windows"))]
        {
            "vim".to_string()
        }
    })
}

fn default_tab_size() -> u8 {
    4
}

fn default_auto_save_interval() -> u64 {
    30
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_forge_config_default() {
        let config = ForgeConfig::new();
        assert_eq!(config.version, 1);
        assert!(config.default_provider.is_none());
    }

    #[test]
    fn test_forge_config_builder() {
        let config = ForgeConfig::new()
            .default_provider("anthropic")
            .default_model("claude-sonnet-4")
            .default_shell("bash");

        assert_eq!(config.default_provider, Some("anthropic".to_string()));
        assert_eq!(config.default_model, Some("claude-sonnet-4".to_string()));
        assert_eq!(config.default_shell, Some("bash".to_string()));
    }

    #[test]
    fn test_config_merge() {
        let mut base = ForgeConfig::new();
        base.default_provider = Some("openai".to_string());

        let overlay = ForgeConfig::new()
            .default_provider("anthropic")
            .default_model("claude-opus-4");

        base.merge(overlay);

        assert_eq!(base.default_provider, Some("anthropic".to_string()));
        assert_eq!(base.default_model, Some("claude-opus-4".to_string()));
    }
}
