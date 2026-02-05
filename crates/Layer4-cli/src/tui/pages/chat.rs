//! Chat page - main conversation interface
//!
//! Layer3 Agent와 완전히 통합된 채팅 인터페이스입니다.
//! - Steering을 통한 실시간 제어 (pause/resume/stop)
//! - 새로운 AgentEvent 전체 처리
//! - Permission Modal 통합
//! - Context 압축 상태 표시
//! - Claude Code 스타일 UI

use crate::tui::components::{ModelSwitcher, ModelSwitcherAction, PermissionModalManager};
use crate::tui::widgets::{
    AgentStatus, ChatMessage, ChatView, ChatViewState, Header, HeaderState, InputArea, InputState,
    MessageRole, SpinnerState, StatusBar, StatusBarState, ToolBlock, ToolExecutionState,
    WelcomeScreen,
};
use crate::tui::{current_theme, HelpOverlay, Theme};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use forge_agent::{Agent, AgentContext, AgentEvent, MessageHistory, SteeringHandle};
use forge_core::ToolRegistry;
use forge_foundation::{PermissionService, ProviderConfig};
use forge_provider::Gateway;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    Frame,
};
use std::sync::Arc;
use tokio::sync::mpsc;

/// Chat page state - Claude Code 스타일 UI
pub struct ChatPage {
    // === 새 위젯 상태 ===
    /// 헤더 상태
    header: HeaderState,
    /// 채팅 뷰 상태
    chat: ChatViewState,
    /// 입력 상태
    input: InputState,
    /// 상태 바 상태
    status_bar: StatusBarState,
    /// 스피너 상태
    spinner: SpinnerState,
    /// 테마
    theme: Theme,

    // === Agent 상태 ===
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

    // === UI 상태 ===
    /// Permission modal manager
    permission_modal: PermissionModalManager,
    /// Show help overlay
    show_help: bool,
    /// Model switcher component
    model_switcher: ModelSwitcher,
}

impl ChatPage {
    /// Create a new chat page
    pub fn new() -> Self {
        let mut header = HeaderState::new();
        header.session_id = uuid::Uuid::new_v4().to_string()[..8].to_string();

        Self {
            header,
            chat: ChatViewState::new(),
            input: InputState::new(),
            status_bar: StatusBarState::new(),
            spinner: SpinnerState::new(),
            theme: current_theme(),
            ctx: None,
            history: MessageHistory::new(),
            session_id: uuid::Uuid::new_v4().to_string(),
            running: false,
            paused: false,
            steering_handle: None,
            permission_modal: PermissionModalManager::new(),
            show_help: false,
            model_switcher: ModelSwitcher::new(),
        }
    }

    /// Initialize with configuration
    pub fn init(&mut self, config: &ProviderConfig) -> Result<(), String> {
        // Create gateway
        let gateway =
            Gateway::from_config(config).map_err(|e| format!("Failed to initialize LLM: {}", e))?;

        // Store provider info
        let provider_name = config.default.clone().unwrap_or_else(|| "anthropic".to_string());
        self.header.provider = provider_name.clone();

        if let Some(provider_config) = config.providers.get(&provider_name) {
            self.header.model = provider_config.model.clone().unwrap_or_default();
        }

        // Create tools
        let tools = ToolRegistry::with_builtins();

        // Create permissions (with auto-approve for now, will integrate modal later)
        let permissions = PermissionService::with_auto_approve();

        // Get working directory
        let working_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        self.header.cwd = working_dir.to_string_lossy().to_string();

        // Create context
        self.ctx = Some(Arc::new(AgentContext::new(
            Arc::new(gateway),
            Arc::new(tools),
            Arc::new(permissions),
            working_dir,
        )));

        self.header.agent_status = AgentStatus::Ready;
        self.status_bar.info("Connected");

        Ok(())
    }

    /// Handle key event
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<ChatAction> {
        // Handle permission modal first if visible
        if self.permission_modal.has_modal() {
            self.permission_modal.handle_key(key.code);
            return None;
        }

        // Handle model switcher if visible
        if self.model_switcher.is_visible() {
            match self.model_switcher.handle_key(key.code) {
                ModelSwitcherAction::None => {}
                ModelSwitcherAction::ModelSelected(model_id) => {
                    self.header.model = model_id.clone();
                    self.chat.push(ChatMessage::system(format!(
                        "Model changed to: {}",
                        model_id
                    )));
                }
                ModelSwitcherAction::Closed => {}
            }
            return None;
        }

        // Handle help overlay
        if self.show_help {
            self.show_help = false;
            return None;
        }

        // Global shortcuts
        match (key.modifiers, key.code) {
            // Ctrl+C: Quit
            (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                return None; // Let app.rs handle quit
            }
            // Ctrl+P: Pause/Resume
            (KeyModifiers::CONTROL, KeyCode::Char('p')) => {
                if self.running {
                    return Some(ChatAction::TogglePause);
                }
            }
            // Ctrl+X: Stop
            (KeyModifiers::CONTROL, KeyCode::Char('x')) | (_, KeyCode::Esc) => {
                if self.running {
                    return Some(ChatAction::StopAgent);
                }
            }
            // Ctrl+M: Model switcher
            (KeyModifiers::CONTROL, KeyCode::Char('m')) => {
                if !self.running {
                    self.model_switcher.show();
                }
                return None;
            }
            // Ctrl+L: Clear
            (KeyModifiers::CONTROL, KeyCode::Char('l')) => {
                if !self.running {
                    self.chat.clear();
                    self.header.tokens = (0, 0);
                    self.header.context_usage = 0.0;
                    self.header.current_turn = 0;
                    self.status_bar.info("Cleared");
                }
                return None;
            }
            // Page Up/Down: Scroll
            (_, KeyCode::PageUp) => {
                self.chat.scroll_up(10);
                return None;
            }
            (_, KeyCode::PageDown) => {
                self.chat.scroll_down(10);
                return None;
            }
            // ?: Help
            (_, KeyCode::Char('?')) if !self.running => {
                self.show_help = true;
                return None;
            }
            _ => {}
        }

        // If agent is running, ignore input keys
        if self.running {
            return None;
        }

        // Handle input
        match key.code {
            KeyCode::Enter => {
                let content = self.input.take(); // take() already adds to history
                if !content.is_empty() {
                    // Check slash command
                    if content.starts_with('/') {
                        return Some(ChatAction::SlashCommand(content));
                    }
                    return Some(ChatAction::SendMessage(content));
                }
            }
            KeyCode::Backspace => {
                self.input.backspace();
            }
            KeyCode::Delete => {
                self.input.delete();
            }
            KeyCode::Left => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    self.input.move_word_left();
                } else {
                    self.input.move_left();
                }
            }
            KeyCode::Right => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    self.input.move_word_right();
                } else {
                    self.input.move_right();
                }
            }
            KeyCode::Home => {
                self.input.move_home();
            }
            KeyCode::End => {
                self.input.move_end();
            }
            KeyCode::Up => {
                self.input.history_up();
            }
            KeyCode::Down => {
                self.input.history_down();
            }
            KeyCode::Char(c) => {
                self.input.insert(c);
            }
            _ => {}
        }

        None
    }

    /// Handle slash commands
    pub fn handle_slash_command(&mut self, cmd: &str) {
        let parts: Vec<&str> = cmd.trim().split_whitespace().collect();
        let command = parts.first().map(|s| s.to_lowercase()).unwrap_or_default();

        match command.as_str() {
            "/help" | "/?" => {
                self.show_help = true;
            }
            "/clear" => {
                self.chat.clear();
                self.header.tokens = (0, 0);
                self.header.context_usage = 0.0;
                self.status_bar.info("Conversation cleared");
            }
            "/new" => {
                self.chat.clear();
                self.history = MessageHistory::new();
                self.session_id = uuid::Uuid::new_v4().to_string();
                self.header.session_id = self.session_id[..8].to_string();
                self.header.tokens = (0, 0);
                self.header.context_usage = 0.0;
                self.header.current_turn = 0;
                self.status_bar.success("New session started");
            }
            "/model" => {
                self.model_switcher.show();
            }
            "/status" => {
                let status = format!(
                    "Provider: {} | Model: {} | Tokens: {}↓ {}↑ | Context: {:.0}%",
                    self.header.provider,
                    self.header.model,
                    self.header.tokens.0,
                    self.header.tokens.1,
                    self.header.context_usage * 100.0
                );
                self.chat.push(ChatMessage::system(status));
            }
            _ => {
                self.chat
                    .push(ChatMessage::system(format!("Unknown command: {}", command)));
            }
        }
    }

    /// Send a message to the agent
    pub async fn send_message(&mut self, content: String) -> mpsc::Receiver<AgentEvent> {
        // Add user message to display
        self.chat.push(ChatMessage::user(content.clone()));

        // Add to history
        self.history.add_user(content.clone());

        // Set running state
        self.running = true;
        self.input.disable("Agent running...");
        self.header.agent_status = AgentStatus::Thinking;
        self.status_bar.set_running_mode();

        // Create channel for events
        let (tx, rx) = mpsc::channel(100);

        // Clone context
        let ctx = self.ctx.clone();
        let session_id = self.session_id.clone();
        let mut history = self.history.clone();
        let user_message = content.clone();

        // Create agent and get steering handle
        if let Some(ref ctx) = ctx {
            let agent = Agent::new(ctx.clone());
            self.steering_handle = Some(agent.steering_handle());

            // Spawn agent task
            tokio::spawn(async move {
                let _ = agent.run(&session_id, &mut history, &user_message, tx).await;
            });
        }

        rx
    }

    /// Toggle pause state
    pub async fn toggle_pause(&mut self) {
        if let Some(ref handle) = self.steering_handle {
            if self.paused {
                let _ = handle.resume().await;
                self.paused = false;
                self.header.agent_status = AgentStatus::Thinking;
                self.status_bar.set_running_mode();
            } else {
                let _ = handle.pause().await;
                self.paused = true;
                self.header.agent_status = AgentStatus::Paused;
                self.status_bar.set_paused_mode();
            }
        }
    }

    /// Stop the agent
    pub async fn stop_agent(&mut self) {
        if let Some(ref handle) = self.steering_handle {
            let _ = handle.stop("User requested stop").await;
        }
        self.running = false;
        self.paused = false;
        self.steering_handle = None;
        self.input.enable();
        self.header.agent_status = AgentStatus::Ready;
        self.status_bar.set_normal_mode();
        self.status_bar.warning("Agent stopped");
    }

    /// Handle agent event
    pub fn handle_agent_event(&mut self, event: AgentEvent) {
        match event {
            AgentEvent::Thinking => {
                self.header.agent_status = AgentStatus::Thinking;
            }
            AgentEvent::Text(text) => {
                // Append to last assistant message or create new one
                if let Some(last) = self.chat.messages.last_mut() {
                    if last.role == MessageRole::Assistant && last.streaming {
                        last.content.push_str(&text);
                        return;
                    }
                }
                // Create new streaming message
                let mut msg = ChatMessage::assistant(text);
                msg.streaming = true;
                self.chat.push(msg);
            }
            AgentEvent::ToolStart { tool_name, .. } => {
                self.header.agent_status = AgentStatus::ToolRunning(tool_name.clone());

                // Add tool block to last assistant message
                let block = ToolBlock::new(&tool_name);
                self.chat.add_tool_block(block);
            }
            AgentEvent::ToolComplete {
                result,
                success,
                duration_ms,
                ..
            } => {
                let state = if success {
                    ToolExecutionState::Success { duration_ms }
                } else {
                    ToolExecutionState::Error {
                        message: truncate(&result, 100),
                    }
                };
                self.chat.update_last_tool(state, Some(truncate(&result, 300)));
                self.header.agent_status = AgentStatus::Thinking;
            }
            AgentEvent::TurnStart { turn } => {
                self.header.current_turn = turn;
            }
            AgentEvent::TurnComplete { .. } => {
                // Finish streaming on last message
                if let Some(last) = self.chat.messages.last_mut() {
                    last.streaming = false;
                }
            }
            AgentEvent::Compressed {
                tokens_before,
                tokens_after,
                tokens_saved,
            } => {
                self.header.context_usage = tokens_after as f32 / 200_000.0;
                self.chat.push(ChatMessage::system(format!(
                    "Context compressed: {} → {} tokens (saved {})",
                    tokens_before, tokens_after, tokens_saved
                )));
            }
            AgentEvent::Paused => {
                self.paused = true;
                self.header.agent_status = AgentStatus::Paused;
                self.status_bar.set_paused_mode();
            }
            AgentEvent::Resumed => {
                self.paused = false;
                self.header.agent_status = AgentStatus::Thinking;
                self.status_bar.set_running_mode();
            }
            AgentEvent::Stopped { reason } => {
                self.running = false;
                self.paused = false;
                self.input.enable();
                self.header.agent_status = AgentStatus::Ready;
                self.status_bar.set_normal_mode();
                self.status_bar.warning(&format!("Stopped: {}", reason));
            }
            AgentEvent::Done { .. } => {
                self.running = false;
                self.paused = false;
                self.steering_handle = None;
                self.input.enable();
                self.header.agent_status = AgentStatus::Ready;
                self.status_bar.set_normal_mode();

                // Finish streaming
                if let Some(last) = self.chat.messages.last_mut() {
                    last.streaming = false;
                }
            }
            AgentEvent::Error(e) => {
                self.running = false;
                self.paused = false;
                self.steering_handle = None;
                self.input.enable();
                self.header.agent_status = AgentStatus::Error;
                self.status_bar.set_normal_mode();
                self.status_bar.error(&truncate(&e, 50));
                self.chat
                    .push(ChatMessage::system(format!("Error: {}", e)));
            }
            AgentEvent::Usage {
                input_tokens,
                output_tokens,
            } => {
                self.header.tokens.0 += input_tokens;
                self.header.tokens.1 += output_tokens;
                self.header.context_usage = (self.header.tokens.0 as f32) / 200_000.0;
            }
        }
    }

    /// Tick for animations
    pub fn tick(&mut self) {
        self.spinner.tick();
        self.status_bar.check_timeout();
    }

    /// Render the chat page with new widgets
    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        // Tick animations
        self.tick();

        // Layout: Header(2) + Chat(flex) + Input(3) + StatusBar(1)
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),  // Header
                Constraint::Min(10),    // Chat
                Constraint::Length(3),  // Input
                Constraint::Length(1),  // Status bar
            ])
            .split(area);

        // Render header
        let header = Header::new(&self.header).with_theme(self.theme.clone());
        frame.render_widget(header, chunks[0]);

        // Render chat view or welcome screen
        if self.chat.messages.is_empty() {
            // Show welcome screen when no messages
            let welcome = WelcomeScreen::new()
                .with_model(&self.header.provider, &self.header.model);
            frame.render_widget(welcome, chunks[1]);
        } else {
            // Show chat view when there are messages
            let chat = ChatView::new(&self.chat)
                .with_theme(self.theme.clone())
                .with_spinner_frame(self.spinner.frame);
            frame.render_widget(chat, chunks[1]);
        }

        // Render input area
        let input = InputArea::new(&self.input)
            .with_theme(self.theme.clone())
            .focused(!self.running);
        frame.render_widget(input, chunks[2]);

        // Render status bar
        let status_bar = StatusBar::new(&self.status_bar).with_theme(self.theme.clone());
        frame.render_widget(status_bar, chunks[3]);

        // Set cursor position if not running
        if !self.running && !self.show_help && !self.model_switcher.is_visible() {
            let cursor_x = chunks[2].x + 3 + self.input.cursor as u16;
            let cursor_y = chunks[2].y + 1;
            frame.set_cursor_position((cursor_x.min(chunks[2].right() - 2), cursor_y));
        }

        // Render overlays
        self.permission_modal.render(frame, area);
        self.model_switcher.render(frame, area);

        if self.show_help {
            frame.render_widget(HelpOverlay::new(), area);
        }
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
