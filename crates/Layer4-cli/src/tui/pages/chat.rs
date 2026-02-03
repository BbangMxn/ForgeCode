//! Chat page - main conversation interface

use crate::tui::components::{
    ChatMessage, InputBox, MessageList, MessageRole, ToolInfo, ToolStatus,
};
use crossterm::event::KeyEvent;
use forge_agent::{Agent, AgentContext, AgentEvent, MessageHistory};
use forge_foundation::{PermissionService, ProviderConfig};
use forge_provider::Gateway;
use forge_tool::ToolRegistry;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::Paragraph,
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

    /// Status text
    status: String,

    /// Token usage
    tokens: (u32, u32),
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
            status: "Ready".to_string(),
            tokens: (0, 0),
        }
    }

    /// Initialize with configuration
    pub fn init(&mut self, config: &ProviderConfig) -> Result<(), String> {
        // Create gateway
        let gateway =
            Gateway::from_config(config).map_err(|e| format!("Failed to initialize LLM: {}", e))?;

        // Create tools
        let tools = ToolRegistry::with_builtins();

        // Create permissions (with auto-approve for now, will add UI later)
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

        self.status = format!("Connected");

        Ok(())
    }

    /// Handle key event
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<ChatAction> {
        if self.running {
            // Ignore input while agent is running
            return None;
        }

        if self.input.handle_key(key) {
            // Enter was pressed
            let content = self.input.take();
            if !content.is_empty() {
                return Some(ChatAction::SendMessage(content));
            }
        }

        None
    }

    /// Send a message to the agent
    pub async fn send_message(&mut self, content: String) -> mpsc::Receiver<AgentEvent> {
        self.running = true;
        self.status = "Thinking...".to_string();

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

        // Spawn agent task
        tokio::spawn(async move {
            if let Some(ctx) = ctx {
                let agent = Agent::new(ctx);
                let _ = agent.run(&session_id, &mut history, &content, tx).await;
            }
        });

        rx
    }

    /// Handle agent event
    pub fn handle_agent_event(&mut self, event: AgentEvent) {
        match event {
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
                ..
            } => {
                // Update last tool message
                if let Some(last) = self.messages.messages.last_mut() {
                    if last.role == MessageRole::Tool {
                        last.content = truncate(&result, 200);
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
            AgentEvent::Done { .. } => {
                self.running = false;
                self.status = "Ready".to_string();
            }
            AgentEvent::Error(e) => {
                self.running = false;
                self.status = format!("Error: {}", e);
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
            }
            AgentEvent::Thinking => {
                self.status = "Thinking...".to_string();
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

        // Render input
        self.input.render(frame, chunks[1]);

        // Render status bar
        let status_text = format!(
            " {} │ Tokens: {} in / {} out │ Session: {}",
            self.status,
            self.tokens.0,
            self.tokens.1,
            &self.session_id[..8]
        );

        let status_bar = Paragraph::new(status_text)
            .style(Style::default().bg(Color::DarkGray).fg(Color::White));

        frame.render_widget(status_bar, chunks[2]);
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
