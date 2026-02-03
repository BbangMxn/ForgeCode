//! Glob Tool - 파일 패턴 검색 도구
//!
//! 글로브 패턴으로 파일을 검색합니다.
//! - gitignore 존중
//! - 수정 시간 정렬
//! - 결과 제한

use async_trait::async_trait;
use forge_foundation::{PermissionAction, Result, Tool, ToolContext, ToolMeta, ToolResult};
use ignore::WalkBuilder;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::Path;
use std::time::SystemTime;

/// Glob 도구 입력
#[derive(Debug, Deserialize)]
pub struct GlobInput {
    /// 글로브 패턴 (예: "**/*.rs", "src/**/*.ts")
    pub pattern: String,

    /// 검색 시작 디렉토리 (기본: 현재 작업 디렉토리)
    #[serde(default)]
    pub path: Option<String>,

    /// 최대 결과 수 (기본: 1000)
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Glob 도구
pub struct GlobTool;

impl GlobTool {
    /// 새 인스턴스 생성
    pub fn new() -> Self {
        Self
    }

    /// 도구 이름
    pub const NAME: &'static str = "glob";

    /// 기본 결과 제한
    const DEFAULT_LIMIT: usize = 1000;

    /// 파일 수정 시간 가져오기
    fn get_modified_time(path: &Path) -> Option<SystemTime> {
        path.metadata().ok()?.modified().ok()
    }
}

impl Default for GlobTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for GlobTool {
    fn meta(&self) -> ToolMeta {
        ToolMeta::new(Self::NAME)
            .display_name("Glob")
            .description("Fast file pattern matching tool that works with any codebase size")
            .category("filesystem")
    }

    fn name(&self) -> &str {
        Self::NAME
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "The glob pattern to match files against (e.g., \"**/*.js\", \"src/**/*.ts\")"
                },
                "path": {
                    "type": "string",
                    "description": "The directory to search in. If not specified, the current working directory will be used."
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of results to return (default: 1000)"
                }
            },
            "required": ["pattern"]
        })
    }

    fn required_permission(&self, _input: &Value) -> Option<PermissionAction> {
        // 읽기 전용 - 권한 필요 없음
        None
    }

    async fn execute(&self, input: Value, context: &dyn ToolContext) -> Result<ToolResult> {
        // 입력 파싱
        let parsed: GlobInput = serde_json::from_value(input).map_err(|e| {
            forge_foundation::Error::InvalidInput(format!("Invalid input: {}", e))
        })?;

        // 검색 디렉토리 결정
        let search_path = match &parsed.path {
            Some(p) => {
                let path = Path::new(p);
                if path.is_absolute() {
                    path.to_path_buf()
                } else {
                    context.working_dir().join(p)
                }
            }
            None => context.working_dir().to_path_buf(),
        };

        // 디렉토리 존재 확인
        if !search_path.exists() {
            return Ok(ToolResult::error(format!(
                "Directory not found: {}",
                search_path.display()
            )));
        }

        if !search_path.is_dir() {
            return Ok(ToolResult::error(format!(
                "Path is not a directory: {}",
                search_path.display()
            )));
        }

        // 글로브 패턴 컴파일
        let glob_pattern = match glob::Pattern::new(&parsed.pattern) {
            Ok(p) => p,
            Err(e) => {
                return Ok(ToolResult::error(format!("Invalid glob pattern: {}", e)));
            }
        };

        let limit = parsed.limit.unwrap_or(Self::DEFAULT_LIMIT);

        // ignore 라이브러리로 gitignore 존중하면서 검색
        let walker = WalkBuilder::new(&search_path)
            .hidden(false) // 숨김 파일도 검색
            .git_ignore(true) // .gitignore 존중
            .git_global(true) // 전역 gitignore 존중
            .git_exclude(true) // .git/info/exclude 존중
            .build();

        let mut matches: Vec<(String, Option<SystemTime>)> = Vec::new();

        for entry in walker {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            let path = entry.path();

            // 디렉토리 건너뛰기
            if path.is_dir() {
                continue;
            }

            // 상대 경로 계산
            let relative_path = match path.strip_prefix(&search_path) {
                Ok(rel) => rel.to_string_lossy().to_string(),
                Err(_) => continue,
            };

            // 패턴 매칭
            if glob_pattern.matches(&relative_path)
                || glob_pattern.matches(&relative_path.replace('\\', "/"))
            {
                let modified = Self::get_modified_time(path);
                matches.push((path.display().to_string(), modified));

                // 결과 제한 체크 (정렬 전이라 더 수집)
                if matches.len() >= limit * 2 {
                    break;
                }
            }
        }

        // 수정 시간 기준 정렬 (최신순)
        matches.sort_by(|a, b| {
            match (&b.1, &a.1) {
                (Some(b_time), Some(a_time)) => b_time.cmp(a_time),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            }
        });

        // 결과 제한 적용
        matches.truncate(limit);

        if matches.is_empty() {
            return Ok(ToolResult::success(format!(
                "No files matched pattern '{}' in {}",
                parsed.pattern,
                search_path.display()
            )));
        }

        // 결과 포맷팅
        let result: Vec<String> = matches.into_iter().map(|(path, _)| path).collect();
        let total = result.len();
        let output = result.join("\n");

        Ok(ToolResult::success(format!(
            "{} files matched:\n{}",
            total, output
        )))
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
        let tool = GlobTool::new();
        let meta = tool.meta();
        assert_eq!(meta.name, "glob");
        assert_eq!(meta.category, "filesystem");
    }

    #[test]
    fn test_schema() {
        let tool = GlobTool::new();
        let schema = tool.schema();
        assert!(schema.get("properties").is_some());
        assert!(schema["properties"]["pattern"].is_object());
    }

    #[test]
    fn test_no_permission_required() {
        let tool = GlobTool::new();
        let input = json!({ "pattern": "**/*.rs" });
        assert!(tool.required_permission(&input).is_none());
    }

    #[test]
    fn test_glob_pattern_compilation() {
        // 유효한 패턴
        assert!(glob::Pattern::new("**/*.rs").is_ok());
        assert!(glob::Pattern::new("src/**/*.ts").is_ok());
        assert!(glob::Pattern::new("*.{js,jsx,ts,tsx}").is_ok());
    }
}
