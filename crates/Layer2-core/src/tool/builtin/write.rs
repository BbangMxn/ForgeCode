//! Write Tool - 파일 쓰기 도구
//!
//! 파일 내용을 쓰거나 덮어씁니다.
//! - 새 파일 생성
//! - 기존 파일 덮어쓰기
//! - 부모 디렉토리 자동 생성
//! - 경로 보안 검증 (path traversal 방지)

use async_trait::async_trait;
use forge_foundation::{
    PermissionAction, PermissionDef, PermissionStatus, Result, Tool, ToolContext, ToolMeta,
    ToolResult,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

use crate::tool::security::{is_sensitive_path, PathValidator};

/// Write 도구 입력
#[derive(Debug, Deserialize)]
pub struct WriteInput {
    /// 파일 경로 (file_path, path, file 모두 허용)
    #[serde(alias = "path", alias = "file")]
    pub file_path: String,

    /// 작성할 내용
    pub content: String,

    /// 부모 디렉토리 자동 생성 여부 (기본: true)
    #[serde(default = "default_create_dirs")]
    pub create_directories: bool,
}

fn default_create_dirs() -> bool {
    true
}

/// Write 도구
pub struct WriteTool;

impl WriteTool {
    /// 새 인스턴스 생성
    pub fn new() -> Self {
        Self
    }

    /// 도구 이름
    pub const NAME: &'static str = "write";
}

impl Default for WriteTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for WriteTool {
    fn meta(&self) -> ToolMeta {
        ToolMeta::new(Self::NAME)
            .display_name("Write File")
            .description("Write content to a file (creates or overwrites)")
            .category("filesystem")
            .permission(
                PermissionDef::new("file.write", "filesystem")
                    .risk_level(6)
                    .description("Write to file")
                    .requires_confirmation(true),
            )
            .permission(
                PermissionDef::new("file.write.system", "filesystem")
                    .risk_level(9)
                    .description("Write to system file")
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
                    "description": "Path to the file to write. Can be absolute or relative to the working directory."
                },
                "content": {
                    "type": "string",
                    "description": "The content to write to the file"
                },
                "create_directories": {
                    "type": "boolean",
                    "description": "Create parent directories if they don't exist (default: true)",
                    "default": true
                }
            },
            "required": ["file_path", "content"]
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
        let parsed: WriteInput = serde_json::from_value(input.clone())
            .map_err(|e| forge_foundation::Error::InvalidInput(format!("Invalid input: {}", e)))?;

        let input_path = Path::new(&parsed.file_path);

        // 상대 경로인 경우 working directory 기준으로 변환
        let path = if input_path.is_absolute() {
            input_path.to_path_buf()
        } else {
            context.working_dir().join(input_path)
        };
        let path = path.as_path();

        // 경로 보안 검증 (path traversal, 위험 경로, 민감 파일 체크)
        let validator = PathValidator::new().with_allowed_root(context.working_dir());

        let validation = validator.validate(path);
        if !validation.is_valid() {
            if let Some(msg) = validation.error_message() {
                return Ok(ToolResult::error(format!(
                    "Path security check failed: {}",
                    msg
                )));
            }
        }

        // 민감한 파일 추가 체크
        if is_sensitive_path(&parsed.file_path) {
            return Ok(ToolResult::error(format!(
                "Cannot write to sensitive file: {}. This could expose credentials or damage security configurations.",
                parsed.file_path
            )));
        }

        // 권한 확인
        if let Some(action) = self.required_permission(&input) {
            let status = context.check_permission(Self::NAME, &action).await;
            match status {
                PermissionStatus::Denied => {
                    return Ok(ToolResult::error("Permission denied for file write"));
                }
                PermissionStatus::Unknown => {
                    let granted = context
                        .request_permission(
                            Self::NAME,
                            &format!("Write file: {}", parsed.file_path),
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

        // 디렉토리가 존재하지 않으면 생성
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                if parsed.create_directories {
                    if let Err(e) = fs::create_dir_all(parent) {
                        return Ok(ToolResult::error(format!(
                            "Failed to create directory {}: {}",
                            parent.display(),
                            e
                        )));
                    }
                } else {
                    return Ok(ToolResult::error(format!(
                        "Parent directory does not exist: {}",
                        parent.display()
                    )));
                }
            }
        }

        // 기존 파일 존재 확인
        let existed = path.exists();

        // 파일 쓰기
        match fs::write(path, &parsed.content) {
            Ok(()) => {
                let bytes = parsed.content.len();
                let lines = parsed.content.lines().count();
                let action = if existed { "Updated" } else { "Created" };

                Ok(ToolResult::success(format!(
                    "{} {} ({} bytes, {} lines)",
                    action, parsed.file_path, bytes, lines
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
        let tool = WriteTool::new();
        let meta = tool.meta();
        assert_eq!(meta.name, "write");
        assert_eq!(meta.category, "filesystem");
    }

    #[test]
    fn test_schema() {
        let tool = WriteTool::new();
        let schema = tool.schema();
        assert!(schema.get("properties").is_some());
        assert!(schema["properties"]["file_path"].is_object());
        assert!(schema["properties"]["content"].is_object());
    }

    #[test]
    fn test_required_permission() {
        let tool = WriteTool::new();
        let input = json!({ "file_path": "/tmp/test.txt", "content": "hello" });
        let perm = tool.required_permission(&input);
        assert!(perm.is_some());
        let perm = perm.unwrap();
        assert!(
            matches!(&perm, PermissionAction::FileWrite { path } if path == "/tmp/test.txt"),
            "Expected FileWrite permission, got {:?}",
            perm
        );
    }

    #[test]
    fn test_sensitive_path_detection() {
        use crate::tool::security::is_sensitive_path;
        // Cross-platform tests
        assert!(is_sensitive_path(".env"));
        assert!(is_sensitive_path(".ssh/config"));
        assert!(is_sensitive_path("credentials.json"));
        assert!(!is_sensitive_path("code.rs"));
    }

    #[test]
    fn test_dangerous_path_detection() {
        use crate::tool::security::PathValidator;
        let validator = PathValidator::new();

        // Test with .ssh path which is dangerous on all platforms
        let dangerous = if cfg!(windows) {
            std::path::Path::new("C:\\Users\\test\\.ssh\\id_rsa")
        } else {
            std::path::Path::new("/home/user/.ssh/id_rsa")
        };
        let result = validator.validate(dangerous);
        assert!(!result.is_valid());

        // Normal path should be valid when no allowed roots set
        let normal = if cfg!(windows) {
            std::path::Path::new("C:\\Users\\test\\file.txt")
        } else {
            std::path::Path::new("/home/user/file.txt")
        };
        let result = validator.validate(normal);
        assert!(result.is_valid());
    }
}
