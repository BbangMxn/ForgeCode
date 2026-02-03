//! Input box component

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

/// Text input box
pub struct InputBox {
    /// Current input text
    content: String,

    /// Cursor position
    cursor: usize,

    /// Whether the input is focused
    focused: bool,

    /// Placeholder text
    placeholder: String,
}

impl InputBox {
    /// Create a new input box
    pub fn new() -> Self {
        Self {
            content: String::new(),
            cursor: 0,
            focused: true,
            placeholder: "Type a message... (Ctrl+C to quit)".to_string(),
        }
    }

    /// Set placeholder text
    pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Get current content
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Take the content (clears the input)
    pub fn take(&mut self) -> String {
        self.cursor = 0;
        std::mem::take(&mut self.content)
    }

    /// Set focus
    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    /// Check if focused
    pub fn is_focused(&self) -> bool {
        self.focused
    }

    /// Handle key event, returns true if Enter was pressed
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Enter => {
                if !self.content.is_empty() {
                    return true;
                }
            }
            KeyCode::Char(c) => {
                self.content.insert(self.cursor, c);
                self.cursor += 1;
            }
            KeyCode::Backspace => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    self.content.remove(self.cursor);
                }
            }
            KeyCode::Delete => {
                if self.cursor < self.content.len() {
                    self.content.remove(self.cursor);
                }
            }
            KeyCode::Left => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
            }
            KeyCode::Right => {
                if self.cursor < self.content.len() {
                    self.cursor += 1;
                }
            }
            KeyCode::Home => {
                self.cursor = 0;
            }
            KeyCode::End => {
                self.cursor = self.content.len();
            }
            _ => {}
        }
        false
    }

    /// Render the input box
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let border_style = if self.focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let display_text = if self.content.is_empty() && !self.focused {
            self.placeholder.clone()
        } else {
            self.content.clone()
        };

        let text_style = if self.content.is_empty() && !self.focused {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default().fg(Color::White)
        };

        let input = Paragraph::new(display_text).style(text_style).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style)
                .title(" Input "),
        );

        frame.render_widget(input, area);

        // Show cursor
        if self.focused {
            frame.set_cursor_position((area.x + self.cursor as u16 + 1, area.y + 1));
        }
    }
}

impl Default for InputBox {
    fn default() -> Self {
        Self::new()
    }
}
