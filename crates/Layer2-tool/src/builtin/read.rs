//! Read tool - read file contents

use crate::{Tool, ToolContext, ToolDef, ToolResult};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;

/// Maximum file size to read (50KB)
const MAX_FILE_SIZE: u64 = 50 * 1024;

/// Default line limit
const DEFAULT_LINE_LIMIT: usize = 2000;

/// Read tool for reading file contents
pub struct ReadTool;

#[derive(Debug, Deserialize)]
struct ReadParams {
    path: String,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    limit: Option<usize>,
}

impl ReadTool {
    pub fn new() -> Self {
        Self
    }

    fn resolve_path(working_dir: &PathBuf, path: &str) -> PathBuf {
        let path = PathBuf::from(path);
        if path.is_absolute() {
            path
        } else {
            working_dir.join(path)
        }
    }
}

impl Default for ReadTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for ReadTool {
    fn definition(&self) -> ToolDef {
        ToolDef::builder(
            "read",
            "Read the contents of a file. Returns the file content as text.",
        )
        .string_param("path", "Path to the file to read (absolute or relative)", true)
        .integer_param(
            "offset",
            "Line number to start reading from (1-based, default: 1)",
            false,
        )
        .integer_param(
            "limit",
            "Maximum number of lines to read (default: 2000)",
            false,
        )
        .build()
    }

    async fn execute(&self, ctx: &ToolContext, params: Value) -> ToolResult {
        // Parse parameters
        let params: ReadParams = match serde_json::from_value(params) {
            Ok(p) => p,
            Err(e) => return ToolResult::error(format!("Invalid parameters: {}", e)),
        };

        let file_path = Self::resolve_path(&ctx.working_dir, &params.path);

        // Check if file exists
        if !file_path.exists() {
            return ToolResult::error(format!("File not found: {}", file_path.display()));
        }

        // Check if it's a file (not directory)
        if !file_path.is_file() {
            return ToolResult::error(format!(
                "Path is not a file: {}. Use 'bash' with 'ls' to list directories.",
                file_path.display()
            ));
        }

        // Check file size
        let metadata = match std::fs::metadata(&file_path) {
            Ok(m) => m,
            Err(e) => return ToolResult::error(format!("Cannot read file metadata: {}", e)),
        };

        if metadata.len() > MAX_FILE_SIZE {
            return ToolResult::error(format!(
                "File too large ({} bytes). Maximum size is {} bytes. Use offset and limit parameters to read portions.",
                metadata.len(),
                MAX_FILE_SIZE
            ));
        }

        // Read file
        let content = match std::fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(e) => {
                // Try reading as binary and show hex
                return match std::fs::read(&file_path) {
                    Ok(bytes) => ToolResult::success(format!(
                        "(Binary file, {} bytes)\nFirst 100 bytes (hex): {:?}",
                        bytes.len(),
                        &bytes[..bytes.len().min(100)]
                    )),
                    Err(_) => ToolResult::error(format!("Cannot read file: {}", e)),
                };
            }
        };

        // Apply offset and limit
        let offset = params.offset.unwrap_or(1).saturating_sub(1);
        let limit = params.limit.unwrap_or(DEFAULT_LINE_LIMIT);

        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();

        let selected_lines: Vec<&str> = lines.into_iter().skip(offset).take(limit).collect();

        let result = selected_lines.join("\n");

        // Add metadata about truncation
        if offset > 0 || total_lines > offset + limit {
            let end_line = (offset + selected_lines.len()).min(total_lines);
            ToolResult::success_with_metadata(
                result,
                serde_json::json!({
                    "path": file_path.display().to_string(),
                    "total_lines": total_lines,
                    "showing_lines": format!("{}-{}", offset + 1, end_line),
                    "truncated": total_lines > offset + limit
                }),
            )
        } else {
            ToolResult::success_with_metadata(
                result,
                serde_json::json!({
                    "path": file_path.display().to_string(),
                    "total_lines": total_lines
                }),
            )
        }
    }
}
