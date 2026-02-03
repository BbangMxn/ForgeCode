//! MCP Bridge - MCP 도구를 Layer2 Tool로 변환
//!
//! MCP 서버의 도구들을 ToolRegistry에 등록

use super::{McpClient, McpTool, McpToolCall};
use async_trait::async_trait;
use forge_foundation::{
    PermissionAction, Result, Tool, ToolMeta, ToolResult, ToolSchema, ToolSource,
};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// MCP Bridge - MCP 서버 관리 및 도구 브릿지
pub struct McpBridge {
    /// 연결된 MCP 클라이언트들
    clients: RwLock<HashMap<String, Arc<RwLock<McpClient>>>>,
}

impl McpBridge {
    pub fn new() -> Self {
        Self {
            clients: RwLock::new(HashMap::new()),
        }
    }

    /// MCP 서버 추가
    pub async fn add_server(&self, client: McpClient) {
        let name = client.name().to_string();
        self.clients
            .write()
            .await
            .insert(name, Arc::new(RwLock::new(client)));
    }

    /// MCP 서버 제거
    pub async fn remove_server(&self, name: &str) -> Option<Arc<RwLock<McpClient>>> {
        self.clients.write().await.remove(name)
    }

    /// 서버 이름으로 클라이언트 조회
    pub async fn get_client(&self, name: &str) -> Option<Arc<RwLock<McpClient>>> {
        self.clients.read().await.get(name).cloned()
    }

    /// 모든 MCP 도구를 Layer2 Tool로 변환
    pub async fn get_all_tools(&self) -> Vec<Arc<dyn Tool>> {
        let mut tools: Vec<Arc<dyn Tool>> = Vec::new();

        for (server_name, client) in self.clients.read().await.iter() {
            let client_guard = client.read().await;

            for mcp_tool in client_guard.tools() {
                let tool = McpToolAdapter::new(
                    server_name.clone(),
                    mcp_tool.clone(),
                    Arc::clone(client),
                );
                tools.push(Arc::new(tool));
            }
        }

        tools
    }

    /// 특정 서버의 도구들만 가져오기
    pub async fn get_server_tools(&self, server_name: &str) -> Vec<Arc<dyn Tool>> {
        let mut tools: Vec<Arc<dyn Tool>> = Vec::new();

        if let Some(client) = self.clients.read().await.get(server_name) {
            let client_guard = client.read().await;

            for mcp_tool in client_guard.tools() {
                let tool = McpToolAdapter::new(
                    server_name.to_string(),
                    mcp_tool.clone(),
                    Arc::clone(client),
                );
                tools.push(Arc::new(tool));
            }
        }

        tools
    }

    /// 연결된 서버 목록
    pub async fn list_servers(&self) -> Vec<String> {
        self.clients.read().await.keys().cloned().collect()
    }
}

impl Default for McpBridge {
    fn default() -> Self {
        Self::new()
    }
}

/// MCP Tool을 Layer2 Tool trait으로 변환하는 어댑터
pub struct McpToolAdapter {
    /// MCP 서버 이름
    server_name: String,

    /// MCP 도구 정보
    mcp_tool: McpTool,

    /// MCP 클라이언트 참조
    client: Arc<RwLock<McpClient>>,
}

impl McpToolAdapter {
    pub fn new(
        server_name: String,
        mcp_tool: McpTool,
        client: Arc<RwLock<McpClient>>,
    ) -> Self {
        Self {
            server_name,
            mcp_tool,
            client,
        }
    }

    /// 전체 도구 ID (mcp:server:tool 형식)
    pub fn full_id(&self) -> String {
        ToolSource::mcp(&self.server_name, &self.mcp_tool.name).full_id()
    }
}

#[async_trait]
impl Tool for McpToolAdapter {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            name: format!("mcp_{}_{}", self.server_name, self.mcp_tool.name),
            description: self
                .mcp_tool
                .description
                .clone()
                .unwrap_or_else(|| format!("MCP tool from {}", self.server_name)),
            version: "1.0.0".to_string(),
        }
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            input_schema: self.mcp_tool.input_schema.clone(),
        }
    }

    fn required_permission(&self, _input: &Value) -> Option<PermissionAction> {
        // MCP 도구는 서버 이름 기반 권한
        Some(PermissionAction::Mcp {
            server: self.server_name.clone(),
            tool: self.mcp_tool.name.clone(),
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolResult> {
        let call = McpToolCall {
            name: self.mcp_tool.name.clone(),
            arguments: input,
        };

        let client = self.client.read().await;
        let result = client.call_tool(&call).await?;

        if result.is_error {
            Ok(ToolResult::failure(
                result.text().unwrap_or("MCP tool error").to_string(),
            ))
        } else {
            Ok(ToolResult::success(
                result.text().unwrap_or("").to_string(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mcp_bridge() {
        let bridge = McpBridge::new();
        assert!(bridge.list_servers().await.is_empty());
    }
}
