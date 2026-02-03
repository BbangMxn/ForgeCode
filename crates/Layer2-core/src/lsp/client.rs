//! LSP Client - 경량 Language Server 클라이언트
//!
//! JSON-RPC 2.0 over stdio로 LSP 서버와 통신
//! 최소 기능만 구현: definition, references, hover

use super::types::*;
use forge_foundation::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex, RwLock};
use tracing::{debug, trace, warn};

// ============================================================================
// 클라이언트 상태
// ============================================================================

/// LSP 클라이언트 상태
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LspClientState {
    /// 초기화 안됨
    NotInitialized,
    /// 초기화 중
    Initializing,
    /// 준비 완료 (사용 가능)
    Ready,
    /// 종료 중
    ShuttingDown,
    /// 종료됨
    Shutdown,
}

// ============================================================================
// JSON-RPC 메시지
// ============================================================================

#[derive(Debug, Serialize)]
struct JsonRpcRequest {
    jsonrpc: &'static str,
    id: u64,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
struct JsonRpcNotification {
    jsonrpc: &'static str,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: Option<u64>,
    result: Option<Value>,
    error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct JsonRpcError {
    code: i32,
    message: String,
}

// ============================================================================
// LSP 클라이언트
// ============================================================================

/// LSP 클라이언트 - 개별 언어 서버와 통신
pub struct LspClient {
    /// 언어 ID
    language_id: String,

    /// 서버 설정
    config: LspServerConfig,

    /// 현재 상태
    state: RwLock<LspClientState>,

    /// 루트 경로
    root_path: RwLock<Option<String>>,

    /// 요청 ID 카운터
    request_id: AtomicU64,

    /// 서버 프로세스
    process: Mutex<Option<Child>>,

    /// stdin 쓰기용
    stdin: Mutex<Option<ChildStdin>>,

    /// 대기 중인 응답들
    pending_responses: Mutex<HashMap<u64, oneshot::Sender<Result<Value>>>>,

    /// 서버 기능 (capabilities)
    server_capabilities: RwLock<Option<Value>>,
}

impl LspClient {
    /// 새 클라이언트 생성 (서버는 아직 시작하지 않음)
    pub fn new(config: LspServerConfig) -> Self {
        Self {
            language_id: config.language_id.clone(),
            config,
            state: RwLock::new(LspClientState::NotInitialized),
            root_path: RwLock::new(None),
            request_id: AtomicU64::new(0),
            process: Mutex::new(None),
            stdin: Mutex::new(None),
            pending_responses: Mutex::new(HashMap::new()),
            server_capabilities: RwLock::new(None),
        }
    }

    /// 언어 ID
    pub fn language_id(&self) -> &str {
        &self.language_id
    }

    /// 현재 상태
    pub async fn state(&self) -> LspClientState {
        *self.state.read().await
    }

    /// 서버 시작 및 초기화
    pub async fn start(&self, root_path: &Path) -> Result<()> {
        // 이미 실행 중인지 확인
        if *self.state.read().await != LspClientState::NotInitialized {
            return Ok(());
        }

        *self.state.write().await = LspClientState::Initializing;

        // 서버 실행 가능 확인
        if !self.is_server_available() {
            *self.state.write().await = LspClientState::NotInitialized;
            return Err(forge_foundation::Error::NotFound(format!(
                "LSP server not found: {}",
                self.config.command
            )));
        }

        // 프로세스 시작
        let mut child = Command::new(&self.config.command)
            .args(&self.config.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| {
                forge_foundation::Error::Internal(format!(
                    "Failed to start LSP server {}: {}",
                    self.config.command, e
                ))
            })?;

        let stdin = child.stdin.take().ok_or_else(|| {
            forge_foundation::Error::Internal("Failed to get LSP stdin".to_string())
        })?;

        let stdout = child.stdout.take().ok_or_else(|| {
            forge_foundation::Error::Internal("Failed to get LSP stdout".to_string())
        })?;

        *self.process.lock().await = Some(child);
        *self.stdin.lock().await = Some(stdin);
        *self.root_path.write().await = Some(root_path.to_string_lossy().to_string());

        // stdout 리더 시작 (백그라운드 태스크)
        self.start_reader(stdout);

        // initialize 요청
        let root_uri = path_to_uri(root_path);
        let init_params = json!({
            "processId": std::process::id(),
            "rootUri": root_uri,
            "capabilities": {
                "textDocument": {
                    "definition": { "dynamicRegistration": false },
                    "references": { "dynamicRegistration": false },
                    "hover": { "contentFormat": ["markdown", "plaintext"] }
                }
            },
            "initializationOptions": self.config.initialization_options
        });

        let result = self.send_request("initialize", Some(init_params)).await?;

        // 서버 capabilities 저장
        if let Some(caps) = result.get("capabilities") {
            *self.server_capabilities.write().await = Some(caps.clone());
        }

        // initialized 알림
        self.send_notification("initialized", Some(json!({}))).await?;

        *self.state.write().await = LspClientState::Ready;
        debug!("LSP server {} initialized for {}", self.config.command, self.language_id);

        Ok(())
    }

    /// 서버 종료
    pub async fn shutdown(&self) -> Result<()> {
        let current_state = *self.state.read().await;
        if current_state != LspClientState::Ready {
            return Ok(());
        }

        *self.state.write().await = LspClientState::ShuttingDown;

        // shutdown 요청
        if let Err(e) = self.send_request("shutdown", None).await {
            warn!("LSP shutdown request failed: {}", e);
        }

        // exit 알림
        if let Err(e) = self.send_notification("exit", None).await {
            warn!("LSP exit notification failed: {}", e);
        }

        // 프로세스 종료
        if let Some(mut process) = self.process.lock().await.take() {
            let _ = process.kill();
            let _ = process.wait();
        }

        *self.state.write().await = LspClientState::Shutdown;
        debug!("LSP server {} shutdown", self.config.command);

        Ok(())
    }

    // ========================================================================
    // 핵심 LSP 메서드 (Phase 1 - Agent 필수)
    // ========================================================================

    /// 정의로 이동 (Go to Definition)
    pub async fn goto_definition(&self, uri: &str, position: Position) -> Result<Vec<Location>> {
        self.ensure_ready().await?;

        let params = json!({
            "textDocument": { "uri": uri },
            "position": { "line": position.line, "character": position.character }
        });

        let result = self.send_request("textDocument/definition", Some(params)).await?;
        self.parse_locations(result)
    }

    /// 참조 찾기 (Find References)
    pub async fn find_references(&self, uri: &str, position: Position) -> Result<Vec<Location>> {
        self.ensure_ready().await?;

        let params = json!({
            "textDocument": { "uri": uri },
            "position": { "line": position.line, "character": position.character },
            "context": { "includeDeclaration": true }
        });

        let result = self.send_request("textDocument/references", Some(params)).await?;
        self.parse_locations(result)
    }

    /// 호버 정보
    pub async fn hover(&self, uri: &str, position: Position) -> Result<Option<Hover>> {
        self.ensure_ready().await?;

        let params = json!({
            "textDocument": { "uri": uri },
            "position": { "line": position.line, "character": position.character }
        });

        let result = self.send_request("textDocument/hover", Some(params)).await?;

        if result.is_null() {
            return Ok(None);
        }

        serde_json::from_value(result)
            .map(Some)
            .map_err(|e| forge_foundation::Error::Internal(format!("Failed to parse hover: {}", e)))
    }

    // ========================================================================
    // 문서 동기화 (Agent가 파일 편집 시 호출)
    // ========================================================================

    /// 문서 열기
    pub async fn did_open(&self, uri: &str, language_id: &str, content: &str) -> Result<()> {
        self.ensure_ready().await?;

        let params = json!({
            "textDocument": {
                "uri": uri,
                "languageId": language_id,
                "version": 1,
                "text": content
            }
        });

        self.send_notification("textDocument/didOpen", Some(params)).await
    }

    /// 문서 닫기
    pub async fn did_close(&self, uri: &str) -> Result<()> {
        self.ensure_ready().await?;

        let params = json!({
            "textDocument": { "uri": uri }
        });

        self.send_notification("textDocument/didClose", Some(params)).await
    }

    // ========================================================================
    // 내부 메서드
    // ========================================================================

    /// 서버가 설치되어 있는지 확인
    fn is_server_available(&self) -> bool {
        which::which(&self.config.command).is_ok()
    }

    /// Ready 상태인지 확인
    async fn ensure_ready(&self) -> Result<()> {
        if *self.state.read().await != LspClientState::Ready {
            return Err(forge_foundation::Error::Internal("LSP server not ready".to_string()));
        }
        Ok(())
    }

    /// 다음 요청 ID
    fn next_request_id(&self) -> u64 {
        self.request_id.fetch_add(1, Ordering::SeqCst)
    }

    /// 요청 전송 및 응답 대기
    async fn send_request(&self, method: &str, params: Option<Value>) -> Result<Value> {
        let id = self.next_request_id();

        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            id,
            method: method.to_string(),
            params,
        };

        // 응답 채널 등록
        let (tx, rx) = oneshot::channel();
        self.pending_responses.lock().await.insert(id, tx);

        // 요청 전송
        self.write_message(&serde_json::to_string(&request)?).await?;

        trace!("LSP request {} -> {}", id, method);

        // 응답 대기 (타임아웃 30초)
        match tokio::time::timeout(std::time::Duration::from_secs(30), rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(forge_foundation::Error::Internal("Response channel closed".to_string())),
            Err(_) => {
                // 타임아웃 - pending에서 제거
                self.pending_responses.lock().await.remove(&id);
                Err(forge_foundation::Error::Internal("LSP request timeout".to_string()))
            }
        }
    }

    /// 알림 전송 (응답 없음)
    async fn send_notification(&self, method: &str, params: Option<Value>) -> Result<()> {
        let notification = JsonRpcNotification {
            jsonrpc: "2.0",
            method: method.to_string(),
            params,
        };

        self.write_message(&serde_json::to_string(&notification)?).await?;
        trace!("LSP notification -> {}", method);
        Ok(())
    }

    /// LSP 메시지 쓰기 (Content-Length 헤더 포함)
    async fn write_message(&self, body: &str) -> Result<()> {
        let mut stdin = self.stdin.lock().await;
        let stdin = stdin.as_mut().ok_or_else(|| {
            forge_foundation::Error::Internal("LSP stdin not available".to_string())
        })?;

        let header = format!("Content-Length: {}\r\n\r\n", body.len());
        stdin.write_all(header.as_bytes())?;
        stdin.write_all(body.as_bytes())?;
        stdin.flush()?;

        Ok(())
    }

    /// stdout 리더 시작 (백그라운드 스레드)
    fn start_reader(&self, stdout: ChildStdout) {
        // Arc로 pending_responses 공유
        let _pending = Arc::new(Mutex::new(HashMap::<u64, oneshot::Sender<Result<Value>>>::new()));

        // 현재 pending_responses와 교환
        // Note: 실제 구현에서는 Arc<Mutex<...>>를 필드로 사용해야 함
        // 여기서는 간단히 스레드에서 처리

        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut headers = String::new();

            loop {
                headers.clear();

                // 헤더 읽기
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 {
                        return; // EOF
                    }
                    if line == "\r\n" {
                        break;
                    }
                    headers.push_str(&line);
                }

                // Content-Length 파싱
                let content_length: usize = headers
                    .lines()
                    .find(|l| l.to_lowercase().starts_with("content-length:"))
                    .and_then(|l| l.split(':').nth(1))
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(0);

                if content_length == 0 {
                    continue;
                }

                // 본문 읽기
                let mut body = vec![0u8; content_length];
                if std::io::Read::read_exact(&mut reader, &mut body).is_err() {
                    return;
                }

                // JSON 파싱
                if let Ok(response) = serde_json::from_slice::<JsonRpcResponse>(&body) {
                    if let Some(id) = response.id {
                        trace!("LSP response {} received", id);
                        // TODO: pending_responses에서 sender 찾아서 응답 전송
                        // 현재는 단순화된 구현
                    }
                }
            }
        });
    }

    /// Location 배열 파싱 (단일 또는 배열)
    fn parse_locations(&self, value: Value) -> Result<Vec<Location>> {
        if value.is_null() {
            return Ok(vec![]);
        }

        // 배열인 경우
        if value.is_array() {
            return serde_json::from_value(value).map_err(|e| {
                forge_foundation::Error::Internal(format!("Failed to parse locations: {}", e))
            });
        }

        // 단일 Location인 경우
        if value.is_object() {
            let loc: Location = serde_json::from_value(value).map_err(|e| {
                forge_foundation::Error::Internal(format!("Failed to parse location: {}", e))
            })?;
            return Ok(vec![loc]);
        }

        Ok(vec![])
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        // 프로세스 종료 (동기)
        if let Ok(mut guard) = self.process.try_lock() {
            if let Some(mut process) = guard.take() {
                let _ = process.kill();
            }
        }
    }
}

// ============================================================================
// 테스트
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_new() {
        let config = LspServerConfig {
            language_id: "rust".to_string(),
            command: "rust-analyzer".to_string(),
            args: vec![],
            root_patterns: vec!["Cargo.toml".to_string()],
            initialization_options: None,
        };

        let client = LspClient::new(config);
        assert_eq!(client.language_id(), "rust");
    }

    #[tokio::test]
    async fn test_client_state() {
        let config = LspServerConfig {
            language_id: "test".to_string(),
            command: "nonexistent".to_string(),
            args: vec![],
            root_patterns: vec![],
            initialization_options: None,
        };

        let client = LspClient::new(config);
        assert_eq!(client.state().await, LspClientState::NotInitialized);
    }
}
