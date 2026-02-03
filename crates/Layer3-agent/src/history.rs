//! Message history management

use forge_provider::{Message, MessageRole, ToolCall, ToolResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Message history for a session
#[derive(Debug, Clone, Default)]
pub struct MessageHistory {
    /// Messages in order
    messages: Vec<Message>,

    /// System prompt
    system_prompt: Option<String>,
}

impl MessageHistory {
    /// Create a new empty history
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with a system prompt
    pub fn with_system_prompt(prompt: impl Into<String>) -> Self {
        Self {
            messages: vec![],
            system_prompt: Some(prompt.into()),
        }
    }

    /// Set system prompt
    pub fn set_system_prompt(&mut self, prompt: impl Into<String>) {
        self.system_prompt = Some(prompt.into());
    }

    /// Get system prompt
    pub fn system_prompt(&self) -> Option<&str> {
        self.system_prompt.as_deref()
    }

    /// Add a user message
    pub fn add_user(&mut self, content: impl Into<String>) {
        self.messages.push(Message::user(content));
    }

    /// Add an assistant message
    pub fn add_assistant(&mut self, content: impl Into<String>) {
        self.messages.push(Message::assistant(content));
    }

    /// Add an assistant message with tool calls
    pub fn add_assistant_with_tools(&mut self, content: impl Into<String>, tool_calls: Vec<ToolCall>) {
        self.messages
            .push(Message::assistant_with_tools(content, tool_calls));
    }

    /// Add a tool result
    pub fn add_tool_result(&mut self, tool_call_id: impl Into<String>, content: impl Into<String>, is_error: bool) {
        self.messages
            .push(Message::tool_result(tool_call_id, content, is_error));
    }

    /// Add a message directly
    pub fn add(&mut self, message: Message) {
        self.messages.push(message);
    }

    /// Get all messages
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// Get messages as owned vec
    pub fn to_messages(&self) -> Vec<Message> {
        self.messages.clone()
    }

    /// Get message count
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// Clear all messages
    pub fn clear(&mut self) {
        self.messages.clear();
    }

    /// Get the last message
    pub fn last(&self) -> Option<&Message> {
        self.messages.last()
    }

    /// Get the last user message
    pub fn last_user(&self) -> Option<&Message> {
        self.messages
            .iter()
            .rev()
            .find(|m| m.role == MessageRole::User)
    }

    /// Get the last assistant message
    pub fn last_assistant(&self) -> Option<&Message> {
        self.messages
            .iter()
            .rev()
            .find(|m| m.role == MessageRole::Assistant)
    }

    /// Estimate token count (rough approximation)
    pub fn estimate_tokens(&self) -> usize {
        let mut tokens = 0;

        // System prompt
        if let Some(ref prompt) = self.system_prompt {
            tokens += prompt.len() / 4;
        }

        // Messages
        for msg in &self.messages {
            tokens += msg.content.len() / 4;

            // Tool calls add tokens
            if let Some(ref tool_calls) = msg.tool_calls {
                for tc in tool_calls {
                    tokens += tc.name.len() / 4;
                    tokens += tc.arguments.to_string().len() / 4;
                }
            }

            // Tool results
            if let Some(ref result) = msg.tool_result {
                tokens += result.content.len() / 4;
            }
        }

        tokens
    }

    /// Summarize history to reduce tokens
    pub fn summarize(&mut self, summary: impl Into<String>) {
        let summary = summary.into();

        // Keep system prompt, clear messages, add summary as context
        self.messages.clear();
        self.messages.push(Message::user(format!(
            "Previous conversation summary:\n{}\n\nPlease continue from here.",
            summary
        )));
    }
}

/// History manager for multiple sessions
pub struct HistoryManager {
    histories: Arc<RwLock<HashMap<String, MessageHistory>>>,
}

impl HistoryManager {
    /// Create a new history manager
    pub fn new() -> Self {
        Self {
            histories: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get or create history for a session
    pub async fn get_or_create(&self, session_id: &str) -> MessageHistory {
        let histories = self.histories.read().await;
        histories.get(session_id).cloned().unwrap_or_default()
    }

    /// Update history for a session
    pub async fn update(&self, session_id: &str, history: MessageHistory) {
        let mut histories = self.histories.write().await;
        histories.insert(session_id.to_string(), history);
    }

    /// Get history for a session
    pub async fn get(&self, session_id: &str) -> Option<MessageHistory> {
        let histories = self.histories.read().await;
        histories.get(session_id).cloned()
    }

    /// Delete history for a session
    pub async fn delete(&self, session_id: &str) {
        let mut histories = self.histories.write().await;
        histories.remove(session_id);
    }

    /// Clear all histories
    pub async fn clear(&self) {
        let mut histories = self.histories.write().await;
        histories.clear();
    }
}

impl Default for HistoryManager {
    fn default() -> Self {
        Self::new()
    }
}
