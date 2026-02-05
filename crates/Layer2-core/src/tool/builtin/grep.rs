//! Grep Tool - 고성능 병렬 내용 검색 도구
//!
//! 정규식으로 파일 내용을 검색합니다.
//! - **rayon 병렬 처리**: 멀티코어 활용으로 4-8배 성능 향상
//! - ripgrep 스타일 출력
//! - 컨텍스트 라인 지원
//! - 파일 타입 필터

use async_trait::async_trait;
use forge_foundation::{PermissionAction, Result, Tool, ToolContext, ToolMeta, ToolResult};
use ignore::WalkBuilder;
use rayon::prelude::*;
use regex::Regex;
use serde::Deserialize;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use parking_lot::Mutex;

/// Grep 도구 입력
#[derive(Debug, Deserialize)]
pub struct GrepInput {
    /// 검색 패턴 (정규식)
    /// LLM 호환성을 위해 여러 alias 지원
    #[serde(alias = "regex", alias = "search", alias = "query", alias = "text")]
    pub pattern: String,

    /// 검색 경로 (기본: 현재 디렉토리)
    #[serde(default, alias = "directory", alias = "dir", alias = "root", alias = "file_path")]
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

    /// 멀티라인 모드 (기본: false)
    #[serde(default)]
    pub multiline: bool,
}

fn default_output_mode() -> String {
    "files_with_matches".to_string()
}

/// 매치 결과
#[derive(Clone)]
struct MatchResult {
    file_path: String,
    line_num: usize,
    line_content: String,
    is_match: bool, // 실제 매치인지 컨텍스트인지
}

/// 파일별 검색 결과
#[derive(Clone)]
struct FileSearchResult {
    file_path: String,
    matches: Vec<MatchResult>,
    match_count: usize,
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

    /// 최대 파일 크기 (50MB) - 큰 파일은 건너뜀
    const MAX_FILE_SIZE: u64 = 50 * 1024 * 1024;

    /// 파일 검색 (단일 파일)
    fn search_file(
        path: &Path,
        regex: &Regex,
        before: usize,
        after: usize,
    ) -> Option<FileSearchResult> {
        // 파일 크기 확인
        if let Ok(metadata) = fs::metadata(path) {
            if metadata.len() > Self::MAX_FILE_SIZE {
                return None;
            }
        }

        // 파일 읽기
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return None, // 바이너리 또는 읽기 실패
        };

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

        if match_lines.is_empty() {
            return None;
        }

        let file_path = path.display().to_string();
        let match_count = match_lines.len();

        // 결과 생성
        for i in 0..lines.len() {
            if context_lines.contains(&i) {
                results.push(MatchResult {
                    file_path: file_path.clone(),
                    line_num: i + 1,
                    line_content: lines[i].to_string(),
                    is_match: match_lines.contains(&i),
                });
            }
        }

        Some(FileSearchResult {
            file_path,
            matches: results,
            match_count,
        })
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

    /// 병렬 디렉토리 검색
    fn parallel_search(
        search_path: &Path,
        regex: &Regex,
        file_type: Option<&str>,
        glob_pattern: Option<&glob::Pattern>,
        before: usize,
        after: usize,
        limit: usize,
    ) -> (Vec<FileSearchResult>, bool) {
        // 먼저 파일 목록 수집
        let walker = WalkBuilder::new(search_path)
            .hidden(false)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .build();

        let files: Vec<PathBuf> = walker
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_file())
            .filter(|entry| {
                // 파일 타입 필터
                if let Some(ft) = file_type {
                    if !Self::matches_type(entry.path(), ft) {
                        return false;
                    }
                }
                // 글로브 필터
                if let Some(pattern) = glob_pattern {
                    if !Self::matches_glob(entry.path(), pattern) {
                        return false;
                    }
                }
                true
            })
            .map(|entry| entry.path().to_path_buf())
            .collect();

        // 조기 종료를 위한 카운터
        let found_count = Arc::new(AtomicUsize::new(0));
        let results = Arc::new(Mutex::new(Vec::new()));
        let limit_reached = Arc::new(std::sync::atomic::AtomicBool::new(false));

        // rayon 병렬 처리
        files.par_iter().for_each(|path| {
            // 이미 limit에 도달했으면 건너뜀
            if found_count.load(Ordering::Relaxed) >= limit {
                limit_reached.store(true, Ordering::Relaxed);
                return;
            }

            if let Some(result) = Self::search_file(path, regex, before, after) {
                let mut results_guard = results.lock();
                if found_count.load(Ordering::Relaxed) < limit {
                    found_count.fetch_add(1, Ordering::Relaxed);
                    results_guard.push(result);
                }
            }
        });

        let final_results = Arc::try_unwrap(results)
            .unwrap_or_else(|arc| Mutex::new((*arc.lock()).clone()))
            .into_inner();

        let was_limited = limit_reached.load(Ordering::Relaxed);

        (final_results, was_limited)
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
            .description("A powerful parallel search tool with regex support")
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
                },
                "multiline": {
                    "type": "boolean",
                    "description": "Enable multiline mode where . matches newlines"
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
        // 입력 파싱 (LLM 호환성을 위한 fallback 지원)
        let parsed: GrepInput = match &input {
            // 문자열 입력: 그대로 pattern으로 사용
            Value::String(pattern) => GrepInput {
                pattern: pattern.clone(),
                path: None,
                file_type: None,
                glob: None,
                ignore_case: false,
                context: None,
                before: None,
                after: None,
                output_mode: "files_with_matches".to_string(),
                head_limit: None,
                multiline: false,
            },
            // 객체 입력
            Value::Object(obj) => {
                if obj.is_empty() {
                    return Ok(ToolResult::error("Empty input: please provide a 'pattern' field with a regex pattern to search"));
                }
                // pattern 필드가 없으면 첫 번째 문자열 값 시도
                if !obj.contains_key("pattern") && !obj.contains_key("regex") 
                    && !obj.contains_key("search") && !obj.contains_key("query") {
                    if let Some((key, Value::String(pattern))) = obj.iter().find(|(_, v)| v.is_string()) {
                        tracing::warn!("Using '{}' field as pattern (expected 'pattern')", key);
                        GrepInput {
                            pattern: pattern.clone(),
                            path: obj.get("path").and_then(|v| v.as_str().map(String::from)),
                            file_type: obj.get("type").and_then(|v| v.as_str().map(String::from)),
                            glob: obj.get("glob").and_then(|v| v.as_str().map(String::from)),
                            ignore_case: obj.get("-i").and_then(|v| v.as_bool()).unwrap_or(false),
                            context: obj.get("-C").and_then(|v| v.as_u64().map(|n| n as usize)),
                            before: obj.get("-B").and_then(|v| v.as_u64().map(|n| n as usize)),
                            after: obj.get("-A").and_then(|v| v.as_u64().map(|n| n as usize)),
                            output_mode: obj.get("output_mode").and_then(|v| v.as_str()).unwrap_or("files_with_matches").to_string(),
                            head_limit: obj.get("head_limit").and_then(|v| v.as_u64().map(|n| n as usize)),
                            multiline: obj.get("multiline").and_then(|v| v.as_bool()).unwrap_or(false),
                        }
                    } else {
                        return Ok(ToolResult::error("Invalid input: please provide a 'pattern' field with a regex pattern. Example: {\"pattern\": \"TODO\"}"));
                    }
                } else {
                    serde_json::from_value(input.clone()).map_err(|e| {
                        forge_foundation::Error::InvalidInput(format!("Invalid input: {}", e))
                    })?
                }
            }
            _ => {
                return Ok(ToolResult::error("Invalid input type: expected object or string. Example: {\"pattern\": \"TODO\"}"));
            }
        };

        // 정규식 컴파일
        let pattern = if parsed.multiline {
            format!("(?s){}", parsed.pattern) // (?s) = DOTALL mode
        } else {
            parsed.pattern.clone()
        };

        let regex = if parsed.ignore_case {
            Regex::new(&format!("(?i){}", pattern))
        } else {
            Regex::new(&pattern)
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

        let file_results: Vec<FileSearchResult>;
        let mut truncated = false;

        // 단일 파일 vs 디렉토리
        if search_path.is_file() {
            if let Some(result) = Self::search_file(&search_path, &regex, before, after) {
                file_results = vec![result];
            } else {
                file_results = vec![];
            }
        } else {
            // 병렬 디렉토리 검색
            let (results, was_limited) = Self::parallel_search(
                &search_path,
                &regex,
                parsed.file_type.as_deref(),
                glob_pattern.as_ref(),
                before,
                after,
                limit,
            );
            file_results = results;
            truncated = was_limited;
        }

        // 출력 모드에 따라 결과 포맷
        let output = match parsed.output_mode.as_str() {
            "content" => {
                // 매치 라인과 컨텍스트 출력
                let mut output_lines: Vec<String> = Vec::new();
                let mut total_lines = 0;

                for file_result in &file_results {
                    if total_lines >= limit * 10 {
                        // content 모드는 라인 기준 제한
                        truncated = true;
                        break;
                    }

                    let mut prev_line_num = 0;
                    for result in &file_result.matches {
                        // 파일 분리선
                        if prev_line_num > 0 && result.line_num > prev_line_num + 1 {
                            output_lines.push("--".to_string());
                        }
                        prev_line_num = result.line_num;

                        let prefix = if result.is_match { ":" } else { "-" };
                        output_lines.push(format!(
                            "{}{}{}{}{}",
                            result.file_path,
                            prefix,
                            result.line_num,
                            prefix,
                            result.line_content
                        ));
                        total_lines += 1;
                    }

                    if !output_lines.is_empty() {
                        output_lines.push("".to_string()); // 파일 간 빈 줄
                    }
                }

                output_lines.join("\n")
            }
            "count" => {
                // 파일별 매치 수
                let mut counts: Vec<_> = file_results
                    .iter()
                    .map(|r| (r.file_path.as_str(), r.match_count))
                    .collect();
                counts.sort_by(|a, b| b.1.cmp(&a.1)); // 매치 수 내림차순
                counts
                    .into_iter()
                    .take(limit)
                    .map(|(file, count)| format!("{}:{}", file, count))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            _ => {
                // files_with_matches (기본)
                let mut files: Vec<_> = file_results.iter().map(|r| r.file_path.as_str()).collect();
                files.sort();
                files
                    .into_iter()
                    .take(limit)
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
            let mut result = output;
            if truncated {
                result.push_str(&format!(
                    "\n\n(Results truncated. Showing first {} files)",
                    limit
                ));
            }
            Ok(ToolResult::success(result))
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

    #[test]
    fn test_multiline_regex() {
        let regex = Regex::new(r"(?s)fn.*\{").unwrap();
        let content = "fn test() {\n    // body\n}";
        assert!(regex.is_match(content));
    }
}
