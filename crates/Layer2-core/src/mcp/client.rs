//! MCP Client - MCP 서버 클라이언트
//!
//! MCP 서버와의 통신을 담당하며 도구 목록 관리 및 도구 호출을 처리

use super::transport::{McpTransport, SseTransport, StdioTransport};
use super::types::{McpServerConfig, McpTool, McpToolCall, McpToolResult, McpTransportConfig};
use forge_foundation::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// MCP 클라이언트 상태
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpClientState {
    /// 연결 안됨
    Disconnected,
    /// 연결 중
    Connecting,
    /// 연결됨 (사용 가능)
    Connected,
    /// 초기화 중
    Initializing,
    /// 에러 상태
    Error,
    /// 재연결 중
    Reconnecting,
}

/// MCP 에러 종류 (상세 분류)
#[derive(Debug, Clone)]
pub enum McpErrorKind {
    /// 프로세스 시작 실패 (명령어 없음, 권한 등)
    ProcessSpawnFailed(String),
    /// 네트워크 연결 실패
    ConnectionFailed(String),
    /// 초기화 핸드셰이크 실패
    InitializeFailed(String),
    /// 프로토콜 버전 불일치
    ProtocolMismatch { expected: String, actual: String },
    /// 서버 응답 타임아웃
    Timeout(String),
    /// 서버 연결 끊김
    Disconnected,
    /// 도구 호출 실패
    ToolCallFailed { tool: String, reason: String },
    /// JSON 파싱 실패
    ParseError(String),
    /// 서버 내부 에러
    ServerError { code: i32, message: String },
}

impl std::fmt::Display for McpErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ProcessSpawnFailed(msg) => write!(f, "Process spawn failed: {}", msg),
            Self::ConnectionFailed(msg) => write!(f, "Connection failed: {}", msg),
            Self::InitializeFailed(msg) => write!(f, "Initialize failed: {}", msg),
            Self::ProtocolMismatch { expected, actual } => {
                write!(
                    f,
                    "Protocol mismatch: expected {}, got {}",
                    expected, actual
                )
            }
            Self::Timeout(msg) => write!(f, "Timeout: {}", msg),
            Self::Disconnected => write!(f, "Server disconnected"),
            Self::ToolCallFailed { tool, reason } => {
                write!(f, "Tool '{}' call failed: {}", tool, reason)
            }
            Self::ParseError(msg) => write!(f, "Parse error: {}", msg),
            Self::ServerError { code, message } => {
                write!(f, "Server error ({}): {}", code, message)
            }
        }
    }
}

/// MCP 재연결 설정
#[derive(Debug, Clone)]
pub struct McpReconnectConfig {
    /// 자동 재연결 활성화
    pub auto_reconnect: bool,
    /// 최대 재연결 횟수
    pub max_reconnects: u32,
    /// 재연결 딜레이 (밀리초)
    pub reconnect_delay_ms: u64,
    /// 백오프 배수
    pub backoff_multiplier: f64,
    /// 최대 딜레이 (밀리초)
    pub max_delay_ms: u64,
}

impl Default for McpReconnectConfig {
    fn default() -> Self {
        Self {
            auto_reconnect: true,
            max_reconnects: 3,
            reconnect_delay_ms: 1000,
            backoff_multiplier: 2.0,
            max_delay_ms: 30000,
        }
    }
}

/// MCP 프로토콜 버전
const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

/// MCP 클라이언트 정보
#[derive(Debug, Clone, Serialize)]
struct ClientInfo {
    name: String,
    version: String,
}

/// MCP 서버 정보
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct ServerInfo {
    name: String,
    version: String,
}

/// MCP 서버 capabilities
#[derive(Debug, Clone, Default, Deserialize)]
#[allow(dead_code)]
struct ServerCapabilities {
    #[serde(default)]
    tools: Option<ToolCapability>,
    #[serde(default)]
    resources: Option<Value>,
    #[serde(default)]
    prompts: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct ToolCapability {
    #[serde(default)]
    list_changed: bool,
}

/// Initialize 응답
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct InitializeResult {
    protocol_version: String,
    server_info: ServerInfo,
    capabilities: ServerCapabilities,
}

/// MCP 클라이언트
pub struct McpClient {
    /// 서버 이름
    name: String,

    /// 전송 계층
    transport: Option<Arc<dyn McpTransport>>,

    /// 사용 가능한 도구들
    tools: RwLock<Vec<McpTool>>,

    /// 서버 capabilities
    capabilities: RwLock<ServerCapabilities>,

    /// 현재 상태
    state: RwLock<McpClientState>,

    /// 재연결 설정
    reconnect_config: McpReconnectConfig,

    /// 재연결 횟수
    reconnect_count: AtomicU32,

    /// 마지막 연결 설정 (재연결용)
    last_transport_config: RwLock<Option<McpTransportConfig>>,

    /// 마지막 에러
    last_error: RwLock<Option<McpErrorKind>>,
}

impl McpClient {
    /// 새 클라이언트 생성
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            transport: None,
            tools: RwLock::new(Vec::new()),
            capabilities: RwLock::new(ServerCapabilities::default()),
            state: RwLock::new(McpClientState::Disconnected),
            reconnect_config: McpReconnectConfig::default(),
            reconnect_count: AtomicU32::new(0),
            last_transport_config: RwLock::new(None),
            last_error: RwLock::new(None),
        }
    }

    /// 재연결 설정과 함께 클라이언트 생성
    pub fn with_reconnect_config(
        name: impl Into<String>,
        reconnect_config: McpReconnectConfig,
    ) -> Self {
        Self {
            name: name.into(),
            transport: None,
            tools: RwLock::new(Vec::new()),
            capabilities: RwLock::new(ServerCapabilities::default()),
            state: RwLock::new(McpClientState::Disconnected),
            reconnect_config,
            reconnect_count: AtomicU32::new(0),
            last_transport_config: RwLock::new(None),
            last_error: RwLock::new(None),
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

    /// 현재 상태
    pub async fn state(&self) -> McpClientState {
        *self.state.read().await
    }

    /// 마지막 에러
    pub async fn last_error(&self) -> Option<McpErrorKind> {
        self.last_error.read().await.clone()
    }

    /// 재연결 횟수
    pub fn reconnect_count(&self) -> u32 {
        self.reconnect_count.load(Ordering::SeqCst)
    }

    /// 연결 상태
    pub fn is_connected(&self) -> bool {
        self.transport
            .as_ref()
            .map(|t| t.is_connected())
            .unwrap_or(false)
    }

    /// 서버에 연결
    pub async fn connect(&mut self, config: &McpTransportConfig) -> Result<()> {
        info!("Connecting to MCP server: {}", self.name);

        *self.state.write().await = McpClientState::Connecting;
        *self.last_transport_config.write().await = Some(config.clone());

        // 전송 계층 생성
        let transport: Arc<dyn McpTransport> = match config {
            McpTransportConfig::Stdio { command, args, env } => {
                match StdioTransport::spawn(command, args, env).await {
                    Ok(t) => Arc::new(t),
                    Err(e) => {
                        let error = McpErrorKind::ProcessSpawnFailed(format!(
                            "Failed to spawn '{}': {}",
                            command, e
                        ));
                        error!("{}", error);
                        *self.last_error.write().await = Some(error.clone());
                        *self.state.write().await = McpClientState::Error;
                        return Err(Error::Internal(error.to_string()));
                    }
                }
            }
            McpTransportConfig::Sse { url } => match SseTransport::connect(url).await {
                Ok(t) => Arc::new(t),
                Err(e) => {
                    let error = McpErrorKind::ConnectionFailed(format!(
                        "Failed to connect to '{}': {}",
                        url, e
                    ));
                    error!("{}", error);
                    *self.last_error.write().await = Some(error.clone());
                    *self.state.write().await = McpClientState::Error;
                    return Err(Error::Internal(error.to_string()));
                }
            },
        };

        self.transport = Some(transport);
        *self.state.write().await = McpClientState::Initializing;

        // Initialize 요청
        if let Err(e) = self.initialize().await {
            let error = McpErrorKind::InitializeFailed(e.to_string());
            error!("{}", error);
            *self.last_error.write().await = Some(error.clone());
            *self.state.write().await = McpClientState::Error;
            self.transport = None;
            return Err(Error::Internal(error.to_string()));
        }

        // 도구 목록 가져오기
        if let Err(e) = self.refresh_tools().await {
            warn!("Failed to refresh tools: {}", e);
            // 도구 목록 실패는 치명적이지 않음 - 계속 진행
        }

        *self.state.write().await = McpClientState::Connected;
        *self.last_error.write().await = None;
        self.reconnect_count.store(0, Ordering::SeqCst);

        info!(
            "Connected to MCP server '{}' with {} tools",
            self.name,
            self.tools.read().await.len()
        );

        Ok(())
    }

    /// 자동 재연결 시도
    pub async fn try_reconnect(&mut self) -> Result<()> {
        if !self.reconnect_config.auto_reconnect {
            return Err(Error::Internal("Auto reconnect is disabled".to_string()));
        }

        let current_count = self.reconnect_count.load(Ordering::SeqCst);
        if current_count >= self.reconnect_config.max_reconnects {
            return Err(Error::Internal(format!(
                "Max reconnect attempts ({}) reached for MCP server '{}'",
                self.reconnect_config.max_reconnects, self.name
            )));
        }

        let config = self.last_transport_config.read().await.clone();
        let config = match config {
            Some(c) => c,
            None => {
                return Err(Error::Internal(
                    "No previous connection config for reconnect".to_string(),
                ));
            }
        };

        *self.state.write().await = McpClientState::Reconnecting;
        self.reconnect_count.fetch_add(1, Ordering::SeqCst);

        // 지수 백오프 딜레이
        let delay = std::cmp::min(
            (self.reconnect_config.reconnect_delay_ms as f64
                * self
                    .reconnect_config
                    .backoff_multiplier
                    .powi(current_count as i32)) as u64,
            self.reconnect_config.max_delay_ms,
        );

        warn!(
            "Reconnecting to MCP server '{}' in {}ms (attempt {}/{})",
            self.name,
            delay,
            current_count + 1,
            self.reconnect_config.max_reconnects
        );

        tokio::time::sleep(std::time::Duration::from_millis(delay)).await;

        // 기존 연결 정리
        if let Some(transport) = self.transport.take() {
            let _ = transport.close().await;
        }

        // 재연결
        match self.connect(&config).await {
            Ok(()) => {
                info!("Reconnected to MCP server '{}' successfully", self.name);
                Ok(())
            }
            Err(e) => {
                error!("Failed to reconnect to MCP server '{}': {}", self.name, e);
                Err(e)
            }
        }
    }

    /// 연결 상태 확인 및 필요 시 재연결
    async fn ensure_connected(&mut self) -> Result<()> {
        let state = *self.state.read().await;

        match state {
            McpClientState::Connected => {
                if self.is_connected() {
                    Ok(())
                } else {
                    // 연결이 끊어진 것으로 감지
                    *self.state.write().await = McpClientState::Error;
                    *self.last_error.write().await = Some(McpErrorKind::Disconnected);

                    if self.reconnect_config.auto_reconnect {
                        self.try_reconnect().await
                    } else {
                        Err(Error::Internal("MCP server disconnected".to_string()))
                    }
                }
            }
            McpClientState::Error | McpClientState::Disconnected => {
                if self.reconnect_config.auto_reconnect {
                    self.try_reconnect().await
                } else {
                    Err(Error::Internal(format!(
                        "MCP server '{}' is not connected (state: {:?})",
                        self.name, state
                    )))
                }
            }
            McpClientState::Reconnecting
            | McpClientState::Connecting
            | McpClientState::Initializing => {
                // 이미 연결 시도 중
                Err(Error::Internal(format!(
                    "MCP server '{}' is currently {:?}",
                    self.name, state
                )))
            }
        }
    }

    /// MCP initialize 핸드셰이크
    async fn initialize(&self) -> Result<()> {
        let transport = self
            .transport
            .as_ref()
            .ok_or_else(|| Error::Internal("No transport".to_string()))?;

        let params = json!({
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "clientInfo": ClientInfo {
                name: "ForgeCode".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            "capabilities": {
                "roots": { "listChanged": true },
                "sampling": {}
            }
        });

        let result = transport.request("initialize", Some(params)).await?;

        // 응답 파싱
        let init_result: InitializeResult = serde_json::from_value(result)
            .map_err(|e| Error::Internal(format!("Invalid initialize response: {}", e)))?;

        debug!(
            "MCP server '{}' v{} initialized (protocol: {})",
            init_result.server_info.name,
            init_result.server_info.version,
            init_result.protocol_version
        );

        // capabilities 저장
        *self.capabilities.write().await = init_result.capabilities;

        // initialized 알림 전송
        transport.notify("notifications/initialized", None).await?;

        Ok(())
    }

    /// 연결 종료
    pub async fn disconnect(&mut self) -> Result<()> {
        if let Some(transport) = self.transport.take() {
            transport.close().await?;
        }

        self.tools.write().await.clear();

        info!("Disconnected from MCP server: {}", self.name);
        Ok(())
    }

    /// 도구 목록 새로고침
    pub async fn refresh_tools(&self) -> Result<()> {
        let transport = self
            .transport
            .as_ref()
            .ok_or_else(|| Error::Internal("Not connected".to_string()))?;

        // tools/list 요청
        let result = transport.request("tools/list", None).await?;

        // 도구 목록 파싱
        #[derive(Deserialize)]
        struct ToolsListResult {
            tools: Vec<McpTool>,
        }

        let tools_result: ToolsListResult = serde_json::from_value(result)
            .map_err(|e| Error::Internal(format!("Invalid tools/list response: {}", e)))?;

        let tool_count = tools_result.tools.len();
        *self.tools.write().await = tools_result.tools;

        debug!(
            "Refreshed {} tools from MCP server '{}'",
            tool_count, self.name
        );

        Ok(())
    }

    /// 사용 가능한 도구 목록
    pub async fn tools(&self) -> Vec<McpTool> {
        self.tools.read().await.clone()
    }

    /// 특정 도구 조회
    pub async fn get_tool(&self, name: &str) -> Option<McpTool> {
        self.tools
            .read()
            .await
            .iter()
            .find(|t| t.name == name)
            .cloned()
    }

    /// 도구 호출
    pub async fn call_tool(&self, call: &McpToolCall) -> Result<McpToolResult> {
        let transport = self
            .transport
            .as_ref()
            .ok_or_else(|| Error::Internal("Not connected".to_string()))?;

        debug!(
            "Calling MCP tool: {} with args: {:?}",
            call.name, call.arguments
        );

        // tools/call 요청
        let params = json!({
            "name": call.name,
            "arguments": call.arguments
        });

        let result = transport.request("tools/call", Some(params)).await?;

        // 결과 파싱
        let tool_result: McpToolResult = serde_json::from_value(result)
            .map_err(|e| Error::Internal(format!("Invalid tools/call response: {}", e)))?;

        if tool_result.is_error {
            warn!(
                "MCP tool '{}' returned error: {:?}",
                call.name,
                tool_result.text()
            );
        }

        Ok(tool_result)
    }

    /// 리소스 목록 조회
    pub async fn list_resources(&self) -> Result<Value> {
        let transport = self
            .transport
            .as_ref()
            .ok_or_else(|| Error::Internal("Not connected".to_string()))?;

        transport.request("resources/list", None).await
    }

    /// 리소스 읽기
    pub async fn read_resource(&self, uri: &str) -> Result<Value> {
        let transport = self
            .transport
            .as_ref()
            .ok_or_else(|| Error::Internal("Not connected".to_string()))?;

        let params = json!({ "uri": uri });
        transport.request("resources/read", Some(params)).await
    }

    /// 프롬프트 목록 조회
    pub async fn list_prompts(&self) -> Result<Value> {
        let transport = self
            .transport
            .as_ref()
            .ok_or_else(|| Error::Internal("Not connected".to_string()))?;

        transport.request("prompts/list", None).await
    }

    /// 프롬프트 가져오기
    pub async fn get_prompt(&self, name: &str, arguments: Option<Value>) -> Result<Value> {
        let transport = self
            .transport
            .as_ref()
            .ok_or_else(|| Error::Internal("Not connected".to_string()))?;

        let params = json!({
            "name": name,
            "arguments": arguments.unwrap_or(Value::Object(Default::default()))
        });

        transport.request("prompts/get", Some(params)).await
    }
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

    #[tokio::test]
    async fn test_client_not_connected() {
        let client = McpClient::new("test");

        // 연결 없이 도구 호출 시도
        let call = McpToolCall {
            name: "test".to_string(),
            arguments: Value::Null,
        };

        let result = client.call_tool(&call).await;
        assert!(result.is_err());
    }
}
