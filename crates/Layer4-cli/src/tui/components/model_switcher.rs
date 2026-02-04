//! Model Switcher Component
//!
//! A TUI component for switching between AI models and providers.
//!
//! Features:
//! - Display available models grouped by provider
//! - Show model capabilities and context limits
//! - Quick keyboard shortcuts for common models
//! - Visual indicator of current model

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

/// Model information
#[derive(Debug, Clone)]
pub struct ModelInfo {
    /// Model identifier (e.g., "claude-3-opus-20240229")
    pub id: String,
    /// Display name (e.g., "Claude 3 Opus")
    pub display_name: String,
    /// Provider name (e.g., "Anthropic")
    pub provider: String,
    /// Context window size
    pub context_window: usize,
    /// Maximum output tokens
    pub max_output_tokens: usize,
    /// Whether this model supports vision
    pub supports_vision: bool,
    /// Whether this model supports tools
    pub supports_tools: bool,
    /// Cost tier (1=cheap, 3=expensive)
    pub cost_tier: u8,
    /// Speed tier (1=slow, 3=fast)
    pub speed_tier: u8,
    /// Short description
    pub description: String,
}

impl ModelInfo {
    pub fn new(
        id: impl Into<String>,
        display_name: impl Into<String>,
        provider: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            display_name: display_name.into(),
            provider: provider.into(),
            context_window: 100_000,
            max_output_tokens: 4_096,
            supports_vision: false,
            supports_tools: true,
            cost_tier: 2,
            speed_tier: 2,
            description: String::new(),
        }
    }

    pub fn with_context(mut self, context_window: usize, max_output: usize) -> Self {
        self.context_window = context_window;
        self.max_output_tokens = max_output;
        self
    }

    pub fn with_vision(mut self) -> Self {
        self.supports_vision = true;
        self
    }

    pub fn with_cost_speed(mut self, cost: u8, speed: u8) -> Self {
        self.cost_tier = cost;
        self.speed_tier = speed;
        self
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Format context window for display
    pub fn format_context(&self) -> String {
        if self.context_window >= 1_000_000 {
            format!("{}M", self.context_window / 1_000_000)
        } else {
            format!("{}K", self.context_window / 1_000)
        }
    }

    /// Get capability icons
    pub fn capability_icons(&self) -> String {
        let mut icons = String::new();
        if self.supports_vision {
            icons.push_str(" [V]");
        }
        if self.supports_tools {
            icons.push_str(" [T]");
        }
        icons
    }

    /// Get cost indicator
    pub fn cost_indicator(&self) -> &str {
        match self.cost_tier {
            1 => "$",
            2 => "$$",
            3 => "$$$",
            _ => "?",
        }
    }

    /// Get speed indicator
    pub fn speed_indicator(&self) -> &str {
        match self.speed_tier {
            1 => "Slow",
            2 => "Medium",
            3 => "Fast",
            _ => "?",
        }
    }
}

/// Provider with models
#[derive(Debug, Clone)]
pub struct ProviderGroup {
    pub name: String,
    pub models: Vec<ModelInfo>,
    pub is_expanded: bool,
}

/// Model switcher component
pub struct ModelSwitcher {
    /// Available models grouped by provider
    providers: Vec<ProviderGroup>,
    /// Currently selected provider index
    selected_provider: usize,
    /// Currently selected model index within provider
    selected_model: usize,
    /// Current active model ID
    current_model: String,
    /// Whether the switcher is visible
    visible: bool,
    /// List state for rendering
    list_state: ListState,
    /// Show detailed view
    show_details: bool,
}

impl ModelSwitcher {
    pub fn new() -> Self {
        let mut switcher = Self {
            providers: Self::default_providers(),
            selected_provider: 0,
            selected_model: 0,
            current_model: "claude-sonnet-4-20250514".to_string(),
            visible: false,
            list_state: ListState::default(),
            show_details: false,
        };
        switcher.list_state.select(Some(0));
        switcher
    }

    /// Get default provider configurations
    fn default_providers() -> Vec<ProviderGroup> {
        vec![
            ProviderGroup {
                name: "Anthropic".to_string(),
                models: vec![
                    ModelInfo::new("claude-opus-4-20250514", "Claude Opus 4", "Anthropic")
                        .with_context(200_000, 32_000)
                        .with_vision()
                        .with_cost_speed(3, 1)
                        .with_description("Most capable model for complex tasks"),
                    ModelInfo::new("claude-sonnet-4-20250514", "Claude Sonnet 4", "Anthropic")
                        .with_context(200_000, 16_000)
                        .with_vision()
                        .with_cost_speed(2, 2)
                        .with_description("Balanced performance and cost"),
                    ModelInfo::new("claude-3-5-haiku-20241022", "Claude 3.5 Haiku", "Anthropic")
                        .with_context(200_000, 8_192)
                        .with_vision()
                        .with_cost_speed(1, 3)
                        .with_description("Fast and cost-effective"),
                ],
                is_expanded: true,
            },
            ProviderGroup {
                name: "OpenAI".to_string(),
                models: vec![
                    ModelInfo::new("gpt-4o", "GPT-4o", "OpenAI")
                        .with_context(128_000, 4_096)
                        .with_vision()
                        .with_cost_speed(3, 2)
                        .with_description("Most capable GPT model"),
                    ModelInfo::new("gpt-4o-mini", "GPT-4o Mini", "OpenAI")
                        .with_context(128_000, 4_096)
                        .with_vision()
                        .with_cost_speed(1, 3)
                        .with_description("Fast and affordable"),
                    ModelInfo::new("o1", "o1", "OpenAI")
                        .with_context(200_000, 100_000)
                        .with_cost_speed(3, 1)
                        .with_description("Advanced reasoning model"),
                ],
                is_expanded: false,
            },
            ProviderGroup {
                name: "Google".to_string(),
                models: vec![
                    ModelInfo::new("gemini-2.0-flash", "Gemini 2.0 Flash", "Google")
                        .with_context(1_000_000, 8_192)
                        .with_vision()
                        .with_cost_speed(1, 3)
                        .with_description("Ultra-long context with fast speed"),
                    ModelInfo::new("gemini-2.0-pro", "Gemini 2.0 Pro", "Google")
                        .with_context(1_000_000, 8_192)
                        .with_vision()
                        .with_cost_speed(2, 2)
                        .with_description("Balanced performance"),
                ],
                is_expanded: false,
            },
        ]
    }

    /// Show the model switcher
    pub fn show(&mut self) {
        self.visible = true;
    }

    /// Hide the model switcher
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Toggle visibility
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    /// Check if visible
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Set current model
    pub fn set_current_model(&mut self, model_id: &str) {
        self.current_model = model_id.to_string();
    }

    /// Get current model ID
    pub fn current_model(&self) -> &str {
        &self.current_model
    }

    /// Get selected model info
    pub fn selected_model_info(&self) -> Option<&ModelInfo> {
        let provider = self.providers.get(self.selected_provider)?;
        provider.models.get(self.selected_model)
    }

    /// Move selection up
    pub fn select_previous(&mut self) {
        if self.selected_model > 0 {
            self.selected_model -= 1;
        } else if self.selected_provider > 0 {
            self.selected_provider -= 1;
            if let Some(provider) = self.providers.get(self.selected_provider) {
                self.selected_model = provider.models.len().saturating_sub(1);
            }
        }
        self.update_list_state();
    }

    /// Move selection down
    pub fn select_next(&mut self) {
        if let Some(provider) = self.providers.get(self.selected_provider) {
            if self.selected_model < provider.models.len().saturating_sub(1) {
                self.selected_model += 1;
            } else if self.selected_provider < self.providers.len().saturating_sub(1) {
                self.selected_provider += 1;
                self.selected_model = 0;
            }
        }
        self.update_list_state();
    }

    /// Toggle provider expansion
    pub fn toggle_provider(&mut self) {
        if let Some(provider) = self.providers.get_mut(self.selected_provider) {
            provider.is_expanded = !provider.is_expanded;
        }
    }

    /// Select current model and hide switcher
    pub fn confirm_selection(&mut self) -> Option<String> {
        if let Some(model) = self.selected_model_info() {
            let model_id = model.id.clone();
            self.current_model = model_id.clone();
            self.hide();
            return Some(model_id);
        }
        None
    }

    /// Toggle detail view
    pub fn toggle_details(&mut self) {
        self.show_details = !self.show_details;
    }

    /// Update list state for rendering
    fn update_list_state(&mut self) {
        let mut index = 0;
        for (i, provider) in self.providers.iter().enumerate() {
            if i == self.selected_provider {
                index += self.selected_model + 1; // +1 for provider header
                break;
            }
            index += 1; // provider header
            if provider.is_expanded {
                index += provider.models.len();
            }
        }
        self.list_state.select(Some(index));
    }

    /// Render the model switcher
    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        if !self.visible {
            return;
        }

        // Calculate centered popup area
        let popup_width = 60.min(area.width.saturating_sub(4));
        let popup_height = 20.min(area.height.saturating_sub(4));
        let popup_x = (area.width.saturating_sub(popup_width)) / 2;
        let popup_y = (area.height.saturating_sub(popup_height)) / 2;

        let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

        // Clear the area behind the popup
        frame.render_widget(Clear, popup_area);

        // Main layout
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Header
                Constraint::Min(5),    // Model list
                Constraint::Length(3), // Footer/help
            ])
            .split(popup_area);

        // Render header
        self.render_header(frame, layout[0]);

        // Render model list or detail view
        if self.show_details {
            self.render_details(frame, layout[1]);
        } else {
            self.render_model_list(frame, layout[1]);
        }

        // Render footer
        self.render_footer(frame, layout[2]);
    }

    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let current = self
            .providers
            .iter()
            .flat_map(|p| &p.models)
            .find(|m| m.id == self.current_model)
            .map(|m| m.display_name.as_str())
            .unwrap_or("Unknown");

        let header = Paragraph::new(vec![Line::from(vec![
            Span::raw("Current: "),
            Span::styled(
                current,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ])])
        .block(
            Block::default()
                .title(" Model Switcher ")
                .title_alignment(Alignment::Center)
                .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
                .border_style(Style::default().fg(Color::Blue)),
        )
        .alignment(Alignment::Center);

        frame.render_widget(header, area);
    }

    fn render_model_list(&mut self, frame: &mut Frame, area: Rect) {
        let mut items: Vec<ListItem> = Vec::new();

        for (provider_idx, provider) in self.providers.iter().enumerate() {
            // Provider header
            let is_current_provider = provider_idx == self.selected_provider;
            let expand_icon = if provider.is_expanded { "▼" } else { "▶" };
            let provider_style = if is_current_provider && self.selected_model == 0 {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            };

            items.push(ListItem::new(Line::from(vec![
                Span::raw(expand_icon),
                Span::raw(" "),
                Span::styled(&provider.name, provider_style),
                Span::styled(
                    format!(" ({} models)", provider.models.len()),
                    Style::default().fg(Color::DarkGray),
                ),
            ])));

            // Models (if expanded)
            if provider.is_expanded {
                for (model_idx, model) in provider.models.iter().enumerate() {
                    let is_selected =
                        provider_idx == self.selected_provider && model_idx == self.selected_model;
                    let is_current = model.id == self.current_model;

                    let prefix = if is_current { "● " } else { "  " };
                    let model_style = if is_selected {
                        Style::default().fg(Color::Black).bg(Color::Cyan)
                    } else if is_current {
                        Style::default().fg(Color::Green)
                    } else {
                        Style::default()
                    };

                    items.push(ListItem::new(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(prefix, Style::default().fg(Color::Green)),
                        Span::styled(&model.display_name, model_style),
                        Span::styled(
                            format!(" [{}]", model.format_context()),
                            Style::default().fg(Color::DarkGray),
                        ),
                        Span::styled(
                            format!(" {}", model.cost_indicator()),
                            Style::default().fg(Color::Yellow),
                        ),
                    ])));
                }
            }
        }

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::LEFT | Borders::RIGHT)
                    .border_style(Style::default().fg(Color::Blue)),
            )
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

        frame.render_stateful_widget(list, area, &mut self.list_state);
    }

    fn render_details(&self, frame: &mut Frame, area: Rect) {
        let content = if let Some(model) = self.selected_model_info() {
            vec![
                Line::from(vec![
                    Span::styled("Model: ", Style::default().fg(Color::Gray)),
                    Span::styled(
                        &model.display_name,
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("ID: ", Style::default().fg(Color::Gray)),
                    Span::raw(&model.id),
                ]),
                Line::from(vec![
                    Span::styled("Provider: ", Style::default().fg(Color::Gray)),
                    Span::raw(&model.provider),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Context: ", Style::default().fg(Color::Gray)),
                    Span::raw(format!("{} tokens", model.context_window)),
                ]),
                Line::from(vec![
                    Span::styled("Max Output: ", Style::default().fg(Color::Gray)),
                    Span::raw(format!("{} tokens", model.max_output_tokens)),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Capabilities: ", Style::default().fg(Color::Gray)),
                    Span::raw(if model.supports_vision { "Vision " } else { "" }),
                    Span::raw(if model.supports_tools { "Tools" } else { "" }),
                ]),
                Line::from(vec![
                    Span::styled("Cost: ", Style::default().fg(Color::Gray)),
                    Span::styled(model.cost_indicator(), Style::default().fg(Color::Yellow)),
                    Span::raw("  "),
                    Span::styled("Speed: ", Style::default().fg(Color::Gray)),
                    Span::raw(model.speed_indicator()),
                ]),
                Line::from(""),
                Line::from(vec![Span::styled(
                    &model.description,
                    Style::default().fg(Color::DarkGray),
                )]),
            ]
        } else {
            vec![Line::from("No model selected")]
        };

        let details = Paragraph::new(content)
            .block(
                Block::default()
                    .title(" Model Details ")
                    .borders(Borders::LEFT | Borders::RIGHT)
                    .border_style(Style::default().fg(Color::Blue)),
            )
            .wrap(Wrap { trim: true });

        frame.render_widget(details, area);
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let help_text = if self.show_details {
            "↑↓: Navigate  Enter: Select  d: Back  Esc: Close"
        } else {
            "↑↓: Navigate  Enter: Select  d: Details  Esc: Close"
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
    pub fn handle_key(&mut self, key: crossterm::event::KeyCode) -> ModelSwitcherAction {
        use crossterm::event::KeyCode;

        match key {
            KeyCode::Up | KeyCode::Char('k') => {
                self.select_previous();
                ModelSwitcherAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.select_next();
                ModelSwitcherAction::None
            }
            KeyCode::Enter => {
                if let Some(model_id) = self.confirm_selection() {
                    ModelSwitcherAction::ModelSelected(model_id)
                } else {
                    ModelSwitcherAction::None
                }
            }
            KeyCode::Char('d') => {
                self.toggle_details();
                ModelSwitcherAction::None
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                self.hide();
                ModelSwitcherAction::Closed
            }
            KeyCode::Tab => {
                self.toggle_provider();
                ModelSwitcherAction::None
            }
            _ => ModelSwitcherAction::None,
        }
    }
}

impl Default for ModelSwitcher {
    fn default() -> Self {
        Self::new()
    }
}

/// Actions returned by model switcher
#[derive(Debug, Clone, PartialEq)]
pub enum ModelSwitcherAction {
    None,
    ModelSelected(String),
    Closed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_info() {
        let model = ModelInfo::new("test-model", "Test Model", "TestProvider")
            .with_context(100_000, 4_096)
            .with_vision()
            .with_cost_speed(2, 3);

        assert_eq!(model.format_context(), "100K");
        assert!(model.supports_vision);
        assert_eq!(model.cost_tier, 2);
        assert_eq!(model.speed_tier, 3);
    }

    #[test]
    fn test_model_switcher_navigation() {
        let mut switcher = ModelSwitcher::new();

        // Initial state
        assert_eq!(switcher.selected_provider, 0);
        assert_eq!(switcher.selected_model, 0);

        // Navigate down
        switcher.select_next();
        assert_eq!(switcher.selected_model, 1);

        // Navigate back up
        switcher.select_previous();
        assert_eq!(switcher.selected_model, 0);
    }

    #[test]
    fn test_model_selection() {
        let mut switcher = ModelSwitcher::new();
        switcher.show();

        // Select first model
        let result = switcher.confirm_selection();
        assert!(result.is_some());
        assert!(!switcher.is_visible());
    }

    #[test]
    fn test_large_context_format() {
        let model = ModelInfo::new("test", "Test", "Provider").with_context(1_000_000, 8_192);

        assert_eq!(model.format_context(), "1M");
    }
}
