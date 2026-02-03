//! # forge-tool
//!
//! Tool system for ForgeCode providing:
//! - Tool trait and registry
//! - Builtin tools (bash, read, write, edit, grep, glob)
//! - ForgeCmd: PTY-based shell environment for LLM agents
//! - MCP client for external tools
//! - Plugin system

pub mod builtin;
pub mod forgecmd;
pub mod registry;
pub mod r#trait;

pub use r#trait::{Tool, ToolContext, ToolDef, ToolResult};
pub use registry::ToolRegistry;

// Re-export builtin tools
pub use builtin::{
    bash::BashTool, edit::EditTool, forgecmd_tool::ForgeCmdTool, glob::GlobTool, grep::GrepTool,
    read::ReadTool, write::WriteTool,
};

// Re-export forgecmd
pub use forgecmd::{
    permission_name_for_category, register_permissions, CheckResult, CommandCategory,
    CommandRecord, CommandResult, ConfirmOption, ConfirmationPrompt, ExecutionStatus, ForgeCmd,
    ForgeCmdBuilder, ForgeCmdConfig, ForgeCmdError, PermissionChecker, PermissionDecision,
    PermissionRule, PermissionRules, PtySession, RiskAnalysis, RiskThresholds, TrackerStats,
};
