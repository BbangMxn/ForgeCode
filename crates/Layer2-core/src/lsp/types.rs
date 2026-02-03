//! LSP Types - 경량 LSP 타입 정의
//!
//! lsp-types 크레이트 없이 필수 타입만 직접 정의
//! Agent가 필요로 하는 최소한의 타입만 포함

use serde::{Deserialize, Serialize};
use std::path::Path;

// ============================================================================
// 핵심 위치 타입
// ============================================================================

/// 텍스트 위치 (0-based, UTF-16 offset)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    /// 라인 번호 (0부터 시작)
    pub line: u32,

    /// 컬럼 (0부터, UTF-16 코드 유닛 기준)
    pub character: u32,
}

impl Position {
    pub fn new(line: u32, character: u32) -> Self {
        Self { line, character }
    }
}

/// 텍스트 범위
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

impl Range {
    pub fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }

    /// 단일 위치 범위
    pub fn point(pos: Position) -> Self {
        Self {
            start: pos,
            end: pos,
        }
    }
}

/// 파일 내 위치
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    /// 파일 URI (file:///path/to/file)
    pub uri: String,

    /// 범위
    pub range: Range,
}

impl Location {
    pub fn new(uri: impl Into<String>, range: Range) -> Self {
        Self {
            uri: uri.into(),
            range,
        }
    }

    /// URI에서 파일 경로 추출
    pub fn file_path(&self) -> Option<String> {
        uri_to_path(&self.uri)
    }
}

// ============================================================================
// 호버 정보
// ============================================================================

/// 호버 정보 (타입, 문서 등)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hover {
    /// 내용 (Markdown 또는 plain text)
    pub contents: HoverContents,

    /// 범위 (선택적)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<Range>,
}

/// 호버 내용
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HoverContents {
    /// 마크다운 문자열
    Markup(MarkupContent),
    /// 단순 문자열
    String(String),
    /// 여러 항목
    Array(Vec<MarkedString>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkupContent {
    pub kind: String, // "plaintext" | "markdown"
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MarkedString {
    String(String),
    LanguageString { language: String, value: String },
}

impl Hover {
    /// 호버 내용을 단순 문자열로 변환
    pub fn to_string(&self) -> String {
        match &self.contents {
            HoverContents::String(s) => s.clone(),
            HoverContents::Markup(m) => m.value.clone(),
            HoverContents::Array(arr) => arr
                .iter()
                .map(|m| match m {
                    MarkedString::String(s) => s.clone(),
                    MarkedString::LanguageString { value, .. } => value.clone(),
                })
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }
}

// ============================================================================
// 심볼 정보 (선택적 - Phase 2)
// ============================================================================

/// 심볼 종류
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum SymbolKind {
    File = 1,
    Module = 2,
    Namespace = 3,
    Package = 4,
    Class = 5,
    Method = 6,
    Property = 7,
    Field = 8,
    Constructor = 9,
    Enum = 10,
    Interface = 11,
    Function = 12,
    Variable = 13,
    Constant = 14,
    String = 15,
    Number = 16,
    Boolean = 17,
    Array = 18,
    Object = 19,
    Key = 20,
    Null = 21,
    EnumMember = 22,
    Struct = 23,
    Event = 24,
    Operator = 25,
    TypeParameter = 26,
}

/// 문서 심볼 (간소화)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentSymbol {
    pub name: String,
    pub kind: SymbolKind,
    pub range: Range,
    #[serde(default)]
    pub children: Vec<DocumentSymbol>,
}

// ============================================================================
// 서버 설정
// ============================================================================

/// LSP 서버 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspServerConfig {
    /// 언어 ID (rust, typescript, python 등)
    pub language_id: String,

    /// 실행 명령어
    pub command: String,

    /// 명령어 인자
    #[serde(default)]
    pub args: Vec<String>,

    /// 프로젝트 루트 감지 패턴
    #[serde(default)]
    pub root_patterns: Vec<String>,

    /// 초기화 옵션 (서버별 설정)
    #[serde(default)]
    pub initialization_options: Option<serde_json::Value>,
}

/// 기본 LSP 서버 설정 (설치 확인은 런타임에)
pub fn default_lsp_configs() -> Vec<LspServerConfig> {
    vec![
        // Rust - rust-analyzer
        LspServerConfig {
            language_id: "rust".to_string(),
            command: "rust-analyzer".to_string(),
            args: vec![],
            root_patterns: vec!["Cargo.toml".to_string()],
            initialization_options: None,
        },
        // TypeScript/JavaScript
        LspServerConfig {
            language_id: "typescript".to_string(),
            command: "typescript-language-server".to_string(),
            args: vec!["--stdio".to_string()],
            root_patterns: vec!["tsconfig.json".to_string(), "package.json".to_string()],
            initialization_options: None,
        },
        // Python
        LspServerConfig {
            language_id: "python".to_string(),
            command: "pylsp".to_string(),
            args: vec![],
            root_patterns: vec![
                "pyproject.toml".to_string(),
                "setup.py".to_string(),
                "requirements.txt".to_string(),
            ],
            initialization_options: None,
        },
        // Go
        LspServerConfig {
            language_id: "go".to_string(),
            command: "gopls".to_string(),
            args: vec![],
            root_patterns: vec!["go.mod".to_string()],
            initialization_options: None,
        },
    ]
}

// ============================================================================
// URI 유틸리티
// ============================================================================

/// 파일 경로를 file:// URI로 변환
pub fn path_to_uri(path: &Path) -> String {
    let path_str = path.to_string_lossy();

    #[cfg(windows)]
    {
        // Windows: C:\path\to\file -> file:///C:/path/to/file
        let normalized = path_str.replace('\\', "/");
        format!("file:///{}", normalized)
    }

    #[cfg(not(windows))]
    {
        // Unix: /path/to/file -> file:///path/to/file
        format!("file://{}", path_str)
    }
}

/// file:// URI를 파일 경로로 변환
pub fn uri_to_path(uri: &str) -> Option<String> {
    if !uri.starts_with("file://") {
        return None;
    }

    let path = uri.strip_prefix("file://")?;

    #[cfg(windows)]
    {
        // file:///C:/path -> C:\path
        let path = path.strip_prefix('/').unwrap_or(path);
        Some(path.replace('/', "\\"))
    }

    #[cfg(not(windows))]
    {
        Some(path.to_string())
    }
}

// ============================================================================
// 테스트
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position() {
        let pos = Position::new(10, 5);
        assert_eq!(pos.line, 10);
        assert_eq!(pos.character, 5);
    }

    #[test]
    fn test_range() {
        let range = Range::new(Position::new(0, 0), Position::new(10, 20));
        assert_eq!(range.start.line, 0);
        assert_eq!(range.end.line, 10);
    }

    #[test]
    fn test_uri_conversion() {
        #[cfg(windows)]
        {
            let path = Path::new("C:\\Users\\test\\file.rs");
            let uri = path_to_uri(path);
            assert!(uri.starts_with("file:///C:"));
            assert!(uri.contains("Users"));
        }

        #[cfg(not(windows))]
        {
            let path = Path::new("/home/user/file.rs");
            let uri = path_to_uri(path);
            assert_eq!(uri, "file:///home/user/file.rs");
        }
    }

    #[test]
    fn test_default_configs() {
        let configs = default_lsp_configs();
        assert!(configs.iter().any(|c| c.language_id == "rust"));
        assert!(configs.iter().any(|c| c.language_id == "typescript"));
        assert!(configs.iter().any(|c| c.language_id == "python"));
        assert!(configs.iter().any(|c| c.language_id == "go"));
    }
}
