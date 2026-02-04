//! Read Tool - 파일 읽기 도구
//!
//! 파일 내용을 읽어서 반환합니다.
//! - 줄 번호 포함 (cat -n 스타일)
//! - offset/limit 지원 (대용량 파일 처리)
//! - 이미지/PDF 등 바이너리 파일 감지
//! - 경로 보안 검증 (path traversal 방지)

use async_trait::async_trait;
use forge_foundation::{
    PermissionAction, PermissionDef, Result, Tool, ToolContext, ToolMeta, ToolResult,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;

use crate::tool::security::{is_sensitive_path, PathValidator};

/// Read 도구 입력
#[derive(Debug, Deserialize)]
pub struct ReadInput {
    /// 파일 경로 (절대 경로 필수)
    pub file_path: String,

    /// 시작 줄 번호 (1-based, optional)
    #[serde(default)]
    pub offset: Option<u32>,

    /// 최대 읽을 줄 수 (optional, 기본: 2000)
    #[serde(default)]
    pub limit: Option<u32>,
}

/// Read 도구
pub struct ReadTool;

impl ReadTool {
    /// 새 인스턴스 생성
    pub fn new() -> Self {
        Self
    }

    /// 도구 이름
    pub const NAME: &'static str = "read";

    /// 기본 줄 제한
    const DEFAULT_LIMIT: u32 = 2000;

    /// 최대 줄 길이 (이 이상은 잘림)
    const MAX_LINE_LENGTH: usize = 2000;


    /// 바이너리 파일인지 확인
    fn is_binary_file(path: &Path) -> bool {
        let binary_extensions = [
            "png", "jpg", "jpeg", "gif", "bmp", "ico", "webp", // 이미지
            "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", // 문서
            "zip", "tar", "gz", "rar", "7z", // 압축
            "exe", "dll", "so", "dylib", // 실행
            "mp3", "mp4", "avi", "mov", "mkv", // 미디어
            "woff", "woff2", "ttf", "otf", // 폰트
        ];

        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| binary_extensions.contains(&e.to_lowercase().as_str()))
            .unwrap_or(false)
    }

    /// 파일을 줄 번호와 함께 읽기
    fn read_with_line_numbers(path: &Path, offset: u32, limit: u32) -> Result<String> {
        let file = fs::File::open(path)?;
        let reader = BufReader::new(file);

        let mut output = String::new();
        let start_line = offset.max(1) as usize;
        let end_line = start_line + limit as usize;

        for (idx, line_result) in reader.lines().enumerate() {
            let line_num = idx + 1;

            // offset 이전 줄 건너뛰기
            if line_num < start_line {
                continue;
            }

            // limit 초과 시 중단
            if line_num >= end_line {
                break;
            }

            let line = line_result?;

            // 줄 길이 제한
            let truncated = if line.len() > Self::MAX_LINE_LENGTH {
                format!("{}... [truncated]", &line[..Self::MAX_LINE_LENGTH])
            } else {
                line
            };

            // 줄 번호 포맷: "   123→내용"
            output.push_str(&format!("{:>6}→{}\n", line_num, truncated));
        }

        Ok(output)
    }
}

impl Default for ReadTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for ReadTool {
    fn meta(&self) -> ToolMeta {
        ToolMeta::new(Self::NAME)
            .display_name("Read File")
            .description("Read file contents with line numbers")
            .category("filesystem")
            .permission(
                PermissionDef::new("file.read.sensitive", "filesystem")
                    .risk_level(7)
                    .description("Read sensitive file (credentials, keys)")
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
                    "description": "Absolute path to the file to read"
                },
                "offset": {
                    "type": "integer",
                    "description": "Start line number (1-based). Only provide if the file is too large to read at once."
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum lines to read (default: 2000). Only provide if the file is too large to read at once."
                }
            },
            "required": ["file_path"]
        })
    }

    fn required_permission(&self, input: &Value) -> Option<PermissionAction> {
        let path = input.get("file_path")?.as_str()?;

        // 민감한 파일인 경우에만 권한 필요
        if is_sensitive_path(path) {
            Some(PermissionAction::FileReadSensitive {
                path: path.to_string(),
            })
        } else {
            None
        }
    }

    async fn execute(&self, input: Value, context: &dyn ToolContext) -> Result<ToolResult> {
        // 입력 파싱
        let parsed: ReadInput = serde_json::from_value(input.clone()).map_err(|e| {
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

        // 경로 보안 검증 (path traversal, 위험 경로 체크)
        let validator = PathValidator::new()
            .with_allowed_root(context.working_dir());

        let validation = validator.validate(path);
        if !validation.is_valid() {
            if let Some(msg) = validation.error_message() {
                return Ok(ToolResult::error(format!("Path security check failed: {}", msg)));
            }
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
                "Cannot read directory: {}. Use glob or ls to list directory contents.",
                parsed.file_path
            )));
        }

        // 권한 확인
        if let Some(action) = self.required_permission(&input) {
            let status = context.check_permission(Self::NAME, &action).await;
            match status {
                forge_foundation::PermissionStatus::Denied => {
                    return Ok(ToolResult::error("Permission denied for sensitive file"));
                }
                forge_foundation::PermissionStatus::Unknown => {
                    let granted = context
                        .request_permission(
                            Self::NAME,
                            &format!("Read sensitive file: {}", parsed.file_path),
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

        // 바이너리 파일 체크
        if Self::is_binary_file(path) {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            return Ok(ToolResult::success(format!(
                "[Binary file: {} - use appropriate viewer for {} files]",
                parsed.file_path, ext
            )));
        }

        // 파일 읽기
        let offset = parsed.offset.unwrap_or(1);
        let limit = parsed.limit.unwrap_or(Self::DEFAULT_LIMIT);

        match Self::read_with_line_numbers(path, offset, limit) {
            Ok(content) => {
                if content.is_empty() {
                    Ok(ToolResult::success("[Empty file]"))
                } else {
                    Ok(ToolResult::success(content))
                }
            }
            Err(e) => Ok(ToolResult::error(format!("Failed to read file: {}", e))),
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
        let tool = ReadTool::new();
        let meta = tool.meta();
        assert_eq!(meta.name, "read");
        assert_eq!(meta.category, "filesystem");
    }

    #[test]
    fn test_schema() {
        let tool = ReadTool::new();
        let schema = tool.schema();
        assert!(schema.get("properties").is_some());
        assert!(schema["properties"]["file_path"].is_object());
    }

    #[test]
    fn test_sensitive_path_detection() {
        use crate::tool::security::is_sensitive_path;
        assert!(is_sensitive_path("/home/user/.ssh/id_rsa"));
        assert!(is_sensitive_path("/app/.env"));
        assert!(is_sensitive_path("/secrets/api.key"));
        assert!(!is_sensitive_path("/home/user/code.rs"));
    }

    #[test]
    fn test_binary_file_detection() {
        assert!(ReadTool::is_binary_file(Path::new("image.png")));
        assert!(ReadTool::is_binary_file(Path::new("doc.pdf")));
        assert!(!ReadTool::is_binary_file(Path::new("code.rs")));
        assert!(!ReadTool::is_binary_file(Path::new("readme.md")));
    }

    #[test]
    fn test_required_permission_sensitive() {
        let tool = ReadTool::new();
        let input = json!({ "file_path": "/home/user/.env" });
        let perm = tool.required_permission(&input);
        assert!(perm.is_some());
    }

    #[test]
    fn test_required_permission_normal() {
        let tool = ReadTool::new();
        let input = json!({ "file_path": "/home/user/code.rs" });
        let perm = tool.required_permission(&input);
        assert!(perm.is_none());
    }
}
