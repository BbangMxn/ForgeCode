//! # Configuration System
//!
//! Claude Code 호환 설정 시스템 구현
//!
//! ## 설정 우선순위 (낮은 → 높은)
//!
//! 1. User-level: `~/.claude/settings.json` (전역 설정)
//! 2. Project-level: `.claude/settings.json` (프로젝트별)
//! 3. Local: `.claude/settings.local.json` (gitignore됨)
//!
//! ## 사용 예시
//!
//! ```ignore
//! use forge_core::config::{ConfigLoader, ForgeConfig};
//!
//! let loader = ConfigLoader::new(Path::new("."));
//! let config = loader.load_all()?;
//!
//! // Provider 설정
//! if let Some(api_key) = config.get_api_key() {
//!     // ...
//! }
//!
//! // MCP 서버 설정
//! for (name, server) in &config.mcp_servers {
//!     // ...
//! }
//!
//! // Git 워크플로우 설정
//! if config.git.workflow.auto_commit.enabled {
//!     // 자동 커밋 활성화
//! }
//!
//! // 환경 변수 보안
//! let filtered_env = config.security.env.filter_env(&std::env::vars().collect());
//! ```

mod loader;
mod types;
mod workflow;

pub use loader::{load_config_from_file, merge_configs, strip_json_comments, ConfigLoader};
pub use types::{
    ForgeConfig, McpServerConfig as ConfigMcpServer, ModelConfig, PermissionConfig, ProviderConfig,
    ShellConfigSection, ThemeConfig,
};
pub use workflow::{
    // Helper
    glob_match,
    // Git workflow
    AutoCommitConfig,
    AutoPushConfig,
    AutoStageConfig,
    AutoTagConfig,
    // Security
    CommandSecurityConfig,
    EnvSecurityConfig,
    GitConfig,
    GitHooksConfig,
    GitWorkflowConfig,
    NetworkSecurityConfig,
    PathSecurityConfig,
    SecurityConfig,
};
