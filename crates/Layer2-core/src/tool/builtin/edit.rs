//! Edit Tool - 파일 편집 도구
//!
//! 파일 내용을 부분적으로 편집합니다.
//! - 문자열 치환 (old_string → new_string)
//! - unique match 검증
//! - replace_all 옵션
//! - 경로 보안 검증 (path traversal 방지)
//! - 퍼지 매칭 (공백/줄바꿈 정규화)
//! - 백업 파일 생성
//! - Diff 미리보기

use async_trait::async_trait;
use forge_foundation::{
    PermissionAction, PermissionDef, PermissionStatus, Result, Tool, ToolContext, ToolMeta,
    ToolResult,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use tracing::{debug, warn};

use crate::tool::security::{is_sensitive_path, PathValidator};

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

    /// 퍼지 매칭 사용 여부 - 공백/줄바꿈 정규화 후 매칭 (기본: false)
    #[serde(default)]
    pub fuzzy_whitespace: bool,

    /// 백업 파일 생성 여부 (기본: false)
    #[serde(default)]
    pub create_backup: bool,

    /// 실제 수정 없이 diff만 반환 (기본: false)
    #[serde(default)]
    pub dry_run: bool,
}

/// 편집 결과 상세 정보
#[derive(Debug)]
pub struct EditResult {
    /// 매칭된 횟수
    pub match_count: usize,
    /// 대체된 횟수
    pub replaced_count: usize,
    /// 백업 파일 경로 (생성된 경우)
    pub backup_path: Option<String>,
    /// Diff 미리보기 (라인 기준)
    pub diff_preview: Option<String>,
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

    /// 공백을 정규화하여 퍼지 매칭용 문자열 생성
    fn normalize_whitespace(s: &str) -> String {
        s.lines()
            .map(|line| line.trim())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// 퍼지 매칭으로 old_string의 위치와 실제 매칭된 원본 문자열 찾기
    fn find_fuzzy_match<'a>(content: &'a str, old_string: &str) -> Option<(usize, &'a str)> {
        let normalized_old = Self::normalize_whitespace(old_string);
        let old_lines: Vec<&str> = normalized_old.lines().collect();
        let old_line_count = old_lines.len();

        if old_line_count == 0 {
            return None;
        }

        let content_lines: Vec<&str> = content.lines().collect();

        // 슬라이딩 윈도우로 매칭 탐색
        for i in 0..=content_lines.len().saturating_sub(old_line_count) {
            let window: Vec<&str> = content_lines[i..i + old_line_count]
                .iter()
                .map(|l| l.trim())
                .collect();

            if window == old_lines {
                // 매칭 성공 - 원본 문자열의 시작/끝 위치 계산
                let start_pos: usize = content_lines[..i]
                    .iter()
                    .map(|l| l.len() + 1) // +1 for newline
                    .sum();

                let matched_lines = &content_lines[i..i + old_line_count];
                let matched_len: usize = matched_lines.iter().map(|l| l.len()).sum::<usize>()
                    + old_line_count.saturating_sub(1); // newlines between

                let end_pos = start_pos + matched_len;

                // content에서 실제 매칭된 부분 추출
                if end_pos <= content.len() {
                    return Some((start_pos, &content[start_pos..end_pos]));
                }
            }
        }

        None
    }

    /// 간단한 unified diff 생성
    fn generate_diff(old_content: &str, new_content: &str, file_path: &str) -> String {
        let old_lines: Vec<&str> = old_content.lines().collect();
        let new_lines: Vec<&str> = new_content.lines().collect();

        let mut diff = String::new();
        diff.push_str(&format!("--- a/{}\n", file_path));
        diff.push_str(&format!("+++ b/{}\n", file_path));

        // 간단한 diff: 변경된 라인만 표시 (최대 20줄)
        let mut changes = Vec::new();
        let max_len = old_lines.len().max(new_lines.len());

        for i in 0..max_len {
            let old_line = old_lines.get(i).copied();
            let new_line = new_lines.get(i).copied();

            match (old_line, new_line) {
                (Some(o), Some(n)) if o != n => {
                    changes.push(format!("-{}", o));
                    changes.push(format!("+{}", n));
                }
                (Some(o), None) => {
                    changes.push(format!("-{}", o));
                }
                (None, Some(n)) => {
                    changes.push(format!("+{}", n));
                }
                _ => {}
            }
        }

        // 최대 20줄로 제한
        if changes.len() > 20 {
            diff.push_str(&changes[..20].join("\n"));
            diff.push_str(&format!("\n... and {} more changes", changes.len() - 20));
        } else {
            diff.push_str(&changes.join("\n"));
        }

        diff
    }

    /// 백업 파일 생성
    fn create_backup_file(path: &Path, content: &str) -> std::result::Result<String, String> {
        let backup_path = format!("{}.bak", path.display());
        fs::write(&backup_path, content).map_err(|e| format!("Failed to create backup: {}", e))?;
        Ok(backup_path)
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
                },
                "fuzzy_whitespace": {
                    "type": "boolean",
                    "description": "Use fuzzy matching that normalizes whitespace/indentation (default: false)",
                    "default": false
                },
                "create_backup": {
                    "type": "boolean",
                    "description": "Create a backup file before editing (default: false)",
                    "default": false
                },
                "dry_run": {
                    "type": "boolean",
                    "description": "Preview changes without actually modifying the file (default: false)",
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
        let parsed: EditInput = serde_json::from_value(input.clone())
            .map_err(|e| forge_foundation::Error::InvalidInput(format!("Invalid input: {}", e)))?;

        let path = Path::new(&parsed.file_path);

        // 경로 검증 - 절대 경로 필수
        if !path.is_absolute() {
            return Ok(ToolResult::error(format!(
                "Path must be absolute, got: {}",
                parsed.file_path
            )));
        }

        // 경로 보안 검증 (path traversal, 위험 경로 체크)
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

        // 민감한 파일 추가 체크
        if is_sensitive_path(&parsed.file_path) {
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

        // old_string이 파일에 존재하는지 확인 (정확 매칭 먼저 시도)
        let mut match_count = content.matches(&parsed.old_string).count();
        let mut use_fuzzy = false;
        let mut fuzzy_match_result: Option<(usize, String)> = None;

        // 정확 매칭 실패 시 퍼지 매칭 시도
        if match_count == 0 {
            if parsed.fuzzy_whitespace {
                debug!("Exact match failed, trying fuzzy whitespace matching");
                if let Some((pos, matched)) = Self::find_fuzzy_match(&content, &parsed.old_string) {
                    match_count = 1;
                    use_fuzzy = true;
                    fuzzy_match_result = Some((pos, matched.to_string()));
                    debug!("Fuzzy match found at position {}", pos);
                }
            }

            if match_count == 0 {
                // 매칭 실패 시 유용한 힌트 제공
                let hint = if content.contains(parsed.old_string.trim()) {
                    " The trimmed content exists - check leading/trailing whitespace."
                } else if Self::normalize_whitespace(&content)
                    .contains(&Self::normalize_whitespace(&parsed.old_string))
                {
                    " Content exists with different whitespace - try fuzzy_whitespace: true."
                } else {
                    ""
                };

                return Ok(ToolResult::error(format!(
                    "old_string not found in file.{}",
                    hint
                )));
            }
        }

        // replace_all이 false인데 여러 개 존재하면 에러
        if !parsed.replace_all && match_count > 1 {
            return Ok(ToolResult::error(format!(
                "old_string found {} times in file. Either provide a larger string with more context to make it unique, or set replace_all to true.",
                match_count
            )));
        }

        // 문자열 치환
        let new_content = if use_fuzzy {
            // 퍼지 매칭인 경우, 실제 매칭된 문자열을 대체
            if let Some((pos, ref matched)) = fuzzy_match_result {
                let mut result = String::with_capacity(content.len());
                result.push_str(&content[..pos]);
                result.push_str(&parsed.new_string);
                result.push_str(&content[pos + matched.len()..]);
                result
            } else {
                content.clone()
            }
        } else if parsed.replace_all {
            content.replace(&parsed.old_string, &parsed.new_string)
        } else {
            content.replacen(&parsed.old_string, &parsed.new_string, 1)
        };

        // Diff 생성 (dry_run이거나 항상)
        let diff_preview = Self::generate_diff(&content, &new_content, &parsed.file_path);

        // dry_run 모드면 diff만 반환
        if parsed.dry_run {
            return Ok(ToolResult::success(format!(
                "[DRY RUN] Would edit {}:\n\n{}",
                parsed.file_path, diff_preview
            )));
        }

        // 백업 생성 (요청된 경우)
        let backup_path = if parsed.create_backup {
            match Self::create_backup_file(path, &content) {
                Ok(backup) => {
                    debug!("Created backup at {}", backup);
                    Some(backup)
                }
                Err(e) => {
                    warn!("Failed to create backup: {}", e);
                    return Ok(ToolResult::error(format!(
                        "Failed to create backup file: {}",
                        e
                    )));
                }
            }
        } else {
            None
        };

        // 파일 쓰기
        match fs::write(path, &new_content) {
            Ok(()) => {
                let replaced_info = if use_fuzzy {
                    "1 occurrence (fuzzy match)".to_string()
                } else if parsed.replace_all {
                    format!("{} occurrences", match_count)
                } else {
                    "1 occurrence".to_string()
                };

                let mut result_msg =
                    format!("Edited {}: replaced {}", parsed.file_path, replaced_info);

                if let Some(ref backup) = backup_path {
                    result_msg.push_str(&format!("\nBackup created: {}", backup));
                }

                // 변경 내용 요약 추가
                result_msg.push_str(&format!("\n\nChanges:\n{}", diff_preview));

                Ok(ToolResult::success(result_msg))
            }
            Err(e) => {
                // 백업이 있으면 복원 시도
                if let Some(ref backup) = backup_path {
                    if let Ok(backup_content) = fs::read_to_string(backup) {
                        let _ = fs::write(path, backup_content);
                        warn!("Write failed, restored from backup");
                    }
                }
                Ok(ToolResult::error(format!("Failed to write file: {}", e)))
            }
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
            other => panic!("Expected FileWrite permission, got {:?}", other),
        }
    }

    #[test]
    fn test_sensitive_path_detection() {
        use crate::tool::security::is_sensitive_path;
        assert!(is_sensitive_path("/home/user/.env"));
        assert!(is_sensitive_path("/app/.ssh/config"));
        assert!(!is_sensitive_path("/home/user/code.rs"));
    }

    #[test]
    fn test_normalize_whitespace() {
        let input = "  hello  \n    world  ";
        let normalized = EditTool::normalize_whitespace(input);
        assert_eq!(normalized, "hello\nworld");
    }

    #[test]
    fn test_fuzzy_match() {
        let content = "fn main() {\n    println!(\"hello\");\n}";
        let search = "fn main() {\nprintln!(\"hello\");\n}"; // without indentation

        let result = EditTool::find_fuzzy_match(content, search);
        assert!(result.is_some());

        let (pos, matched) = result.unwrap();
        assert_eq!(pos, 0);
        assert!(matched.contains("println"));
    }

    #[test]
    fn test_generate_diff() {
        let old = "line1\nline2\nline3";
        let new = "line1\nmodified\nline3";

        let diff = EditTool::generate_diff(old, new, "test.txt");
        assert!(diff.contains("-line2"));
        assert!(diff.contains("+modified"));
    }

    #[test]
    fn test_schema_has_new_fields() {
        let tool = EditTool::new();
        let schema = tool.schema();
        let props = &schema["properties"];

        assert!(props["fuzzy_whitespace"].is_object());
        assert!(props["create_backup"].is_object());
        assert!(props["dry_run"].is_object());
    }
}
