//! Repository Analyzer - 코드베이스 분석기
//!
//! 파일을 파싱하여 심볼을 추출합니다.

use super::types::{FileInfo, RepoMap, RepoMapConfig, SymbolDef, SymbolKind};
use forge_foundation::{Error, Result};
use std::path::{Path, PathBuf};
use tokio::fs;
use tracing::{debug, warn};

/// Repository 분석기
pub struct RepoAnalyzer {
    /// 설정
    config: RepoMapConfig,
    /// 루트 경로
    root: PathBuf,
}

impl RepoAnalyzer {
    /// 새 분석기 생성
    pub fn new(root: impl Into<PathBuf>, config: RepoMapConfig) -> Self {
        Self {
            config,
            root: root.into(),
        }
    }

    /// 기본 설정으로 생성
    pub fn with_defaults(root: impl Into<PathBuf>) -> Self {
        Self::new(root, RepoMapConfig::default())
    }

    /// Repository Map 생성
    pub async fn analyze(&self) -> Result<RepoMap> {
        let mut map = RepoMap::new(self.root.clone());

        // 파일 목록 수집
        let files = self.collect_files().await?;
        debug!("Found {} files to analyze", files.len());

        // 각 파일 분석
        for path in files.into_iter().take(self.config.max_files) {
            match self.analyze_file(&path).await {
                Ok(file_info) => {
                    map.add_file(file_info);
                }
                Err(e) => {
                    warn!("Failed to analyze {}: {}", path.display(), e);
                }
            }
        }

        Ok(map)
    }

    /// 파일 목록 수집
    async fn collect_files(&self) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        self.collect_files_recursive(&self.root, &mut files).await?;
        Ok(files)
    }

    /// 재귀적으로 파일 수집
    async fn collect_files_recursive(&self, dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
        let mut entries = fs::read_dir(dir)
            .await
            .map_err(|e| Error::Internal(format!("Failed to read directory: {}", e)))?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| Error::Internal(format!("Failed to read entry: {}", e)))?
        {
            let path = entry.path();

            // 제외 패턴 체크
            if self.should_exclude(&path) {
                continue;
            }

            if path.is_dir() {
                // .git, node_modules 등 특수 디렉토리 스킵
                let dir_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if dir_name.starts_with('.') || self.is_excluded_dir(dir_name) {
                    continue;
                }
                Box::pin(self.collect_files_recursive(&path, files)).await?;
            } else if self.should_include(&path) {
                files.push(path);
            }
        }

        Ok(())
    }

    /// 파일 포함 여부 확인
    fn should_include(&self, path: &Path) -> bool {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        matches!(
            ext,
            "rs" | "py" | "js" | "ts" | "tsx" | "jsx" | "go" | "java" | "c" | "cpp" | "h" | "hpp"
        )
    }

    /// 파일 제외 여부 확인
    fn should_exclude(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        for pattern in &self.config.exclude_patterns {
            if path_str.contains(pattern.trim_matches('*').trim_matches('/')) {
                return true;
            }
        }
        false
    }

    /// 제외할 디렉토리인지 확인
    fn is_excluded_dir(&self, name: &str) -> bool {
        matches!(
            name,
            "node_modules"
                | "target"
                | "dist"
                | "build"
                | "__pycache__"
                | "vendor"
                | ".git"
                | ".svn"
                | ".hg"
        )
    }

    /// 단일 파일 분석
    async fn analyze_file(&self, path: &Path) -> Result<FileInfo> {
        let content = fs::read_to_string(path)
            .await
            .map_err(|e| Error::Internal(format!("Failed to read file: {}", e)))?;

        let relative_path = path
            .strip_prefix(&self.root)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();

        let language = self.detect_language(path);
        let line_count = content.lines().count();

        let mut file_info = FileInfo::new(path.to_path_buf(), relative_path, language.clone());
        file_info.line_count = line_count;

        // 언어별 파싱
        match language.as_str() {
            "rust" => self.parse_rust(&content, &mut file_info),
            "python" => self.parse_python(&content, &mut file_info),
            "javascript" | "typescript" => self.parse_javascript(&content, &mut file_info),
            "go" => self.parse_go(&content, &mut file_info),
            "java" => self.parse_java(&content, &mut file_info),
            "c" | "cpp" => self.parse_c(&content, &mut file_info),
            _ => {}
        }

        Ok(file_info)
    }

    /// 언어 감지
    fn detect_language(&self, path: &Path) -> String {
        match path.extension().and_then(|e| e.to_str()) {
            Some("rs") => "rust".to_string(),
            Some("py") => "python".to_string(),
            Some("js" | "jsx") => "javascript".to_string(),
            Some("ts" | "tsx") => "typescript".to_string(),
            Some("go") => "go".to_string(),
            Some("java") => "java".to_string(),
            Some("c" | "h") => "c".to_string(),
            Some("cpp" | "hpp" | "cc" | "cxx") => "cpp".to_string(),
            _ => "unknown".to_string(),
        }
    }

    /// Rust 파일 파싱
    fn parse_rust(&self, content: &str, file_info: &mut FileInfo) {
        for (line_num, line) in content.lines().enumerate() {
            let line_num = line_num + 1;
            let trimmed = line.trim();

            // use statements
            if trimmed.starts_with("use ") {
                if let Some(import) = trimmed
                    .strip_prefix("use ")
                    .and_then(|s| s.strip_suffix(';'))
                {
                    file_info.add_import(import.to_string());
                }
            }

            // pub/private 감지
            let (vis, rest) = if trimmed.starts_with("pub ") {
                (Some("pub"), trimmed.strip_prefix("pub ").unwrap_or(trimmed))
            } else if trimmed.starts_with("pub(crate) ") {
                (
                    Some("pub(crate)"),
                    trimmed.strip_prefix("pub(crate) ").unwrap_or(trimmed),
                )
            } else {
                (None, trimmed)
            };

            // struct
            if rest.starts_with("struct ") {
                if let Some(name) = self.extract_identifier(rest, "struct ") {
                    let mut sym = SymbolDef::new(name, SymbolKind::Struct, line_num);
                    if let Some(v) = vis {
                        sym = sym.with_visibility(v);
                    }
                    file_info.add_symbol(sym);
                }
            }
            // enum
            else if rest.starts_with("enum ") {
                if let Some(name) = self.extract_identifier(rest, "enum ") {
                    let mut sym = SymbolDef::new(name, SymbolKind::Enum, line_num);
                    if let Some(v) = vis {
                        sym = sym.with_visibility(v);
                    }
                    file_info.add_symbol(sym);
                }
            }
            // trait
            else if rest.starts_with("trait ") {
                if let Some(name) = self.extract_identifier(rest, "trait ") {
                    let mut sym = SymbolDef::new(name, SymbolKind::Interface, line_num);
                    if let Some(v) = vis {
                        sym = sym.with_visibility(v);
                    }
                    file_info.add_symbol(sym);
                }
            }
            // fn
            else if rest.starts_with("fn ") || rest.starts_with("async fn ") {
                let fn_start = if rest.starts_with("async fn ") {
                    "async fn "
                } else {
                    "fn "
                };
                if let Some(sig) = self.extract_fn_signature(rest, fn_start) {
                    let mut sym = SymbolDef::new(&sig.0, SymbolKind::Function, line_num)
                        .with_signature(&sig.1);
                    if let Some(v) = vis {
                        sym = sym.with_visibility(v);
                    }
                    file_info.add_symbol(sym);
                }
            }
            // impl
            else if rest.starts_with("impl ") || rest.starts_with("impl<") {
                // impl 블록은 특별히 처리하지 않음 (메서드는 내부에서 처리)
            }
            // mod
            else if rest.starts_with("mod ") {
                if let Some(name) = self.extract_identifier(rest, "mod ") {
                    let mut sym = SymbolDef::new(name, SymbolKind::Module, line_num);
                    if let Some(v) = vis {
                        sym = sym.with_visibility(v);
                    }
                    file_info.add_symbol(sym);
                }
            }
            // const
            else if rest.starts_with("const ") {
                if let Some(name) = self.extract_const_name(rest) {
                    let mut sym = SymbolDef::new(name, SymbolKind::Constant, line_num);
                    if let Some(v) = vis {
                        sym = sym.with_visibility(v);
                    }
                    file_info.add_symbol(sym);
                }
            }
            // type alias
            else if rest.starts_with("type ") {
                if let Some(name) = self.extract_identifier(rest, "type ") {
                    let mut sym = SymbolDef::new(name, SymbolKind::TypeAlias, line_num);
                    if let Some(v) = vis {
                        sym = sym.with_visibility(v);
                    }
                    file_info.add_symbol(sym);
                }
            }
            // macro_rules!
            else if rest.starts_with("macro_rules!") {
                if let Some(name) = rest
                    .strip_prefix("macro_rules!")
                    .and_then(|s| s.trim().split_whitespace().next())
                {
                    file_info.add_symbol(SymbolDef::new(name, SymbolKind::Macro, line_num));
                }
            }
        }
    }

    /// Python 파일 파싱
    fn parse_python(&self, content: &str, file_info: &mut FileInfo) {
        for (line_num, line) in content.lines().enumerate() {
            let line_num = line_num + 1;
            let trimmed = line.trim();

            // import statements
            if trimmed.starts_with("import ") || trimmed.starts_with("from ") {
                file_info.add_import(trimmed.to_string());
            }
            // class
            else if trimmed.starts_with("class ") {
                if let Some(name) = self.extract_python_class(trimmed) {
                    file_info.add_symbol(SymbolDef::new(name, SymbolKind::Class, line_num));
                }
            }
            // def (함수)
            else if trimmed.starts_with("def ") {
                if let Some((name, sig)) = self.extract_python_function(trimmed) {
                    file_info.add_symbol(
                        SymbolDef::new(name, SymbolKind::Function, line_num).with_signature(sig),
                    );
                }
            }
            // async def
            else if trimmed.starts_with("async def ") {
                if let Some((name, sig)) =
                    self.extract_python_function(&trimmed.replace("async def ", "def "))
                {
                    file_info.add_symbol(
                        SymbolDef::new(name, SymbolKind::Function, line_num)
                            .with_signature(format!("async {}", sig)),
                    );
                }
            }
        }
    }

    /// JavaScript/TypeScript 파일 파싱
    fn parse_javascript(&self, content: &str, file_info: &mut FileInfo) {
        for (line_num, line) in content.lines().enumerate() {
            let line_num = line_num + 1;
            let trimmed = line.trim();

            // import statements
            if trimmed.starts_with("import ") {
                file_info.add_import(trimmed.to_string());
            }
            // export
            else if trimmed.starts_with("export ") {
                let rest = trimmed.strip_prefix("export ").unwrap_or(trimmed);

                if rest.starts_with("default ") {
                    // export default
                    continue;
                } else if rest.starts_with("class ") {
                    if let Some(name) = self.extract_js_class(rest) {
                        file_info.add_symbol(
                            SymbolDef::new(name, SymbolKind::Class, line_num)
                                .with_visibility("export"),
                        );
                    }
                } else if rest.starts_with("function ")
                    || rest.starts_with("async function ")
                    || rest.starts_with("const ")
                {
                    if let Some((name, kind)) = self.extract_js_declaration(rest) {
                        file_info.add_symbol(
                            SymbolDef::new(name, kind, line_num).with_visibility("export"),
                        );
                    }
                } else if rest.starts_with("interface ") {
                    if let Some(name) = self.extract_identifier(rest, "interface ") {
                        file_info.add_symbol(
                            SymbolDef::new(name, SymbolKind::Interface, line_num)
                                .with_visibility("export"),
                        );
                    }
                } else if rest.starts_with("type ") {
                    if let Some(name) = self.extract_identifier(rest, "type ") {
                        file_info.add_symbol(
                            SymbolDef::new(name, SymbolKind::TypeAlias, line_num)
                                .with_visibility("export"),
                        );
                    }
                }
            }
            // class
            else if trimmed.starts_with("class ") {
                if let Some(name) = self.extract_js_class(trimmed) {
                    file_info.add_symbol(SymbolDef::new(name, SymbolKind::Class, line_num));
                }
            }
            // function
            else if trimmed.starts_with("function ") || trimmed.starts_with("async function ") {
                if let Some((name, kind)) = self.extract_js_declaration(trimmed) {
                    file_info.add_symbol(SymbolDef::new(name, kind, line_num));
                }
            }
            // interface (TypeScript)
            else if trimmed.starts_with("interface ") {
                if let Some(name) = self.extract_identifier(trimmed, "interface ") {
                    file_info.add_symbol(SymbolDef::new(name, SymbolKind::Interface, line_num));
                }
            }
            // type (TypeScript)
            else if trimmed.starts_with("type ") && trimmed.contains('=') {
                if let Some(name) = self.extract_identifier(trimmed, "type ") {
                    file_info.add_symbol(SymbolDef::new(name, SymbolKind::TypeAlias, line_num));
                }
            }
        }
    }

    /// Go 파일 파싱
    fn parse_go(&self, content: &str, file_info: &mut FileInfo) {
        for (line_num, line) in content.lines().enumerate() {
            let line_num = line_num + 1;
            let trimmed = line.trim();

            // import
            if trimmed.starts_with("import ") {
                file_info.add_import(trimmed.to_string());
            }
            // func
            else if trimmed.starts_with("func ") {
                if let Some((name, sig)) = self.extract_go_function(trimmed) {
                    let vis = if name
                        .chars()
                        .next()
                        .map(|c| c.is_uppercase())
                        .unwrap_or(false)
                    {
                        Some("pub")
                    } else {
                        None
                    };
                    let mut sym =
                        SymbolDef::new(name, SymbolKind::Function, line_num).with_signature(sig);
                    if let Some(v) = vis {
                        sym = sym.with_visibility(v);
                    }
                    file_info.add_symbol(sym);
                }
            }
            // type struct
            else if trimmed.starts_with("type ") && trimmed.contains("struct") {
                if let Some(name) = self.extract_identifier(trimmed, "type ") {
                    let vis = if name
                        .chars()
                        .next()
                        .map(|c| c.is_uppercase())
                        .unwrap_or(false)
                    {
                        Some("pub")
                    } else {
                        None
                    };
                    let mut sym = SymbolDef::new(name, SymbolKind::Struct, line_num);
                    if let Some(v) = vis {
                        sym = sym.with_visibility(v);
                    }
                    file_info.add_symbol(sym);
                }
            }
            // type interface
            else if trimmed.starts_with("type ") && trimmed.contains("interface") {
                if let Some(name) = self.extract_identifier(trimmed, "type ") {
                    file_info.add_symbol(SymbolDef::new(name, SymbolKind::Interface, line_num));
                }
            }
        }
    }

    /// Java 파일 파싱
    fn parse_java(&self, content: &str, file_info: &mut FileInfo) {
        for (line_num, line) in content.lines().enumerate() {
            let line_num = line_num + 1;
            let trimmed = line.trim();

            // import
            if trimmed.starts_with("import ") {
                file_info.add_import(trimmed.to_string());
            }
            // class
            else if trimmed.contains("class ") && !trimmed.starts_with("//") {
                if let Some((vis, name)) = self.extract_java_class(trimmed) {
                    let mut sym = SymbolDef::new(name, SymbolKind::Class, line_num);
                    if let Some(v) = vis {
                        sym = sym.with_visibility(v);
                    }
                    file_info.add_symbol(sym);
                }
            }
            // interface
            else if trimmed.contains("interface ") && !trimmed.starts_with("//") {
                if let Some((vis, name)) = self.extract_java_interface(trimmed) {
                    let mut sym = SymbolDef::new(name, SymbolKind::Interface, line_num);
                    if let Some(v) = vis {
                        sym = sym.with_visibility(v);
                    }
                    file_info.add_symbol(sym);
                }
            }
            // enum
            else if trimmed.contains("enum ") && !trimmed.starts_with("//") {
                if let Some(name) = self.extract_java_enum(trimmed) {
                    file_info.add_symbol(SymbolDef::new(name, SymbolKind::Enum, line_num));
                }
            }
        }
    }

    /// C/C++ 파일 파싱
    fn parse_c(&self, content: &str, file_info: &mut FileInfo) {
        for (line_num, line) in content.lines().enumerate() {
            let line_num = line_num + 1;
            let trimmed = line.trim();

            // #include
            if trimmed.starts_with("#include ") {
                file_info.add_import(trimmed.to_string());
            }
            // struct
            else if trimmed.starts_with("struct ") || trimmed.starts_with("typedef struct") {
                if let Some(name) = self.extract_c_struct(trimmed) {
                    file_info.add_symbol(SymbolDef::new(name, SymbolKind::Struct, line_num));
                }
            }
            // class (C++)
            else if trimmed.starts_with("class ") {
                if let Some(name) = self.extract_identifier(trimmed, "class ") {
                    file_info.add_symbol(SymbolDef::new(name, SymbolKind::Class, line_num));
                }
            }
            // enum
            else if trimmed.starts_with("enum ") {
                if let Some(name) = self.extract_identifier(trimmed, "enum ") {
                    file_info.add_symbol(SymbolDef::new(name, SymbolKind::Enum, line_num));
                }
            }
            // function declaration (간단한 휴리스틱)
            else if trimmed.contains('(')
                && !trimmed.starts_with("//")
                && !trimmed.starts_with("/*")
                && !trimmed.starts_with("if ")
                && !trimmed.starts_with("while ")
                && !trimmed.starts_with("for ")
                && !trimmed.starts_with("switch ")
            {
                if let Some((name, sig)) = self.extract_c_function(trimmed) {
                    file_info.add_symbol(
                        SymbolDef::new(name, SymbolKind::Function, line_num).with_signature(sig),
                    );
                }
            }
        }
    }

    // --- Helper methods ---

    /// 식별자 추출
    fn extract_identifier(&self, line: &str, prefix: &str) -> Option<String> {
        let rest = line.strip_prefix(prefix)?;
        let name: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if name.is_empty() {
            None
        } else {
            Some(name)
        }
    }

    /// 함수 시그니처 추출 (Rust)
    fn extract_fn_signature(&self, line: &str, prefix: &str) -> Option<(String, String)> {
        let rest = line.strip_prefix(prefix)?;
        let name_end = rest.find('(')?;
        let name: String = rest[..name_end]
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '<')
            .collect();

        // 시그니처 추출 (파라미터 부분만)
        let sig_start = rest.find('(')?;
        let sig_end = rest.rfind(')').map(|i| i + 1).unwrap_or(rest.len());
        let signature = rest[sig_start..sig_end].to_string();

        Some((name, signature))
    }

    /// const 이름 추출
    fn extract_const_name(&self, line: &str) -> Option<String> {
        let rest = line.strip_prefix("const ")?;
        let name: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if name.is_empty() {
            None
        } else {
            Some(name)
        }
    }

    /// Python 클래스 추출
    fn extract_python_class(&self, line: &str) -> Option<String> {
        let rest = line.strip_prefix("class ")?;
        let name: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if name.is_empty() {
            None
        } else {
            Some(name)
        }
    }

    /// Python 함수 추출
    fn extract_python_function(&self, line: &str) -> Option<(String, String)> {
        let rest = line.strip_prefix("def ")?;
        let paren_pos = rest.find('(')?;
        let name = rest[..paren_pos].to_string();

        let sig_end = rest.find(')').map(|i| i + 1).unwrap_or(rest.len());
        let signature = rest[paren_pos..sig_end].to_string();

        Some((name, signature))
    }

    /// JS 클래스 추출
    fn extract_js_class(&self, line: &str) -> Option<String> {
        let rest = line.strip_prefix("class ")?;
        let name: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if name.is_empty() {
            None
        } else {
            Some(name)
        }
    }

    /// JS 선언 추출
    fn extract_js_declaration(&self, line: &str) -> Option<(String, SymbolKind)> {
        if line.starts_with("function ") || line.starts_with("async function ") {
            let prefix = if line.starts_with("async ") {
                "async function "
            } else {
                "function "
            };
            let rest = line.strip_prefix(prefix)?;
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                return Some((name, SymbolKind::Function));
            }
        } else if line.starts_with("const ") {
            let rest = line.strip_prefix("const ")?;
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                return Some((name, SymbolKind::Constant));
            }
        }
        None
    }

    /// Go 함수 추출
    fn extract_go_function(&self, line: &str) -> Option<(String, String)> {
        let rest = line.strip_prefix("func ")?;

        // 메서드인 경우 (receiver 있음)
        if rest.starts_with('(') {
            let receiver_end = rest.find(')')?;
            let after_receiver = &rest[receiver_end + 1..].trim();
            let name_end = after_receiver.find('(')?;
            let name = after_receiver[..name_end].trim().to_string();
            let sig_end = after_receiver
                .rfind(')')
                .map(|i| i + 1)
                .unwrap_or(after_receiver.len());
            let sig = after_receiver[name_end..sig_end].to_string();
            Some((name, sig))
        } else {
            let name_end = rest.find('(')?;
            let name = rest[..name_end].trim().to_string();
            let sig_end = rest.rfind(')').map(|i| i + 1).unwrap_or(rest.len());
            let sig = rest[name_end..sig_end].to_string();
            Some((name, sig))
        }
    }

    /// Java 클래스 추출
    fn extract_java_class(&self, line: &str) -> Option<(Option<&str>, String)> {
        let vis = if line.contains("public ") {
            Some("public")
        } else if line.contains("private ") {
            Some("private")
        } else if line.contains("protected ") {
            Some("protected")
        } else {
            None
        };

        let class_pos = line.find("class ")?;
        let rest = &line[class_pos + 6..];
        let name: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();

        if name.is_empty() {
            None
        } else {
            Some((vis, name))
        }
    }

    /// Java 인터페이스 추출
    fn extract_java_interface(&self, line: &str) -> Option<(Option<&str>, String)> {
        let vis = if line.contains("public ") {
            Some("public")
        } else {
            None
        };

        let iface_pos = line.find("interface ")?;
        let rest = &line[iface_pos + 10..];
        let name: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();

        if name.is_empty() {
            None
        } else {
            Some((vis, name))
        }
    }

    /// Java enum 추출
    fn extract_java_enum(&self, line: &str) -> Option<String> {
        let enum_pos = line.find("enum ")?;
        let rest = &line[enum_pos + 5..];
        let name: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();

        if name.is_empty() {
            None
        } else {
            Some(name)
        }
    }

    /// C struct 추출
    fn extract_c_struct(&self, line: &str) -> Option<String> {
        if line.starts_with("typedef struct") {
            // typedef struct { ... } Name;
            None // 복잡한 케이스, 스킵
        } else {
            let rest = line.strip_prefix("struct ")?;
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if name.is_empty() {
                None
            } else {
                Some(name)
            }
        }
    }

    /// C 함수 추출
    fn extract_c_function(&self, line: &str) -> Option<(String, String)> {
        let paren_pos = line.find('(')?;
        let before_paren = &line[..paren_pos];

        // 마지막 단어가 함수명
        let words: Vec<&str> = before_paren.split_whitespace().collect();
        let name = words.last()?.trim_start_matches('*').to_string();

        if name.is_empty() || name.starts_with('{') || name.starts_with('=') {
            return None;
        }

        let sig_end = line.find(')').map(|i| i + 1).unwrap_or(line.len());
        let sig = line[paren_pos..sig_end].to_string();

        Some((name, sig))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_rust_function() {
        let analyzer = RepoAnalyzer::with_defaults("/tmp");

        let sig = analyzer.extract_fn_signature("fn process(data: &str) -> Result<()>", "fn ");
        assert!(sig.is_some());
        let (name, params) = sig.unwrap();
        assert_eq!(name, "process");
        assert!(params.contains("data"));
    }

    #[test]
    fn test_extract_python_class() {
        let analyzer = RepoAnalyzer::with_defaults("/tmp");

        let name = analyzer.extract_python_class("class MyClass(BaseClass):");
        assert_eq!(name, Some("MyClass".to_string()));
    }

    #[test]
    fn test_detect_language() {
        let analyzer = RepoAnalyzer::with_defaults("/tmp");

        assert_eq!(
            analyzer.detect_language(Path::new("test.rs")),
            "rust".to_string()
        );
        assert_eq!(
            analyzer.detect_language(Path::new("test.py")),
            "python".to_string()
        );
        assert_eq!(
            analyzer.detect_language(Path::new("test.tsx")),
            "typescript".to_string()
        );
    }
}
