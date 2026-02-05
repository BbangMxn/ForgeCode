//! Code Block Widget - 코드 구문 강조 및 복사 기능
//!
//! ```text
//! ┌─ rust ──────────────────────────────────── [Copy] ─┐
//! │  1 │ fn main() {                                   │
//! │  2 │     println!("Hello, world!");                │
//! │  3 │ }                                             │
//! └────────────────────────────────────────────────────┘
//! ```

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders, Widget},
};

/// 지원하는 언어
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Go,
    C,
    Cpp,
    Java,
    Shell,
    Json,
    Yaml,
    Toml,
    Markdown,
    Sql,
    Html,
    Css,
    Unknown,
}

impl Language {
    /// 언어 문자열에서 파싱
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "rust" | "rs" => Self::Rust,
            "python" | "py" => Self::Python,
            "javascript" | "js" => Self::JavaScript,
            "typescript" | "ts" => Self::TypeScript,
            "go" | "golang" => Self::Go,
            "c" => Self::C,
            "cpp" | "c++" | "cxx" => Self::Cpp,
            "java" => Self::Java,
            "shell" | "bash" | "sh" | "zsh" | "powershell" | "ps1" => Self::Shell,
            "json" => Self::Json,
            "yaml" | "yml" => Self::Yaml,
            "toml" => Self::Toml,
            "markdown" | "md" => Self::Markdown,
            "sql" => Self::Sql,
            "html" | "htm" => Self::Html,
            "css" | "scss" | "sass" => Self::Css,
            _ => Self::Unknown,
        }
    }

    /// 언어 이름
    pub fn name(&self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Python => "python",
            Self::JavaScript => "javascript",
            Self::TypeScript => "typescript",
            Self::Go => "go",
            Self::C => "c",
            Self::Cpp => "c++",
            Self::Java => "java",
            Self::Shell => "shell",
            Self::Json => "json",
            Self::Yaml => "yaml",
            Self::Toml => "toml",
            Self::Markdown => "markdown",
            Self::Sql => "sql",
            Self::Html => "html",
            Self::Css => "css",
            Self::Unknown => "text",
        }
    }

    /// 키워드 색상
    fn keyword_color(&self) -> Color {
        Color::Magenta
    }

    /// 문자열 색상
    fn string_color(&self) -> Color {
        Color::Green
    }

    /// 숫자 색상
    fn number_color(&self) -> Color {
        Color::Yellow
    }

    /// 주석 색상
    fn comment_color(&self) -> Color {
        Color::DarkGray
    }

    /// 함수 색상
    fn function_color(&self) -> Color {
        Color::Blue
    }

    /// 타입 색상
    fn type_color(&self) -> Color {
        Color::Cyan
    }
}

/// 코드 토큰
#[derive(Debug, Clone)]
pub struct Token {
    pub text: String,
    pub kind: TokenKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Keyword,
    String,
    Number,
    Comment,
    Function,
    Type,
    Operator,
    Punctuation,
    Normal,
}

/// 간단한 토크나이저 (기본 구문 강조)
pub fn tokenize(code: &str, language: Language) -> Vec<Vec<Token>> {
    let keywords = match language {
        Language::Rust => vec![
            "fn", "let", "mut", "const", "static", "if", "else", "match", "loop", "while", "for",
            "in", "return", "break", "continue", "struct", "enum", "impl", "trait", "pub", "use",
            "mod", "crate", "self", "super", "async", "await", "move", "ref", "where", "type",
            "dyn", "unsafe", "extern",
        ],
        Language::Python => vec![
            "def", "class", "if", "elif", "else", "for", "while", "try", "except", "finally",
            "with", "as", "import", "from", "return", "yield", "raise", "pass", "break",
            "continue", "lambda", "and", "or", "not", "in", "is", "True", "False", "None",
            "async", "await", "global", "nonlocal",
        ],
        Language::JavaScript | Language::TypeScript => vec![
            "function", "const", "let", "var", "if", "else", "for", "while", "do", "switch",
            "case", "break", "continue", "return", "try", "catch", "finally", "throw", "new",
            "class", "extends", "import", "export", "default", "async", "await", "yield",
            "true", "false", "null", "undefined", "this", "super", "typeof", "instanceof",
        ],
        Language::Go => vec![
            "func", "var", "const", "type", "struct", "interface", "map", "chan", "if", "else",
            "for", "range", "switch", "case", "default", "break", "continue", "return", "go",
            "defer", "select", "import", "package", "nil", "true", "false",
        ],
        Language::Shell => vec![
            "if", "then", "else", "elif", "fi", "for", "while", "do", "done", "case", "esac",
            "function", "return", "exit", "export", "local", "readonly", "declare", "set",
            "unset", "source", "alias", "cd", "pwd", "echo", "printf",
        ],
        _ => vec![],
    };

    let types = match language {
        Language::Rust => vec![
            "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128",
            "usize", "f32", "f64", "bool", "char", "str", "String", "Vec", "Option", "Result",
            "Box", "Rc", "Arc", "Cell", "RefCell", "Mutex", "RwLock",
        ],
        Language::TypeScript => vec![
            "string", "number", "boolean", "void", "null", "undefined", "any", "never",
            "unknown", "object", "Array", "Promise", "Map", "Set",
        ],
        Language::Go => vec![
            "int", "int8", "int16", "int32", "int64", "uint", "uint8", "uint16", "uint32",
            "uint64", "float32", "float64", "complex64", "complex128", "byte", "rune",
            "string", "bool", "error",
        ],
        _ => vec![],
    };

    let mut result = Vec::new();

    for line in code.lines() {
        let mut tokens = Vec::new();
        let mut current = String::new();
        let mut chars = line.chars().peekable();
        let mut in_string = false;
        let mut string_char = '"';
        let mut in_comment = false;

        while let Some(c) = chars.next() {
            // 주석 체크
            if !in_string {
                match language {
                    Language::Rust | Language::Go | Language::JavaScript | 
                    Language::TypeScript | Language::C | Language::Cpp | Language::Java => {
                        if c == '/' {
                            if let Some(&'/') = chars.peek() {
                                // 현재 토큰 저장
                                if !current.is_empty() {
                                    tokens.push(classify_token(&current, &keywords, &types));
                                    current.clear();
                                }
                                // 나머지를 주석으로
                                let rest: String = std::iter::once(c).chain(chars).collect();
                                tokens.push(Token { text: rest, kind: TokenKind::Comment });
                                in_comment = true;
                                break;
                            }
                        }
                    }
                    Language::Python | Language::Shell | Language::Yaml | Language::Toml => {
                        if c == '#' {
                            if !current.is_empty() {
                                tokens.push(classify_token(&current, &keywords, &types));
                                current.clear();
                            }
                            let rest: String = std::iter::once(c).chain(chars).collect();
                            tokens.push(Token { text: rest, kind: TokenKind::Comment });
                            in_comment = true;
                            break;
                        }
                    }
                    _ => {}
                }
            }

            // 문자열 체크
            if !in_comment && (c == '"' || c == '\'' || c == '`') {
                if in_string && c == string_char {
                    current.push(c);
                    tokens.push(Token { text: current.clone(), kind: TokenKind::String });
                    current.clear();
                    in_string = false;
                } else if !in_string {
                    if !current.is_empty() {
                        tokens.push(classify_token(&current, &keywords, &types));
                        current.clear();
                    }
                    string_char = c;
                    in_string = true;
                    current.push(c);
                } else {
                    current.push(c);
                }
                continue;
            }

            if in_string {
                current.push(c);
                continue;
            }

            // 공백 또는 구분자
            if c.is_whitespace() || "(){}[]<>;:,.+-*/=!&|^%@".contains(c) {
                if !current.is_empty() {
                    tokens.push(classify_token(&current, &keywords, &types));
                    current.clear();
                }
                
                let kind = if "+-*/=!&|^%<>".contains(c) {
                    TokenKind::Operator
                } else if "(){}[]<>;:,.@".contains(c) {
                    TokenKind::Punctuation
                } else {
                    TokenKind::Normal
                };
                
                tokens.push(Token { text: c.to_string(), kind });
            } else {
                current.push(c);
            }
        }

        if !current.is_empty() && !in_comment {
            tokens.push(classify_token(&current, &keywords, &types));
        }

        result.push(tokens);
    }

    result
}

fn classify_token(text: &str, keywords: &[&str], types: &[&str]) -> Token {
    let kind = if keywords.contains(&text) {
        TokenKind::Keyword
    } else if types.contains(&text) {
        TokenKind::Type
    } else if text.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '_') 
        && text.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        TokenKind::Number
    } else if text.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
        TokenKind::Type
    } else {
        TokenKind::Normal
    };

    Token { text: text.to_string(), kind }
}

/// 코드 블록 위젯
pub struct CodeBlock<'a> {
    code: &'a str,
    language: Language,
    show_line_numbers: bool,
    title: Option<String>,
    focused: bool,
}

impl<'a> CodeBlock<'a> {
    pub fn new(code: &'a str, language: Language) -> Self {
        Self {
            code,
            language,
            show_line_numbers: true,
            title: None,
            focused: false,
        }
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn line_numbers(mut self, show: bool) -> Self {
        self.show_line_numbers = show;
        self
    }

    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }
}

impl Widget for CodeBlock<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 10 || area.height < 3 {
            return;
        }

        let border_style = if self.focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let title = format!(" {} ", self.title.unwrap_or_else(|| self.language.name().to_string()));
        
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(Span::styled(title, border_style))
            .title_alignment(ratatui::layout::Alignment::Left);

        let inner = block.inner(area);
        block.render(area, buf);

        // 코드 토큰화
        let tokens = tokenize(self.code, self.language);
        let line_num_width = if self.show_line_numbers {
            tokens.len().to_string().len() + 2
        } else {
            0
        };

        let mut y = inner.y;
        for (i, line_tokens) in tokens.iter().enumerate() {
            if y >= inner.y + inner.height {
                break;
            }

            let mut x = inner.x;

            // 줄 번호
            if self.show_line_numbers {
                let line_num = format!("{:>width$} │ ", i + 1, width = line_num_width - 3);
                buf.set_string(x, y, &line_num, Style::default().fg(Color::DarkGray));
                x += line_num_width as u16;
            }

            // 코드 토큰
            for token in line_tokens {
                let style = match token.kind {
                    TokenKind::Keyword => Style::default().fg(self.language.keyword_color()).add_modifier(Modifier::BOLD),
                    TokenKind::String => Style::default().fg(self.language.string_color()),
                    TokenKind::Number => Style::default().fg(self.language.number_color()),
                    TokenKind::Comment => Style::default().fg(self.language.comment_color()).add_modifier(Modifier::ITALIC),
                    TokenKind::Function => Style::default().fg(self.language.function_color()),
                    TokenKind::Type => Style::default().fg(self.language.type_color()),
                    TokenKind::Operator => Style::default().fg(Color::Red),
                    TokenKind::Punctuation => Style::default().fg(Color::White),
                    TokenKind::Normal => Style::default().fg(Color::White),
                };

                let text_width = token.text.len() as u16;
                if x + text_width <= inner.x + inner.width {
                    buf.set_string(x, y, &token.text, style);
                    x += text_width;
                }
            }

            y += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_detection() {
        assert_eq!(Language::from_str("rust"), Language::Rust);
        assert_eq!(Language::from_str("py"), Language::Python);
        assert_eq!(Language::from_str("js"), Language::JavaScript);
    }

    #[test]
    fn test_tokenize_rust() {
        let code = "fn main() { println!(\"Hello\"); }";
        let tokens = tokenize(code, Language::Rust);
        assert!(!tokens.is_empty());
        assert!(tokens[0].iter().any(|t| t.kind == TokenKind::Keyword));
    }
}
