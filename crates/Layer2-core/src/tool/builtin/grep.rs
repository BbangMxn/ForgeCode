//! Grep Tool - 내용 검색 도구
//!
//! 정규식으로 파일 내용을 검색합니다.
//! - ripgrep 스타일 출력
//! - 컨텍스트 라인 지원
//! - 파일 타입 필터

use async_trait::async_trait;
use forge_foundation::{PermissionAction, Result, Tool, ToolContext, ToolMeta, ToolResult};
use ignore::WalkBuilder;
use regex::Regex;
use serde::Deserialize;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

/// Grep 도구 입력
#[derive(Debug, Deserialize)]
pub struct GrepInput {
    /// 검색 패턴 (정규식)
    pub pattern: String,

    /// 검색 경로 (기본: 현재 디렉토리)
    #[serde(default)]
    pub path: Option<String>,

    /// 파일 확장자 필터 (예: "rs", "ts")
    #[serde(rename = "type", default)]
    pub file_type: Option<String>,

    /// 글로브 패턴 필터 (예: "*.rs", "**/*.tsx")
    #[serde(default)]
    pub glob: Option<String>,

    /// 대소문자 무시 (기본: false)
    #[serde(rename = "-i", default)]
    pub ignore_case: bool,

    /// 전후 컨텍스트 라인 수
    #[serde(rename = "-C", default)]
    pub context: Option<usize>,

    /// 이전 컨텍스트 라인 수
    #[serde(rename = "-B", default)]
    pub before: Option<usize>,

    /// 이후 컨텍스트 라인 수
    #[serde(rename = "-A", default)]
    pub after: Option<usize>,

    /// 출력 모드: "content", "files_with_matches", "count"
    #[serde(default = "default_output_mode")]
    pub output_mode: String,

    /// 최대 결과 수
    #[serde(default)]
    pub head_limit: Option<usize>,
}

fn default_output_mode() -> String {
    "files_with_matches".to_string()
}

/// 매치 결과
struct MatchResult {
    file_path: String,
    line_num: usize,
    line_content: String,
    is_match: bool, // 실제 매치인지 컨텍스트인지
}

/// Grep 도구
pub struct GrepTool;

impl GrepTool {
    /// 새 인스턴스 생성
    pub fn new() -> Self {
        Self
    }

    /// 도구 이름
    pub const NAME: &'static str = "grep";

    /// 기본 결과 제한
    const DEFAULT_HEAD_LIMIT: usize = 100;

    /// 파일 검색
    fn search_file(
        path: &Path,
        regex: &Regex,
        before: usize,
        after: usize,
    ) -> Result<Vec<MatchResult>> {
        let content = fs::read_to_string(path)?;
        let lines: Vec<&str> = content.lines().collect();
        let mut results = Vec::new();
        let mut context_lines: std::collections::HashSet<usize> = std::collections::HashSet::new();

        // 먼저 매치되는 라인 찾기
        let mut match_lines: Vec<usize> = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            if regex.is_match(line) {
                match_lines.push(i);
                // 컨텍스트 라인 추가
                let start = i.saturating_sub(before);
                let end = (i + after + 1).min(lines.len());
                for j in start..end {
                    context_lines.insert(j);
                }
            }
        }

        // 결과 생성
        for i in 0..lines.len() {
            if context_lines.contains(&i) {
                results.push(MatchResult {
                    file_path: path.display().to_string(),
                    line_num: i + 1,
                    line_content: lines[i].to_string(),
                    is_match: match_lines.contains(&i),
                });
            }
        }

        Ok(results)
    }

    /// 확장자가 매칭되는지 확인
    fn matches_type(path: &Path, file_type: &str) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case(file_type))
            .unwrap_or(false)
    }

    /// 글로브 패턴이 매칭되는지 확인
    fn matches_glob(path: &Path, pattern: &glob::Pattern) -> bool {
        let path_str = path.to_string_lossy();
        pattern.matches(&path_str) || pattern.matches(&path_str.replace('\\', "/"))
    }
}

impl Default for GrepTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for GrepTool {
    fn meta(&self) -> ToolMeta {
        ToolMeta::new(Self::NAME)
            .display_name("Grep")
            .description("A powerful search tool built on ripgrep")
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
                    "description": "The regular expression pattern to search for in file contents"
                },
                "path": {
                    "type": "string",
                    "description": "File or directory to search in. Defaults to current working directory."
                },
                "type": {
                    "type": "string",
                    "description": "File type to search (e.g., \"js\", \"py\", \"rust\")"
                },
                "glob": {
                    "type": "string",
                    "description": "Glob pattern to filter files (e.g., \"*.js\", \"**/*.tsx\")"
                },
                "-i": {
                    "type": "boolean",
                    "description": "Case insensitive search"
                },
                "-C": {
                    "type": "number",
                    "description": "Number of lines to show before and after each match"
                },
                "-B": {
                    "type": "number",
                    "description": "Number of lines to show before each match"
                },
                "-A": {
                    "type": "number",
                    "description": "Number of lines to show after each match"
                },
                "output_mode": {
                    "type": "string",
                    "enum": ["content", "files_with_matches", "count"],
                    "description": "Output mode: 'content' shows matching lines, 'files_with_matches' shows file paths (default), 'count' shows match counts"
                },
                "head_limit": {
                    "type": "number",
                    "description": "Limit output to first N entries"
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
        let parsed: GrepInput = serde_json::from_value(input).map_err(|e| {
            forge_foundation::Error::InvalidInput(format!("Invalid input: {}", e))
        })?;

        // 정규식 컴파일
        let regex = if parsed.ignore_case {
            Regex::new(&format!("(?i){}", parsed.pattern))
        } else {
            Regex::new(&parsed.pattern)
        };

        let regex = match regex {
            Ok(r) => r,
            Err(e) => {
                return Ok(ToolResult::error(format!("Invalid regex pattern: {}", e)));
            }
        };

        // 검색 경로 결정
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

        // 경로 존재 확인
        if !search_path.exists() {
            return Ok(ToolResult::error(format!(
                "Path not found: {}",
                search_path.display()
            )));
        }

        // 글로브 패턴 컴파일
        let glob_pattern = parsed
            .glob
            .as_ref()
            .and_then(|g| glob::Pattern::new(g).ok());

        // 컨텍스트 라인 수 결정
        let before = parsed.before.or(parsed.context).unwrap_or(0);
        let after = parsed.after.or(parsed.context).unwrap_or(0);
        let limit = parsed.head_limit.unwrap_or(Self::DEFAULT_HEAD_LIMIT);

        // 파일 수집
        let mut all_results: Vec<MatchResult> = Vec::new();
        let mut files_with_matches: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();

        // 단일 파일 검색
        if search_path.is_file() {
            match Self::search_file(&search_path, &regex, before, after) {
                Ok(results) => {
                    if !results.is_empty() {
                        files_with_matches
                            .insert(search_path.display().to_string(), results.len());
                        all_results.extend(results);
                    }
                }
                Err(_) => {} // 읽기 실패 무시
            }
        } else {
            // 디렉토리 검색
            let walker = WalkBuilder::new(&search_path)
                .hidden(false)
                .git_ignore(true)
                .git_global(true)
                .git_exclude(true)
                .build();

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

                // 파일 타입 필터
                if let Some(ref ft) = parsed.file_type {
                    if !Self::matches_type(path, ft) {
                        continue;
                    }
                }

                // 글로브 필터
                if let Some(ref pattern) = glob_pattern {
                    if !Self::matches_glob(path, pattern) {
                        continue;
                    }
                }

                // 파일 검색
                match Self::search_file(path, &regex, before, after) {
                    Ok(results) => {
                        if !results.is_empty() {
                            let match_count =
                                results.iter().filter(|r| r.is_match).count();
                            files_with_matches
                                .insert(path.display().to_string(), match_count);
                            all_results.extend(results);
                        }
                    }
                    Err(_) => {} // 읽기 실패 무시 (바이너리 등)
                }

                // 결과 제한
                if files_with_matches.len() >= limit {
                    break;
                }
            }
        }

        // 출력 모드에 따라 결과 포맷
        let output = match parsed.output_mode.as_str() {
            "content" => {
                // 매치 라인과 컨텍스트 출력
                let mut output_lines: Vec<String> = Vec::new();
                let mut current_file = String::new();

                for result in all_results.iter().take(limit) {
                    if result.file_path != current_file {
                        if !current_file.is_empty() {
                            output_lines.push("--".to_string());
                        }
                        current_file = result.file_path.clone();
                    }

                    let prefix = if result.is_match { ":" } else { "-" };
                    output_lines.push(format!(
                        "{}{}{}{}",
                        result.file_path, prefix, result.line_num, prefix
                    ));
                    output_lines.push(result.line_content.clone());
                }

                output_lines.join("\n")
            }
            "count" => {
                // 파일별 매치 수
                let mut counts: Vec<_> = files_with_matches.iter().collect();
                counts.sort_by(|a, b| b.1.cmp(a.1));
                counts
                    .into_iter()
                    .take(limit)
                    .map(|(file, count)| format!("{}:{}", file, count))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            _ => {
                // files_with_matches (기본)
                let mut files: Vec<_> = files_with_matches.keys().collect();
                files.sort();
                files
                    .into_iter()
                    .take(limit)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        };

        if output.is_empty() {
            Ok(ToolResult::success(format!(
                "No matches found for pattern '{}'",
                parsed.pattern
            )))
        } else {
            Ok(ToolResult::success(output))
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
        let tool = GrepTool::new();
        let meta = tool.meta();
        assert_eq!(meta.name, "grep");
        assert_eq!(meta.category, "filesystem");
    }

    #[test]
    fn test_schema() {
        let tool = GrepTool::new();
        let schema = tool.schema();
        assert!(schema.get("properties").is_some());
        assert!(schema["properties"]["pattern"].is_object());
    }

    #[test]
    fn test_no_permission_required() {
        let tool = GrepTool::new();
        let input = json!({ "pattern": "test" });
        assert!(tool.required_permission(&input).is_none());
    }

    #[test]
    fn test_regex_compilation() {
        assert!(Regex::new(r"fn\s+\w+").is_ok());
        assert!(Regex::new(r"(?i)todo").is_ok());
        assert!(Regex::new(r"[invalid").is_err());
    }

    #[test]
    fn test_matches_type() {
        assert!(GrepTool::matches_type(Path::new("test.rs"), "rs"));
        assert!(GrepTool::matches_type(Path::new("test.RS"), "rs"));
        assert!(!GrepTool::matches_type(Path::new("test.ts"), "rs"));
    }
}
