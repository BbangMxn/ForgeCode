//! Syntax Highlighter - 코드 구문 강조
//!
//! syntect를 사용한 터미널 친화적 구문 강조

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use std::collections::HashMap;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style as SyntectStyle, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

/// 구문 강조기
pub struct SyntaxHighlighter {
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
    /// 현재 테마 이름
    theme_name: String,
    /// 언어 확장자 -> 구문 이름 매핑
    extension_map: HashMap<String, String>,
}

impl SyntaxHighlighter {
    /// 새 구문 강조기 생성
    pub fn new() -> Self {
        let syntax_set = SyntaxSet::load_defaults_newlines();
        let theme_set = ThemeSet::load_defaults();

        let mut extension_map = HashMap::new();
        // 일반적인 확장자 매핑
        extension_map.insert("rs".to_string(), "Rust".to_string());
        extension_map.insert("py".to_string(), "Python".to_string());
        extension_map.insert("js".to_string(), "JavaScript".to_string());
        extension_map.insert("ts".to_string(), "TypeScript".to_string());
        extension_map.insert("jsx".to_string(), "JavaScript (JSX)".to_string());
        extension_map.insert("tsx".to_string(), "TypeScript (TSX)".to_string());
        extension_map.insert("go".to_string(), "Go".to_string());
        extension_map.insert("java".to_string(), "Java".to_string());
        extension_map.insert("c".to_string(), "C".to_string());
        extension_map.insert("cpp".to_string(), "C++".to_string());
        extension_map.insert("h".to_string(), "C".to_string());
        extension_map.insert("hpp".to_string(), "C++".to_string());
        extension_map.insert("rb".to_string(), "Ruby".to_string());
        extension_map.insert("php".to_string(), "PHP".to_string());
        extension_map.insert("sh".to_string(), "Bash".to_string());
        extension_map.insert("bash".to_string(), "Bash".to_string());
        extension_map.insert("zsh".to_string(), "Bash".to_string());
        extension_map.insert("ps1".to_string(), "PowerShell".to_string());
        extension_map.insert("json".to_string(), "JSON".to_string());
        extension_map.insert("yaml".to_string(), "YAML".to_string());
        extension_map.insert("yml".to_string(), "YAML".to_string());
        extension_map.insert("toml".to_string(), "TOML".to_string());
        extension_map.insert("xml".to_string(), "XML".to_string());
        extension_map.insert("html".to_string(), "HTML".to_string());
        extension_map.insert("css".to_string(), "CSS".to_string());
        extension_map.insert("scss".to_string(), "SCSS".to_string());
        extension_map.insert("sql".to_string(), "SQL".to_string());
        extension_map.insert("md".to_string(), "Markdown".to_string());
        extension_map.insert("dockerfile".to_string(), "Dockerfile".to_string());

        Self {
            syntax_set,
            theme_set,
            theme_name: "base16-ocean.dark".to_string(),
            extension_map,
        }
    }

    /// 테마 설정
    pub fn with_theme(mut self, theme: &str) -> Self {
        if self.theme_set.themes.contains_key(theme) {
            self.theme_name = theme.to_string();
        }
        self
    }

    /// 사용 가능한 테마 목록
    pub fn available_themes(&self) -> Vec<&str> {
        self.theme_set.themes.keys().map(|s| s.as_str()).collect()
    }

    /// 사용 가능한 언어 목록
    pub fn available_languages(&self) -> Vec<&str> {
        self.syntax_set
            .syntaxes()
            .iter()
            .map(|s| s.name.as_str())
            .collect()
    }

    /// 언어 감지 (확장자 또는 언어 이름)
    fn detect_syntax(&self, lang_hint: &str) -> Option<&syntect::parsing::SyntaxReference> {
        let lang = lang_hint.trim().to_lowercase();

        // 1. 확장자 매핑 확인
        if let Some(syntax_name) = self.extension_map.get(&lang) {
            if let Some(syntax) = self.syntax_set.find_syntax_by_name(syntax_name) {
                return Some(syntax);
            }
        }

        // 2. 확장자로 직접 검색
        if let Some(syntax) = self.syntax_set.find_syntax_by_extension(&lang) {
            return Some(syntax);
        }

        // 3. 언어 이름으로 검색 (대소문자 무시)
        for syntax in self.syntax_set.syntaxes() {
            if syntax.name.to_lowercase() == lang {
                return Some(syntax);
            }
        }

        // 4. 언어 이름에 포함되는지 검색
        for syntax in self.syntax_set.syntaxes() {
            if syntax.name.to_lowercase().contains(&lang) {
                return Some(syntax);
            }
        }

        None
    }

    /// 코드 하이라이트 (ratatui Line 반환)
    pub fn highlight(&self, code: &str, lang: &str) -> Vec<Line<'static>> {
        let syntax = self
            .detect_syntax(lang)
            .or_else(|| Some(self.syntax_set.find_syntax_plain_text()));

        let syntax = match syntax {
            Some(s) => s,
            None => return Self::plain_lines(code),
        };

        let theme = match self.theme_set.themes.get(&self.theme_name) {
            Some(t) => t,
            None => return Self::plain_lines(code),
        };

        let mut highlighter = HighlightLines::new(syntax, theme);
        let mut lines = Vec::new();

        for line in LinesWithEndings::from(code) {
            let regions = match highlighter.highlight_line(line, &self.syntax_set) {
                Ok(r) => r,
                Err(_) => {
                    lines.push(Line::from(line.trim_end().to_string()));
                    continue;
                }
            };

            let spans: Vec<Span<'static>> = regions
                .into_iter()
                .map(|(style, text)| {
                    let ratatui_style = syntect_to_ratatui_style(&style);
                    Span::styled(text.trim_end_matches('\n').to_string(), ratatui_style)
                })
                .collect();

            lines.push(Line::from(spans));
        }

        lines
    }

    /// 하이라이트 없이 일반 라인 반환
    fn plain_lines(code: &str) -> Vec<Line<'static>> {
        code.lines()
            .map(|l| Line::from(l.to_string()))
            .collect()
    }

    /// 단일 라인 하이라이트 (인라인 코드용)
    pub fn highlight_inline(&self, code: &str, lang: &str) -> Vec<Span<'static>> {
        let syntax = self
            .detect_syntax(lang)
            .or_else(|| Some(self.syntax_set.find_syntax_plain_text()));

        let syntax = match syntax {
            Some(s) => s,
            None => return vec![Span::raw(code.to_string())],
        };

        let theme = match self.theme_set.themes.get(&self.theme_name) {
            Some(t) => t,
            None => return vec![Span::raw(code.to_string())],
        };

        let mut highlighter = HighlightLines::new(syntax, theme);

        match highlighter.highlight_line(code, &self.syntax_set) {
            Ok(regions) => regions
                .into_iter()
                .map(|(style, text)| {
                    let ratatui_style = syntect_to_ratatui_style(&style);
                    Span::styled(text.to_string(), ratatui_style)
                })
                .collect(),
            Err(_) => vec![Span::raw(code.to_string())],
        }
    }
}

impl Default for SyntaxHighlighter {
    fn default() -> Self {
        Self::new()
    }
}

/// Syntect 스타일을 Ratatui 스타일로 변환
fn syntect_to_ratatui_style(style: &SyntectStyle) -> Style {
    let fg = Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b);

    let mut ratatui_style = Style::default().fg(fg);

    // 폰트 스타일
    if style.font_style.contains(syntect::highlighting::FontStyle::BOLD) {
        ratatui_style = ratatui_style.add_modifier(Modifier::BOLD);
    }
    if style.font_style.contains(syntect::highlighting::FontStyle::ITALIC) {
        ratatui_style = ratatui_style.add_modifier(Modifier::ITALIC);
    }
    if style.font_style.contains(syntect::highlighting::FontStyle::UNDERLINE) {
        ratatui_style = ratatui_style.add_modifier(Modifier::UNDERLINED);
    }

    ratatui_style
}

/// 코드 블록 파서
pub struct CodeBlockParser;

impl CodeBlockParser {
    /// 마크다운에서 코드 블록 추출
    pub fn extract_code_blocks(text: &str) -> Vec<CodeBlock> {
        let mut blocks = Vec::new();
        let mut in_block = false;
        let mut current_lang = String::new();
        let mut current_code = String::new();
        let mut start_line = 0;

        for (line_num, line) in text.lines().enumerate() {
            if line.starts_with("```") {
                if in_block {
                    // 코드 블록 종료
                    blocks.push(CodeBlock {
                        language: current_lang.clone(),
                        code: current_code.trim_end().to_string(),
                        start_line,
                        end_line: line_num,
                    });
                    current_code.clear();
                    in_block = false;
                } else {
                    // 코드 블록 시작
                    current_lang = line[3..].trim().to_string();
                    start_line = line_num;
                    in_block = true;
                }
            } else if in_block {
                current_code.push_str(line);
                current_code.push('\n');
            }
        }

        blocks
    }

    /// 인라인 코드 추출
    pub fn extract_inline_code(text: &str) -> Vec<(usize, usize, String)> {
        let mut results = Vec::new();
        let mut chars = text.char_indices().peekable();
        
        while let Some((start, ch)) = chars.next() {
            if ch == '`' && chars.peek().map(|(_, c)| *c != '`').unwrap_or(true) {
                let mut code = String::new();
                
                for (_, c) in chars.by_ref() {
                    if c == '`' {
                        results.push((start, start + code.len() + 2, code));
                        break;
                    }
                    code.push(c);
                }
            }
        }

        results
    }
}

/// 코드 블록 정보
#[derive(Debug, Clone)]
pub struct CodeBlock {
    pub language: String,
    pub code: String,
    pub start_line: usize,
    pub end_line: usize,
}

/// 라인 넘버가 포함된 하이라이트된 코드
pub struct HighlightedCode {
    /// 하이라이트된 라인들
    pub lines: Vec<Line<'static>>,
    /// 언어
    pub language: String,
    /// 라인 넘버 표시 여부
    pub show_line_numbers: bool,
    /// 시작 라인 번호
    pub start_line: usize,
}

impl HighlightedCode {
    /// 새 하이라이트된 코드 생성
    pub fn new(code: &str, language: &str, highlighter: &SyntaxHighlighter) -> Self {
        let lines = highlighter.highlight(code, language);
        Self {
            lines,
            language: language.to_string(),
            show_line_numbers: true,
            start_line: 1,
        }
    }

    /// 라인 넘버 설정
    pub fn with_line_numbers(mut self, show: bool) -> Self {
        self.show_line_numbers = show;
        self
    }

    /// 시작 라인 번호 설정
    pub fn with_start_line(mut self, start: usize) -> Self {
        self.start_line = start;
        self
    }

    /// 라인 넘버가 포함된 라인들 생성
    pub fn lines_with_numbers(&self, gutter_style: Style, separator: &str) -> Vec<Line<'static>> {
        if !self.show_line_numbers {
            return self.lines.clone();
        }

        let total_lines = self.lines.len();
        let width = total_lines.to_string().len().max(2);

        self.lines
            .iter()
            .enumerate()
            .map(|(i, line)| {
                let line_num = self.start_line + i;
                let mut spans = vec![
                    Span::styled(format!("{:>width$}", line_num, width = width), gutter_style),
                    Span::styled(separator.to_string(), gutter_style),
                ];
                spans.extend(line.spans.iter().cloned());
                Line::from(spans)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_highlighting() {
        let highlighter = SyntaxHighlighter::new();
        let code = r#"fn main() {
    println!("Hello, world!");
}"#;

        let lines = highlighter.highlight(code, "rust");
        assert_eq!(lines.len(), 3);
        
        // 첫 라인에 여러 span이 있어야 함 (키워드, 함수명 등)
        assert!(lines[0].spans.len() > 1);
    }

    #[test]
    fn test_language_detection() {
        let highlighter = SyntaxHighlighter::new();
        
        // 확장자
        let lines = highlighter.highlight("let x = 1", "js");
        assert!(!lines.is_empty());

        // 언어 이름
        let lines = highlighter.highlight("fn main() {}", "Rust");
        assert!(!lines.is_empty());

        // 소문자 언어 이름
        let lines = highlighter.highlight("def foo():", "python");
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_code_block_extraction() {
        let text = r#"
Some text

```rust
fn main() {}
```

More text

```python
def foo():
    pass
```
"#;

        let blocks = CodeBlockParser::extract_code_blocks(text);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].language, "rust");
        assert_eq!(blocks[1].language, "python");
    }

    #[test]
    fn test_inline_code_extraction() {
        let text = "Use `println!` to print and `let` for variables.";
        let codes = CodeBlockParser::extract_inline_code(text);
        
        assert_eq!(codes.len(), 2);
        assert_eq!(codes[0].2, "println!");
        assert_eq!(codes[1].2, "let");
    }

    #[test]
    fn test_highlighted_code_with_line_numbers() {
        let highlighter = SyntaxHighlighter::new();
        let code = "fn main() {\n    println!(\"Hello\");\n}";
        
        let highlighted = HighlightedCode::new(code, "rust", &highlighter);
        let lines = highlighted.lines_with_numbers(Style::default(), " │ ");
        
        assert_eq!(lines.len(), 3);
        // 라인 넘버가 포함되어 있어야 함
        assert!(lines[0].spans[0].content.contains("1"));
    }
}
