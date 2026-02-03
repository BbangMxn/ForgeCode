//! Grep tool - search file contents

use crate::{Tool, ToolContext, ToolDef, ToolResult};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;

/// Maximum number of results to return
const MAX_RESULTS: usize = 100;

/// Grep tool for searching file contents
pub struct GrepTool;

#[derive(Debug, Deserialize)]
struct GrepParams {
    pattern: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    case_insensitive: Option<bool>,
    #[serde(default)]
    file_type: Option<String>,
    #[serde(default)]
    context: Option<usize>,
}

impl GrepTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for GrepTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for GrepTool {
    fn definition(&self) -> ToolDef {
        ToolDef::builder(
            "grep",
            "Search for a pattern in files using ripgrep. Returns matching lines with file paths and line numbers.",
        )
        .string_param(
            "pattern",
            "Regular expression pattern to search for",
            true,
        )
        .string_param(
            "path",
            "Directory or file to search in (default: current directory)",
            false,
        )
        .boolean_param(
            "case_insensitive",
            "Perform case-insensitive search (default: false)",
            false,
        )
        .string_param(
            "file_type",
            "Filter by file type (e.g., 'rs', 'py', 'js')",
            false,
        )
        .integer_param(
            "context",
            "Number of context lines to show before and after matches",
            false,
        )
        .build()
    }

    async fn execute(&self, ctx: &ToolContext, params: Value) -> ToolResult {
        // Parse parameters
        let params: GrepParams = match serde_json::from_value(params) {
            Ok(p) => p,
            Err(e) => return ToolResult::error(format!("Invalid parameters: {}", e)),
        };

        // Build search path
        let search_path = match &params.path {
            Some(p) => {
                let path = PathBuf::from(p);
                if path.is_absolute() {
                    path
                } else {
                    ctx.working_dir.join(path)
                }
            }
            None => ctx.working_dir.clone(),
        };

        // Try to use ripgrep (rg) first, fall back to grep
        let rg_available = which::which("rg").is_ok();

        let output = if rg_available {
            let mut cmd = Command::new("rg");
            cmd.arg("--line-number")
                .arg("--no-heading")
                .arg("--color=never")
                .arg("--max-count")
                .arg(MAX_RESULTS.to_string());

            if params.case_insensitive.unwrap_or(false) {
                cmd.arg("-i");
            }

            if let Some(ref file_type) = params.file_type {
                cmd.arg("-t").arg(file_type);
            }

            if let Some(context) = params.context {
                cmd.arg("-C").arg(context.to_string());
            }

            cmd.arg(&params.pattern).arg(&search_path);

            cmd.stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .await
        } else {
            // Fallback to grep
            let mut cmd = Command::new("grep");
            cmd.arg("-r")
                .arg("-n")
                .arg("--color=never");

            if params.case_insensitive.unwrap_or(false) {
                cmd.arg("-i");
            }

            if let Some(context) = params.context {
                cmd.arg("-C").arg(context.to_string());
            }

            if let Some(ref file_type) = params.file_type {
                cmd.arg("--include").arg(format!("*.{}", file_type));
            }

            cmd.arg(&params.pattern).arg(&search_path);

            cmd.stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .await
        };

        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                if !output.status.success() && stdout.is_empty() {
                    if stderr.contains("No such file") || stderr.contains("not found") {
                        return ToolResult::error(format!(
                            "Path not found: {}",
                            search_path.display()
                        ));
                    }

                    // No matches found (grep returns exit code 1)
                    return ToolResult::success_with_metadata(
                        "No matches found.",
                        serde_json::json!({
                            "matches": 0,
                            "pattern": params.pattern
                        }),
                    );
                }

                let lines: Vec<&str> = stdout.lines().collect();
                let match_count = lines.len();
                let truncated = match_count >= MAX_RESULTS;

                let result = if truncated {
                    format!(
                        "{}\n\n... (showing first {} matches, more exist)",
                        lines.join("\n"),
                        MAX_RESULTS
                    )
                } else {
                    lines.join("\n")
                };

                ToolResult::success_with_metadata(
                    result,
                    serde_json::json!({
                        "matches": match_count,
                        "pattern": params.pattern,
                        "truncated": truncated
                    }),
                )
            }
            Err(e) => ToolResult::error(format!("Search failed: {}", e)),
        }
    }
}
