//! Markdown Renderer - 터미널용 마크다운 렌더링
//!
//! pulldown-cmark를 사용해서 마크다운을 ratatui Line으로 변환

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::syntax::SyntaxHighlighter;
use crate::tui::theme::Theme;

/// 마크다운 렌더러
pub struct MarkdownRenderer {
    /// 구문 강조기
    highlighter: SyntaxHighlighter,
    /// 테마
    theme: Theme,
    /// 줄 바꿈 너비
    wrap_width: Option<usize>,
}

impl MarkdownRenderer {
    /// 새 렌더러 생성
    pub fn new(highlighter: SyntaxHighlighter, theme: Theme) -> Self {
        Self {
            highlighter,
            theme,
            wrap_width: None,
        }
    }

    /// 줄 바꿈 너비 설정
    pub fn with_wrap_width(mut self, width: usize) -> Self {
        self.wrap_width = Some(width);
        self
    }

    /// 마크다운을 ratatui 라인으로 렌더링
    pub fn render(&self, markdown: &str) -> Vec<Line<'static>> {
        let mut options = Options::empty();
        options.insert(Options::ENABLE_STRIKETHROUGH);
        options.insert(Options::ENABLE_TABLES);

        let parser = Parser::new_ext(markdown, options);
        let mut renderer = RenderState::new(&self.highlighter, &self.theme);

        for event in parser {
            renderer.process_event(event);
        }

        renderer.finish()
    }

    /// 인라인 마크다운 렌더링 (단일 라인)
    pub fn render_inline(&self, markdown: &str) -> Line<'static> {
        let lines = self.render(markdown);
        if lines.is_empty() {
            Line::from("")
        } else if lines.len() == 1 {
            lines.into_iter().next().unwrap()
        } else {
            // 여러 줄이면 합치기
            let mut spans = Vec::new();
            for (i, line) in lines.into_iter().enumerate() {
                if i > 0 {
                    spans.push(Span::raw(" "));
                }
                spans.extend(line.spans);
            }
            Line::from(spans)
        }
    }
}

impl Default for MarkdownRenderer {
    fn default() -> Self {
        Self::new(SyntaxHighlighter::new(), Theme::default())
    }
}

/// 렌더링 상태
struct RenderState<'a> {
    highlighter: &'a SyntaxHighlighter,
    theme: &'a Theme,
    /// 현재 스타일 스택
    style_stack: Vec<Style>,
    /// 현재 라인의 Span들
    current_spans: Vec<Span<'static>>,
    /// 완성된 라인들
    lines: Vec<Line<'static>>,
    /// 코드 블록 내용
    code_buffer: String,
    /// 현재 코드 블록 언어
    code_lang: String,
    /// 코드 블록 내부인지
    in_code_block: bool,
    /// 리스트 깊이
    list_depth: usize,
    /// 리스트 아이템 번호 (ordered list)
    list_counters: Vec<Option<u64>>,
    /// 인용 깊이
    quote_depth: usize,
}

impl<'a> RenderState<'a> {
    fn new(highlighter: &'a SyntaxHighlighter, theme: &'a Theme) -> Self {
        Self {
            highlighter,
            theme,
            style_stack: vec![Style::default()],
            current_spans: Vec::new(),
            lines: Vec::new(),
            code_buffer: String::new(),
            code_lang: String::new(),
            in_code_block: false,
            list_depth: 0,
            list_counters: Vec::new(),
            quote_depth: 0,
        }
    }

    fn current_style(&self) -> Style {
        self.style_stack.last().copied().unwrap_or_default()
    }

    fn push_style(&mut self, style: Style) {
        let current = self.current_style();
        self.style_stack.push(current.patch(style));
    }

    fn pop_style(&mut self) {
        if self.style_stack.len() > 1 {
            self.style_stack.pop();
        }
    }

    fn finish_line(&mut self) {
        if !self.current_spans.is_empty() || self.lines.is_empty() {
            // 인용 접두어 추가
            if self.quote_depth > 0 {
                let prefix = format!("{} ", "│".repeat(self.quote_depth));
                self.current_spans.insert(
                    0,
                    Span::styled(prefix, Style::default().fg(Color::Cyan)),
                );
            }

            let line = Line::from(std::mem::take(&mut self.current_spans));
            self.lines.push(line);
        }
    }

    fn add_text(&mut self, text: &str) {
        if self.in_code_block {
            self.code_buffer.push_str(text);
        } else {
            let style = self.current_style();
            self.current_spans.push(Span::styled(text.to_string(), style));
        }
    }

    fn process_event(&mut self, event: Event) {
        match event {
            Event::Start(tag) => self.start_tag(tag),
            Event::End(tag) => self.end_tag(tag),
            Event::Text(text) => self.add_text(&text),
            Event::Code(code) => self.add_inline_code(&code),
            Event::SoftBreak => self.add_text(" "),
            Event::HardBreak => self.finish_line(),
            Event::Rule => self.add_horizontal_rule(),
            _ => {}
        }
    }

    fn start_tag(&mut self, tag: Tag) {
        match tag {
            Tag::Heading { level, .. } => {
                self.finish_line();
                let style = heading_style(level, self.theme);
                self.push_style(style);

                // 헤딩 마커 추가
                let marker = heading_marker(level);
                self.current_spans
                    .push(Span::styled(marker, style));
            }
            Tag::Paragraph => {
                if !self.lines.is_empty() {
                    self.finish_line();
                }
            }
            Tag::BlockQuote(_) => {
                self.quote_depth += 1;
                self.finish_line();
            }
            Tag::CodeBlock(kind) => {
                self.in_code_block = true;
                self.code_lang = match kind {
                    pulldown_cmark::CodeBlockKind::Fenced(lang) => lang.to_string(),
                    pulldown_cmark::CodeBlockKind::Indented => String::new(),
                };
                self.code_buffer.clear();
            }
            Tag::List(start) => {
                self.list_depth += 1;
                self.list_counters.push(start);
            }
            Tag::Item => {
                self.finish_line();
                
                // 들여쓰기
                let indent = "  ".repeat(self.list_depth.saturating_sub(1));
                
                // 마커
                let marker = if let Some(Some(num)) = self.list_counters.last_mut() {
                    let m = format!("{}. ", num);
                    *num += 1;
                    m
                } else {
                    match self.list_depth % 3 {
                        1 => "• ".to_string(),
                        2 => "◦ ".to_string(),
                        _ => "▪ ".to_string(),
                    }
                };

                self.current_spans.push(Span::styled(
                    format!("{}{}", indent, marker),
                    Style::default().fg(Color::Yellow),
                ));
            }
            Tag::Emphasis => {
                self.push_style(Style::default().add_modifier(Modifier::ITALIC));
            }
            Tag::Strong => {
                self.push_style(Style::default().add_modifier(Modifier::BOLD));
            }
            Tag::Strikethrough => {
                self.push_style(Style::default().add_modifier(Modifier::CROSSED_OUT));
            }
            Tag::Link { dest_url, .. } => {
                self.push_style(Style::default().fg(Color::Blue).add_modifier(Modifier::UNDERLINED));
                // URL은 나중에 추가
                self.current_spans.push(Span::raw("")); // placeholder
                self.style_stack.push(Style::default()); // URL 저장용
                                                         // 실제 URL은 TagEnd에서 처리
                let _ = dest_url; // 사용하지 않음 (ratatui에서 클릭 불가)
            }
            _ => {}
        }
    }

    fn end_tag(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Heading(_) => {
                self.pop_style();
                self.finish_line();
                self.lines.push(Line::from("")); // 빈 줄 추가
            }
            TagEnd::Paragraph => {
                self.finish_line();
            }
            TagEnd::BlockQuote(_) => {
                self.quote_depth = self.quote_depth.saturating_sub(1);
            }
            TagEnd::CodeBlock => {
                self.in_code_block = false;
                self.add_code_block();
            }
            TagEnd::List(_) => {
                self.list_depth = self.list_depth.saturating_sub(1);
                self.list_counters.pop();
            }
            TagEnd::Item => {
                self.finish_line();
            }
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => {
                self.pop_style();
            }
            TagEnd::Link => {
                self.pop_style();
            }
            _ => {}
        }
    }

    fn add_inline_code(&mut self, code: &str) {
        let style = Style::default()
            .fg(Color::Rgb(255, 198, 109)) // 오렌지
            .bg(Color::Rgb(40, 42, 54)); // 어두운 배경

        self.current_spans.push(Span::styled(
            format!("`{}`", code),
            style,
        ));
    }

    fn add_code_block(&mut self) {
        self.finish_line();

        // 코드 블록 헤더
        let lang_display = if self.code_lang.is_empty() {
            "code"
        } else {
            &self.code_lang
        };

        self.lines.push(Line::from(vec![
            Span::styled("┌─ ", Style::default().fg(Color::DarkGray)),
            Span::styled(lang_display.to_string(), Style::default().fg(Color::Cyan)),
            Span::styled(" ─".to_string(), Style::default().fg(Color::DarkGray)),
        ]));

        // 코드 하이라이트
        let highlighted = self.highlighter.highlight(&self.code_buffer, &self.code_lang);

        for (i, line) in highlighted.into_iter().enumerate() {
            let line_num = format!("{:>3} │ ", i + 1);
            let mut spans = vec![Span::styled(line_num, Style::default().fg(Color::DarkGray))];
            spans.extend(line.spans);
            self.lines.push(Line::from(spans));
        }

        // 코드 블록 푸터
        self.lines.push(Line::from(Span::styled(
            "└────────────────────────────────",
            Style::default().fg(Color::DarkGray),
        )));

        self.code_buffer.clear();
    }

    fn add_horizontal_rule(&mut self) {
        self.finish_line();
        self.lines.push(Line::from(Span::styled(
            "────────────────────────────────────",
            Style::default().fg(Color::DarkGray),
        )));
    }

    fn finish(mut self) -> Vec<Line<'static>> {
        self.finish_line();
        self.lines
    }
}

/// 헤딩 레벨에 따른 스타일
fn heading_style(level: HeadingLevel, _theme: &Theme) -> Style {
    match level {
        HeadingLevel::H1 => Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD),
        HeadingLevel::H2 => Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
        HeadingLevel::H3 => Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD),
        HeadingLevel::H4 => Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
        HeadingLevel::H5 => Style::default()
            .fg(Color::Blue)
            .add_modifier(Modifier::BOLD),
        HeadingLevel::H6 => Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    }
}

/// 헤딩 마커
fn heading_marker(level: HeadingLevel) -> String {
    match level {
        HeadingLevel::H1 => "# ".to_string(),
        HeadingLevel::H2 => "## ".to_string(),
        HeadingLevel::H3 => "### ".to_string(),
        HeadingLevel::H4 => "#### ".to_string(),
        HeadingLevel::H5 => "##### ".to_string(),
        HeadingLevel::H6 => "###### ".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_text() {
        let renderer = MarkdownRenderer::default();
        let lines = renderer.render("Hello, world!");
        
        assert_eq!(lines.len(), 1);
        assert!(lines[0].spans.iter().any(|s| s.content.contains("Hello")));
    }

    #[test]
    fn test_heading() {
        let renderer = MarkdownRenderer::default();
        let lines = renderer.render("# Title\n\nParagraph");
        
        // # Title, 빈 줄, Paragraph = 3줄
        assert!(lines.len() >= 2);
    }

    #[test]
    fn test_code_block() {
        let renderer = MarkdownRenderer::default();
        let lines = renderer.render("```rust\nfn main() {}\n```");
        
        // 헤더 + 코드 + 푸터
        assert!(lines.len() >= 3);
    }

    #[test]
    fn test_inline_code() {
        let renderer = MarkdownRenderer::default();
        let lines = renderer.render("Use `println!` macro");
        
        assert_eq!(lines.len(), 1);
        // 인라인 코드가 백틱으로 감싸져 있어야 함
        let content: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(content.contains("`println!`"));
    }

    #[test]
    fn test_list() {
        let renderer = MarkdownRenderer::default();
        let lines = renderer.render("- Item 1\n- Item 2\n- Item 3");
        
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_emphasis() {
        let renderer = MarkdownRenderer::default();
        let lines = renderer.render("This is **bold** and *italic*");
        
        assert_eq!(lines.len(), 1);
        // bold와 italic 텍스트가 있어야 함
        let has_bold = lines[0].spans.iter().any(|s| {
            s.style.add_modifier.contains(Modifier::BOLD)
        });
        let has_italic = lines[0].spans.iter().any(|s| {
            s.style.add_modifier.contains(Modifier::ITALIC)
        });
        
        assert!(has_bold);
        assert!(has_italic);
    }
}
