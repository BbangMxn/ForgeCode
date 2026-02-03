//! Edit Tool - 파일 편집 도구
//!
//! 파일 내용을 부분적으로 편집합니다.
//! - 문자열 치환 (old_string → new_string)
//! - unique match 검증
//! - replace_all 옵션

use async_trait::async_trait;
use forge_foundation::{
    PermissionAction, PermissionDef, PermissionStatus, Result, Tool, ToolContext, ToolMeta,
    ToolResult,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

/// Edit 도구 입력
#[derive(Debug, Deserialize)]
pub struct EditInput {
    /// 파일 경로 (절대 경로 필수)
    pub file_path: String,

    /// 대체할 문자열 (현재 파일에 존재해야 함)
    pub old_string: String,

    /// 새 문자열 (old_string과 달라야 함)
    pub new_string: String,

    /// 모든 occurrence 대체 여부 (기본: false)
    #[serde(default)]
    pub replace_all: bool,
}

/// Edit 도구
pub struct EditTool;

impl EditTool {
    /// 새 인스턴스 생성
    pub fn new() -> Self {
        Self
    }

    /// 도구 이름
    pub const NAME: &'static str = "edit";

    /// 민감한 파일인지 확인
    fn is_sensitive_path(path: &str) -> bool {
        let sensitive_patterns = [
            ".env",
            ".ssh",
            "credentials",
            "secrets",
            ".pem",
            ".key",
            "_rsa",
            ".aws",
            ".config/gcloud",
        ];

        let path_lower = path.to_lowercase();
        sensitive_patterns.iter().any(|p| path_lower.contains(p))
    }
}

impl Default for EditTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for EditTool {
    fn meta(&self) -> ToolMeta {
        ToolMeta::new(Self::NAME)
            .display_name("Edit File")
            .description("Performs exact string replacements in files")
            .category("filesystem")
            .permission(
                PermissionDef::new("file.edit", "filesystem")
                    .risk_level(6)
                    .description("Edit file contents")
                    .requires_confirmation(true),
            )
    }

    fn name(&self) -> &str {
        Self::NAME
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The absolute path to the file to modify"
                },
                "old_string": {
                    "type": "string",
                    "description": "The text to replace"
                },
                "new_string": {
                    "type": "string",
                    "description": "The text to replace it with (must be different from old_string)"
                },
                "replace_all": {
                    "type": "boolean",
                    "description": "Replace all occurrences of old_string (default: false)",
                    "default": false
                }
            },
            "required": ["file_path", "old_string", "new_string"]
        })
    }

    fn required_permission(&self, input: &Value) -> Option<PermissionAction> {
        let path = input.get("file_path")?.as_str()?;
        Some(PermissionAction::FileWrite {
            path: path.to_string(),
        })
    }

    async fn execute(&self, input: Value, context: &dyn ToolContext) -> Result<ToolResult> {
        // 입력 파싱
        let parsed: EditInput = serde_json::from_value(input.clone()).map_err(|e| {
            forge_foundation::Error::InvalidInput(format!("Invalid input: {}", e))
        })?;

        let path = Path::new(&parsed.file_path);

        // 경로 검증 - 절대 경로 필수
        if !path.is_absolute() {
            return Ok(ToolResult::error(format!(
                "Path must be absolute, got: {}",
                parsed.file_path
            )));
        }

        // 파일 존재 확인
        if !path.exists() {
            return Ok(ToolResult::error(format!(
                "File not found: {}",
                parsed.file_path
            )));
        }

        // 디렉토리인지 확인
        if path.is_dir() {
            return Ok(ToolResult::error(format!(
                "Cannot edit directory: {}",
                parsed.file_path
            )));
        }

        // old_string과 new_string이 같으면 에러
        if parsed.old_string == parsed.new_string {
            return Ok(ToolResult::error(
                "old_string and new_string must be different",
            ));
        }

        // old_string이 비어있으면 에러
        if parsed.old_string.is_empty() {
            return Ok(ToolResult::error("old_string cannot be empty"));
        }

        // 민감한 파일 경고
        if Self::is_sensitive_path(&parsed.file_path) {
            return Ok(ToolResult::error(format!(
                "Cannot edit sensitive file: {}",
                parsed.file_path
            )));
        }

        // 권한 확인
        if let Some(action) = self.required_permission(&input) {
            let status = context.check_permission(Self::NAME, &action).await;
            match status {
                PermissionStatus::Denied => {
                    return Ok(ToolResult::error("Permission denied for file edit"));
                }
                PermissionStatus::Unknown => {
                    let granted = context
                        .request_permission(
                            Self::NAME,
                            &format!("Edit file: {}", parsed.file_path),
                            action,
                        )
                        .await?;
                    if !granted {
                        return Ok(ToolResult::error("Permission denied by user"));
                    }
                }
                _ => {}
            }
        }

        // 파일 읽기
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                return Ok(ToolResult::error(format!("Failed to read file: {}", e)));
            }
        };

        // old_string이 파일에 존재하는지 확인
        let match_count = content.matches(&parsed.old_string).count();

        if match_count == 0 {
            return Ok(ToolResult::error(format!(
                "old_string not found in file. Make sure to include exact content including whitespace and indentation."
            )));
        }

        // replace_all이 false인데 여러 개 존재하면 에러
        if !parsed.replace_all && match_count > 1 {
            return Ok(ToolResult::error(format!(
                "old_string found {} times in file. Either provide a larger string with more context to make it unique, or set replace_all to true.",
                match_count
            )));
        }

        // 문자열 치환
        let new_content = if parsed.replace_all {
            content.replace(&parsed.old_string, &parsed.new_string)
        } else {
            content.replacen(&parsed.old_string, &parsed.new_string, 1)
        };

        // 파일 쓰기
        match fs::write(path, &new_content) {
            Ok(()) => {
                let replaced = if parsed.replace_all {
                    format!("{} occurrences", match_count)
                } else {
                    "1 occurrence".to_string()
                };

                Ok(ToolResult::success(format!(
                    "Edited {}: replaced {}",
                    parsed.file_path, replaced
                )))
            }
            Err(e) => Ok(ToolResult::error(format!("Failed to write file: {}", e))),
        }
    }
}

// ============================================================================
// 테스트
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_meta() {
        let tool = EditTool::new();
        let meta = tool.meta();
        assert_eq!(meta.name, "edit");
        assert_eq!(meta.category, "filesystem");
    }

    #[test]
    fn test_schema() {
        let tool = EditTool::new();
        let schema = tool.schema();
        assert!(schema.get("properties").is_some());
        assert!(schema["properties"]["file_path"].is_object());
        assert!(schema["properties"]["old_string"].is_object());
        assert!(schema["properties"]["new_string"].is_object());
    }

    #[test]
    fn test_required_permission() {
        let tool = EditTool::new();
        let input = json!({
            "file_path": "/tmp/test.txt",
            "old_string": "old",
            "new_string": "new"
        });
        let perm = tool.required_permission(&input);
        assert!(perm.is_some());
        match perm.unwrap() {
            PermissionAction::FileWrite { path } => {
                assert_eq!(path, "/tmp/test.txt");
            }
            _ => panic!("Expected FileWrite permission"),
        }
    }

    #[test]
    fn test_sensitive_path_detection() {
        assert!(EditTool::is_sensitive_path("/home/user/.env"));
        assert!(EditTool::is_sensitive_path("/app/.ssh/config"));
        assert!(!EditTool::is_sensitive_path("/home/user/code.rs"));
    }
}
