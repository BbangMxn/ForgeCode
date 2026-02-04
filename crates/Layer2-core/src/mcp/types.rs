//! MCP Types - MCP 관련 타입 정의

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// MCP 서버에서 제공하는 도구 정보
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    /// 도구 이름
    pub name: String,

    /// 도구 설명
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// 입력 스키마 (JSON Schema)
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}

/// MCP 도구 호출
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolCall {
    /// 도구 이름
    pub name: String,

    /// 인자
    #[serde(default)]
    pub arguments: Value,
}

/// MCP 도구 실행 결과
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolResult {
    /// 성공 여부
    #[serde(default)]
    pub is_error: bool,

    /// 결과 콘텐츠
    pub content: Vec<McpContent>,
}

/// MCP 콘텐츠
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum McpContent {
    /// 텍스트 콘텐츠
    Text { text: String },

    /// 이미지 콘텐츠
    Image {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },

    /// 리소스 참조
    Resource {
        uri: String,
        #[serde(rename = "mimeType", skip_serializing_if = "Option::is_none")]
        mime_type: Option<String>,
    },
}

impl McpToolResult {
    /// 성공 결과 생성
    pub fn success(text: impl Into<String>) -> Self {
        Self {
            is_error: false,
            content: vec![McpContent::Text { text: text.into() }],
        }
    }

    /// 오류 결과 생성
    pub fn error(text: impl Into<String>) -> Self {
        Self {
            is_error: true,
            content: vec![McpContent::Text { text: text.into() }],
        }
    }

    /// 텍스트 결과 추출
    pub fn text(&self) -> Option<&str> {
        for content in &self.content {
            if let McpContent::Text { text } = content {
                return Some(text);
            }
        }
        None
    }
}

/// MCP 서버 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// 서버 이름
    pub name: String,

    /// 전송 타입
    pub transport: McpTransportConfig,

    /// 자동 연결 여부
    #[serde(default)]
    pub auto_connect: bool,
}

/// MCP 전송 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum McpTransportConfig {
    /// stdio 전송 (로컬 프로세스)
    Stdio {
        /// 실행 명령어
        command: String,
        /// 인자
        #[serde(default)]
        args: Vec<String>,
        /// 환경 변수
        #[serde(default)]
        env: std::collections::HashMap<String, String>,
    },

    /// SSE 전송 (HTTP)
    Sse {
        /// 서버 URL
        url: String,
    },
}

/// MCP 리소스
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResource {
    /// 리소스 URI
    pub uri: String,

    /// 리소스 이름
    pub name: String,

    /// 설명
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// MIME 타입
    #[serde(rename = "mimeType", skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

/// MCP 프롬프트
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPrompt {
    /// 프롬프트 이름
    pub name: String,

    /// 설명
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// 인자 목록
    #[serde(default)]
    pub arguments: Vec<McpPromptArgument>,
}

/// MCP 프롬프트 인자
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPromptArgument {
    /// 인자 이름
    pub name: String,

    /// 설명
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// 필수 여부
    #[serde(default)]
    pub required: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_tool_result() {
        let result = McpToolResult::success("Hello");
        assert!(!result.is_error);
        assert_eq!(result.text(), Some("Hello"));

        let error = McpToolResult::error("Failed");
        assert!(error.is_error);
    }

    #[test]
    fn test_mcp_resource() {
        let resource = McpResource {
            uri: "file:///test.txt".to_string(),
            name: "Test File".to_string(),
            description: Some("A test file".to_string()),
            mime_type: Some("text/plain".to_string()),
        };
        assert_eq!(resource.uri, "file:///test.txt");
    }

    #[test]
    fn test_mcp_prompt() {
        let prompt = McpPrompt {
            name: "test_prompt".to_string(),
            description: Some("Test prompt".to_string()),
            arguments: vec![McpPromptArgument {
                name: "arg1".to_string(),
                description: None,
                required: true,
            }],
        };
        assert_eq!(prompt.arguments.len(), 1);
    }
}
