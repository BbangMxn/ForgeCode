//! Edit tool - edit portions of files

use crate::{Tool, ToolContext, ToolDef, ToolResult};
use async_trait::async_trait;
use forge_foundation::permission::PermissionAction;
use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;

/// Edit tool for modifying portions of files
pub struct EditTool;

#[derive(Debug, Deserialize)]
struct EditParams {
    path: String,
    old_string: String,
    new_string: String,
    #[serde(default)]
    replace_all: bool,
}

impl EditTool {
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

impl Default for EditTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for EditTool {
    fn definition(&self) -> ToolDef {
        ToolDef::builder(
            "edit",
            "Edit a file by replacing a specific string with new content. The old_string must uniquely identify the location to edit - include enough context (3-5 surrounding lines) to make it unique.",
        )
        .string_param("path", "Path to the file to edit (absolute or relative)", true)
        .string_param(
            "old_string",
            "The exact string to find and replace. Must be unique in the file unless replace_all is true.",
            true,
        )
        .string_param(
            "new_string",
            "The string to replace old_string with. Use empty string to delete.",
            true,
        )
        .boolean_param(
            "replace_all",
            "Replace all occurrences instead of requiring uniqueness (default: false)",
            false,
        )
        .build()
    }

    async fn execute(&self, ctx: &ToolContext, params: Value) -> ToolResult {
        // Parse parameters
        let params: EditParams = match serde_json::from_value(params) {
            Ok(p) => p,
            Err(e) => return ToolResult::error(format!("Invalid parameters: {}", e)),
        };

        let file_path = Self::resolve_path(&ctx.working_dir, &params.path);

        // Handle file creation (empty old_string)
        if params.old_string.is_empty() {
            // This is a create operation
            if file_path.exists() {
                return ToolResult::error(
                    "File already exists. Use non-empty old_string to edit, or use 'write' tool to overwrite.",
                );
            }

            // Check permission for file write
            if !ctx.auto_approve {
                let action = PermissionAction::FileWrite {
                    path: file_path.display().to_string(),
                };

                let permitted = ctx
                    .permissions
                    .request(
                        &ctx.session_id,
                        "edit",
                        &format!("Create file: {}", file_path.display()),
                        action,
                    )
                    .await;

                match permitted {
                    Ok(true) => {}
                    Ok(false) => {
                        return ToolResult::permission_denied(format!(
                            "Create: {}",
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

            // Create parent directories
            if let Some(parent) = file_path.parent() {
                if !parent.exists() {
                    if let Err(e) = std::fs::create_dir_all(parent) {
                        return ToolResult::error(format!("Failed to create directories: {}", e));
                    }
                }
            }

            // Write new file
            match std::fs::write(&file_path, &params.new_string) {
                Ok(_) => {
                    return ToolResult::success_with_metadata(
                        format!("Created file: {}", file_path.display()),
                        serde_json::json!({
                            "path": file_path.display().to_string(),
                            "created": true,
                            "lines_added": params.new_string.lines().count()
                        }),
                    );
                }
                Err(e) => return ToolResult::error(format!("Failed to create file: {}", e)),
            }
        }

        // Check if file exists for edit
        if !file_path.exists() {
            return ToolResult::error(format!(
                "File not found: {}. Use empty old_string to create a new file.",
                file_path.display()
            ));
        }

        // Read current content
        let content = match std::fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(e) => return ToolResult::error(format!("Cannot read file: {}", e)),
        };

        // Find occurrences
        let matches: Vec<_> = content.match_indices(&params.old_string).collect();

        if matches.is_empty() {
            return ToolResult::error(format!(
                "String not found in file. Make sure old_string exactly matches the content including whitespace and indentation."
            ));
        }

        if matches.len() > 1 && !params.replace_all {
            return ToolResult::error(format!(
                "Found {} occurrences of old_string. Either:\n1. Include more context to make it unique\n2. Set replace_all: true to replace all occurrences",
                matches.len()
            ));
        }

        // Check permission
        if !ctx.auto_approve {
            let action = PermissionAction::FileWrite {
                path: file_path.display().to_string(),
            };

            let permitted = ctx
                .permissions
                .request(
                    &ctx.session_id,
                    "edit",
                    &format!(
                        "Edit file: {} ({} replacement{})",
                        file_path.display(),
                        matches.len(),
                        if matches.len() > 1 { "s" } else { "" }
                    ),
                    action,
                )
                .await;

            match permitted {
                Ok(true) => {}
                Ok(false) => {
                    return ToolResult::permission_denied(format!("Edit: {}", file_path.display()));
                }
                Err(e) => {
                    return ToolResult::permission_denied(format!(
                        "Could not verify permission: {}",
                        e
                    ));
                }
            }
        }

        // Perform replacement
        let new_content = if params.replace_all {
            content.replace(&params.old_string, &params.new_string)
        } else {
            content.replacen(&params.old_string, &params.new_string, 1)
        };

        // Calculate diff stats
        let old_lines = params.old_string.lines().count();
        let new_lines = params.new_string.lines().count();
        let lines_added = new_lines as i32 - old_lines as i32;

        // Write back
        match std::fs::write(&file_path, &new_content) {
            Ok(_) => ToolResult::success_with_metadata(
                format!(
                    "Edited {}: {} replacement{}, {} lines",
                    file_path.display(),
                    matches.len(),
                    if matches.len() > 1 { "s" } else { "" },
                    if lines_added >= 0 {
                        format!("+{}", lines_added)
                    } else {
                        format!("{}", lines_added)
                    }
                ),
                serde_json::json!({
                    "path": file_path.display().to_string(),
                    "replacements": matches.len(),
                    "lines_changed": lines_added
                }),
            ),
            Err(e) => ToolResult::error(format!("Failed to write file: {}", e)),
        }
    }
}
