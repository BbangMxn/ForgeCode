//! Repository Map 타입 정의

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// Repository Map 설정
#[derive(Debug, Clone)]
pub struct RepoMapConfig {
    /// 최대 토큰 예산
    pub max_tokens: usize,
    /// 포함할 파일 패턴
    pub include_patterns: Vec<String>,
    /// 제외할 파일 패턴
    pub exclude_patterns: Vec<String>,
    /// 최대 파일 수
    pub max_files: usize,
    /// 심볼 깊이 (중첩 레벨)
    pub symbol_depth: usize,
    /// 의존성 분석 활성화
    pub analyze_dependencies: bool,
    /// 관련 파일 추천 활성화
    pub enable_ranking: bool,
}

impl Default for RepoMapConfig {
    fn default() -> Self {
        Self {
            max_tokens: 8000,
            include_patterns: vec![
                "**/*.rs".into(),
                "**/*.py".into(),
                "**/*.js".into(),
                "**/*.ts".into(),
                "**/*.tsx".into(),
                "**/*.go".into(),
                "**/*.java".into(),
                "**/*.c".into(),
                "**/*.cpp".into(),
                "**/*.h".into(),
            ],
            exclude_patterns: vec![
                "**/node_modules/**".into(),
                "**/target/**".into(),
                "**/.git/**".into(),
                "**/dist/**".into(),
                "**/build/**".into(),
                "**/__pycache__/**".into(),
                "**/vendor/**".into(),
            ],
            max_files: 500,
            symbol_depth: 2,
            analyze_dependencies: true,
            enable_ranking: true,
        }
    }
}

/// 심볼 종류
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    /// 모듈/패키지
    Module,
    /// 클래스
    Class,
    /// 구조체
    Struct,
    /// 열거형
    Enum,
    /// 인터페이스/트레이트
    Interface,
    /// 함수
    Function,
    /// 메서드
    Method,
    /// 상수
    Constant,
    /// 변수
    Variable,
    /// 타입 별칭
    TypeAlias,
    /// 매크로
    Macro,
    /// 임포트
    Import,
}

impl SymbolKind {
    /// 심볼 종류를 문자열로 변환
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Module => "mod",
            Self::Class => "class",
            Self::Struct => "struct",
            Self::Enum => "enum",
            Self::Interface => "trait",
            Self::Function => "fn",
            Self::Method => "method",
            Self::Constant => "const",
            Self::Variable => "var",
            Self::TypeAlias => "type",
            Self::Macro => "macro",
            Self::Import => "use",
        }
    }

    /// 아이콘 문자 반환
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Module => "📦",
            Self::Class => "🏛",
            Self::Struct => "🔷",
            Self::Enum => "🔶",
            Self::Interface => "🔹",
            Self::Function => "⚡",
            Self::Method => "🔸",
            Self::Constant => "📌",
            Self::Variable => "📎",
            Self::TypeAlias => "🏷",
            Self::Macro => "🔧",
            Self::Import => "📥",
        }
    }
}

/// 심볼 정의
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolDef {
    /// 심볼 이름
    pub name: String,
    /// 심볼 종류
    pub kind: SymbolKind,
    /// 시작 라인
    pub line: usize,
    /// 끝 라인
    pub end_line: usize,
    /// 시그니처 (함수의 경우 파라미터 등)
    pub signature: Option<String>,
    /// 가시성 (pub, private 등)
    pub visibility: Option<String>,
    /// 부모 심볼 (메서드의 경우 클래스 등)
    pub parent: Option<String>,
    /// 문서 주석
    pub doc: Option<String>,
    /// 중첩 심볼들
    pub children: Vec<SymbolDef>,
}

impl SymbolDef {
    /// 새 심볼 정의 생성
    pub fn new(name: impl Into<String>, kind: SymbolKind, line: usize) -> Self {
        Self {
            name: name.into(),
            kind,
            line,
            end_line: line,
            signature: None,
            visibility: None,
            parent: None,
            doc: None,
            children: Vec::new(),
        }
    }

    /// 시그니처 설정
    pub fn with_signature(mut self, sig: impl Into<String>) -> Self {
        self.signature = Some(sig.into());
        self
    }

    /// 가시성 설정
    pub fn with_visibility(mut self, vis: impl Into<String>) -> Self {
        self.visibility = Some(vis.into());
        self
    }

    /// 문서 주석 설정
    pub fn with_doc(mut self, doc: impl Into<String>) -> Self {
        self.doc = Some(doc.into());
        self
    }

    /// 자식 심볼 추가
    pub fn add_child(&mut self, child: SymbolDef) {
        self.children.push(child);
    }

    /// 압축된 표현 생성 (토큰 절약)
    pub fn to_compact_string(&self, depth: usize) -> String {
        let indent = "  ".repeat(depth);
        let vis = self.visibility.as_deref().unwrap_or("");
        let sig = self.signature.as_deref().unwrap_or("");

        let line = if sig.is_empty() {
            format!(
                "{}{} {} {}:{}",
                indent,
                self.kind.as_str(),
                vis,
                self.name,
                self.line
            )
        } else {
            format!(
                "{}{} {} {}{}:{}",
                indent,
                self.kind.as_str(),
                vis,
                self.name,
                sig,
                self.line
            )
        };

        if self.children.is_empty() {
            line.trim().to_string()
        } else {
            let children: Vec<String> = self
                .children
                .iter()
                .map(|c| c.to_compact_string(depth + 1))
                .collect();
            format!("{}\n{}", line.trim(), children.join("\n"))
        }
    }
}

/// 심볼 참조 (사용 위치)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolRef {
    /// 참조하는 심볼 이름
    pub name: String,
    /// 참조 위치 (라인)
    pub line: usize,
    /// 참조 파일
    pub file: PathBuf,
}

/// 심볼 사용 정보
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolUsage {
    /// 정의 위치
    pub definition: Option<SymbolRef>,
    /// 참조 위치들
    pub references: Vec<SymbolRef>,
}

/// 파일 정보
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    /// 파일 경로
    pub path: PathBuf,
    /// 상대 경로
    pub relative_path: String,
    /// 파일 언어
    pub language: String,
    /// 라인 수
    pub line_count: usize,
    /// 심볼 정의들
    pub symbols: Vec<SymbolDef>,
    /// 임포트/의존성
    pub imports: Vec<String>,
    /// 익스포트
    pub exports: Vec<String>,
    /// 예상 토큰 수
    pub estimated_tokens: usize,
    /// 중요도 점수 (랭킹용)
    pub importance_score: f64,
}

impl FileInfo {
    /// 새 파일 정보 생성
    pub fn new(path: PathBuf, relative_path: String, language: String) -> Self {
        Self {
            path,
            relative_path,
            language,
            line_count: 0,
            symbols: Vec::new(),
            imports: Vec::new(),
            exports: Vec::new(),
            estimated_tokens: 0,
            importance_score: 0.0,
        }
    }

    /// 심볼 추가
    pub fn add_symbol(&mut self, symbol: SymbolDef) {
        self.symbols.push(symbol);
    }

    /// 임포트 추가
    pub fn add_import(&mut self, import: String) {
        self.imports.push(import);
    }

    /// 압축된 표현 생성
    pub fn to_compact_string(&self) -> String {
        let mut lines = vec![format!("# {}", self.relative_path)];

        // Imports (간략히)
        if !self.imports.is_empty() && self.imports.len() <= 5 {
            lines.push(format!("imports: {}", self.imports.join(", ")));
        } else if self.imports.len() > 5 {
            lines.push(format!("imports: {} items", self.imports.len()));
        }

        // Symbols
        for symbol in &self.symbols {
            lines.push(symbol.to_compact_string(0));
        }

        lines.join("\n")
    }

    /// 예상 토큰 수 계산
    pub fn estimate_tokens(&mut self) {
        // 대략적인 추정: 4 문자 = 1 토큰
        let content = self.to_compact_string();
        self.estimated_tokens = content.len() / 4;
    }
}

/// Repository Map
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoMap {
    /// 루트 경로
    pub root: PathBuf,
    /// 파일 정보들
    pub files: Vec<FileInfo>,
    /// 심볼 인덱스 (이름 -> 파일 경로들)
    #[serde(skip)]
    pub symbol_index: HashMap<String, Vec<PathBuf>>,
    /// 의존성 그래프 (파일 -> 의존 파일들)
    #[serde(skip)]
    pub dependencies: HashMap<PathBuf, HashSet<PathBuf>>,
    /// 총 예상 토큰 수
    pub total_tokens: usize,
    /// 생성 시간
    pub generated_at: chrono::DateTime<chrono::Utc>,
}

impl RepoMap {
    /// 새 Repository Map 생성
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            files: Vec::new(),
            symbol_index: HashMap::new(),
            dependencies: HashMap::new(),
            total_tokens: 0,
            generated_at: chrono::Utc::now(),
        }
    }

    /// 파일 추가
    pub fn add_file(&mut self, mut file: FileInfo) {
        // 심볼 인덱스 업데이트
        for symbol in &file.symbols {
            self.index_symbol(&symbol.name, &file.path);
            for child in &symbol.children {
                self.index_symbol(&child.name, &file.path);
            }
        }

        // 토큰 추정
        file.estimate_tokens();
        self.total_tokens += file.estimated_tokens;

        self.files.push(file);
    }

    /// 심볼 인덱싱
    fn index_symbol(&mut self, name: &str, path: &PathBuf) {
        self.symbol_index
            .entry(name.to_string())
            .or_default()
            .push(path.clone());
    }

    /// 심볼로 파일 찾기
    pub fn find_files_by_symbol(&self, symbol: &str) -> Vec<&FileInfo> {
        if let Some(paths) = self.symbol_index.get(symbol) {
            self.files
                .iter()
                .filter(|f| paths.contains(&f.path))
                .collect()
        } else {
            Vec::new()
        }
    }

    /// 토큰 예산 내에서 압축된 맵 생성
    pub fn to_string_within_budget(&self, max_tokens: usize) -> String {
        let mut result = Vec::new();
        let mut current_tokens = 0;

        // 중요도 순으로 정렬된 파일들
        let mut sorted_files: Vec<_> = self.files.iter().collect();
        sorted_files.sort_by(|a, b| {
            b.importance_score
                .partial_cmp(&a.importance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        result.push(format!("# Repository: {}", self.root.display()));
        result.push(format!("# Files: {}", self.files.len()));
        result.push(String::new());

        for file in sorted_files {
            let file_str = file.to_compact_string();
            let file_tokens = file_str.len() / 4;

            if current_tokens + file_tokens > max_tokens {
                // 예산 초과 시 요약만 추가
                result.push(format!(
                    "# ... and {} more files",
                    self.files.len() - result.len()
                ));
                break;
            }

            result.push(file_str);
            result.push(String::new());
            current_tokens += file_tokens;
        }

        result.join("\n")
    }

    /// 전체 맵을 문자열로 변환
    pub fn to_full_string(&self) -> String {
        let mut result = Vec::new();

        result.push(format!("# Repository: {}", self.root.display()));
        result.push(format!("# Files: {}", self.files.len()));
        result.push(format!("# Total tokens: ~{}", self.total_tokens));
        result.push(String::new());

        for file in &self.files {
            result.push(file.to_compact_string());
            result.push(String::new());
        }

        result.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_def() {
        let mut func = SymbolDef::new("process_data", SymbolKind::Function, 10)
            .with_signature("(data: &[u8]) -> Result<()>")
            .with_visibility("pub");

        func.end_line = 25;

        let compact = func.to_compact_string(0);
        assert!(compact.contains("fn"));
        assert!(compact.contains("process_data"));
        assert!(compact.contains("pub"));
    }

    #[test]
    fn test_file_info() {
        let mut file = FileInfo::new(
            PathBuf::from("/src/lib.rs"),
            "src/lib.rs".to_string(),
            "rust".to_string(),
        );

        file.add_symbol(SymbolDef::new("MyStruct", SymbolKind::Struct, 5));
        file.add_import("std::collections::HashMap".to_string());

        let compact = file.to_compact_string();
        assert!(compact.contains("src/lib.rs"));
        assert!(compact.contains("MyStruct"));
    }

    #[test]
    fn test_repo_map() {
        let mut map = RepoMap::new(PathBuf::from("/project"));

        let mut file = FileInfo::new(
            PathBuf::from("/project/src/main.rs"),
            "src/main.rs".to_string(),
            "rust".to_string(),
        );
        file.add_symbol(SymbolDef::new("main", SymbolKind::Function, 1));
        file.importance_score = 1.0;

        map.add_file(file);

        assert_eq!(map.files.len(), 1);
        assert!(!map.symbol_index.is_empty());
    }
}
