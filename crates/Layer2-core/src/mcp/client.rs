//! MCP Client - MCP 서버 클라이언트
//!
//! MCP 서버와의 통신을 담당

use super::types::{McpServerConfig, McpTool, McpToolCall, McpToolResult, McpTransportConfig};
use forge_foundation::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// MCP 전송 방식
pub enum McpTransport {
    /// stdio 전송 (로컬 프로세스)
    Stdio {
        // TODO: Child 프로세스 핸들
    },

    /// SSE 전송 (HTTP)
    Sse {
        url: String,
        // TODO: HTTP 클라이언트
    },
}

/// MCP 클라이언트
pub struct McpClient {
    /// 서버 이름
    name: String,

    /// 전송 방식
    transport: Option<McpTransport>,

    /// 사용 가능한 도구들
    tools: Vec<McpTool>,

    /// 연결 상태
    connected: bool,
}

impl McpClient {
    /// 새 클라이언트 생성
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            transport: None,
            tools: Vec::new(),
            connected: false,
        }
    }

    /// 설정에서 클라이언트 생성
    pub fn from_config(config: &McpServerConfig) -> Self {
        Self::new(&config.name)
    }

    /// 서버 이름
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 연결 상태
    pub fn is_connected(&self) -> bool {
        self.connected
    }

    /// 서버에 연결
    pub async fn connect(&mut self, config: &McpTransportConfig) -> Result<()> {
        // TODO: 실제 연결 구현
        // 1. 전송 타입에 따라 연결
        // 2. initialize 요청
        // 3. 도구 목록 가져오기

        match config {
            McpTransportConfig::Stdio { command, args, env } => {
                // TODO: 프로세스 시작
                // let child = tokio::process::Command::new(command)
                //     .args(args)
                //     .envs(env)
                //     .stdin(Stdio::piped())
                //     .stdout(Stdio::piped())
                //     .spawn()?;

                self.transport = Some(McpTransport::Stdio {});
            }

            McpTransportConfig::Sse { url } => {
                // TODO: SSE 연결
                self.transport = Some(McpTransport::Sse { url: url.clone() });
            }
        }

        // TODO: initialize 요청
        self.connected = true;

        // TODO: 도구 목록 가져오기
        self.refresh_tools().await?;

        Ok(())
    }

    /// 연결 종료
    pub async fn disconnect(&mut self) -> Result<()> {
        // TODO: 정리 작업
        self.transport = None;
        self.connected = false;
        self.tools.clear();

        Ok(())
    }

    /// 도구 목록 새로고침
    pub async fn refresh_tools(&mut self) -> Result<()> {
        if !self.connected {
            return Err(forge_foundation::Error::internal("Not connected"));
        }

        // TODO: tools/list 요청
        // let response = self.request("tools/list", json!({})).await?;
        // self.tools = serde_json::from_value(response["tools"])?;

        Ok(())
    }

    /// 사용 가능한 도구 목록
    pub fn tools(&self) -> &[McpTool] {
        &self.tools
    }

    /// 도구 호출
    pub async fn call_tool(&self, call: &McpToolCall) -> Result<McpToolResult> {
        if !self.connected {
            return Err(forge_foundation::Error::internal("Not connected"));
        }

        // TODO: tools/call 요청
        // let response = self.request("tools/call", json!({
        //     "name": call.name,
        //     "arguments": call.arguments
        // })).await?;

        Ok(McpToolResult::success(format!(
            "TODO: Call MCP tool {}",
            call.name
        )))
    }

    /// JSON-RPC 요청 전송
    async fn request(&self, method: &str, params: Value) -> Result<Value> {
        // TODO: 실제 요청 구현
        // 1. JSON-RPC 메시지 생성
        // 2. 전송
        // 3. 응답 대기
        // 4. 파싱

        Ok(Value::Null)
    }
}

/// JSON-RPC 메시지
#[derive(Debug, Serialize, Deserialize)]
struct JsonRpcMessage {
    jsonrpc: String,
    id: Option<u64>,
    method: Option<String>,
    params: Option<Value>,
    result: Option<Value>,
    error: Option<JsonRpcError>,
}

/// JSON-RPC 에러
#[derive(Debug, Serialize, Deserialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    data: Option<Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_new() {
        let client = McpClient::new("test-server");
        assert_eq!(client.name(), "test-server");
        assert!(!client.is_connected());
    }
}
