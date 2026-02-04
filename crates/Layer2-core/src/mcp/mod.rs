//! MCP Bridge - Model Context Protocol 연동
//!
//! 외부 MCP 서버를 통해 도구를 확장합니다.
//!
//! ## 기능
//! - MCP 서버 연결 관리 (stdio, SSE)
//! - 도구 목록 동기화
//! - 도구 호출 프록시
//! - 리소스 및 프롬프트 접근
//!
//! ## 지원 전송
//! - stdio: 로컬 프로세스와 stdin/stdout 통신
//! - SSE: HTTP Server-Sent Events 기반 원격 통신
//!
//! ## 프로토콜
//! - JSON-RPC 2.0 over stdio/SSE
//! - MCP 2024-11-05 specification
//!
//! ## 참고
//! - https://modelcontextprotocol.io/

mod bridge;
mod client;
mod transport;
mod types;

pub use bridge::{McpBridge, McpToolAdapter, ServerStatus};
pub use client::{McpClient, McpClientState, McpErrorKind, McpReconnectConfig};
pub use transport::{McpTransport, SseTransport, StdioTransport};
pub use types::{
    McpContent, McpPrompt, McpPromptArgument, McpResource, McpServerConfig, McpTool, McpToolCall,
    McpToolResult, McpTransportConfig,
};
