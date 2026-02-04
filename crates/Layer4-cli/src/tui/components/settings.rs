//! Settings Page Component
//!
//! A TUI component for configuring ForgeCode settings.
//!
//! Features:
//! - Provider configuration (API keys, endpoints)
//! - Tool permissions
//! - Theme settings
//! - Session preferences

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs, Wrap},
    Frame,
};

/// Setting category tabs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    General,
    Providers,
    Permissions,
    Theme,
    Advanced,
}

impl SettingsTab {
    pub fn all() -> Vec<Self> {
        vec![
            Self::General,
            Self::Providers,
            Self::Permissions,
            Self::Theme,
            Self::Advanced,
        ]
    }

    pub fn title(&self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Providers => "Providers",
            Self::Permissions => "Permissions",
            Self::Theme => "Theme",
            Self::Advanced => "Advanced",
        }
    }
}

/// A single setting item
#[derive(Debug, Clone)]
pub struct SettingItem {
    pub key: String,
    pub label: String,
    pub description: String,
    pub value: SettingValue,
    pub modified: bool,
}

impl SettingItem {
    pub fn new(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            description: String::new(),
            value: SettingValue::String(String::new()),
            modified: false,
        }
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    pub fn with_value(mut self, value: SettingValue) -> Self {
        self.value = value;
        self
    }

    pub fn display_value(&self) -> String {
        match &self.value {
            SettingValue::String(s) => {
                if s.is_empty() {
                    "(not set)".to_string()
                } else if self.key.contains("key") || self.key.contains("secret") {
                    // Mask sensitive values
                    format!("{}***", &s[..4.min(s.len())])
                } else {
                    s.clone()
                }
            }
            SettingValue::Bool(b) => if *b { "Enabled" } else { "Disabled" }.to_string(),
            SettingValue::Number(n) => n.to_string(),
            SettingValue::Choice { selected, options } => {
                options.get(*selected).cloned().unwrap_or_default()
            }
        }
    }
}

/// Setting value types
#[derive(Debug, Clone)]
pub enum SettingValue {
    String(String),
    Bool(bool),
    Number(i64),
    Choice {
        selected: usize,
        options: Vec<String>,
    },
}

impl SettingValue {
    pub fn toggle(&mut self) {
        if let SettingValue::Bool(b) = self {
            *b = !*b;
        }
    }

    pub fn next_choice(&mut self) {
        if let SettingValue::Choice { selected, options } = self {
            *selected = (*selected + 1) % options.len();
        }
    }

    pub fn prev_choice(&mut self) {
        if let SettingValue::Choice { selected, options } = self {
            *selected = if *selected == 0 {
                options.len().saturating_sub(1)
            } else {
                *selected - 1
            };
        }
    }
}

/// Settings page component
pub struct SettingsPage {
    /// Current tab
    current_tab: SettingsTab,
    /// Settings for each tab
    general_settings: Vec<SettingItem>,
    provider_settings: Vec<SettingItem>,
    permission_settings: Vec<SettingItem>,
    theme_settings: Vec<SettingItem>,
    advanced_settings: Vec<SettingItem>,
    /// Selected item in current tab
    selected_item: usize,
    /// List state for rendering
    list_state: ListState,
    /// Whether the settings page is visible
    visible: bool,
    /// Editing mode (for string values)
    editing: bool,
    /// Edit buffer
    edit_buffer: String,
    /// Has unsaved changes
    has_changes: bool,
}

impl SettingsPage {
    pub fn new() -> Self {
        let mut page = Self {
            current_tab: SettingsTab::General,
            general_settings: Self::default_general_settings(),
            provider_settings: Self::default_provider_settings(),
            permission_settings: Self::default_permission_settings(),
            theme_settings: Self::default_theme_settings(),
            advanced_settings: Self::default_advanced_settings(),
            selected_item: 0,
            list_state: ListState::default(),
            visible: false,
            editing: false,
            edit_buffer: String::new(),
            has_changes: false,
        };
        page.list_state.select(Some(0));
        page
    }

    fn default_general_settings() -> Vec<SettingItem> {
        vec![
            SettingItem::new("default_model", "Default Model")
                .with_description("The default AI model to use")
                .with_value(SettingValue::Choice {
                    selected: 0,
                    options: vec![
                        "claude-sonnet-4".to_string(),
                        "claude-opus-4".to_string(),
                        "claude-3-5-haiku".to_string(),
                        "gpt-4o".to_string(),
                    ],
                }),
            SettingItem::new("auto_save", "Auto-save Sessions")
                .with_description("Automatically save conversation history")
                .with_value(SettingValue::Bool(true)),
            SettingItem::new("max_turns", "Max Agent Turns")
                .with_description("Maximum number of tool call rounds per request")
                .with_value(SettingValue::Number(20)),
            SettingItem::new("working_dir", "Working Directory")
                .with_description("Default working directory for file operations")
                .with_value(SettingValue::String(".".to_string())),
        ]
    }

    fn default_provider_settings() -> Vec<SettingItem> {
        vec![
            SettingItem::new("anthropic_api_key", "Anthropic API Key")
                .with_description("API key for Claude models")
                .with_value(SettingValue::String(String::new())),
            SettingItem::new("openai_api_key", "OpenAI API Key")
                .with_description("API key for GPT models")
                .with_value(SettingValue::String(String::new())),
            SettingItem::new("google_api_key", "Google API Key")
                .with_description("API key for Gemini models")
                .with_value(SettingValue::String(String::new())),
            SettingItem::new("api_base_url", "Custom API Base URL")
                .with_description("Override the default API endpoint (optional)")
                .with_value(SettingValue::String(String::new())),
        ]
    }

    fn default_permission_settings() -> Vec<SettingItem> {
        vec![
            SettingItem::new("auto_approve_read", "Auto-approve File Reads")
                .with_description("Automatically allow reading files")
                .with_value(SettingValue::Bool(true)),
            SettingItem::new("auto_approve_write", "Auto-approve File Writes")
                .with_description("Automatically allow writing files")
                .with_value(SettingValue::Bool(false)),
            SettingItem::new("auto_approve_bash", "Auto-approve Bash Commands")
                .with_description("Automatically allow shell commands")
                .with_value(SettingValue::Bool(false)),
            SettingItem::new("permission_scope", "Default Permission Scope")
                .with_description("Default scope for granted permissions")
                .with_value(SettingValue::Choice {
                    selected: 1,
                    options: vec![
                        "Once".to_string(),
                        "Session".to_string(),
                        "Permanent".to_string(),
                    ],
                }),
        ]
    }

    fn default_theme_settings() -> Vec<SettingItem> {
        vec![
            SettingItem::new("color_scheme", "Color Scheme")
                .with_description("UI color theme")
                .with_value(SettingValue::Choice {
                    selected: 0,
                    options: vec![
                        "Dark".to_string(),
                        "Light".to_string(),
                        "Monokai".to_string(),
                        "Solarized".to_string(),
                    ],
                }),
            SettingItem::new("show_timestamps", "Show Timestamps")
                .with_description("Display timestamps on messages")
                .with_value(SettingValue::Bool(true)),
            SettingItem::new("show_token_count", "Show Token Count")
                .with_description("Display token usage statistics")
                .with_value(SettingValue::Bool(true)),
            SettingItem::new("syntax_highlight", "Syntax Highlighting")
                .with_description("Enable code syntax highlighting")
                .with_value(SettingValue::Bool(true)),
        ]
    }

    fn default_advanced_settings() -> Vec<SettingItem> {
        vec![
            SettingItem::new("context_limit", "Context Window Limit")
                .with_description("Maximum tokens for context (0 = auto)")
                .with_value(SettingValue::Number(0)),
            SettingItem::new("parallel_tools", "Parallel Tool Execution")
                .with_description("Execute independent tools in parallel")
                .with_value(SettingValue::Bool(true)),
            SettingItem::new("max_parallel", "Max Parallel Tools")
                .with_description("Maximum concurrent tool executions")
                .with_value(SettingValue::Number(4)),
            SettingItem::new("enable_mcp", "Enable MCP Servers")
                .with_description("Connect to MCP tool servers")
                .with_value(SettingValue::Bool(true)),
            SettingItem::new("debug_mode", "Debug Mode")
                .with_description("Enable verbose logging")
                .with_value(SettingValue::Bool(false)),
        ]
    }

    /// Show the settings page
    pub fn show(&mut self) {
        self.visible = true;
    }

    /// Hide the settings page
    pub fn hide(&mut self) {
        self.visible = false;
        self.editing = false;
    }

    /// Toggle visibility
    pub fn toggle(&mut self) {
        if self.visible {
            self.hide();
        } else {
            self.show();
        }
    }

    /// Check if visible
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Check if has unsaved changes
    pub fn has_unsaved_changes(&self) -> bool {
        self.has_changes
    }

    /// Get current settings for the active tab
    fn current_settings(&self) -> &[SettingItem] {
        match self.current_tab {
            SettingsTab::General => &self.general_settings,
            SettingsTab::Providers => &self.provider_settings,
            SettingsTab::Permissions => &self.permission_settings,
            SettingsTab::Theme => &self.theme_settings,
            SettingsTab::Advanced => &self.advanced_settings,
        }
    }

    /// Get mutable current settings
    fn current_settings_mut(&mut self) -> &mut Vec<SettingItem> {
        match self.current_tab {
            SettingsTab::General => &mut self.general_settings,
            SettingsTab::Providers => &mut self.provider_settings,
            SettingsTab::Permissions => &mut self.permission_settings,
            SettingsTab::Theme => &mut self.theme_settings,
            SettingsTab::Advanced => &mut self.advanced_settings,
        }
    }

    /// Switch to next tab
    pub fn next_tab(&mut self) {
        let tabs = SettingsTab::all();
        let current_idx = tabs
            .iter()
            .position(|t| *t == self.current_tab)
            .unwrap_or(0);
        self.current_tab = tabs[(current_idx + 1) % tabs.len()];
        self.selected_item = 0;
        self.list_state.select(Some(0));
    }

    /// Switch to previous tab
    pub fn prev_tab(&mut self) {
        let tabs = SettingsTab::all();
        let current_idx = tabs
            .iter()
            .position(|t| *t == self.current_tab)
            .unwrap_or(0);
        self.current_tab = tabs[if current_idx == 0 {
            tabs.len() - 1
        } else {
            current_idx - 1
        }];
        self.selected_item = 0;
        self.list_state.select(Some(0));
    }

    /// Select next item
    pub fn select_next(&mut self) {
        let max = self.current_settings().len().saturating_sub(1);
        self.selected_item = (self.selected_item + 1).min(max);
        self.list_state.select(Some(self.selected_item));
    }

    /// Select previous item
    pub fn select_prev(&mut self) {
        self.selected_item = self.selected_item.saturating_sub(1);
        self.list_state.select(Some(self.selected_item));
    }

    /// Toggle or edit selected item
    pub fn activate_selected(&mut self) {
        let selected = self.selected_item;

        // Check value type and get string value if needed
        let string_value: Option<String> = self.current_settings().get(selected).and_then(|item| {
            if let SettingValue::String(s) = &item.value {
                Some(s.clone())
            } else {
                None
            }
        });

        if let Some(s) = string_value {
            self.edit_buffer = s;
            self.editing = true;
            return;
        }

        if let Some(item) = self.current_settings_mut().get_mut(selected) {
            match &mut item.value {
                SettingValue::Bool(_) => {
                    item.value.toggle();
                    item.modified = true;
                    self.has_changes = true;
                }
                SettingValue::Choice { .. } => {
                    item.value.next_choice();
                    item.modified = true;
                    self.has_changes = true;
                }
                SettingValue::String(_) | SettingValue::Number(_) => {
                    // String handled above, Number not implemented
                }
            }
        }
    }

    /// Handle text input during editing
    pub fn handle_edit_input(&mut self, c: char) {
        if self.editing {
            self.edit_buffer.push(c);
        }
    }

    /// Handle backspace during editing
    pub fn handle_edit_backspace(&mut self) {
        if self.editing {
            self.edit_buffer.pop();
        }
    }

    /// Confirm edit
    pub fn confirm_edit(&mut self) {
        if self.editing {
            let new_value = self.edit_buffer.clone();
            let selected = self.selected_item;
            if let Some(item) = self.current_settings_mut().get_mut(selected) {
                if let SettingValue::String(s) = &mut item.value {
                    *s = new_value;
                    item.modified = true;
                    self.has_changes = true;
                }
            }
            self.editing = false;
            self.edit_buffer.clear();
        }
    }

    /// Cancel edit
    pub fn cancel_edit(&mut self) {
        self.editing = false;
        self.edit_buffer.clear();
    }

    /// Save all settings
    pub fn save(&mut self) -> Result<(), String> {
        // In a real implementation, this would persist to config file
        self.has_changes = false;
        for item in self.general_settings.iter_mut() {
            item.modified = false;
        }
        for item in self.provider_settings.iter_mut() {
            item.modified = false;
        }
        for item in self.permission_settings.iter_mut() {
            item.modified = false;
        }
        for item in self.theme_settings.iter_mut() {
            item.modified = false;
        }
        for item in self.advanced_settings.iter_mut() {
            item.modified = false;
        }
        Ok(())
    }

    /// Render the settings page
    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        if !self.visible {
            return;
        }

        // Calculate popup area (larger than other modals)
        let popup_width = 70.min(area.width.saturating_sub(4));
        let popup_height = 25.min(area.height.saturating_sub(4));
        let popup_x = (area.width.saturating_sub(popup_width)) / 2;
        let popup_y = (area.height.saturating_sub(popup_height)) / 2;

        let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

        // Clear background
        frame.render_widget(Clear, popup_area);

        // Main layout
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Tabs
                Constraint::Min(10),   // Content
                Constraint::Length(3), // Footer
            ])
            .split(popup_area);

        // Render tabs
        self.render_tabs(frame, layout[0]);

        // Render settings list
        self.render_settings(frame, layout[1]);

        // Render footer
        self.render_footer(frame, layout[2]);
    }

    fn render_tabs(&self, frame: &mut Frame, area: Rect) {
        let titles: Vec<Line> = SettingsTab::all()
            .iter()
            .map(|t| {
                let style = if *t == self.current_tab {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Gray)
                };
                Line::from(Span::styled(t.title(), style))
            })
            .collect();

        let tabs = Tabs::new(titles)
            .block(
                Block::default()
                    .title(" Settings ")
                    .title_alignment(Alignment::Center)
                    .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
                    .border_style(Style::default().fg(Color::Blue)),
            )
            .select(
                SettingsTab::all()
                    .iter()
                    .position(|t| *t == self.current_tab)
                    .unwrap_or(0),
            )
            .highlight_style(Style::default().fg(Color::Cyan));

        frame.render_widget(tabs, area);
    }

    fn render_settings(&mut self, frame: &mut Frame, area: Rect) {
        let selected_item = self.selected_item;
        let editing = self.editing;
        let edit_buffer = self.edit_buffer.clone();
        let settings = self.current_settings().to_vec();

        let items: Vec<ListItem> = settings
            .iter()
            .enumerate()
            .map(|(idx, item)| {
                let is_selected = idx == selected_item;
                let is_editing = is_selected && editing;

                let value_str = if is_editing {
                    format!("{}|", edit_buffer)
                } else {
                    item.display_value()
                };

                let modified_marker = if item.modified { "*" } else { " " };

                let label_style = if is_selected {
                    Style::default().fg(Color::Black).bg(Color::Cyan)
                } else {
                    Style::default().fg(Color::White)
                };

                let value_style = if is_editing {
                    Style::default().fg(Color::Yellow)
                } else if is_selected {
                    Style::default().fg(Color::Black).bg(Color::Cyan)
                } else {
                    Style::default().fg(Color::Green)
                };

                ListItem::new(Line::from(vec![
                    Span::styled(modified_marker, Style::default().fg(Color::Red)),
                    Span::styled(&item.label, label_style),
                    Span::raw(": "),
                    Span::styled(value_str, value_style),
                ]))
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::LEFT | Borders::RIGHT)
                    .border_style(Style::default().fg(Color::Blue)),
            )
            .highlight_style(Style::default().add_modifier(Modifier::BOLD));

        frame.render_stateful_widget(list, area, &mut self.list_state);

        // Show description for selected item
        if let Some(item) = settings.get(selected_item) {
            if !item.description.is_empty() {
                let desc_area = Rect::new(
                    area.x + 2,
                    area.y + area.height.saturating_sub(2),
                    area.width.saturating_sub(4),
                    1,
                );
                let desc = Paragraph::new(item.description.as_str())
                    .style(Style::default().fg(Color::DarkGray));
                frame.render_widget(desc, desc_area);
            }
        }
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let help_text = if self.editing {
            "Type to edit | Enter: Confirm | Esc: Cancel"
        } else if self.has_changes {
            "Tab: Switch tabs | Enter: Edit | s: Save | Esc: Close (unsaved!)"
        } else {
            "Tab: Switch tabs | Enter: Edit | Esc: Close"
        };

        let footer = Paragraph::new(help_text)
            .style(Style::default().fg(Color::DarkGray))
            .block(
                Block::default()
                    .borders(Borders::BOTTOM | Borders::LEFT | Borders::RIGHT)
                    .border_style(Style::default().fg(Color::Blue)),
            )
            .alignment(Alignment::Center);

        frame.render_widget(footer, area);
    }

    /// Handle keyboard input
    pub fn handle_key(&mut self, key: crossterm::event::KeyCode) -> SettingsAction {
        use crossterm::event::KeyCode;

        if self.editing {
            match key {
                KeyCode::Enter => {
                    self.confirm_edit();
                    SettingsAction::None
                }
                KeyCode::Esc => {
                    self.cancel_edit();
                    SettingsAction::None
                }
                KeyCode::Backspace => {
                    self.handle_edit_backspace();
                    SettingsAction::None
                }
                KeyCode::Char(c) => {
                    self.handle_edit_input(c);
                    SettingsAction::None
                }
                _ => SettingsAction::None,
            }
        } else {
            match key {
                KeyCode::Up | KeyCode::Char('k') => {
                    self.select_prev();
                    SettingsAction::None
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.select_next();
                    SettingsAction::None
                }
                KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                    self.next_tab();
                    SettingsAction::None
                }
                KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                    self.prev_tab();
                    SettingsAction::None
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    self.activate_selected();
                    SettingsAction::None
                }
                KeyCode::Char('s') => {
                    if let Err(e) = self.save() {
                        SettingsAction::Error(e)
                    } else {
                        SettingsAction::Saved
                    }
                }
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.hide();
                    SettingsAction::Closed
                }
                _ => SettingsAction::None,
            }
        }
    }

    /// Get a setting value by key
    pub fn get_setting(&self, key: &str) -> Option<&SettingValue> {
        for settings in [
            &self.general_settings,
            &self.provider_settings,
            &self.permission_settings,
            &self.theme_settings,
            &self.advanced_settings,
        ] {
            if let Some(item) = settings.iter().find(|i| i.key == key) {
                return Some(&item.value);
            }
        }
        None
    }
}

impl Default for SettingsPage {
    fn default() -> Self {
        Self::new()
    }
}

/// Actions returned by settings page
#[derive(Debug, Clone, PartialEq)]
pub enum SettingsAction {
    None,
    Saved,
    Closed,
    Error(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_page_creation() {
        let page = SettingsPage::new();
        assert!(!page.is_visible());
        assert!(!page.has_unsaved_changes());
    }

    #[test]
    fn test_tab_navigation() {
        let mut page = SettingsPage::new();
        assert_eq!(page.current_tab, SettingsTab::General);

        page.next_tab();
        assert_eq!(page.current_tab, SettingsTab::Providers);

        page.prev_tab();
        assert_eq!(page.current_tab, SettingsTab::General);
    }

    #[test]
    fn test_setting_toggle() {
        let mut page = SettingsPage::new();

        // Find and toggle a boolean setting
        page.selected_item = 1; // auto_save
        page.activate_selected();

        assert!(page.has_unsaved_changes());
    }

    #[test]
    fn test_setting_value_display() {
        let item = SettingItem::new("api_key", "API Key")
            .with_value(SettingValue::String("sk-1234567890".to_string()));

        // Should mask the key
        let display = item.display_value();
        assert!(display.contains("***"));
        assert!(!display.contains("1234567890"));
    }

    #[test]
    fn test_save_clears_changes() {
        let mut page = SettingsPage::new();
        page.selected_item = 1;
        page.activate_selected();

        assert!(page.has_unsaved_changes());

        let _ = page.save();
        assert!(!page.has_unsaved_changes());
    }
}
