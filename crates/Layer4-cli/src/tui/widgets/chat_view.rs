//! Chat View Widget - ForgeCode 메시지 표시 영역
//!
//! Claude Code 스타일의 대화형 인터페이스
//!
//! ```text
//!  You                                                    12:34
//!  Fix the bug in main.rs
//!
//!  ─────────────────────────────────────────────────────────
//!
//!  Assistant                                              12:34
//!  I'll analyze and fix the bug.
//!
//!  ┌─ read main.rs ──────────────────────────── ✓ 0.3s ─┐
//!  │ fn main() {                                        │
//!  │     println!("Hello");                             │
//!  │ }                                                  │
//!  └────────────────────────────────────────────────────┘
//! ```

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Widget},
};

use crate::tui::theme::{current_theme, icons, Theme};

/// 메시지 역할
#[derive(Debug, Clone, PartialEq)]
pub enum MessageRole {
    User,
    Assistant,
    System,
    Tool,
}

/// 도구 실행 상태
#[derive(Debug, Clone, PartialEq)]
pub enum ToolExecutionState {
    Running,
    Success { duration_ms: u64 },
    Error { message: String },
}

/// 도구 실행 블록
#[derive(Debug, Clone)]
pub struct ToolBlock {
    /// 도구 이름
    pub name: String,
    /// 실행 상태
    pub state: ToolExecutionState,
    /// 출력/내용 (일부만 표시)
    pub content: String,
    /// 접힌 상태
    pub collapsed: bool,
}

impl ToolBlock {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            state: ToolExecutionState::Running,
            content: String::new(),
            collapsed: false,
        }
    }

    pub fn with_success(mut self, duration_ms: u64) -> Self {
        self.state = ToolExecutionState::Success { duration_ms };
        self
    }

    pub fn with_error(mut self, message: impl Into<String>) -> Self {
        self.state = ToolExecutionState::Error {
            message: message.into(),
        };
        self
    }

    pub fn with_content(mut self, content: impl Into<String>) -> Self {
        self.content = content.into();
        self
    }
}

/// 채팅 메시지
#[derive(Debug, Clone)]
pub struct ChatMessage {
    /// 역할
    pub role: MessageRole,
    /// 메시지 내용
    pub content: String,
    /// 타임스탬프
    pub timestamp: Option<chrono::DateTime<chrono::Local>>,
    /// 도구 블록 (어시스턴트 메시지에서)
    pub tool_blocks: Vec<ToolBlock>,
    /// 스트리밍 중인지
    pub streaming: bool,
}

impl ChatMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            content: content.into(),
            timestamp: Some(chrono::Local::now()),
            tool_blocks: Vec::new(),
            streaming: false,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: content.into(),
            timestamp: Some(chrono::Local::now()),
            tool_blocks: Vec::new(),
            streaming: false,
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::System,
            content: content.into(),
            timestamp: None,
            tool_blocks: Vec::new(),
            streaming: false,
        }
    }

    pub fn with_tool_block(mut self, block: ToolBlock) -> Self {
        self.tool_blocks.push(block);
        self
    }

    pub fn streaming(mut self) -> Self {
        self.streaming = true;
        self
    }

    /// 포맷된 타임스탬프
    pub fn formatted_time(&self) -> String {
        self.timestamp
            .map(|t| t.format("%H:%M").to_string())
            .unwrap_or_default()
    }
}

/// 채팅 뷰 상태
#[derive(Debug, Clone)]
pub struct ChatViewState {
    /// 메시지 목록
    pub messages: Vec<ChatMessage>,
    /// 스크롤 오프셋
    pub scroll_offset: usize,
    /// 자동 스크롤 활성화
    pub auto_scroll: bool,
}

impl ChatViewState {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            scroll_offset: 0,
            auto_scroll: true,
        }
    }

    pub fn push(&mut self, message: ChatMessage) {
        self.messages.push(message);
        if self.auto_scroll {
            self.scroll_to_bottom();
        }
    }

    pub fn scroll_up(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
        self.auto_scroll = false;
    }

    pub fn scroll_down(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_add(lines);
        // 하단에 도달하면 auto_scroll 재활성화
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = usize::MAX; // 렌더링 시 조정됨
        self.auto_scroll = true;
    }

    pub fn clear(&mut self) {
        self.messages.clear();
        self.scroll_offset = 0;
    }

    /// 마지막 어시스턴트 메시지에 텍스트 추가 (스트리밍용)
    pub fn append_to_last(&mut self, text: &str) {
        if let Some(last) = self.messages.last_mut() {
            if last.role == MessageRole::Assistant {
                last.content.push_str(text);
            }
        }
    }

    /// 마지막 어시스턴트 메시지에 도구 블록 추가
    pub fn add_tool_block(&mut self, block: ToolBlock) {
        if let Some(last) = self.messages.last_mut() {
            if last.role == MessageRole::Assistant {
                last.tool_blocks.push(block);
            }
        }
    }

    /// 마지막 도구 블록 업데이트
    pub fn update_last_tool(&mut self, state: ToolExecutionState, content: Option<String>) {
        if let Some(last_msg) = self.messages.last_mut() {
            if let Some(last_tool) = last_msg.tool_blocks.last_mut() {
                last_tool.state = state;
                if let Some(c) = content {
                    last_tool.content = c;
                }
            }
        }
    }
}

impl Default for ChatViewState {
    fn default() -> Self {
        Self::new()
    }
}

/// 채팅 뷰 위젯
pub struct ChatView<'a> {
    state: &'a ChatViewState,
    theme: Theme,
    /// 스피너 프레임 (애니메이션용)
    spinner_frame: usize,
}

impl<'a> ChatView<'a> {
    pub fn new(state: &'a ChatViewState) -> Self {
        Self {
            state,
            theme: current_theme(),
            spinner_frame: 0,
        }
    }

    pub fn with_theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    pub fn with_spinner_frame(mut self, frame: usize) -> Self {
        self.spinner_frame = frame;
        self
    }

    /// 메시지를 렌더링 가능한 라인으로 변환
    fn render_message(&self, msg: &ChatMessage, width: u16) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        let content_width = width.saturating_sub(4) as usize;

        // 역할 라벨 + 타임스탬프
        let (icon, label, label_style) = match msg.role {
            MessageRole::User => (icons::USER, "You", self.theme.user_label()),
            MessageRole::Assistant => (icons::ASSISTANT, "Assistant", self.theme.assistant_label()),
            MessageRole::System => (icons::SYSTEM, "System", self.theme.system_message()),
            MessageRole::Tool => (icons::TOOL, "Tool", self.theme.tool_running()),
        };

        let time_str = msg.formatted_time();
        let padding = content_width
            .saturating_sub(label.len() + icon.len() + time_str.len() + 4);

        lines.push(Line::from(vec![
            Span::raw(" "),
            Span::styled(format!("{} {}", icon, label), label_style),
            Span::raw(" ".repeat(padding)),
            Span::styled(time_str, self.theme.text_muted()),
            Span::raw(" "),
        ]));

        // 메시지 내용 (줄바꿈 처리)
        for line in msg.content.lines() {
            if line.is_empty() {
                lines.push(Line::from(""));
            } else {
                // 긴 줄 자동 줄바꿈
                let wrapped = textwrap::wrap(line, content_width);
                for wrapped_line in wrapped {
                    let style = if msg.role == MessageRole::System {
                        self.theme.system_message()
                    } else {
                        self.theme.text()
                    };
                    lines.push(Line::from(vec![
                        Span::raw(" "),
                        Span::styled(wrapped_line.to_string(), style),
                    ]));
                }
            }
        }

        // 스트리밍 커서
        if msg.streaming {
            let spinner = icons::SPINNER[self.spinner_frame % icons::SPINNER.len()];
            if let Some(last) = lines.last_mut() {
                last.spans.push(Span::styled(
                    format!(" {}", spinner),
                    self.theme.text_accent(),
                ));
            }
        }

        // 도구 블록 렌더링
        for tool in &msg.tool_blocks {
            lines.push(Line::from("")); // 빈 줄
            lines.extend(self.render_tool_block(tool, content_width as u16));
        }

        // 메시지 후 빈 줄
        lines.push(Line::from(""));

        lines
    }

    /// 도구 블록 렌더링
    fn render_tool_block(&self, tool: &ToolBlock, width: u16) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        let inner_width = width.saturating_sub(6) as usize;

        // 상태 표시
        let (status_icon, status_style, status_text) = match &tool.state {
            ToolExecutionState::Running => {
                let spinner = icons::SPINNER[self.spinner_frame % icons::SPINNER.len()];
                (spinner, self.theme.tool_running(), "running".to_string())
            }
            ToolExecutionState::Success { duration_ms } => (
                icons::CHECK,
                self.theme.tool_success(),
                format!("{:.1}s", *duration_ms as f64 / 1000.0),
            ),
            ToolExecutionState::Error { message: _ } => {
                (icons::CROSS, self.theme.tool_error(), "error".to_string())
            }
        };

        // 헤더 라인: ┌─ tool_name ────────── ✓ 0.3s ─┐
        let name_part = format!("─ {} ", tool.name);
        let status_part = format!(" {} {} ", status_icon, status_text);
        let fill_len = inner_width
            .saturating_sub(name_part.len() + status_part.len() + 2);

        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled("┌", self.theme.border()),
            Span::styled(name_part, self.theme.text_accent()),
            Span::styled("─".repeat(fill_len), self.theme.border()),
            Span::styled(status_part, status_style),
            Span::styled("─┐", self.theme.border()),
        ]));

        // 내용 (최대 5줄)
        let content_lines: Vec<&str> = tool.content.lines().take(5).collect();
        for content_line in &content_lines {
            let truncated = if content_line.len() > inner_width {
                format!("{}...", &content_line[..inner_width - 3])
            } else {
                content_line.to_string()
            };
            let padding = inner_width.saturating_sub(truncated.len());

            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled("│ ", self.theme.border()),
                Span::styled(truncated, self.theme.code_block()),
                Span::raw(" ".repeat(padding)),
                Span::styled(" │", self.theme.border()),
            ]));
        }

        // 더 많은 내용이 있으면 ... 표시
        if tool.content.lines().count() > 5 {
            let more_text = "... (more)";
            let padding = inner_width.saturating_sub(more_text.len());
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled("│ ", self.theme.border()),
                Span::styled(more_text, self.theme.text_muted()),
                Span::raw(" ".repeat(padding)),
                Span::styled(" │", self.theme.border()),
            ]));
        }

        // 하단 라인: └────────────────────────────────┘
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled("└", self.theme.border()),
            Span::styled("─".repeat(inner_width + 2), self.theme.border()),
            Span::styled("┘", self.theme.border()),
        ]));

        lines
    }

    /// 구분선
    fn render_separator(&self, width: u16) -> Line<'static> {
        let sep = "─".repeat(width.saturating_sub(4) as usize);
        Line::from(vec![
            Span::raw("  "),
            Span::styled(sep, self.theme.border()),
            Span::raw("  "),
        ])
    }
}

impl Widget for ChatView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::LEFT | Borders::RIGHT)
            .border_style(self.theme.border());

        let inner = block.inner(area);
        block.render(area, buf);

        // 모든 메시지를 라인으로 변환
        let mut all_lines: Vec<Line<'static>> = Vec::new();

        for (i, msg) in self.state.messages.iter().enumerate() {
            // 메시지 사이에 구분선 (유저 메시지 전에)
            if i > 0 && msg.role == MessageRole::User {
                all_lines.push(self.render_separator(inner.width));
            }

            all_lines.extend(self.render_message(msg, inner.width));
        }

        // 스크롤 계산
        let total_lines = all_lines.len();
        let visible_lines = inner.height as usize;
        let max_scroll = total_lines.saturating_sub(visible_lines);
        
        let scroll_offset = if self.state.auto_scroll || self.state.scroll_offset > max_scroll {
            max_scroll
        } else {
            self.state.scroll_offset
        };

        // 보이는 라인만 렌더링
        let visible: Vec<Line<'static>> = all_lines
            .into_iter()
            .skip(scroll_offset)
            .take(visible_lines)
            .collect();

        let text = Text::from(visible);
        Paragraph::new(text).render(inner, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_message() {
        let msg = ChatMessage::user("Hello");
        assert_eq!(msg.role, MessageRole::User);
        assert_eq!(msg.content, "Hello");
    }

    #[test]
    fn test_tool_block() {
        let tool = ToolBlock::new("read")
            .with_success(350)
            .with_content("file content");
        
        assert!(matches!(tool.state, ToolExecutionState::Success { duration_ms: 350 }));
    }

    #[test]
    fn test_chat_view_state() {
        let mut state = ChatViewState::new();
        state.push(ChatMessage::user("Test"));
        state.push(ChatMessage::assistant("Response"));
        
        assert_eq!(state.messages.len(), 2);
    }
}
