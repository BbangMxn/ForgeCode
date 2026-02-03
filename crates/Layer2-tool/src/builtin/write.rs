//! Write tool - write/create files

use crate::{Tool, ToolContext, ToolDef, ToolResult};
use async_trait::async_trait;
use forge_foundation::permission::PermissionAction;
use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;

/// Write tool for creating/overwriting files
pub struct WriteTool;

#[derive(Debug, Deserialize)]
struct WriteParams {
    path: String,
    content: String,
}

impl WriteTool {
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

impl Default for WriteTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for WriteTool {
    fn definition(&self) -> ToolDef {
        ToolDef::builder(
            "write",
            "Create a new file or completely overwrite an existing file with new content. For partial edits, use the 'edit' tool instead.",
        )
        .string_param("path", "Path to the file to write (absolute or relative)", true)
        .string_param("content", "The complete content to write to the file", true)
        .build()
    }

    async fn execute(&self, ctx: &ToolContext, params: Value) -> ToolResult {
        // Parse parameters
        let params: WriteParams = match serde_json::from_value(params) {
            Ok(p) => p,
            Err(e) => return ToolResult::error(format!("Invalid parameters: {}", e)),
        };

        let file_path = Self::resolve_path(&ctx.working_dir, &params.path);

        // Check permission
        if !ctx.auto_approve {
            let action = PermissionAction::FileWrite {
                path: file_path.display().to_string(),
            };

            let permitted = ctx
                .permissions
                .request(
                    &ctx.session_id,
                    "write",
                    &format!("Write file: {}", file_path.display()),
                    action,
                )
                .await;

            match permitted {
                Ok(true) => {}
                Ok(false) => {
                    return ToolResult::permission_denied(format!(
                        "Write to: {}",
                        file_path.display()
                    ));
                }
                Err(e) => {
                    return ToolResult::permission_denied(format!(
                        "Could not verify permission: {}",
                        e
                    ));
                }
            }
        }

        // Create parent directories if needed
        if let Some(parent) = file_path.parent() {
            if !parent.exists() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    return ToolResult::error(format!("Failed to create directories: {}", e));
                }
            }
        }

        // Check if file exists (for metadata)
        let is_new = !file_path.exists();

        // Write file
        match std::fs::write(&file_path, &params.content) {
            Ok(_) => {
                let lines = params.content.lines().count();
                let bytes = params.content.len();

                ToolResult::success_with_metadata(
                    format!(
                        "{} file: {} ({} lines, {} bytes)",
                        if is_new { "Created" } else { "Wrote" },
                        file_path.display(),
                        lines,
                        bytes
                    ),
                    serde_json::json!({
                        "path": file_path.display().to_string(),
                        "created": is_new,
                        "lines": lines,
                        "bytes": bytes
                    }),
                )
            }
            Err(e) => ToolResult::error(format!("Failed to write file: {}", e)),
        }
    }
}
