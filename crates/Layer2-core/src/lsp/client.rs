//! LSP Client - 경량 Language Server 클라이언트
//!
//! JSON-RPC 2.0 over stdio로 LSP 서버와 통신
//! 최소 기능만 구현: definition, references, hover

use super::types::*;
use forge_foundation::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{mpsc, oneshot, Mutex, RwLock};
use tracing::{debug, error, trace, warn};

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
    /// 크래시됨 (재시작 필요)
    Crashed,
    /// 재시작 중
    Restarting,
}

/// LSP 재시작 설정
#[derive(Debug, Clone)]
pub struct LspRestartConfig {
    /// 자동 재시작 활성화
    pub auto_restart: bool,
    /// 최대 재시작 횟수
    pub max_restarts: u32,
    /// 재시작 간격 (밀리초)
    pub restart_delay_ms: u64,
    /// 재시작 횟수 리셋 시간 (초) - 이 시간 동안 안정적이면 카운터 리셋
    pub restart_count_reset_secs: u64,
}

impl Default for LspRestartConfig {
    fn default() -> Self {
        Self {
            auto_restart: true,
            max_restarts: 3,
            restart_delay_ms: 1000,
            restart_count_reset_secs: 300, // 5분
        }
    }
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

    /// 재시작 설정
    restart_config: LspRestartConfig,

    /// 현재 상태
    state: RwLock<LspClientState>,

    /// 루트 경로
    root_path: RwLock<Option<String>>,

    /// 요청 ID 카운터
    request_id: AtomicU64,

    /// 서버 프로세스
    process: Mutex<Option<Child>>,

    /// 메시지 전송 채널
    message_tx: Mutex<Option<mpsc::Sender<String>>>,

    /// 대기 중인 응답들 (Arc로 공유)
    pending_responses: Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value>>>>>,

    /// 서버 기능 (capabilities)
    server_capabilities: RwLock<Option<Value>>,

    /// 재시작 횟수
    restart_count: AtomicU64,

    /// 마지막 성공적인 요청 시간
    last_successful_request: RwLock<Option<std::time::Instant>>,

    /// 프로세스 상태 모니터 핸들
    process_monitor_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl LspClient {
    /// 새 클라이언트 생성 (서버는 아직 시작하지 않음)
    pub fn new(config: LspServerConfig) -> Self {
        Self {
            language_id: config.language_id.clone(),
            config,
            restart_config: LspRestartConfig::default(),
            state: RwLock::new(LspClientState::NotInitialized),
            root_path: RwLock::new(None),
            request_id: AtomicU64::new(0),
            process: Mutex::new(None),
            message_tx: Mutex::new(None),
            pending_responses: Arc::new(Mutex::new(HashMap::new())),
            server_capabilities: RwLock::new(None),
            restart_count: AtomicU64::new(0),
            last_successful_request: RwLock::new(None),
            process_monitor_handle: Mutex::new(None),
        }
    }

    /// 새 클라이언트 생성 (재시작 설정 포함)
    pub fn with_restart_config(config: LspServerConfig, restart_config: LspRestartConfig) -> Self {
        Self {
            language_id: config.language_id.clone(),
            config,
            restart_config,
            state: RwLock::new(LspClientState::NotInitialized),
            root_path: RwLock::new(None),
            request_id: AtomicU64::new(0),
            process: Mutex::new(None),
            message_tx: Mutex::new(None),
            pending_responses: Arc::new(Mutex::new(HashMap::new())),
            server_capabilities: RwLock::new(None),
            restart_count: AtomicU64::new(0),
            last_successful_request: RwLock::new(None),
            process_monitor_handle: Mutex::new(None),
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

        // 프로세스 시작 (tokio::process::Command 사용)
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
        *self.root_path.write().await = Some(root_path.to_string_lossy().to_string());

        // 메시지 전송용 채널 생성
        let (message_tx, message_rx) = mpsc::channel::<String>(32);
        *self.message_tx.lock().await = Some(message_tx);

        // stdin writer 태스크 시작
        self.start_writer(stdin, message_rx);

        // stdout reader 태스크 시작
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
                    "hover": { "contentFormat": ["markdown", "plaintext"] },
                    "synchronization": {
                        "didOpen": true,
                        "didClose": true,
                        "didChange": true
                    }
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
        self.send_notification("initialized", Some(json!({})))
            .await?;

        *self.state.write().await = LspClientState::Ready;
        *self.last_successful_request.write().await = Some(std::time::Instant::now());

        debug!(
            "LSP server {} initialized for {}",
            self.config.command, self.language_id
        );

        Ok(())
    }

    /// 서버 크래시 감지 및 처리
    async fn handle_crash(&self) {
        let current_state = *self.state.read().await;
        if current_state == LspClientState::Shutdown
            || current_state == LspClientState::ShuttingDown
        {
            return;
        }

        warn!("LSP server {} crashed", self.config.command);
        *self.state.write().await = LspClientState::Crashed;

        // 모든 대기 중인 요청에 에러 전송
        let mut pending = self.pending_responses.lock().await;
        for (id, sender) in pending.drain() {
            let _ = sender.send(Err(forge_foundation::Error::Internal(format!(
                "LSP server crashed (request {})",
                id
            ))));
        }

        // 프로세스 정리
        if let Some(mut process) = self.process.lock().await.take() {
            let _ = process.kill().await;
        }
        *self.message_tx.lock().await = None;
    }

    /// 자동 재시작 시도
    pub async fn try_restart(&self) -> Result<()> {
        if !self.restart_config.auto_restart {
            return Err(forge_foundation::Error::Internal(
                "Auto restart is disabled".to_string(),
            ));
        }

        let current_count = self.restart_count.load(Ordering::SeqCst);

        // 재시작 횟수 리셋 체크 (일정 시간 안정적이었으면)
        if let Some(last_success) = *self.last_successful_request.read().await {
            if last_success.elapsed().as_secs() >= self.restart_config.restart_count_reset_secs {
                self.restart_count.store(0, Ordering::SeqCst);
            }
        }

        if current_count >= self.restart_config.max_restarts as u64 {
            return Err(forge_foundation::Error::Internal(format!(
                "Max restart attempts ({}) reached for LSP server {}",
                self.restart_config.max_restarts, self.config.command
            )));
        }

        *self.state.write().await = LspClientState::Restarting;
        self.restart_count.fetch_add(1, Ordering::SeqCst);

        // 재시작 딜레이
        tokio::time::sleep(std::time::Duration::from_millis(
            self.restart_config.restart_delay_ms,
        ))
        .await;

        // 루트 경로 가져오기
        let root_path = self.root_path.read().await.clone();
        let root_path = match root_path {
            Some(p) => p,
            None => {
                *self.state.write().await = LspClientState::Crashed;
                return Err(forge_foundation::Error::Internal(
                    "No root path for restart".to_string(),
                ));
            }
        };

        // 상태 리셋
        *self.state.write().await = LspClientState::NotInitialized;

        // 재시작
        match self.start(Path::new(&root_path)).await {
            Ok(()) => {
                warn!(
                    "LSP server {} restarted successfully (attempt {})",
                    self.config.command,
                    self.restart_count.load(Ordering::SeqCst)
                );
                Ok(())
            }
            Err(e) => {
                *self.state.write().await = LspClientState::Crashed;
                Err(e)
            }
        }
    }

    /// 재시작 횟수 조회
    pub fn restart_count(&self) -> u64 {
        self.restart_count.load(Ordering::SeqCst)
    }

    /// 재시작 횟수 리셋
    pub fn reset_restart_count(&self) {
        self.restart_count.store(0, Ordering::SeqCst);
    }

    /// 서버가 정상 상태인지 확인
    pub async fn is_healthy(&self) -> bool {
        matches!(*self.state.read().await, LspClientState::Ready)
    }

    /// 크래시된 상태인지 확인
    pub async fn is_crashed(&self) -> bool {
        matches!(*self.state.read().await, LspClientState::Crashed)
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

        // 메시지 채널 닫기
        *self.message_tx.lock().await = None;

        // 프로세스 종료
        if let Some(mut process) = self.process.lock().await.take() {
            let _ = process.kill().await;
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

        let result = self
            .send_request("textDocument/definition", Some(params))
            .await?;
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

        let result = self
            .send_request("textDocument/references", Some(params))
            .await?;
        self.parse_locations(result)
    }

    /// 호버 정보
    pub async fn hover(&self, uri: &str, position: Position) -> Result<Option<Hover>> {
        self.ensure_ready().await?;

        let params = json!({
            "textDocument": { "uri": uri },
            "position": { "line": position.line, "character": position.character }
        });

        let result = self
            .send_request("textDocument/hover", Some(params))
            .await?;

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

        self.send_notification("textDocument/didOpen", Some(params))
            .await
    }

    /// 문서 변경
    pub async fn did_change(&self, uri: &str, version: i32, content: &str) -> Result<()> {
        self.ensure_ready().await?;

        let params = json!({
            "textDocument": {
                "uri": uri,
                "version": version
            },
            "contentChanges": [{ "text": content }]
        });

        self.send_notification("textDocument/didChange", Some(params))
            .await
    }

    /// 문서 닫기
    pub async fn did_close(&self, uri: &str) -> Result<()> {
        self.ensure_ready().await?;

        let params = json!({
            "textDocument": { "uri": uri }
        });

        self.send_notification("textDocument/didClose", Some(params))
            .await
    }

    // ========================================================================
    // 내부 메서드
    // ========================================================================

    /// 서버가 설치되어 있는지 확인
    fn is_server_available(&self) -> bool {
        which::which(&self.config.command).is_ok()
    }

    /// Ready 상태인지 확인 (크래시 시 자동 재시작 시도)
    async fn ensure_ready(&self) -> Result<()> {
        let state = *self.state.read().await;

        match state {
            LspClientState::Ready => Ok(()),
            LspClientState::Crashed => {
                // 자동 재시작 시도
                if self.restart_config.auto_restart {
                    warn!(
                        "LSP server {} is crashed, attempting restart",
                        self.config.command
                    );
                    self.try_restart().await?;
                    Ok(())
                } else {
                    Err(forge_foundation::Error::Internal(
                        "LSP server crashed and auto-restart is disabled".to_string(),
                    ))
                }
            }
            LspClientState::Restarting => {
                // 재시작 중이면 잠시 대기 후 재확인
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                if *self.state.read().await == LspClientState::Ready {
                    Ok(())
                } else {
                    Err(forge_foundation::Error::Internal(
                        "LSP server is restarting".to_string(),
                    ))
                }
            }
            _ => Err(forge_foundation::Error::Internal(format!(
                "LSP server not ready (state: {:?})",
                state
            ))),
        }
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
        self.write_message(&serde_json::to_string(&request)?)
            .await?;

        trace!("LSP request {} -> {}", id, method);

        // 응답 대기 (타임아웃 30초)
        match tokio::time::timeout(std::time::Duration::from_secs(30), rx).await {
            Ok(Ok(result)) => {
                // 성공 시 마지막 성공 시간 업데이트
                *self.last_successful_request.write().await = Some(std::time::Instant::now());
                result
            }
            Ok(Err(_)) => {
                // 채널이 닫힘 - 서버 크래시 가능성
                self.handle_crash().await;
                Err(forge_foundation::Error::Internal(
                    "Response channel closed (server may have crashed)".to_string(),
                ))
            }
            Err(_) => {
                // 타임아웃 - pending에서 제거
                self.pending_responses.lock().await.remove(&id);
                Err(forge_foundation::Error::Internal(
                    "LSP request timeout".to_string(),
                ))
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

        self.write_message(&serde_json::to_string(&notification)?)
            .await?;
        trace!("LSP notification -> {}", method);
        Ok(())
    }

    /// LSP 메시지 쓰기 (메시지 채널을 통해)
    async fn write_message(&self, body: &str) -> Result<()> {
        let tx = self.message_tx.lock().await;
        let tx = tx.as_ref().ok_or_else(|| {
            forge_foundation::Error::Internal("LSP message channel not available".to_string())
        })?;

        let message = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);

        tx.send(message).await.map_err(|e| {
            forge_foundation::Error::Internal(format!("Failed to send LSP message: {}", e))
        })?;

        Ok(())
    }

    /// stdin writer 시작 (tokio 태스크)
    fn start_writer(&self, mut stdin: ChildStdin, mut rx: mpsc::Receiver<String>) {
        tokio::spawn(async move {
            while let Some(message) = rx.recv().await {
                if let Err(e) = stdin.write_all(message.as_bytes()).await {
                    error!("Failed to write to LSP stdin: {}", e);
                    break;
                }
                if let Err(e) = stdin.flush().await {
                    error!("Failed to flush LSP stdin: {}", e);
                    break;
                }
            }
            debug!("LSP stdin writer finished");
        });
    }

    /// stdout reader 시작 (tokio 태스크)
    fn start_reader(&self, stdout: tokio::process::ChildStdout) {
        let pending = Arc::clone(&self.pending_responses);
        let language_id = self.language_id.clone();

        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);

            loop {
                // 헤더 읽기 (빈 줄까지)
                let mut content_length: usize = 0;
                loop {
                    let mut line = String::new();
                    match reader.read_line(&mut line).await {
                        Ok(0) => {
                            // EOF - 서버 종료됨
                            warn!("LSP stdout EOF for {} - server terminated", language_id);
                            // 대기 중인 모든 요청에 에러 전송
                            let mut pending_guard = pending.lock().await;
                            for (id, sender) in pending_guard.drain() {
                                let _ = sender.send(Err(forge_foundation::Error::Internal(
                                    format!("LSP server terminated (request {})", id),
                                )));
                            }
                            return;
                        }
                        Ok(_) => {
                            if line == "\r\n" || line == "\n" {
                                break;
                            }
                            // Content-Length 파싱
                            if line.to_lowercase().starts_with("content-length:") {
                                if let Some(len_str) = line.split(':').nth(1) {
                                    content_length = len_str.trim().parse().unwrap_or(0);
                                }
                            }
                        }
                        Err(e) => {
                            error!("Failed to read LSP header for {}: {}", language_id, e);
                            // 대기 중인 모든 요청에 에러 전송
                            let mut pending_guard = pending.lock().await;
                            for (id, sender) in pending_guard.drain() {
                                let _ = sender.send(Err(forge_foundation::Error::Internal(
                                    format!("LSP read error (request {}): {}", id, e),
                                )));
                            }
                            return;
                        }
                    }
                }

                if content_length == 0 {
                    continue;
                }

                // 본문 읽기
                let mut body = vec![0u8; content_length];
                if let Err(e) = tokio::io::AsyncReadExt::read_exact(&mut reader, &mut body).await {
                    error!("Failed to read LSP body for {}: {}", language_id, e);
                    // 대기 중인 모든 요청에 에러 전송
                    let mut pending_guard = pending.lock().await;
                    for (id, sender) in pending_guard.drain() {
                        let _ = sender.send(Err(forge_foundation::Error::Internal(format!(
                            "LSP read error (request {}): {}",
                            id, e
                        ))));
                    }
                    return;
                }

                // JSON 파싱
                match serde_json::from_slice::<JsonRpcResponse>(&body) {
                    Ok(response) => {
                        if let Some(id) = response.id {
                            trace!("LSP response {} received for {}", id, language_id);

                            // pending에서 sender 찾아서 응답 전송
                            let mut pending_guard = pending.lock().await;
                            if let Some(sender) = pending_guard.remove(&id) {
                                let result = if let Some(error) = response.error {
                                    Err(forge_foundation::Error::Internal(format!(
                                        "LSP error {}: {}",
                                        error.code, error.message
                                    )))
                                } else {
                                    Ok(response.result.unwrap_or(Value::Null))
                                };
                                let _ = sender.send(result);
                            }
                        } else {
                            // 알림 (id 없음) - 현재는 무시
                            trace!("LSP notification received for {}", language_id);
                        }
                    }
                    Err(e) => {
                        // JSON 파싱 실패 - 로그만 남김
                        let body_str = String::from_utf8_lossy(&body);
                        trace!("Failed to parse LSP response: {} - body: {}", e, body_str);
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
        // 프로세스 종료는 shutdown()에서 처리
        // Drop에서는 async 작업 불가
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
