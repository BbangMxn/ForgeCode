//! Message list component

#![allow(dead_code)]

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

/// A message in the chat
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
    pub tool_info: Option<ToolInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MessageRole {
    User,
    Assistant,
    Tool,
    System,
}

#[derive(Debug, Clone)]
pub struct ToolInfo {
    pub name: String,
    pub status: ToolStatus,
}

#[derive(Debug, Clone, Copy)]
pub enum ToolStatus {
    Running,
    Success,
    Error,
}

/// Message list widget
pub struct MessageList {
    pub messages: Vec<ChatMessage>,
    scroll: u16,
}

#[allow(dead_code)]
impl MessageList {
    /// Create a new message list
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            scroll: 0,
        }
    }

    /// Add a message
    pub fn push(&mut self, message: ChatMessage) {
        self.messages.push(message);
        // Auto-scroll to bottom
        self.scroll_to_bottom();
    }

    /// Clear all messages
    pub fn clear(&mut self) {
        self.messages.clear();
        self.scroll = 0;
    }

    /// Scroll up
    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    /// Scroll down
    pub fn scroll_down(&mut self) {
        self.scroll = self.scroll.saturating_add(1);
    }

    /// Scroll to bottom
    pub fn scroll_to_bottom(&mut self) {
        // This will be clamped during render
        self.scroll = u16::MAX;
    }

    /// Render the message list
    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let mut lines: Vec<Line> = Vec::new();

        for msg in &self.messages {
            // Add role prefix
            let (prefix, style) = match msg.role {
                MessageRole::User => (
                    "You",
                    Style::default()
                        .fg(Color::Blue)
                        .add_modifier(Modifier::BOLD),
                ),
                MessageRole::Assistant => (
                    "Assistant",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                MessageRole::Tool => {
                    if let Some(ref info) = msg.tool_info {
                        let _status_symbol = match info.status {
                            ToolStatus::Running => "⟳",
                            ToolStatus::Success => "✓",
                            ToolStatus::Error => "✗",
                        };
                        let color = match info.status {
                            ToolStatus::Running => Color::Yellow,
                            ToolStatus::Success => Color::Green,
                            ToolStatus::Error => Color::Red,
                        };
                        ("", Style::default().fg(color))
                    } else {
                        ("Tool", Style::default().fg(Color::Magenta))
                    }
                }
                MessageRole::System => ("System", Style::default().fg(Color::DarkGray)),
            };

            // Build the message line
            if msg.role == MessageRole::Tool {
                if let Some(ref info) = msg.tool_info {
                    let status_symbol = match info.status {
                        ToolStatus::Running => "⟳",
                        ToolStatus::Success => "✓",
                        ToolStatus::Error => "✗",
                    };
                    let color = match info.status {
                        ToolStatus::Running => Color::Yellow,
                        ToolStatus::Success => Color::Green,
                        ToolStatus::Error => Color::Red,
                    };

                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("[{}] ", info.name),
                            Style::default().fg(Color::Magenta),
                        ),
                        Span::styled(status_symbol, Style::default().fg(color)),
                        Span::raw(" "),
                        Span::raw(truncate_line(&msg.content, area.width as usize - 20)),
                    ]));
                }
            } else {
                lines.push(Line::from(vec![Span::styled(
                    format!("{}: ", prefix),
                    style,
                )]));

                // Add content lines
                for line in msg.content.lines() {
                    lines.push(Line::from(vec![Span::raw("  "), Span::raw(line)]));
                }
            }

            // Add empty line between messages
            lines.push(Line::from(""));
        }

        // Calculate max scroll
        let content_height = lines.len() as u16;
        let visible_height = area.height.saturating_sub(2); // Account for borders
        let max_scroll = content_height.saturating_sub(visible_height);

        // Clamp scroll
        self.scroll = self.scroll.min(max_scroll);

        let paragraph = Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray))
                    .title(" Chat "),
            )
            .wrap(Wrap { trim: false })
            .scroll((self.scroll, 0));

        frame.render_widget(paragraph, area);
    }
}

impl Default for MessageList {
    fn default() -> Self {
        Self::new()
    }
}

/// Truncate a line for display
fn truncate_line(s: &str, max_len: usize) -> String {
    let s = s.replace('\n', " ");
    if s.len() <= max_len {
        s
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}
