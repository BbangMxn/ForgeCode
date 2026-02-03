//! Message types for LLM communication

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Role of a message in the conversation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

/// A message in the conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Unique message ID
    pub id: Uuid,

    /// Role of this message
    pub role: MessageRole,

    /// Text content
    pub content: String,

    /// Tool calls made by assistant (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,

    /// Tool result (if this is a tool response message)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_result: Option<ToolResult>,
}

impl Message {
    /// Create a system message
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: MessageRole::System,
            content: content.into(),
            tool_calls: None,
            tool_result: None,
        }
    }

    /// Create a user message
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: MessageRole::User,
            content: content.into(),
            tool_calls: None,
            tool_result: None,
        }
    }

    /// Create an assistant message
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: MessageRole::Assistant,
            content: content.into(),
            tool_calls: None,
            tool_result: None,
        }
    }

    /// Create an assistant message with tool calls
    pub fn assistant_with_tools(content: impl Into<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: MessageRole::Assistant,
            content: content.into(),
            tool_calls: Some(tool_calls),
            tool_result: None,
        }
    }

    /// Create a tool result message
    pub fn tool_result(
        tool_call_id: impl Into<String>,
        content: impl Into<String>,
        is_error: bool,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: MessageRole::Tool,
            content: String::new(),
            tool_calls: None,
            tool_result: Some(ToolResult {
                tool_call_id: tool_call_id.into(),
                content: content.into(),
                is_error,
            }),
        }
    }
}

/// A tool call requested by the assistant
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Unique ID for this tool call
    pub id: String,

    /// Name of the tool to call
    pub name: String,

    /// Arguments as JSON
    pub arguments: serde_json::Value,
}

impl ToolCall {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        arguments: serde_json::Value,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            arguments,
        }
    }
}

/// Result of a tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// ID of the tool call this is responding to
    pub tool_call_id: String,

    /// Result content
    pub content: String,

    /// Whether this is an error result
    pub is_error: bool,
}
