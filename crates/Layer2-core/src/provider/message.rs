//! Message and ToolCall types for LLM communication

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 메시지 역할
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    /// 시스템 메시지
    System,
    /// 사용자 메시지
    User,
    /// 어시스턴트 메시지
    Assistant,
    /// 도구 결과
    Tool,
}

impl Default for MessageRole {
    fn default() -> Self {
        Self::User
    }
}

/// LLM 메시지
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// 역할
    pub role: MessageRole,

    /// 텍스트 내용
    pub content: String,

    /// 도구 호출 (assistant 메시지)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,

    /// 도구 결과 (tool 메시지)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_result: Option<ToolResult>,
}

impl Message {
    /// 시스템 메시지 생성
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::System,
            content: content.into(),
            tool_calls: None,
            tool_result: None,
        }
    }

    /// 사용자 메시지 생성
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            content: content.into(),
            tool_calls: None,
            tool_result: None,
        }
    }

    /// 어시스턴트 메시지 생성
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: content.into(),
            tool_calls: None,
            tool_result: None,
        }
    }

    /// 도구 호출이 있는 어시스턴트 메시지
    pub fn assistant_with_tools(content: impl Into<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: content.into(),
            tool_calls: Some(tool_calls),
            tool_result: None,
        }
    }

    /// 도구 결과 메시지
    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Tool,
            content: String::new(),
            tool_calls: None,
            tool_result: Some(ToolResult {
                tool_call_id: tool_call_id.into(),
                content: content.into(),
                is_error: false,
            }),
        }
    }

    /// 도구 에러 결과 메시지
    pub fn tool_error(tool_call_id: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Tool,
            content: String::new(),
            tool_calls: None,
            tool_result: Some(ToolResult {
                tool_call_id: tool_call_id.into(),
                content: error.into(),
                is_error: true,
            }),
        }
    }
}

/// 도구 호출
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// 호출 ID
    pub id: String,

    /// 도구 이름
    pub name: String,

    /// 인자 (JSON)
    pub arguments: Value,
}

impl ToolCall {
    pub fn new(id: impl Into<String>, name: impl Into<String>, arguments: Value) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            arguments,
        }
    }
}

/// 도구 실행 결과
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// 도구 호출 ID
    pub tool_call_id: String,

    /// 결과 내용
    pub content: String,

    /// 에러 여부
    #[serde(default)]
    pub is_error: bool,
}

impl ToolResult {
    pub fn success(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            tool_call_id: tool_call_id.into(),
            content: content.into(),
            is_error: false,
        }
    }

    pub fn error(tool_call_id: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            tool_call_id: tool_call_id.into(),
            content: error.into(),
            is_error: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_creation() {
        let sys = Message::system("You are a helpful assistant");
        assert_eq!(sys.role, MessageRole::System);

        let user = Message::user("Hello");
        assert_eq!(user.role, MessageRole::User);

        let assistant = Message::assistant("Hi there!");
        assert_eq!(assistant.role, MessageRole::Assistant);
    }

    #[test]
    fn test_tool_call() {
        let tc = ToolCall::new("call_123", "bash", serde_json::json!({"command": "ls"}));
        assert_eq!(tc.id, "call_123");
        assert_eq!(tc.name, "bash");
    }

    #[test]
    fn test_tool_result() {
        let result = ToolResult::success("call_123", "file1.txt\nfile2.txt");
        assert!(!result.is_error);

        let error = ToolResult::error("call_456", "Command failed");
        assert!(error.is_error);
    }
}
