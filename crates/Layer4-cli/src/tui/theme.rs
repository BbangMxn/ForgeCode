//! TUI Theme definitions

use ratatui::style::{Color, Modifier, Style};

/// Theme colors and styles
#[allow(dead_code)]
pub struct Theme {
    /// Primary accent color
    pub primary: Color,

    /// Secondary color
    pub secondary: Color,

    /// Background color
    pub background: Color,

    /// Foreground (text) color
    pub foreground: Color,

    /// Muted text color
    pub muted: Color,

    /// Success color
    pub success: Color,

    /// Error color
    pub error: Color,

    /// Warning color
    pub warning: Color,

    /// Border color
    pub border: Color,

    /// Highlight color
    pub highlight: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            primary: Color::Cyan,
            secondary: Color::Magenta,
            background: Color::Reset,
            foreground: Color::White,
            muted: Color::DarkGray,
            success: Color::Green,
            error: Color::Red,
            warning: Color::Yellow,
            border: Color::DarkGray,
            highlight: Color::Cyan,
        }
    }
}

#[allow(dead_code)]
impl Theme {
    /// Normal text style
    pub fn text(&self) -> Style {
        Style::default().fg(self.foreground)
    }

    /// Muted text style
    pub fn text_muted(&self) -> Style {
        Style::default().fg(self.muted)
    }

    /// Bold text style
    pub fn text_bold(&self) -> Style {
        Style::default()
            .fg(self.foreground)
            .add_modifier(Modifier::BOLD)
    }

    /// Primary accent style
    pub fn accent(&self) -> Style {
        Style::default().fg(self.primary)
    }

    /// Success style
    pub fn success(&self) -> Style {
        Style::default().fg(self.success)
    }

    /// Error style
    pub fn error(&self) -> Style {
        Style::default().fg(self.error)
    }

    /// Warning style
    pub fn warning(&self) -> Style {
        Style::default().fg(self.warning)
    }

    /// Border style
    pub fn border(&self) -> Style {
        Style::default().fg(self.border)
    }

    /// Highlighted border style
    pub fn border_focused(&self) -> Style {
        Style::default().fg(self.highlight)
    }

    /// User message style
    pub fn user_message(&self) -> Style {
        Style::default().fg(Color::Blue)
    }

    /// Assistant message style
    pub fn assistant_message(&self) -> Style {
        Style::default().fg(self.foreground)
    }

    /// Tool name style
    pub fn tool_name(&self) -> Style {
        Style::default()
            .fg(self.secondary)
            .add_modifier(Modifier::BOLD)
    }

    /// Status bar style
    pub fn status_bar(&self) -> Style {
        Style::default().bg(Color::DarkGray).fg(Color::White)
    }
}
