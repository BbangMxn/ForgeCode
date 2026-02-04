//! Chat page - main conversation interface
//!
//! Layer3 Agent와 완전히 통합된 채팅 인터페이스입니다.
//! - Steering을 통한 실시간 제어 (pause/resume/stop)
//! - 새로운 AgentEvent 전체 처리
//! - Permission Modal 통합
//! - Context 압축 상태 표시

use crate::tui::components::{
    ChatMessage, InputBox, MessageList, MessageRole, PermissionModalManager, ToolInfo, ToolStatus,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use forge_agent::{Agent, AgentConfig, AgentContext, AgentEvent, MessageHistory, SteeringHandle};
use forge_core::ToolRegistry;
use forge_foundation::{PermissionService, ProviderConfig};
use forge_provider::Gateway;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph},
    Frame,
};
use std::sync::Arc;
use tokio::sync::mpsc;

/// Chat page state
pub struct ChatPage {
    /// Message list display
    messages: MessageList,

    /// Input box
    input: InputBox,

    /// Agent context
    ctx: Option<Arc<AgentContext>>,

    /// Message history for LLM
    history: MessageHistory,

    /// Session ID
    session_id: String,

    /// Whether agent is currently running
    running: bool,

    /// Agent is paused
    paused: bool,

    /// Steering handle for controlling the agent
    steering_handle: Option<SteeringHandle>,

    /// Status text
    status: String,

    /// Token usage (input, output)
    tokens: (u32, u32),

    /// Current turn
    current_turn: u32,

    /// Context usage (0.0 - 1.0)
    context_usage: f32,

    /// Last compression info
    last_compression: Option<(usize, usize)>, // (saved, total)

    /// Permission modal manager
    permission_modal: PermissionModalManager,

    /// Show help overlay
    show_help: bool,

    /// Provider name
    provider_name: String,

    /// Model name
    model_name: String,
}

impl ChatPage {
    /// Create a new chat page
    pub fn new() -> Self {
        Self {
            messages: MessageList::new(),
            input: InputBox::new(),
            ctx: None,
            history: MessageHistory::new(),
            session_id: uuid::Uuid::new_v4().to_string(),
            running: false,
            paused: false,
            steering_handle: None,
            status: "Ready".to_string(),
            tokens: (0, 0),
            current_turn: 0,
            context_usage: 0.0,
            last_compression: None,
            permission_modal: PermissionModalManager::new(),
            show_help: false,
            provider_name: "Unknown".to_string(),
            model_name: "Unknown".to_string(),
        }
    }

    /// Initialize with configuration
    pub fn init(&mut self, config: &ProviderConfig) -> Result<(), String> {
        // Create gateway
        let gateway =
            Gateway::from_config(config).map_err(|e| format!("Failed to initialize LLM: {}", e))?;

        // Store provider info
        self.provider_name = config.default.clone().unwrap_or_else(|| "anthropic".to_string());
        if let Some(provider_config) = config.providers.get(&self.provider_name) {
            self.model_name = provider_config.model.clone().unwrap_or_default();
        }

        // Create tools
        let tools = ToolRegistry::with_builtins();

        // Create permissions (with auto-approve for now, will integrate modal later)
        let permissions = PermissionService::with_auto_approve();

        // Get working directory
        let working_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));

        // Create context
        self.ctx = Some(Arc::new(AgentContext::new(
            Arc::new(gateway),
            Arc::new(tools),
            Arc::new(permissions),
            working_dir,
        )));

        self.status = "Connected".to_string();

        Ok(())
    }

    /// Handle key event
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<ChatAction> {
        // Handle permission modal first if visible
        if self.permission_modal.has_modal() {
            self.permission_modal.handle_key(key.code);
            return None;
        }

        // Handle help overlay
        if self.show_help {
            self.show_help = false;
            return None;
        }

        // Global shortcuts
        match (key.modifiers, key.code) {
            // Ctrl+P: Pause/Resume
            (KeyModifiers::CONTROL, KeyCode::Char('p')) => {
                if self.running {
                    return Some(ChatAction::TogglePause);
                }
            }
            // Ctrl+X: Stop agent
            (KeyModifiers::CONTROL, KeyCode::Char('x')) => {
                if self.running {
                    return Some(ChatAction::StopAgent);
                }
            }
            // Ctrl+H or F1: Help
            (KeyModifiers::CONTROL, KeyCode::Char('h')) | (_, KeyCode::F(1)) => {
                self.show_help = true;
                return None;
            }
            // Ctrl+L: Clear messages
            (KeyModifiers::CONTROL, KeyCode::Char('l')) => {
                if !self.running {
                    self.messages.clear();
                    self.history.clear();
                    self.tokens = (0, 0);
                    self.current_turn = 0;
                    self.context_usage = 0.0;
                    return None;
                }
            }
            // Escape: Cancel current operation
            (_, KeyCode::Esc) => {
                if self.running {
                    return Some(ChatAction::StopAgent);
                }
            }
            // Page Up/Down for scrolling
            (_, KeyCode::PageUp) => {
                for _ in 0..5 {
                    self.messages.scroll_up();
                }
                return None;
            }
            (_, KeyCode::PageDown) => {
                for _ in 0..5 {
                    self.messages.scroll_down();
                }
                return None;
            }
            _ => {}
        }

        // Don't process input while agent is running
        if self.running {
            return None;
        }

        // Handle input
        if self.input.handle_key(key) {
            let content = self.input.take();
            if !content.is_empty() {
                // Check for slash commands
                if content.starts_with('/') {
                    return Some(ChatAction::SlashCommand(content));
                }
                return Some(ChatAction::SendMessage(content));
            }
        }

        None
    }

    /// Send a message to the agent
    pub async fn send_message(&mut self, content: String) -> mpsc::Receiver<AgentEvent> {
        self.running = true;
        self.paused = false;
        self.status = "Thinking...".to_string();
        self.current_turn = 0;

        // Add user message to display
        self.messages.push(ChatMessage {
            role: MessageRole::User,
            content: content.clone(),
            tool_info: None,
        });

        // Create channel for events
        let (tx, rx) = mpsc::channel(100);

        // Clone what we need for the async task
        let ctx = self.ctx.clone();
        let session_id = self.session_id.clone();
        let mut history = self.history.clone();

        // Spawn agent task and get steering handle
        let (steering_tx, mut steering_rx) = mpsc::channel::<SteeringHandle>(1);

        tokio::spawn(async move {
            if let Some(ctx) = ctx {
                let agent = Agent::with_config(ctx, AgentConfig::default());

                // Send steering handle back
                let _ = steering_tx.send(agent.steering_handle()).await;

                let _ = agent.run(&session_id, &mut history, &content, tx).await;
            }
        });

        // Get steering handle
        if let Some(handle) = steering_rx.recv().await {
            self.steering_handle = Some(handle);
        }

        rx
    }

    /// Toggle pause state
    pub async fn toggle_pause(&mut self) {
        if let Some(ref handle) = self.steering_handle {
            if self.paused {
                let _ = handle.resume().await;
                self.paused = false;
                self.status = "Resumed".to_string();
            } else {
                let _ = handle.pause().await;
                self.paused = true;
                self.status = "Paused (Ctrl+P to resume)".to_string();
            }
        }
    }

    /// Stop the agent
    pub async fn stop_agent(&mut self) {
        if let Some(ref handle) = self.steering_handle {
            let _ = handle.stop("User requested stop").await;
            self.running = false;
            self.paused = false;
            self.status = "Stopped".to_string();
        }
    }

    /// Handle slash command
    pub fn handle_slash_command(&mut self, command: &str) -> Option<ChatAction> {
        let parts: Vec<&str> = command.split_whitespace().collect();
        let cmd = parts.first().map(|s| &s[1..]); // Remove leading /

        match cmd {
            Some("help") | Some("h") => {
                self.show_help = true;
                None
            }
            Some("clear") => {
                self.messages.clear();
                self.history.clear();
                self.tokens = (0, 0);
                self.current_turn = 0;
                None
            }
            Some("new") => {
                self.messages.clear();
                self.history.clear();
                self.tokens = (0, 0);
                self.current_turn = 0;
                self.session_id = uuid::Uuid::new_v4().to_string();
                self.messages.push(ChatMessage {
                    role: MessageRole::System,
                    content: "Started new session".to_string(),
                    tool_info: None,
                });
                None
            }
            Some("model") => {
                // TODO: Open model switcher
                self.messages.push(ChatMessage {
                    role: MessageRole::System,
                    content: format!(
                        "Current model: {} ({})",
                        self.model_name, self.provider_name
                    ),
                    tool_info: None,
                });
                None
            }
            Some("tokens") => {
                self.messages.push(ChatMessage {
                    role: MessageRole::System,
                    content: format!(
                        "Token usage: {} input, {} output, {} total",
                        self.tokens.0,
                        self.tokens.1,
                        self.tokens.0 + self.tokens.1
                    ),
                    tool_info: None,
                });
                None
            }
            _ => {
                self.messages.push(ChatMessage {
                    role: MessageRole::System,
                    content: format!("Unknown command: {}", command),
                    tool_info: None,
                });
                None
            }
        }
    }

    /// Handle agent event
    pub fn handle_agent_event(&mut self, event: AgentEvent) {
        match event {
            AgentEvent::Thinking => {
                self.status = format!("Turn {} - Thinking...", self.current_turn + 1);
            }
            AgentEvent::Text(text) => {
                // Append to last assistant message or create new one
                if let Some(last) = self.messages.messages.last_mut() {
                    if last.role == MessageRole::Assistant && last.tool_info.is_none() {
                        last.content.push_str(&text);
                        return;
                    }
                }
                self.messages.push(ChatMessage {
                    role: MessageRole::Assistant,
                    content: text,
                    tool_info: None,
                });
            }
            AgentEvent::ToolStart { tool_name, .. } => {
                self.status = format!("Running {}...", tool_name);
                self.messages.push(ChatMessage {
                    role: MessageRole::Tool,
                    content: "".to_string(),
                    tool_info: Some(ToolInfo {
                        name: tool_name,
                        status: ToolStatus::Running,
                    }),
                });
            }
            AgentEvent::ToolComplete {
                result,
                success,
                duration_ms,
                ..
            } => {
                // Update last tool message
                if let Some(last) = self.messages.messages.last_mut() {
                    if last.role == MessageRole::Tool {
                        last.content = format!("{} ({}ms)", truncate(&result, 150), duration_ms);
                        if let Some(ref mut info) = last.tool_info {
                            info.status = if success {
                                ToolStatus::Success
                            } else {
                                ToolStatus::Error
                            };
                        }
                    }
                }
            }
            AgentEvent::TurnStart { turn } => {
                self.current_turn = turn;
                self.status = format!("Turn {}", turn);
            }
            AgentEvent::TurnComplete { turn } => {
                self.status = format!("Turn {} complete", turn);
            }
            AgentEvent::Compressed {
                tokens_before,
                tokens_after,
                tokens_saved,
            } => {
                self.last_compression = Some((tokens_saved, tokens_before));
                self.context_usage = tokens_after as f32 / 200_000.0; // Approximate
                self.messages.push(ChatMessage {
                    role: MessageRole::System,
                    content: format!(
                        "Context compressed: {} → {} tokens (saved {})",
                        tokens_before, tokens_after, tokens_saved
                    ),
                    tool_info: None,
                });
            }
            AgentEvent::Paused => {
                self.paused = true;
                self.status = "Paused (Ctrl+P to resume)".to_string();
            }
            AgentEvent::Resumed => {
                self.paused = false;
                self.status = "Resumed".to_string();
            }
            AgentEvent::Stopped { reason } => {
                self.running = false;
                self.paused = false;
                self.status = format!("Stopped: {}", reason);
            }
            AgentEvent::Done { .. } => {
                self.running = false;
                self.paused = false;
                self.steering_handle = None;
                self.status = "Ready".to_string();
            }
            AgentEvent::Error(e) => {
                self.running = false;
                self.paused = false;
                self.steering_handle = None;
                self.status = format!("Error: {}", truncate(&e, 50));
                self.messages.push(ChatMessage {
                    role: MessageRole::System,
                    content: format!("Error: {}", e),
                    tool_info: None,
                });
            }
            AgentEvent::Usage {
                input_tokens,
                output_tokens,
            } => {
                self.tokens.0 += input_tokens;
                self.tokens.1 += output_tokens;
                // Update context usage estimate
                self.context_usage = (self.tokens.0 as f32) / 200_000.0;
            }
        }
    }

    /// Render the chat page
    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(5),    // Messages
                Constraint::Length(3), // Input
                Constraint::Length(1), // Status bar
            ])
            .split(area);

        // Render messages
        self.messages.render(frame, chunks[0]);

        // Render input (with pause indicator)
        self.render_input(frame, chunks[1]);

        // Render status bar
        self.render_status_bar(frame, chunks[2]);

        // Render permission modal if visible
        self.permission_modal.render(frame, area);

        // Render help overlay if visible
        if self.show_help {
            self.render_help(frame, area);
        }
    }

    fn render_input(&self, frame: &mut Frame, area: Rect) {
        let border_color = if self.running {
            if self.paused {
                Color::Yellow
            } else {
                Color::DarkGray
            }
        } else {
            Color::Cyan
        };

        let title = if self.running {
            if self.paused {
                " PAUSED - Ctrl+P to resume "
            } else {
                " Agent running... "
            }
        } else {
            " Input (Enter to send) "
        };

        let input = Paragraph::new(self.input.content())
            .style(Style::default().fg(Color::White))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(border_color))
                    .title(title),
            );

        frame.render_widget(input, area);

        // Show cursor if not running
        if !self.running {
            frame.set_cursor_position((area.x + self.input.content().len() as u16 + 1, area.y + 1));
        }
    }

    fn render_status_bar(&self, frame: &mut Frame, area: Rect) {
        // Split status bar into sections
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(40), // Status
                Constraint::Percentage(25), // Tokens
                Constraint::Percentage(20), // Context gauge
                Constraint::Percentage(15), // Session
            ])
            .split(area);

        // Status text with color based on state
        let status_color = if self.running {
            if self.paused {
                Color::Yellow
            } else {
                Color::Green
            }
        } else {
            Color::White
        };

        let status = Paragraph::new(format!(" {}", self.status))
            .style(Style::default().bg(Color::DarkGray).fg(status_color));
        frame.render_widget(status, chunks[0]);

        // Token count
        let tokens = Paragraph::new(format!("{}↓ {}↑", self.tokens.0, self.tokens.1))
            .style(Style::default().bg(Color::DarkGray).fg(Color::Cyan));
        frame.render_widget(tokens, chunks[1]);

        // Context usage gauge
        let context_percent = (self.context_usage * 100.0).min(100.0) as u16;
        let gauge_color = if context_percent > 90 {
            Color::Red
        } else if context_percent > 70 {
            Color::Yellow
        } else {
            Color::Green
        };

        let gauge = Gauge::default()
            .gauge_style(Style::default().fg(gauge_color).bg(Color::DarkGray))
            .percent(context_percent)
            .label(format!("{}%", context_percent));
        frame.render_widget(gauge, chunks[2]);

        // Session ID
        let session = Paragraph::new(format!(" {}", &self.session_id[..8]))
            .style(Style::default().bg(Color::DarkGray).fg(Color::Gray));
        frame.render_widget(session, chunks[3]);
    }

    fn render_help(&self, frame: &mut Frame, area: Rect) {
        // Center the help box
        let width = 50.min(area.width - 4);
        let height = 16.min(area.height - 4);
        let x = (area.width - width) / 2;
        let y = (area.height - height) / 2;
        let help_area = Rect::new(x, y, width, height);

        // Clear background
        frame.render_widget(ratatui::widgets::Clear, help_area);

        let help_text = vec![
            Line::from(vec![Span::styled(
                "Keyboard Shortcuts",
                Style::default().add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
            Line::from(vec![
                Span::styled("Ctrl+C  ", Style::default().fg(Color::Cyan)),
                Span::raw("Quit"),
            ]),
            Line::from(vec![
                Span::styled("Ctrl+P  ", Style::default().fg(Color::Cyan)),
                Span::raw("Pause/Resume agent"),
            ]),
            Line::from(vec![
                Span::styled("Ctrl+X  ", Style::default().fg(Color::Cyan)),
                Span::raw("Stop agent"),
            ]),
            Line::from(vec![
                Span::styled("Ctrl+L  ", Style::default().fg(Color::Cyan)),
                Span::raw("Clear messages"),
            ]),
            Line::from(vec![
                Span::styled("Esc     ", Style::default().fg(Color::Cyan)),
                Span::raw("Cancel/Close"),
            ]),
            Line::from(vec![
                Span::styled("PgUp/Dn ", Style::default().fg(Color::Cyan)),
                Span::raw("Scroll messages"),
            ]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Slash Commands",
                Style::default().add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
            Line::from(vec![
                Span::styled("/help   ", Style::default().fg(Color::Yellow)),
                Span::raw("Show this help"),
            ]),
            Line::from(vec![
                Span::styled("/clear  ", Style::default().fg(Color::Yellow)),
                Span::raw("Clear conversation"),
            ]),
            Line::from(vec![
                Span::styled("/new    ", Style::default().fg(Color::Yellow)),
                Span::raw("Start new session"),
            ]),
            Line::from(vec![
                Span::styled("/model  ", Style::default().fg(Color::Yellow)),
                Span::raw("Show current model"),
            ]),
        ];

        let help = Paragraph::new(help_text)
            .block(
                Block::default()
                    .title(" Help (press any key to close) ")
                    .title_style(Style::default().fg(Color::White))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Blue))
                    .style(Style::default().bg(Color::Black)),
            )
            .alignment(ratatui::layout::Alignment::Left);

        frame.render_widget(help, help_area);
    }
}

impl Default for ChatPage {
    fn default() -> Self {
        Self::new()
    }
}

/// Actions that can be triggered from the chat page
pub enum ChatAction {
    SendMessage(String),
    SlashCommand(String),
    TogglePause,
    StopAgent,
}

/// Truncate text for display
fn truncate(s: &str, max_len: usize) -> String {
    let s = s.replace('\n', " ");
    if s.len() <= max_len {
        s
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}
