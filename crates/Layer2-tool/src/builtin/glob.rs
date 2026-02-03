//! Glob tool - find files by pattern

use crate::{Tool, ToolContext, ToolDef, ToolResult};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;

/// Maximum number of results to return
const MAX_RESULTS: usize = 200;

/// Glob tool for finding files by pattern
pub struct GlobTool;

#[derive(Debug, Deserialize)]
struct GlobParams {
    pattern: String,
    #[serde(default)]
    path: Option<String>,
}

impl GlobTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for GlobTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for GlobTool {
    fn definition(&self) -> ToolDef {
        ToolDef::builder(
            "glob",
            "Find files matching a glob pattern. Returns list of matching file paths.",
        )
        .string_param(
            "pattern",
            "Glob pattern (e.g., '**/*.rs', 'src/**/*.ts', '*.json')",
            true,
        )
        .string_param(
            "path",
            "Base directory to search from (default: current directory)",
            false,
        )
        .build()
    }

    async fn execute(&self, ctx: &ToolContext, params: Value) -> ToolResult {
        // Parse parameters
        let params: GlobParams = match serde_json::from_value(params) {
            Ok(p) => p,
            Err(e) => return ToolResult::error(format!("Invalid parameters: {}", e)),
        };

        // Build search path
        let base_path = match &params.path {
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

        // Build full pattern
        let full_pattern = base_path.join(&params.pattern);
        let pattern_str = full_pattern.to_string_lossy();

        // Execute glob
        let paths = match glob::glob(&pattern_str) {
            Ok(paths) => paths,
            Err(e) => return ToolResult::error(format!("Invalid glob pattern: {}", e)),
        };

        let mut results: Vec<String> = Vec::new();
        let mut error_count = 0;

        for entry in paths {
            if results.len() >= MAX_RESULTS {
                break;
            }

            match entry {
                Ok(path) => {
                    // Make path relative to working directory if possible
                    let display_path = path
                        .strip_prefix(&ctx.working_dir)
                        .map(|p| p.to_path_buf())
                        .unwrap_or(path);

                    results.push(display_path.display().to_string());
                }
                Err(_) => {
                    error_count += 1;
                }
            }
        }

        // Sort results for consistent output
        results.sort();

        let truncated = results.len() >= MAX_RESULTS;
        let result_count = results.len();

        if results.is_empty() {
            ToolResult::success_with_metadata(
                "No files found matching pattern.",
                serde_json::json!({
                    "matches": 0,
                    "pattern": params.pattern
                }),
            )
        } else {
            let output = if truncated {
                format!(
                    "{}\n\n... (showing first {} matches, more may exist)",
                    results.join("\n"),
                    MAX_RESULTS
                )
            } else {
                results.join("\n")
            };

            ToolResult::success_with_metadata(
                output,
                serde_json::json!({
                    "matches": result_count,
                    "pattern": params.pattern,
                    "truncated": truncated,
                    "errors": error_count
                }),
            )
        }
    }
}
