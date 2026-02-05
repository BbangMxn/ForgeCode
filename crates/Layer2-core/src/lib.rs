//! forge-core: Core Runtime for ForgeCode
//!
//! Layer2 - Agent 도구 구현 레이어
//!
//! # 주요 모듈
//!
//! - `context`: Agent 통합 컨텍스트 (Provider + Tool + Task)
//! - `forgecmd`: PTY 기반 셸 환경 (ForgeCmd)
//! - `lsp`: 경량 LSP (Language Server Protocol) 연동
//! - `mcp`: MCP (Model Context Protocol) 브릿지
//! - `tool`: Tool 시스템 및 Builtin 도구들
//! - `skill`: Skill 시스템 (슬래시 명령어)
//! - `hook`: Hook 시스템 (Claude Code 호환)
//! - `config`: 설정 시스템 (Claude Code 호환)
//! - `plugin`: Plugin 확장 시스템
//!
//! # 사용 예시
//!
//! ```ignore
//! use forge_core::{AgentContext, ToolRegistry, SkillRegistry, PluginManager};
//!
//! // AgentContext로 통합 인터페이스 사용
//! let ctx = AgentContext::builder()
//!     .session_id("my-session")
//!     .build();
//!
//! // 도구 실행
//! let result = ctx.execute_tool("read", json!({
//!     "path": "src/main.rs"
//! })).await?;
//!
//! // 도구 목록
//! let tools = ctx.list_tools().await;
//!
//! // Skill 실행 (/commit, /review-pr 등)
//! let skills = SkillRegistry::with_builtins();
//! if let Some(skill) = skills.find_for_input("/commit -m 'fix bug'") {
//!     // 스킬 실행...
//! }
//!
//! // Plugin 로드
//! let mut plugins = PluginManager::new(PathBuf::from("."));
//! plugins.load(my_plugin).await?;
//! ```

// Core modules
pub mod config;
pub mod context;
pub mod forgecmd;
pub mod git;
pub mod hook;
pub mod lsp;
pub mod mcp;
pub mod plugin;
pub mod registry;
pub mod repomap;
pub mod skill;
pub mod tool;

// Re-exports: Agent Context
pub use context::{
    AgentContext, AgentContextBuilder, AgentContextConfig, ExecutionStats, ToolExecutionResult,
};

// Re-exports: LSP
pub use lsp::{
    create_disabled_lsp_manager, create_lsp_manager, default_lsp_configs, path_to_uri, uri_to_path,
    DocumentSymbol, Hover, Location, LspClient, LspClientState, LspManager, LspServerConfig,
    Position, Range, SymbolKind,
};

// Re-exports: MCP
pub use mcp::{
    McpBridge, McpClient, McpContent, McpPrompt, McpPromptArgument, McpResource, McpServerConfig,
    McpTool, McpToolAdapter, McpToolCall, McpToolResult, McpTransportConfig, ServerStatus,
    SseTransport, StdioTransport,
};

// Re-exports: Tool
pub use tool::{
    // Functions
    all_tools,
    core_tools,
    filesystem_tools,
    is_safe_extension,
    is_sensitive_path,
    // Tools
    BashTool,
    // Context
    DefaultShellConfig,
    EditTool,
    GlobTool,
    GrepTool,
    // Security
    PathValidation,
    PathValidator,
    ReadTool,
    RuntimeContext,
    // Tool trait
    Tool,
    ToolContext,
    // Registry
    ToolRegistry,
    WriteTool,
};

// Re-exports: Skill
pub use skill::{
    // Built-in Skills
    CommitSkill,
    ExplainSkill,
    FileBasedSkill,
    ReviewPrSkill,
    // Traits
    Skill,
    SkillConfig,
    SkillContext,
    SkillDefinition,
    SkillInput,
    // File-based skill loader (Claude Code compatible)
    SkillLoader,
    SkillMetadata,
    SkillOutput,
    // Registry
    SkillRegistry,
};

// Re-exports: Plugin
pub use plugin::{
    EventBus,
    // Traits
    Plugin,
    PluginCapability,
    PluginContext,
    PluginDependency,
    // Events
    PluginEvent,
    PluginEventHandler,
    // Manager
    PluginManager,
    // Manifest
    PluginManifest,
    // Registry
    PluginRegistry,
    PluginVersion,
};

// Re-exports: Dynamic Registry
pub use registry::{
    // Core types
    DynamicRegistry,
    DynamicSkillRegistry,
    DynamicToolRegistry,
    EntryMetadata,
    EntryState,
    HotReloadConfig,
    HotReloadResult,
    HotReloadState,
    // Traits
    Registerable,
    // Entry types
    RegistryEntry,
    RegistryEvent,
    RegistryEventHandler,
    // Snapshot/Rollback
    RegistrySnapshot,
    RegistryStats,
    SnapshotInfo,
    SnapshotManager,
};

// Re-exports: Hook (Claude Code compatible)
pub use hook::{
    load_hooks_from_dir,
    load_hooks_from_file,
    BlockReason,
    HookAction,
    HookConfig,
    HookContext,
    // Types
    HookEvent,
    HookEventType,
    // Executor
    HookExecutor,
    // Loader
    HookLoader,
    HookMatcher,
    HookOutcome,
    HookResult,
};

// Re-exports: Config (Claude Code compatible)
pub use config::{
    load_config_from_file,
    merge_configs,
    // Loader
    ConfigLoader,
    ConfigMcpServer,
    // Types
    ForgeConfig,
    ModelConfig,
    ProviderConfig,
    ShellConfigSection,
    ThemeConfig,
};

// Re-exports: Repository Map (AST-based codebase analysis)
pub use repomap::{
    DependencyGraph, FileInfo, FileRanker, RepoAnalyzer, RepoMap, RepoMapConfig, SymbolDef,
    SymbolKind as RepoSymbolKind, SymbolRef, SymbolUsage,
};

// Re-exports: Git Integration (auto-commit, checkpoint, rollback)
pub use git::{
    AutoCommitConfig, Checkpoint, CheckpointId, CheckpointManager, CommitGenerator, CommitStyle,
    FileStatus, GitError, GitOps, GitStatus,
};

// Re-exports: ForgeCmd (PTY-based shell)
pub use forgecmd::{
    // Functions
    permission_name_for_category,
    register_permissions,
    // Permission
    CheckResult,
    // Filter & Analysis
    CommandCategory,
    CommandFilter,
    // Execution
    CommandRecord,
    CommandResult,
    ConfirmOption,
    ConfirmationPrompt,
    ExecutionStatus,
    // Main types
    ForgeCmd,
    ForgeCmdBuilder,
    ForgeCmdConfig,
    ForgeCmdError,
    PermissionChecker,
    PermissionDecision,
    PermissionRule,
    PermissionRules,
    PtySession,
    RiskAnalysis,
    RiskThresholds,
    TrackerStats,
};

// Layer1 re-exports
pub use forge_foundation::{Error, Result};

/// Layer2 버전
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
    }

    #[test]
    fn test_lsp_exports() {
        // LSP 타입 export 확인
        let pos = Position::new(10, 5);
        assert_eq!(pos.line, 10);
        assert_eq!(pos.character, 5);
    }

    #[test]
    fn test_tool_exports() {
        // Tool registry 확인
        let registry = ToolRegistry::with_builtins();
        assert!(!registry.is_empty());
        assert!(registry.contains("read"));
        assert!(registry.contains("write"));
        assert!(registry.contains("edit"));
        assert!(registry.contains("glob"));
        assert!(registry.contains("grep"));
        assert!(registry.contains("bash"));
    }

    #[test]
    fn test_all_tools_count() {
        let tools = all_tools();
        // 6 filesystem/execute tools + 7 task tools = 13
        assert_eq!(tools.len(), 13);
    }

    #[tokio::test]
    async fn test_agent_context() {
        let ctx = AgentContext::new();
        let tools = ctx.list_tools().await;
        assert!(!tools.is_empty());
    }

    #[tokio::test]
    async fn test_agent_context_mcp_integration() {
        let ctx = AgentContext::new();

        // MCP 서버 목록 (초기에는 비어있음)
        let servers = ctx.list_mcp_servers().await;
        assert!(servers.is_empty());

        // MCP 상태
        let status = ctx.all_mcp_status().await;
        assert!(status.is_empty());
    }

    #[test]
    fn test_mcp_exports() {
        // MCP 타입 export 확인
        let _bridge = McpBridge::new();
        let _client = McpClient::new("test");
    }
}
