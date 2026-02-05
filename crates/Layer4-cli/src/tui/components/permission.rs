//! Permission Modal Component
//!
//! Displays a modal dialog for permission requests.
//! Implements Layer1's PermissionDelegate trait for TUI.

#![allow(dead_code)]

use forge_foundation::permission::{PermissionAction, PermissionScope};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

/// Permission response from user
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionResponse {
    /// Allow for this request only
    AllowOnce,
    /// Allow for current session
    AllowSession,
    /// Allow permanently
    AllowPermanent,
    /// Deny this request
    Deny,
    /// Deny permanently
    DenyPermanent,
}

impl PermissionResponse {
    /// Convert to Layer1 PermissionScope (for grants)
    pub fn to_scope(&self) -> Option<PermissionScope> {
        match self {
            Self::AllowOnce => Some(PermissionScope::Once),
            Self::AllowSession => Some(PermissionScope::Session),
            Self::AllowPermanent => Some(PermissionScope::Permanent),
            _ => None,
        }
    }

    /// Check if this is an allow response
    pub fn is_allow(&self) -> bool {
        matches!(
            self,
            Self::AllowOnce | Self::AllowSession | Self::AllowPermanent
        )
    }
}

/// Option in the permission modal
#[derive(Clone)]
struct PermissionOption {
    /// Display label
    label: String,
    /// Keyboard shortcut
    key: char,
    /// Response value
    response: PermissionResponse,
    /// Style for this option
    style: Style,
}

/// Permission Modal widget
pub struct PermissionModal {
    /// Tool requesting permission
    tool_name: String,
    /// Action being requested
    action: PermissionAction,
    /// Human-readable description
    description: String,
    /// Risk score (0-10)
    risk_score: u8,
    /// Available options
    options: Vec<PermissionOption>,
    /// Currently selected option index
    selected: usize,
    /// Whether modal is visible
    visible: bool,
}

impl PermissionModal {
    /// Create a new permission modal
    pub fn new(
        tool_name: &str,
        action: PermissionAction,
        description: &str,
        risk_score: u8,
    ) -> Self {
        let options = vec![
            PermissionOption {
                label: "Allow Once".to_string(),
                key: 'o',
                response: PermissionResponse::AllowOnce,
                style: Style::default().fg(Color::Green),
            },
            PermissionOption {
                label: "Allow Session".to_string(),
                key: 's',
                response: PermissionResponse::AllowSession,
                style: Style::default().fg(Color::Cyan),
            },
            PermissionOption {
                label: "Allow Permanent".to_string(),
                key: 'p',
                response: PermissionResponse::AllowPermanent,
                style: Style::default().fg(Color::Blue),
            },
            PermissionOption {
                label: "Deny".to_string(),
                key: 'd',
                response: PermissionResponse::Deny,
                style: Style::default().fg(Color::Yellow),
            },
            PermissionOption {
                label: "Deny Permanent".to_string(),
                key: 'n',
                response: PermissionResponse::DenyPermanent,
                style: Style::default().fg(Color::Red),
            },
        ];

        Self {
            tool_name: tool_name.to_string(),
            action,
            description: description.to_string(),
            risk_score,
            options,
            selected: 0,
            visible: true,
        }
    }

    /// Show the modal
    pub fn show(&mut self) {
        self.visible = true;
        self.selected = 0;
    }

    /// Hide the modal
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Check if modal is visible
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Move selection up
    pub fn select_prev(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        } else {
            self.selected = self.options.len() - 1;
        }
    }

    /// Move selection down
    pub fn select_next(&mut self) {
        if self.selected < self.options.len() - 1 {
            self.selected += 1;
        } else {
            self.selected = 0;
        }
    }

    /// Get the currently selected response
    pub fn get_selected(&self) -> PermissionResponse {
        self.options[self.selected].response
    }

    /// Handle key press, returns Some(response) if a choice was made
    pub fn handle_key(&mut self, key: char) -> Option<PermissionResponse> {
        // Check for shortcut keys
        let key_lower = key.to_ascii_lowercase();
        for option in &self.options {
            if option.key == key_lower {
                return Some(option.response);
            }
        }

        // Handle navigation
        match key {
            'j' => self.select_next(),
            'k' => self.select_prev(),
            _ => {}
        }

        None
    }

    /// Confirm current selection
    pub fn confirm(&self) -> PermissionResponse {
        self.get_selected()
    }

    /// Get risk level style based on score
    fn risk_style(&self) -> Style {
        match self.risk_score {
            0..=3 => Style::default().fg(Color::Green),
            4..=6 => Style::default().fg(Color::Yellow),
            7..=8 => Style::default().fg(Color::Rgb(255, 165, 0)), // Orange
            9..=10 => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            _ => Style::default().fg(Color::Red),
        }
    }

    /// Get risk level text
    fn risk_text(&self) -> &'static str {
        match self.risk_score {
            0..=3 => "Low Risk",
            4..=6 => "Medium Risk",
            7..=8 => "High Risk",
            9..=10 => "DANGEROUS",
            _ => "Unknown",
        }
    }

    /// Create a centered rect for the modal
    fn centered_rect(&self, percent_x: u16, percent_y: u16, area: Rect) -> Rect {
        let popup_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage((100 - percent_y) / 2),
                Constraint::Percentage(percent_y),
                Constraint::Percentage((100 - percent_y) / 2),
            ])
            .split(area);

        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage((100 - percent_x) / 2),
                Constraint::Percentage(percent_x),
                Constraint::Percentage((100 - percent_x) / 2),
            ])
            .split(popup_layout[1])[1]
    }

    /// Render the modal
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        if !self.visible {
            return;
        }

        // Calculate modal area (60% width, 50% height)
        let modal_area = self.centered_rect(60, 50, area);

        // Clear the area behind modal
        frame.render_widget(Clear, modal_area);

        // Modal border with risk-colored title
        let title = format!(" Permission Required: {} ", self.tool_name);
        let block = Block::default()
            .title(title)
            .title_style(self.risk_style().add_modifier(Modifier::BOLD))
            .borders(Borders::ALL)
            .border_style(self.risk_style())
            .style(Style::default().bg(Color::Black));

        frame.render_widget(block.clone(), modal_area);

        // Inner area for content
        let inner = modal_area.inner(Margin::new(2, 1));

        // Split into sections
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // Risk indicator
                Constraint::Length(1), // Spacer
                Constraint::Length(3), // Action description
                Constraint::Length(1), // Spacer
                Constraint::Min(5),    // Options
                Constraint::Length(2), // Help text
            ])
            .split(inner);

        // Risk indicator
        let risk_line = Line::from(vec![
            Span::raw("Risk Level: "),
            Span::styled(
                format!("{}/10 - {}", self.risk_score, self.risk_text()),
                self.risk_style().add_modifier(Modifier::BOLD),
            ),
        ]);
        let risk_para = Paragraph::new(risk_line).alignment(Alignment::Center);
        frame.render_widget(risk_para, chunks[0]);

        // Action description
        let action_text = format!("{}\n\n{}", self.action.description(), self.description);
        let action_para = Paragraph::new(action_text)
            .wrap(Wrap { trim: true })
            .style(Style::default().fg(Color::White));
        frame.render_widget(action_para, chunks[2]);

        // Options
        let options_area = chunks[4];
        let option_height = 1;
        for (i, option) in self.options.iter().enumerate() {
            if i as u16 * option_height >= options_area.height {
                break;
            }

            let option_area = Rect::new(
                options_area.x,
                options_area.y + (i as u16 * option_height),
                options_area.width,
                option_height,
            );

            let is_selected = i == self.selected;
            let style = if is_selected {
                option.style.add_modifier(Modifier::REVERSED)
            } else {
                option.style
            };

            let prefix = if is_selected { "▶ " } else { "  " };
            let line = Line::from(vec![
                Span::raw(prefix),
                Span::styled(
                    format!("[{}] ", option.key.to_uppercase()),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(&option.label, style),
            ]);

            let para = Paragraph::new(line);
            frame.render_widget(para, option_area);
        }

        // Help text
        let help_text = "↑↓/jk: Navigate  Enter: Select  o/s/p/d/n: Quick select  Esc: Deny";
        let help_para = Paragraph::new(help_text)
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(help_para, chunks[5]);
    }
}

/// Manager for permission modals with async response handling
pub struct PermissionModalManager {
    /// Current modal (if any)
    current: Option<PermissionModal>,
    /// Pending response sender
    response_tx: Option<tokio::sync::oneshot::Sender<PermissionResponse>>,
}

impl PermissionModalManager {
    /// Create a new manager
    pub fn new() -> Self {
        Self {
            current: None,
            response_tx: None,
        }
    }

    /// Check if a modal is currently showing
    pub fn has_modal(&self) -> bool {
        self.current.is_some()
    }

    /// Show a permission request modal and return a receiver for the response
    pub fn show(
        &mut self,
        tool_name: &str,
        action: PermissionAction,
        description: &str,
        risk_score: u8,
    ) -> tokio::sync::oneshot::Receiver<PermissionResponse> {
        let (tx, rx) = tokio::sync::oneshot::channel();

        self.current = Some(PermissionModal::new(
            tool_name,
            action,
            description,
            risk_score,
        ));
        self.response_tx = Some(tx);

        rx
    }

    /// Handle key input for the modal
    /// Returns true if the key was consumed
    pub fn handle_key(&mut self, key: crossterm::event::KeyCode) -> bool {
        let Some(modal) = &mut self.current else {
            return false;
        };

        use crossterm::event::KeyCode;

        match key {
            KeyCode::Up | KeyCode::Char('k') => {
                modal.select_prev();
                true
            }
            KeyCode::Down | KeyCode::Char('j') => {
                modal.select_next();
                true
            }
            KeyCode::Enter => {
                let response = modal.confirm();
                self.send_response(response);
                true
            }
            KeyCode::Esc => {
                self.send_response(PermissionResponse::Deny);
                true
            }
            KeyCode::Char(c) => {
                if let Some(response) = modal.handle_key(c) {
                    self.send_response(response);
                }
                true
            }
            _ => false,
        }
    }

    /// Send response and close modal
    fn send_response(&mut self, response: PermissionResponse) {
        if let Some(tx) = self.response_tx.take() {
            let _ = tx.send(response);
        }
        self.current = None;
    }

    /// Render the current modal (if any)
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        if let Some(modal) = &self.current {
            modal.render(frame, area);
        }
    }
}

impl Default for PermissionModalManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_modal_creation() {
        let action = PermissionAction::Execute {
            command: "rm -rf /tmp/test".to_string(),
        };
        let modal = PermissionModal::new("bash", action, "Delete temporary files", 5);

        assert!(modal.is_visible());
        assert_eq!(modal.risk_score, 5);
        assert_eq!(modal.selected, 0);
    }

    #[test]
    fn test_navigation() {
        let action = PermissionAction::FileWrite {
            path: "/tmp/test.txt".to_string(),
        };
        let mut modal = PermissionModal::new("write", action, "Write test file", 3);

        assert_eq!(modal.selected, 0);

        modal.select_next();
        assert_eq!(modal.selected, 1);

        modal.select_next();
        assert_eq!(modal.selected, 2);

        modal.select_prev();
        assert_eq!(modal.selected, 1);

        // Wrap around
        modal.selected = 0;
        modal.select_prev();
        assert_eq!(modal.selected, 4); // Last option
    }

    #[test]
    fn test_shortcut_keys() {
        let action = PermissionAction::Network {
            url: "https://example.com".to_string(),
        };
        let mut modal = PermissionModal::new("http", action, "HTTP request", 2);

        assert_eq!(modal.handle_key('o'), Some(PermissionResponse::AllowOnce));
        assert_eq!(
            modal.handle_key('s'),
            Some(PermissionResponse::AllowSession)
        );
        assert_eq!(
            modal.handle_key('p'),
            Some(PermissionResponse::AllowPermanent)
        );
        assert_eq!(modal.handle_key('d'), Some(PermissionResponse::Deny));
        assert_eq!(
            modal.handle_key('n'),
            Some(PermissionResponse::DenyPermanent)
        );
        assert_eq!(modal.handle_key('x'), None); // Unknown key
    }

    #[test]
    fn test_risk_levels() {
        let action = PermissionAction::Execute {
            command: "ls".to_string(),
        };

        let low_risk = PermissionModal::new("bash", action.clone(), "", 2);
        assert_eq!(low_risk.risk_text(), "Low Risk");

        let medium_risk = PermissionModal::new("bash", action.clone(), "", 5);
        assert_eq!(medium_risk.risk_text(), "Medium Risk");

        let high_risk = PermissionModal::new("bash", action.clone(), "", 8);
        assert_eq!(high_risk.risk_text(), "High Risk");

        let dangerous = PermissionModal::new("bash", action, "", 10);
        assert_eq!(dangerous.risk_text(), "DANGEROUS");
    }
}
