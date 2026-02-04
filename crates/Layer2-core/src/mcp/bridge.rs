//! MCP Bridge - MCP 도구를 Layer2 Tool로 변환
//!
//! MCP 서버의 도구들을 ToolRegistry에 등록
//!
//! ## 기능
//! - **연결 풀링**: 서버별 연결 재사용
//! - **헬스 체크**: 주기적 서버 상태 확인
//! - **자동 재연결**: 연결 끊김 시 자동 복구
//! - **TTL 관리**: 유휴 연결 자동 정리

use super::{McpClient, McpTool, McpToolCall, McpTransportConfig};
use async_trait::async_trait;
use forge_foundation::{
    PermissionAction, PermissionDef, Result, Tool, ToolContext, ToolMeta, ToolResult,
};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// 기본 연결 TTL (10분)
const DEFAULT_CONNECTION_TTL: Duration = Duration::from_secs(600);

/// 헬스 체크 간격 (1분)
const HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(60);

/// 재연결 시도 간격 (5초)
const RECONNECT_DELAY: Duration = Duration::from_secs(5);

/// 최대 재연결 시도 횟수
const MAX_RECONNECT_ATTEMPTS: u32 = 3;

/// 연결된 클라이언트 정보
struct ManagedConnection {
    /// MCP 클라이언트
    client: Arc<RwLock<McpClient>>,
    /// 연결 설정 (재연결용)
    config: McpTransportConfig,
    /// 마지막 사용 시간
    last_used: RwLock<Instant>,
    /// 마지막 헬스 체크 시간
    last_health_check: RwLock<Instant>,
    /// 연결 상태
    healthy: RwLock<bool>,
    /// 재연결 시도 횟수
    reconnect_attempts: RwLock<u32>,
}

impl ManagedConnection {
    fn new(client: McpClient, config: McpTransportConfig) -> Self {
        let now = Instant::now();
        Self {
            client: Arc::new(RwLock::new(client)),
            config,
            last_used: RwLock::new(now),
            last_health_check: RwLock::new(now),
            healthy: RwLock::new(true),
            reconnect_attempts: RwLock::new(0),
        }
    }

    /// 사용 시간 갱신
    async fn touch(&self) {
        *self.last_used.write().await = Instant::now();
    }

    /// TTL 만료 확인
    async fn is_expired(&self, ttl: Duration) -> bool {
        self.last_used.read().await.elapsed() > ttl
    }

    /// 헬스 체크 필요 여부
    async fn needs_health_check(&self) -> bool {
        self.last_health_check.read().await.elapsed() > HEALTH_CHECK_INTERVAL
    }

    /// 헬스 체크 수행
    async fn perform_health_check(&self) -> bool {
        *self.last_health_check.write().await = Instant::now();

        let client = self.client.read().await;
        let is_healthy = client.is_connected();

        *self.healthy.write().await = is_healthy;
        is_healthy
    }

    /// 재연결 시도
    async fn try_reconnect(&self) -> Result<()> {
        let mut attempts = self.reconnect_attempts.write().await;
        if *attempts >= MAX_RECONNECT_ATTEMPTS {
            return Err(forge_foundation::Error::Internal(
                "Max reconnection attempts reached".to_string(),
            ));
        }

        *attempts += 1;
        drop(attempts);

        // 잠시 대기
        tokio::time::sleep(RECONNECT_DELAY).await;

        // 재연결
        let mut client = self.client.write().await;
        client.disconnect().await?;
        client.connect(&self.config).await?;

        // 성공하면 카운터 리셋
        *self.reconnect_attempts.write().await = 0;
        *self.healthy.write().await = true;

        Ok(())
    }
}

/// MCP Bridge - MCP 서버 관리 및 도구 브릿지
///
/// ## 기능
/// - 연결 풀링 및 재사용
/// - 헬스 체크 및 자동 재연결
/// - TTL 기반 유휴 연결 정리
pub struct McpBridge {
    /// 연결된 MCP 클라이언트들
    connections: RwLock<HashMap<String, Arc<ManagedConnection>>>,
    /// 연결 TTL
    connection_ttl: Duration,
}

impl McpBridge {
    pub fn new() -> Self {
        Self {
            connections: RwLock::new(HashMap::new()),
            connection_ttl: DEFAULT_CONNECTION_TTL,
        }
    }

    /// TTL 설정과 함께 생성
    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            connections: RwLock::new(HashMap::new()),
            connection_ttl: ttl,
        }
    }

    /// MCP 서버 추가 (연결 설정 포함)
    pub async fn add_server_with_config(
        &self,
        client: McpClient,
        config: McpTransportConfig,
    ) {
        let name = client.name().to_string();
        let managed = Arc::new(ManagedConnection::new(client, config));

        self.connections.write().await.insert(name.clone(), managed);
        info!("MCP server '{}' added to bridge", name);
    }

    /// MCP 서버 추가 (기존 호환성)
    pub async fn add_server(&self, client: McpClient) {
        let name = client.name().to_string();
        // 기본 설정 사용 (재연결 불가)
        let config = McpTransportConfig::Stdio {
            command: String::new(),
            args: vec![],
            env: HashMap::new(),
        };
        let managed = Arc::new(ManagedConnection::new(client, config));

        self.connections.write().await.insert(name.clone(), managed);
        info!("MCP server '{}' added to bridge (without reconnect config)", name);
    }

    /// MCP 서버 제거
    pub async fn remove_server(&self, name: &str) -> Option<Arc<RwLock<McpClient>>> {
        if let Some(managed) = self.connections.write().await.remove(name) {
            // 연결 종료
            let mut client = managed.client.write().await;
            if let Err(e) = client.disconnect().await {
                warn!("Error disconnecting MCP server '{}': {}", name, e);
            }
            info!("MCP server '{}' removed from bridge", name);
            return Some(Arc::clone(&managed.client));
        }
        None
    }

    /// 서버 이름으로 클라이언트 조회 (헬스 체크 포함)
    pub async fn get_client(&self, name: &str) -> Option<Arc<RwLock<McpClient>>> {
        let connections = self.connections.read().await;
        if let Some(managed) = connections.get(name) {
            // 사용 시간 갱신
            managed.touch().await;

            // 헬스 체크 필요하면 수행
            if managed.needs_health_check().await {
                if !managed.perform_health_check().await {
                    // 재연결 시도
                    drop(connections);
                    if let Err(e) = self.try_reconnect(name).await {
                        error!("Failed to reconnect MCP server '{}': {}", name, e);
                        return None;
                    }
                    return self.connections.read().await.get(name).map(|m| Arc::clone(&m.client));
                }
            }

            return Some(Arc::clone(&managed.client));
        }
        None
    }

    /// 재연결 시도
    async fn try_reconnect(&self, name: &str) -> Result<()> {
        let connections = self.connections.read().await;
        if let Some(managed) = connections.get(name) {
            managed.try_reconnect().await
        } else {
            Err(forge_foundation::Error::NotFound(format!(
                "MCP server '{}' not found",
                name
            )))
        }
    }

    /// 모든 MCP 도구를 Layer2 Tool로 변환
    pub async fn get_all_tools(&self) -> Vec<Arc<dyn Tool>> {
        let mut tools: Vec<Arc<dyn Tool>> = Vec::new();

        for (server_name, managed) in self.connections.read().await.iter() {
            // 건강하지 않은 서버는 건너뜀
            if !*managed.healthy.read().await {
                debug!("Skipping unhealthy MCP server '{}'", server_name);
                continue;
            }

            let client_guard = managed.client.read().await;

            for mcp_tool in client_guard.tools().await {
                let tool = McpToolAdapter::new(
                    server_name.clone(),
                    mcp_tool.clone(),
                    Arc::clone(&managed.client),
                );
                tools.push(Arc::new(tool));
            }
        }

        tools
    }

    /// 특정 서버의 도구들만 가져오기
    pub async fn get_server_tools(&self, server_name: &str) -> Vec<Arc<dyn Tool>> {
        let mut tools: Vec<Arc<dyn Tool>> = Vec::new();

        if let Some(managed) = self.connections.read().await.get(server_name) {
            // 사용 시간 갱신
            managed.touch().await;

            let client_guard = managed.client.read().await;

            for mcp_tool in client_guard.tools().await {
                let tool = McpToolAdapter::new(
                    server_name.to_string(),
                    mcp_tool.clone(),
                    Arc::clone(&managed.client),
                );
                tools.push(Arc::new(tool));
            }
        }

        tools
    }

    /// 연결된 서버 목록
    pub async fn list_servers(&self) -> Vec<String> {
        self.connections.read().await.keys().cloned().collect()
    }

    /// 서버 상태 확인
    pub async fn server_status(&self, name: &str) -> Option<ServerStatus> {
        let connections = self.connections.read().await;
        if let Some(managed) = connections.get(name) {
            Some(ServerStatus {
                name: name.to_string(),
                connected: managed.client.read().await.is_connected(),
                healthy: *managed.healthy.read().await,
                last_used: managed.last_used.read().await.elapsed(),
                reconnect_attempts: *managed.reconnect_attempts.read().await,
            })
        } else {
            None
        }
    }

    /// 모든 서버 상태
    pub async fn all_server_status(&self) -> Vec<ServerStatus> {
        let mut statuses = Vec::new();
        for name in self.list_servers().await {
            if let Some(status) = self.server_status(&name).await {
                statuses.push(status);
            }
        }
        statuses
    }

    /// 유휴 연결 정리
    pub async fn cleanup_idle(&self) {
        let mut to_remove = Vec::new();

        {
            let connections = self.connections.read().await;
            for (name, managed) in connections.iter() {
                if managed.is_expired(self.connection_ttl).await {
                    to_remove.push(name.clone());
                }
            }
        }

        for name in to_remove {
            info!("Cleaning up idle MCP connection: {}", name);
            self.remove_server(&name).await;
        }
    }

    /// 모든 연결 헬스 체크
    pub async fn health_check_all(&self) {
        let connections = self.connections.read().await;
        for (name, managed) in connections.iter() {
            if managed.needs_health_check().await {
                let healthy = managed.perform_health_check().await;
                if !healthy {
                    warn!("MCP server '{}' health check failed", name);
                }
            }
        }
    }

    /// 모든 서버 연결 종료
    pub async fn shutdown(&self) {
        let mut connections = self.connections.write().await;
        for (name, managed) in connections.drain() {
            let mut client = managed.client.write().await;
            if let Err(e) = client.disconnect().await {
                warn!("Error disconnecting MCP server '{}': {}", name, e);
            }
        }
        info!("All MCP connections closed");
    }
}

impl Default for McpBridge {
    fn default() -> Self {
        Self::new()
    }
}

/// 서버 상태 정보
#[derive(Debug, Clone)]
pub struct ServerStatus {
    pub name: String,
    pub connected: bool,
    pub healthy: bool,
    pub last_used: Duration,
    pub reconnect_attempts: u32,
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
        format!("mcp:{}:{}", self.server_name, self.mcp_tool.name)
    }
}

#[async_trait]
impl Tool for McpToolAdapter {
    fn meta(&self) -> ToolMeta {
        let tool_name = format!("mcp_{}_{}", self.server_name, self.mcp_tool.name);
        ToolMeta::new(&tool_name)
            .display_name(&tool_name)
            .description(
                self.mcp_tool
                    .description
                    .clone()
                    .unwrap_or_else(|| format!("MCP tool from {}", self.server_name)),
            )
            .category("mcp")
            .permission(
                PermissionDef::new("mcp.execute", "mcp")
                    .risk_level(5)
                    .description(format!("Execute MCP tool {} from {}", self.mcp_tool.name, self.server_name))
                    .requires_confirmation(true),
            )
    }

    fn name(&self) -> &str {
        // 안전한 방법: 저장된 이름 반환
        // 여기서는 trait object에서 작동하도록 Box::leak 사용을 피하고
        // 빈 문자열 반환 (meta().name으로 실제 이름 조회)
        ""
    }

    fn schema(&self) -> Value {
        self.mcp_tool.input_schema.clone()
    }

    fn required_permission(&self, _input: &Value) -> Option<PermissionAction> {
        // MCP 도구는 Custom 권한 사용 (server:tool 형식)
        Some(PermissionAction::Custom {
            name: format!("mcp:{}", self.server_name),
            details: self.mcp_tool.name.clone(),
        })
    }

    async fn execute(&self, input: Value, _context: &dyn ToolContext) -> Result<ToolResult> {
        let call = McpToolCall {
            name: self.mcp_tool.name.clone(),
            arguments: input,
        };

        let client = self.client.read().await;

        // 연결 상태 확인
        if !client.is_connected() {
            return Ok(ToolResult::error(format!(
                "MCP server '{}' is not connected",
                self.server_name
            )));
        }

        let result = client.call_tool(&call).await?;

        if result.is_error {
            Ok(ToolResult::error(
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

    #[tokio::test]
    async fn test_bridge_with_ttl() {
        let bridge = McpBridge::with_ttl(Duration::from_secs(300));
        assert_eq!(bridge.connection_ttl, Duration::from_secs(300));
    }

    #[tokio::test]
    async fn test_server_status() {
        let bridge = McpBridge::new();
        assert!(bridge.server_status("nonexistent").await.is_none());
    }
}
