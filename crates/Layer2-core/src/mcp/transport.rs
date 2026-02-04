//! MCP Transport - 전송 계층 구현
//!
//! MCP 서버와의 통신을 위한 전송 계층
//! - Stdio: 로컬 프로세스와 stdin/stdout 통신
//! - SSE: HTTP Server-Sent Events

use async_trait::async_trait;
use forge_foundation::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot, Mutex, RwLock};
use tracing::{debug, error, info};

/// JSON-RPC 2.0 요청
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl JsonRpcRequest {
    pub fn new(id: u64, method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.into(),
            params,
        }
    }
}

/// JSON-RPC 2.0 응답
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC 에러
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[allow(dead_code)]
impl JsonRpcError {
    pub fn parse_error() -> Self {
        Self {
            code: -32700,
            message: "Parse error".to_string(),
            data: None,
        }
    }

    pub fn invalid_request() -> Self {
        Self {
            code: -32600,
            message: "Invalid Request".to_string(),
            data: None,
        }
    }

    pub fn method_not_found() -> Self {
        Self {
            code: -32601,
            message: "Method not found".to_string(),
            data: None,
        }
    }

    pub fn internal_error(msg: impl Into<String>) -> Self {
        Self {
            code: -32603,
            message: msg.into(),
            data: None,
        }
    }
}

/// JSON-RPC 알림 (응답 없음)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// MCP Transport trait
#[async_trait]
pub trait McpTransport: Send + Sync {
    /// 요청 전송 및 응답 수신
    async fn request(&self, method: &str, params: Option<Value>) -> Result<Value>;

    /// 알림 전송 (응답 없음)
    async fn notify(&self, method: &str, params: Option<Value>) -> Result<()>;

    /// 연결 종료
    async fn close(&self) -> Result<()>;

    /// 연결 상태 확인
    fn is_connected(&self) -> bool;
}

/// Stdio Transport - 프로세스 기반 통신
pub struct StdioTransport {
    /// 요청 ID 카운터
    request_id: AtomicU64,

    /// 자식 프로세스
    child: Arc<Mutex<Option<Child>>>,

    /// stdin writer
    stdin_tx: mpsc::Sender<String>,

    /// 대기 중인 요청들 (id -> response sender)
    pending_requests: Arc<RwLock<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>>,

    /// 연결 상태
    connected: Arc<std::sync::atomic::AtomicBool>,
}

impl StdioTransport {
    /// 새 stdio transport 생성 및 프로세스 시작
    pub async fn spawn(
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
    ) -> Result<Self> {
        info!("Spawning MCP process: {} {:?}", command, args);

        let mut cmd = Command::new(command);
        cmd.args(args)
            .envs(env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| {
            Error::Internal(format!("Failed to spawn MCP process '{}': {}", command, e))
        })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| Error::Internal("Failed to capture stdin".to_string()))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::Internal("Failed to capture stdout".to_string()))?;

        // 요청 전송용 채널
        let (stdin_tx, mut stdin_rx) = mpsc::channel::<String>(32);

        // 대기 중인 요청
        let pending_requests: Arc<RwLock<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>> =
            Arc::new(RwLock::new(HashMap::new()));
        let pending_for_reader = Arc::clone(&pending_requests);

        let connected = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let connected_for_writer = Arc::clone(&connected);
        let connected_for_reader = Arc::clone(&connected);

        // stdin writer task
        let mut stdin_writer = stdin;
        tokio::spawn(async move {
            while let Some(msg) = stdin_rx.recv().await {
                if let Err(e) = stdin_writer.write_all(msg.as_bytes()).await {
                    error!("Failed to write to stdin: {}", e);
                    connected_for_writer.store(false, Ordering::SeqCst);
                    break;
                }
                if let Err(e) = stdin_writer.flush().await {
                    error!("Failed to flush stdin: {}", e);
                    connected_for_writer.store(false, Ordering::SeqCst);
                    break;
                }
            }
        });

        // stdout reader task
        let mut reader = BufReader::new(stdout).lines();
        tokio::spawn(async move {
            while let Ok(Some(line)) = reader.next_line().await {
                debug!("MCP stdout: {}", line);

                // JSON-RPC 응답 파싱
                match serde_json::from_str::<JsonRpcResponse>(&line) {
                    Ok(response) => {
                        if let Some(id) = response.id {
                            let mut pending = pending_for_reader.write().await;
                            if let Some(sender) = pending.remove(&id) {
                                let _ = sender.send(response);
                            }
                        }
                    }
                    Err(e) => {
                        debug!("Non-JSON-RPC line or parse error: {}", e);
                    }
                }
            }
            connected_for_reader.store(false, Ordering::SeqCst);
            info!("MCP stdout reader finished");
        });

        Ok(Self {
            request_id: AtomicU64::new(1),
            child: Arc::new(Mutex::new(Some(child))),
            stdin_tx,
            pending_requests,
            connected,
        })
    }

    /// 다음 요청 ID 생성
    fn next_id(&self) -> u64 {
        self.request_id.fetch_add(1, Ordering::SeqCst)
    }
}

#[async_trait]
impl McpTransport for StdioTransport {
    async fn request(&self, method: &str, params: Option<Value>) -> Result<Value> {
        if !self.is_connected() {
            return Err(Error::Internal("MCP transport not connected".to_string()));
        }

        let id = self.next_id();
        let request = JsonRpcRequest::new(id, method, params);

        // 응답 수신 채널 생성
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending_requests.write().await;
            pending.insert(id, tx);
        }

        // 요청 전송
        let msg = serde_json::to_string(&request)
            .map_err(|e| Error::Internal(format!("Failed to serialize request: {}", e)))?;

        debug!("Sending MCP request: {}", msg);

        self.stdin_tx
            .send(format!("{}\n", msg))
            .await
            .map_err(|e| Error::Internal(format!("Failed to send request: {}", e)))?;

        // 응답 대기 (타임아웃 30초)
        let response = tokio::time::timeout(std::time::Duration::from_secs(30), rx)
            .await
            .map_err(|_| Error::Internal("MCP request timeout".to_string()))?
            .map_err(|_| Error::Internal("MCP response channel closed".to_string()))?;

        // 에러 확인
        if let Some(error) = response.error {
            return Err(Error::Internal(format!(
                "MCP error {}: {}",
                error.code, error.message
            )));
        }

        Ok(response.result.unwrap_or(Value::Null))
    }

    async fn notify(&self, method: &str, params: Option<Value>) -> Result<()> {
        if !self.is_connected() {
            return Err(Error::Internal("MCP transport not connected".to_string()));
        }

        let notification = JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params,
        };

        let msg = serde_json::to_string(&notification)
            .map_err(|e| Error::Internal(format!("Failed to serialize notification: {}", e)))?;

        self.stdin_tx
            .send(format!("{}\n", msg))
            .await
            .map_err(|e| Error::Internal(format!("Failed to send notification: {}", e)))?;

        Ok(())
    }

    async fn close(&self) -> Result<()> {
        self.connected.store(false, Ordering::SeqCst);

        // 프로세스 종료
        let mut child_guard = self.child.lock().await;
        if let Some(mut child) = child_guard.take() {
            // 정상 종료 시도
            let _ = child.kill().await;
        }

        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }
}

/// SSE Transport - HTTP Server-Sent Events 기반 통신
#[allow(dead_code)]
pub struct SseTransport {
    /// 서버 URL
    url: String,

    /// 요청 ID 카운터
    request_id: AtomicU64,

    /// HTTP 클라이언트
    client: reqwest::Client,

    /// 대기 중인 요청들
    pending_requests: Arc<RwLock<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>>,

    /// 연결 상태
    connected: Arc<std::sync::atomic::AtomicBool>,

    /// 메시지 엔드포인트 URL
    message_url: String,
}

impl SseTransport {
    /// SSE 연결 생성
    pub async fn connect(url: &str) -> Result<Self> {
        info!("Connecting to MCP SSE server: {}", url);

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| Error::Internal(format!("Failed to create HTTP client: {}", e)))?;

        let pending_requests: Arc<RwLock<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>> =
            Arc::new(RwLock::new(HashMap::new()));

        let connected = Arc::new(std::sync::atomic::AtomicBool::new(true));

        // SSE endpoint에 연결 후 message endpoint URL 획득
        // MCP SSE는 /sse에 연결하면 /messages endpoint를 알려줌
        let message_url = format!(
            "{}/messages",
            url.trim_end_matches("/sse").trim_end_matches('/')
        );

        // SSE 이벤트 수신 태스크 시작
        let pending_for_sse = Arc::clone(&pending_requests);
        let connected_for_sse = Arc::clone(&connected);
        let sse_url = url.to_string();
        let client_clone = client.clone();

        tokio::spawn(async move {
            Self::sse_listener(sse_url, client_clone, pending_for_sse, connected_for_sse).await;
        });

        Ok(Self {
            url: url.to_string(),
            request_id: AtomicU64::new(1),
            client,
            pending_requests,
            connected,
            message_url,
        })
    }

    /// SSE 이벤트 수신 루프
    async fn sse_listener(
        url: String,
        client: reqwest::Client,
        pending: Arc<RwLock<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>>,
        connected: Arc<std::sync::atomic::AtomicBool>,
    ) {
        use reqwest_eventsource::{Event, EventSource};

        let mut es = EventSource::new(client.get(&url)).expect("Failed to create EventSource");

        while let Some(event) = es.next().await {
            match event {
                Ok(Event::Open) => {
                    info!("SSE connection opened");
                }
                Ok(Event::Message(message)) => {
                    debug!("SSE message: {}", message.data);

                    // JSON-RPC 응답 파싱
                    match serde_json::from_str::<JsonRpcResponse>(&message.data) {
                        Ok(response) => {
                            if let Some(id) = response.id {
                                let mut pending_guard = pending.write().await;
                                if let Some(sender) = pending_guard.remove(&id) {
                                    let _ = sender.send(response);
                                }
                            }
                        }
                        Err(e) => {
                            debug!("Failed to parse SSE message: {}", e);
                        }
                    }
                }
                Err(e) => {
                    error!("SSE error: {}", e);
                    connected.store(false, Ordering::SeqCst);
                    break;
                }
            }
        }

        connected.store(false, Ordering::SeqCst);
        info!("SSE connection closed");
    }

    /// 다음 요청 ID 생성
    fn next_id(&self) -> u64 {
        self.request_id.fetch_add(1, Ordering::SeqCst)
    }
}

use futures::StreamExt;

#[async_trait]
impl McpTransport for SseTransport {
    async fn request(&self, method: &str, params: Option<Value>) -> Result<Value> {
        if !self.is_connected() {
            return Err(Error::Internal(
                "MCP SSE transport not connected".to_string(),
            ));
        }

        let id = self.next_id();
        let request = JsonRpcRequest::new(id, method, params);

        // 응답 수신 채널 생성
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending_requests.write().await;
            pending.insert(id, tx);
        }

        // POST 요청으로 메시지 전송
        let response = self
            .client
            .post(&self.message_url)
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::Internal(format!("Failed to send request: {}", e)))?;

        if !response.status().is_success() {
            return Err(Error::Internal(format!(
                "HTTP error: {}",
                response.status()
            )));
        }

        // SSE로 응답 대기 (타임아웃 30초)
        let response = tokio::time::timeout(std::time::Duration::from_secs(30), rx)
            .await
            .map_err(|_| Error::Internal("MCP request timeout".to_string()))?
            .map_err(|_| Error::Internal("MCP response channel closed".to_string()))?;

        // 에러 확인
        if let Some(error) = response.error {
            return Err(Error::Internal(format!(
                "MCP error {}: {}",
                error.code, error.message
            )));
        }

        Ok(response.result.unwrap_or(Value::Null))
    }

    async fn notify(&self, method: &str, params: Option<Value>) -> Result<()> {
        if !self.is_connected() {
            return Err(Error::Internal(
                "MCP SSE transport not connected".to_string(),
            ));
        }

        let notification = JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params,
        };

        self.client
            .post(&self.message_url)
            .json(&notification)
            .send()
            .await
            .map_err(|e| Error::Internal(format!("Failed to send notification: {}", e)))?;

        Ok(())
    }

    async fn close(&self) -> Result<()> {
        self.connected.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_rpc_request() {
        let request =
            JsonRpcRequest::new(1, "test/method", Some(serde_json::json!({"key": "value"})));
        assert_eq!(request.jsonrpc, "2.0");
        assert_eq!(request.id, 1);
        assert_eq!(request.method, "test/method");
    }

    #[test]
    fn test_json_rpc_error() {
        let error = JsonRpcError::parse_error();
        assert_eq!(error.code, -32700);

        let error = JsonRpcError::method_not_found();
        assert_eq!(error.code, -32601);
    }
}
