//! Registry - 도구 등록/관리
//!
//! - `mcp/` - MCP 서버 등록 (자체 load/save)
//! - `provider/` - LLM Provider 등록 (자체 load/save)
//! - `model/` - 모델 메타데이터 레지스트리
//! - `shell/` - Shell 설정 관리 (bash, powershell, cmd 등)

pub mod mcp;
pub mod model;
pub mod provider;
pub mod shell;

// MCP
pub use mcp::{McpConfig, McpConfigFile, McpServer, McpTransport, MCP_FILE};

// Provider
pub use provider::{Provider, ProviderConfig, ProviderType, PROVIDERS_FILE};

// Model
pub use model::{
    registry as model_registry, ModelCapabilities, ModelInfo, ModelPricing, ModelRegistry,
};

// Shell
pub use shell::{ShellConfig, ShellRunner, ShellSettings, ShellType, SHELL_FILE};
