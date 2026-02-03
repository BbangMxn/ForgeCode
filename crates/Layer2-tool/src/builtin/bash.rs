//! Bash tool - execute shell commands

use crate::{Tool, ToolContext, ToolDef, ToolResult};
use async_trait::async_trait;
use forge_foundation::permission::PermissionAction;
use serde::Deserialize;
use serde_json::Value;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

/// Maximum output length before truncation
const MAX_OUTPUT_LENGTH: usize = 30000;

/// Default timeout in seconds
const DEFAULT_TIMEOUT_SECS: u64 = 60;

/// Maximum timeout in seconds
const MAX_TIMEOUT_SECS: u64 = 600;

/// Commands that are blocked for security
const BLOCKED_COMMANDS: &[&str] = &["curl", "wget", "nc", "telnet"];

/// Commands that are safe (no permission needed)
const SAFE_COMMANDS: &[&str] = &[
    "ls", "pwd", "echo", "cat", "head", "tail", "wc", "find", "which", "type", "git status",
    "git log", "git branch", "git diff", "cargo check", "cargo build", "cargo test", "npm list",
    "node --version", "python --version", "rustc --version",
];

/// Bash tool for executing shell commands
pub struct BashTool;

#[derive(Debug, Deserialize)]
struct BashParams {
    command: String,
    #[serde(default)]
    timeout: Option<u64>,
}

impl BashTool {
    pub fn new() -> Self {
        Self
    }

    fn is_blocked_command(command: &str) -> bool {
        let first_word = command.split_whitespace().next().unwrap_or("");
        BLOCKED_COMMANDS.contains(&first_word)
    }

    fn is_safe_command(command: &str) -> bool {
        // Check if the command starts with any safe command
        for safe in SAFE_COMMANDS {
            if command.starts_with(safe) {
                return true;
            }
        }
        false
    }

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

impl Default for BashTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for BashTool {
    fn definition(&self) -> ToolDef {
        ToolDef::builder(
            "bash",
            "Execute a shell command. Use this for running programs, scripts, git commands, etc.",
        )
        .string_param(
            "command",
            "The shell command to execute",
            true,
        )
        .integer_param(
            "timeout",
            "Timeout in seconds (default: 60, max: 600)",
            false,
        )
        .build()
    }

    async fn execute(&self, ctx: &ToolContext, params: Value) -> ToolResult {
        // Parse parameters
        let params: BashParams = match serde_json::from_value(params) {
            Ok(p) => p,
            Err(e) => return ToolResult::error(format!("Invalid parameters: {}", e)),
        };

        let command = params.command.trim();

        // Check for blocked commands
        if Self::is_blocked_command(command) {
            return ToolResult::error(format!(
                "Command '{}' is blocked for security reasons",
                command.split_whitespace().next().unwrap_or("")
            ));
        }

        // Check permission for non-safe commands
        if !ctx.auto_approve && !Self::is_safe_command(command) {
            let action = PermissionAction::Execute {
                command: command.to_string(),
            };

            let permitted = ctx
                .permissions
                .request(
                    &ctx.session_id,
                    "bash",
                    &format!("Execute command: {}", command),
                    action,
                )
                .await;

            match permitted {
                Ok(true) => {}
                Ok(false) => {
                    return ToolResult::permission_denied(format!("Command: {}", command));
                }
                Err(e) => {
                    // If permission system unavailable and not auto-approve, deny
                    return ToolResult::permission_denied(format!(
                        "Could not verify permission: {}",
                        e
                    ));
                }
            }
        }

        // Calculate timeout
        let timeout_secs = params
            .timeout
            .unwrap_or(DEFAULT_TIMEOUT_SECS)
            .min(MAX_TIMEOUT_SECS);

        // Determine shell based on platform
        let (shell, shell_arg) = if cfg!(windows) {
            ("cmd", "/C")
        } else {
            ("sh", "-c")
        };

        // Execute command
        let result = timeout(
            Duration::from_secs(timeout_secs),
            Command::new(shell)
                .arg(shell_arg)
                .arg(command)
                .current_dir(&ctx.working_dir)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output(),
        )
        .await;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                let mut content = String::new();

                if !stdout.is_empty() {
                    content.push_str(&Self::truncate_output(&stdout));
                }

                if !stderr.is_empty() {
                    if !content.is_empty() {
                        content.push_str("\n\n--- stderr ---\n");
                    }
                    content.push_str(&Self::truncate_output(&stderr));
                }

                if content.is_empty() {
                    content = "(no output)".to_string();
                }

                if output.status.success() {
                    ToolResult::success(content)
                } else {
                    ToolResult::success_with_metadata(
                        content,
                        serde_json::json!({
                            "exit_code": output.status.code()
                        }),
                    )
                }
            }
            Ok(Err(e)) => ToolResult::error(format!("Failed to execute command: {}", e)),
            Err(_) => ToolResult::error(format!(
                "Command timed out after {} seconds",
                timeout_secs
            )),
        }
    }
}
