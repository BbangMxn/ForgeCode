//! MCP Bridge - Model Context Protocol 연동
//!
//! 외부 MCP 서버를 통해 도구를 확장합니다.
//!
//! ## 기능
//! - MCP 서버 연결 관리
//! - 도구 목록 동기화
//! - 도구 호출 프록시
//!
//! ## 지원 전송
//! - stdio (로컬 프로세스)
//! - SSE (HTTP Server-Sent Events)
//!
//! ## 참고
//! - https://modelcontextprotocol.io/

mod bridge;
mod client;
mod types;

pub use bridge::McpBridge;
pub use client::{McpClient, McpTransport};
pub use types::{McpTool, McpToolCall, McpToolResult};
