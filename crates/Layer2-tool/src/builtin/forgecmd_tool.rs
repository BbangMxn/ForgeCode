//! ForgeCmdTool - PTY-based shell command execution tool
//!
//! This tool provides a PTY-backed shell environment for LLM agents,
//! supporting interactive commands (vim, htop, etc.) that the basic
//! BashTool cannot handle.
//!
//! Note: ForgeCmd uses portable-pty which is not Sync, so we use
//! spawn_blocking to run PTY operations in a dedicated thread.

use crate::forgecmd::{
    permission_name_for_category, CommandCategory, ForgeCmdBuilder, ForgeCmdConfig, ForgeCmdError,
};
use crate::{Tool, ToolContext, ToolDef, ToolResult};
use async_trait::async_trait;
use forge_foundation::permission::{PermissionAction, PermissionScope, PermissionStatus};
use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;

/// Maximum output length before truncation
const MAX_OUTPUT_LENGTH: usize = 50000;

/// ForgeCmdTool - PTY-based command execution
///
/// Unlike BashTool which uses simple process spawning, ForgeCmdTool
/// provides full PTY support for:
/// - Interactive programs (vim, htop, python REPL)
/// - Proper terminal emulation
/// - Enhanced permission control with risk analysis
pub struct ForgeCmdTool {
    /// Configuration
    config: ForgeCmdConfig,
}

#[derive(Debug, Clone, Deserialize)]
struct ForgeCmdParams {
    /// The command to execute
    command: String,

    /// Timeout in seconds (optional, for future use)
    #[serde(default)]
    #[allow(dead_code)]
    timeout: Option<u64>,

    /// Working directory override
    #[serde(default)]
    working_dir: Option<String>,
}

impl ForgeCmdTool {
    /// Create a new ForgeCmdTool with default configuration
    pub fn new() -> Self {
        Self {
            config: ForgeCmdConfig::default(),
        }
    }

    /// Create with custom configuration
    pub fn with_config(config: ForgeCmdConfig) -> Self {
        Self { config }
    }

    /// Truncate output if too long
    fn truncate_output(output: &str) -> String {
        if output.len() <= MAX_OUTPUT_LENGTH {
            return output.to_string();
        }

        let half = MAX_OUTPUT_LENGTH / 2;
        let start = &output[..half];
        let end = &output[output.len() - half..];

        format!(
            "{}\n\n... [truncated {} characters] ...\n\n{}",
            start,
            output.len() - MAX_OUTPUT_LENGTH,
            end
        )
    }
}

impl Default for ForgeCmdTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for ForgeCmdTool {
    fn definition(&self) -> ToolDef {
        ToolDef::builder(
            "forgecmd",
            "Execute shell commands with PTY support. Use this for interactive commands (vim, htop, git rebase -i) or when you need proper terminal emulation. For simple commands, prefer 'bash' tool.",
        )
        .string_param("command", "The shell command to execute", true)
        .integer_param("timeout", "Timeout in seconds (default: 60)", false)
        .string_param(
            "working_dir",
            "Working directory override (default: current directory)",
            false,
        )
        .build()
    }

    async fn execute(&self, ctx: &ToolContext, params: Value) -> ToolResult {
        // Parse parameters
        let params: ForgeCmdParams = match serde_json::from_value(params) {
            Ok(p) => p,
            Err(e) => return ToolResult::error(format!("Invalid parameters: {}", e)),
        };

        let command = params.command.trim().to_string();
        if command.is_empty() {
            return ToolResult::error("Command cannot be empty");
        }

        // Clone values needed for spawn_blocking
        let config = self.config.clone();
        let permissions = ctx.permissions.clone();
        let working_dir = params
            .working_dir
            .map(PathBuf::from)
            .unwrap_or_else(|| ctx.working_dir.clone());
        let auto_approve = ctx.auto_approve;
        let command_clone = command.clone();

        // Run ForgeCmd in a blocking thread since portable-pty is not Sync
        let result = tokio::task::spawn_blocking(move || {
            // Create a new ForgeCmd for this execution
            let mut forge_cmd = match ForgeCmdBuilder::new()
                .permission_service(permissions.clone())
                .config(config)
                .working_dir(working_dir)
                .build()
            {
                Ok(cmd) => cmd,
                Err(e) => {
                    return Err(format!("Failed to initialize ForgeCmd: {}", e));
                }
            };

            // Analyze command risk
            let analysis = forge_cmd.analyze(&command_clone);

            // Check if command is forbidden
            if analysis.category == CommandCategory::Forbidden {
                return Err(format!(
                    "FORBIDDEN:Command is forbidden: {}",
                    analysis
                        .reason
                        .clone()
                        .unwrap_or_else(|| "Security policy".to_string())
                ));
            }

            // Check permission based on category (if not auto_approve)
            if !auto_approve {
                let perm_name = permission_name_for_category(analysis.category);
                let action = PermissionAction::Execute {
                    command: command_clone.clone(),
                };

                match permissions.check(perm_name, &action) {
                    PermissionStatus::Granted | PermissionStatus::AutoApproved => {
                        // Permission granted, continue
                    }
                    PermissionStatus::Denied => {
                        return Err(format!(
                            "DENIED:Command denied by permission policy: {}",
                            command_clone
                        ));
                    }
                    PermissionStatus::Unknown => {
                        return Err(format!(
                            "PERMISSION_REQUIRED:Permission required for '{}' (category: {:?})",
                            command_clone, analysis.category
                        ));
                    }
                }
            }

            // Execute the command synchronously (ForgeCmd::execute is async but we're in blocking context)
            // We need to create a small runtime for this
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| format!("Failed to create runtime: {}", e))?;

            let cmd_result = rt.block_on(async { forge_cmd.execute(&command_clone).await });

            match cmd_result {
                Ok(result) => Ok((result, analysis.category, analysis.risk_score)),
                Err(ForgeCmdError::PermissionRequired { description, .. }) => {
                    Err(format!("PERMISSION_REQUIRED:{}", description))
                }
                Err(ForgeCmdError::PermissionDenied(reason)) => Err(format!("DENIED:{}", reason)),
                Err(ForgeCmdError::Timeout(secs)) => {
                    Err(format!("Command timed out after {} seconds", secs))
                }
                Err(ForgeCmdError::ForbiddenCommand(cmd)) => {
                    Err(format!("FORBIDDEN:Forbidden command: {}", cmd))
                }
                Err(e) => Err(format!("Execution failed: {}", e)),
            }
        })
        .await;

        // Handle the result
        match result {
            Ok(Ok((cmd_result, category, risk_score))) => {
                let output = cmd_result.combined_output();
                let truncated = Self::truncate_output(&output);

                if cmd_result.success() {
                    ToolResult::success_with_metadata(
                        truncated,
                        serde_json::json!({
                            "exit_code": cmd_result.exit_code,
                            "duration_ms": cmd_result.duration_ms,
                            "category": format!("{:?}", category),
                            "risk_score": risk_score,
                        }),
                    )
                } else {
                    ToolResult::success_with_metadata(
                        truncated,
                        serde_json::json!({
                            "exit_code": cmd_result.exit_code,
                            "duration_ms": cmd_result.duration_ms,
                            "success": false,
                        }),
                    )
                }
            }
            Ok(Err(msg)) => {
                if msg.starts_with("FORBIDDEN:") || msg.starts_with("DENIED:") {
                    ToolResult::permission_denied(msg.split_once(':').unwrap().1)
                } else if msg.starts_with("PERMISSION_REQUIRED:") {
                    ToolResult::error(msg.split_once(':').unwrap().1)
                } else {
                    ToolResult::error(msg)
                }
            }
            Err(e) => ToolResult::error(format!("Task failed: {}", e)),
        }
    }
}

/// Grant permission for a command (to be called by CLI/TUI after user approval)
pub fn grant_command_permission(ctx: &ToolContext, command: &str, scope: PermissionScope) {
    let action = PermissionAction::Execute {
        command: command.to_string(),
    };

    match scope {
        PermissionScope::Once => {
            // One-time permissions are handled at execution time
        }
        PermissionScope::Session => {
            ctx.permissions.grant_session("forgecmd", action);
        }
        PermissionScope::Permanent => {
            let _ = ctx.permissions.grant_permanent("forgecmd", action);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_foundation::permission::PermissionService;
    use std::sync::Arc;

    fn create_test_context() -> ToolContext {
        let permissions = Arc::new(PermissionService::with_auto_approve());
        ToolContext::new("test-session", PathBuf::from("."), permissions).with_auto_approve()
    }

    #[tokio::test]
    async fn test_tool_definition() {
        let tool = ForgeCmdTool::new();
        let def = tool.definition();

        assert_eq!(def.name, "forgecmd");
        assert!(def.description.contains("PTY"));
    }

    #[tokio::test]
    async fn test_empty_command_error() {
        let tool = ForgeCmdTool::new();
        let ctx = create_test_context();

        let params = serde_json::json!({
            "command": ""
        });

        let result = tool.execute(&ctx, params).await;
        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[tokio::test]
    async fn test_execute_simple_command() {
        let tool = ForgeCmdTool::new();
        let ctx = create_test_context();

        let params = serde_json::json!({
            "command": "echo hello"
        });

        let result = tool.execute(&ctx, params).await;
        assert!(result.success);
        assert!(result.content.contains("hello"));
    }

    #[tokio::test]
    async fn test_forbidden_command() {
        let tool = ForgeCmdTool::new();
        let ctx = create_test_context();

        let params = serde_json::json!({
            "command": "rm -rf /"
        });

        let result = tool.execute(&ctx, params).await;
        assert!(!result.success);
    }
}
