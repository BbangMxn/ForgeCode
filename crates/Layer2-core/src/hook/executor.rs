//! Hook Executor - Hook 실행 엔진
//!
//! Hook 액션을 실행하고 결과를 반환합니다.
//! Prompt와 Agent 액션은 콜백을 통해 Layer3-agent에서 처리됩니다.

use super::types::{BlockReason, HookAction, HookConfig, HookEvent, HookOutcome, HookResult};
use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Instant;
use tokio::process::Command;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

// ============================================================================
// Prompt/Agent 액션 타입
// ============================================================================

/// Prompt 액션 요청
#[derive(Debug, Clone)]
pub struct PromptRequest {
    /// 프롬프트 내용
    pub prompt: String,
    /// 요청을 트리거한 Hook 이벤트
    pub source_event: HookEventSource,
    /// 응답 채널 (optional)
    pub response_tx: Option<mpsc::Sender<PromptResponse>>,
}

/// Prompt 액션 응답
#[derive(Debug, Clone)]
pub struct PromptResponse {
    /// 성공 여부
    pub success: bool,
    /// LLM 응답 내용
    pub content: Option<String>,
    /// 에러 메시지
    pub error: Option<String>,
    /// 처리 시간 (ms)
    pub duration_ms: u64,
}

impl PromptResponse {
    /// 성공 응답 생성
    pub fn success(content: String, duration_ms: u64) -> Self {
        Self {
            success: true,
            content: Some(content),
            error: None,
            duration_ms,
        }
    }

    /// 실패 응답 생성
    pub fn failure(error: String, duration_ms: u64) -> Self {
        Self {
            success: false,
            content: None,
            error: Some(error),
            duration_ms,
        }
    }
}

/// Agent 액션 요청
#[derive(Debug, Clone)]
pub struct AgentRequest {
    /// Agent 타입 (예: "explore", "bash", "plan")
    pub agent_type: String,
    /// 프롬프트 내용
    pub prompt: String,
    /// 최대 턴 수
    pub max_turns: u32,
    /// 요청을 트리거한 Hook 이벤트
    pub source_event: HookEventSource,
    /// 응답 채널 (optional)
    pub response_tx: Option<mpsc::Sender<AgentResponse>>,
}

/// Agent 액션 응답
#[derive(Debug, Clone)]
pub struct AgentResponse {
    /// 성공 여부
    pub success: bool,
    /// Agent 실행 결과
    pub result: Option<AgentResult>,
    /// 에러 메시지
    pub error: Option<String>,
    /// 처리 시간 (ms)
    pub duration_ms: u64,
}

/// Agent 실행 결과
#[derive(Debug, Clone)]
pub struct AgentResult {
    /// 최종 응답 내용
    pub content: String,
    /// 사용한 턴 수
    pub turns_used: u32,
    /// Agent ID
    pub agent_id: String,
    /// 생성/수정된 파일 목록
    pub affected_files: Vec<String>,
}

impl AgentResponse {
    /// 성공 응답 생성
    pub fn success(result: AgentResult, duration_ms: u64) -> Self {
        Self {
            success: true,
            result: Some(result),
            error: None,
            duration_ms,
        }
    }

    /// 실패 응답 생성
    pub fn failure(error: String, duration_ms: u64) -> Self {
        Self {
            success: false,
            result: None,
            error: Some(error),
            duration_ms,
        }
    }
}

/// Hook 이벤트 소스 정보 (요청 추적용)
#[derive(Debug, Clone)]
pub struct HookEventSource {
    /// 이벤트 타입
    pub event_type: String,
    /// 관련 Tool 이름
    pub tool_name: Option<String>,
    /// 세션 ID
    pub session_id: String,
}

impl From<(&HookEvent, &HookContext)> for HookEventSource {
    fn from((event, ctx): (&HookEvent, &HookContext)) -> Self {
        Self {
            event_type: event.event_type.to_string(),
            tool_name: event.tool_name.clone(),
            session_id: ctx.session_id.clone(),
        }
    }
}

// ============================================================================
// 콜백 타입
// ============================================================================

/// Prompt 콜백 타입 (비동기)
pub type PromptCallback = Arc<
    dyn Fn(PromptRequest) -> Pin<Box<dyn Future<Output = PromptResponse> + Send>> + Send + Sync,
>;

/// Agent 콜백 타입 (비동기)
pub type AgentCallback =
    Arc<dyn Fn(AgentRequest) -> Pin<Box<dyn Future<Output = AgentResponse> + Send>> + Send + Sync>;

/// Hook 액션 핸들러 설정
#[derive(Default, Clone)]
pub struct HookActionHandlers {
    /// Prompt 액션 콜백
    pub prompt_handler: Option<PromptCallback>,
    /// Agent 액션 콜백
    pub agent_handler: Option<AgentCallback>,
    /// Prompt 요청 채널 (fire-and-forget 모드용)
    pub prompt_tx: Option<mpsc::Sender<PromptRequest>>,
    /// Agent 요청 채널 (fire-and-forget 모드용)
    pub agent_tx: Option<mpsc::Sender<AgentRequest>>,
}

impl HookActionHandlers {
    /// 새 핸들러 설정 생성
    pub fn new() -> Self {
        Self::default()
    }

    /// Prompt 콜백 설정
    pub fn with_prompt_handler(mut self, handler: PromptCallback) -> Self {
        self.prompt_handler = Some(handler);
        self
    }

    /// Agent 콜백 설정
    pub fn with_agent_handler(mut self, handler: AgentCallback) -> Self {
        self.agent_handler = Some(handler);
        self
    }

    /// Prompt 채널 설정 (fire-and-forget)
    pub fn with_prompt_channel(mut self, tx: mpsc::Sender<PromptRequest>) -> Self {
        self.prompt_tx = Some(tx);
        self
    }

    /// Agent 채널 설정 (fire-and-forget)
    pub fn with_agent_channel(mut self, tx: mpsc::Sender<AgentRequest>) -> Self {
        self.agent_tx = Some(tx);
        self
    }
}

// ============================================================================
// HookContext - 실행 컨텍스트
// ============================================================================

/// Hook 실행 컨텍스트
pub struct HookContext {
    /// 작업 디렉토리
    pub working_dir: PathBuf,

    /// 세션 ID
    pub session_id: String,

    /// 환경 변수
    pub env: HashMap<String, String>,

    /// 타임아웃 배수 (기본 1.0)
    pub timeout_multiplier: f64,
}

impl HookContext {
    /// 새 컨텍스트 생성
    pub fn new(working_dir: impl Into<PathBuf>, session_id: impl Into<String>) -> Self {
        Self {
            working_dir: working_dir.into(),
            session_id: session_id.into(),
            env: std::env::vars().collect(),
            timeout_multiplier: 1.0,
        }
    }

    /// 환경 변수 추가
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// 타임아웃 배수 설정
    pub fn with_timeout_multiplier(mut self, multiplier: f64) -> Self {
        self.timeout_multiplier = multiplier;
        self
    }

    /// 이벤트 데이터로 환경 변수 설정
    fn setup_event_env(&self, event: &HookEvent) -> HashMap<String, String> {
        let mut env = self.env.clone();

        // 이벤트 타입
        env.insert("HOOK_EVENT_TYPE".to_string(), event.event_type.to_string());

        // Tool 관련
        if let Some(ref tool_name) = event.tool_name {
            env.insert("HOOK_TOOL_NAME".to_string(), tool_name.clone());
        }
        if let Some(ref tool_input) = event.tool_input {
            env.insert(
                "HOOK_TOOL_INPUT".to_string(),
                serde_json::to_string(tool_input).unwrap_or_default(),
            );
        }
        if let Some(ref tool_output) = event.tool_output {
            env.insert("HOOK_TOOL_OUTPUT".to_string(), tool_output.clone());
        }

        // 파일 경로
        if let Some(ref file_path) = event.file_path {
            env.insert("HOOK_FILE_PATH".to_string(), file_path.clone());
        }

        // 프롬프트
        if let Some(ref prompt) = event.prompt {
            env.insert("HOOK_PROMPT".to_string(), prompt.clone());
        }

        env
    }
}

// ============================================================================
// HookExecutor - Hook 실행기
// ============================================================================

/// Hook 실행기
pub struct HookExecutor {
    /// Hook 설정
    config: HookConfig,
    /// 액션 핸들러
    handlers: HookActionHandlers,
}

impl HookExecutor {
    /// 새 실행기 생성
    pub fn new(config: HookConfig) -> Self {
        Self {
            config,
            handlers: HookActionHandlers::default(),
        }
    }

    /// 핸들러와 함께 실행기 생성
    pub fn with_handlers(config: HookConfig, handlers: HookActionHandlers) -> Self {
        Self { config, handlers }
    }

    /// 설정 업데이트
    pub fn update_config(&mut self, config: HookConfig) {
        self.config = config;
    }

    /// 핸들러 업데이트
    pub fn update_handlers(&mut self, handlers: HookActionHandlers) {
        self.handlers = handlers;
    }

    /// 설정 참조
    pub fn config(&self) -> &HookConfig {
        &self.config
    }

    /// 핸들러 참조
    pub fn handlers(&self) -> &HookActionHandlers {
        &self.handlers
    }

    /// 이벤트에 대해 모든 매칭 Hook 실행
    ///
    /// PreToolUse의 경우 블로킹 액션이 실패하면 즉시 중단하고 Blocked 결과 반환
    pub async fn execute(&self, event: &HookEvent, ctx: &HookContext) -> Vec<HookResult> {
        let matchers = self.config.matchers_for(event.event_type);
        let mut results = Vec::new();

        for matcher in matchers {
            if !matcher.matches(event) {
                continue;
            }

            debug!(
                "Hook matcher '{}' matched for event {:?}",
                matcher.matcher, event.event_type
            );

            for action in &matcher.hooks {
                let result = self.execute_action(action, event, ctx).await;

                // PreToolUse에서 블로킹 액션이 실패하면 즉시 중단
                if matches!(result.outcome, HookOutcome::Blocked(_)) {
                    results.push(result);
                    return results;
                }

                results.push(result);
            }
        }

        results
    }

    /// 단일 액션 실행
    async fn execute_action(
        &self,
        action: &HookAction,
        event: &HookEvent,
        ctx: &HookContext,
    ) -> HookResult {
        let start = Instant::now();

        match action {
            HookAction::Command {
                command,
                timeout,
                blocking,
            } => {
                self.execute_command(command, *timeout, *blocking, event, ctx, start)
                    .await
            }
            HookAction::Prompt { prompt } => self.execute_prompt(prompt, event, ctx, start).await,
            HookAction::Agent {
                agent,
                prompt,
                max_turns,
            } => {
                self.execute_agent(agent, prompt, *max_turns, event, ctx, start)
                    .await
            }
            HookAction::Notify { message, level } => {
                match level.as_str() {
                    "error" => tracing::error!("Hook notify: {}", message),
                    "warn" => warn!("Hook notify: {}", message),
                    _ => info!("Hook notify: {}", message),
                }
                let duration = start.elapsed().as_millis() as u64;
                HookResult::success(message.clone(), duration)
            }
        }
    }

    /// Command 액션 실행
    async fn execute_command(
        &self,
        command: &str,
        timeout_secs: u64,
        blocking: bool,
        event: &HookEvent,
        ctx: &HookContext,
        start: Instant,
    ) -> HookResult {
        let env = ctx.setup_event_env(event);
        let timeout =
            std::time::Duration::from_secs((timeout_secs as f64 * ctx.timeout_multiplier) as u64);

        debug!("Executing hook command: {}", command);

        // Shell 명령 실행
        let shell = if cfg!(windows) { "cmd" } else { "sh" };
        let shell_arg = if cfg!(windows) { "/C" } else { "-c" };

        let result = tokio::time::timeout(timeout, async {
            Command::new(shell)
                .arg(shell_arg)
                .arg(command)
                .current_dir(&ctx.working_dir)
                .envs(&env)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .await
        })
        .await;

        let duration = start.elapsed().as_millis() as u64;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();

                if output.status.success() {
                    debug!("Hook command succeeded: {}", stdout.trim());
                    HookResult::success(stdout, duration)
                } else {
                    let error_msg = if stderr.is_empty() {
                        format!("Command failed with exit code: {:?}", output.status.code())
                    } else {
                        stderr
                    };

                    warn!("Hook command failed: {}", error_msg);

                    if blocking {
                        HookResult::blocked(
                            BlockReason::new("Command failed").with_details(error_msg),
                            duration,
                        )
                    } else {
                        HookResult::failure(error_msg, duration)
                    }
                }
            }
            Ok(Err(e)) => {
                let error_msg = format!("Failed to execute command: {}", e);
                warn!("{}", error_msg);

                if blocking {
                    HookResult::blocked(BlockReason::new(error_msg), duration)
                } else {
                    HookResult::failure(format!("Execution error: {}", e), duration)
                }
            }
            Err(_) => {
                let error_msg = format!("Command timed out after {}s", timeout_secs);
                warn!("{}", error_msg);

                if blocking {
                    HookResult::blocked(BlockReason::new(error_msg), duration)
                } else {
                    HookResult::failure("Timeout", duration)
                }
            }
        }
    }

    /// Prompt 액션 실행
    async fn execute_prompt(
        &self,
        prompt: &str,
        event: &HookEvent,
        ctx: &HookContext,
        start: Instant,
    ) -> HookResult {
        let source = HookEventSource::from((event, ctx));

        debug!("Executing hook prompt: {}", prompt);

        // 콜백이 설정된 경우 동기적으로 실행
        if let Some(ref handler) = self.handlers.prompt_handler {
            let request = PromptRequest {
                prompt: prompt.to_string(),
                source_event: source,
                response_tx: None,
            };

            let response = handler(request).await;
            let duration = start.elapsed().as_millis() as u64;

            if response.success {
                HookResult::success(
                    response
                        .content
                        .unwrap_or_else(|| "Prompt completed".to_string()),
                    duration,
                )
            } else {
                HookResult::failure(
                    response
                        .error
                        .unwrap_or_else(|| "Prompt failed".to_string()),
                    duration,
                )
            }
        }
        // 채널이 설정된 경우 fire-and-forget으로 전송
        else if let Some(ref tx) = self.handlers.prompt_tx {
            let request = PromptRequest {
                prompt: prompt.to_string(),
                source_event: source,
                response_tx: None,
            };

            match tx.try_send(request) {
                Ok(_) => {
                    info!("Hook prompt queued: {}", prompt);
                    let duration = start.elapsed().as_millis() as u64;
                    HookResult::success(format!("Prompt queued: {}", prompt), duration)
                }
                Err(e) => {
                    warn!("Failed to queue prompt: {}", e);
                    let duration = start.elapsed().as_millis() as u64;
                    HookResult::failure(format!("Failed to queue prompt: {}", e), duration)
                }
            }
        }
        // 핸들러가 설정되지 않은 경우 로깅만 수행
        else {
            info!("Hook prompt (no handler): {}", prompt);
            let duration = start.elapsed().as_millis() as u64;
            HookResult::success(format!("Prompt logged (no handler): {}", prompt), duration)
        }
    }

    /// Agent 액션 실행
    async fn execute_agent(
        &self,
        agent_type: &str,
        prompt: &str,
        max_turns: u32,
        event: &HookEvent,
        ctx: &HookContext,
        start: Instant,
    ) -> HookResult {
        let source = HookEventSource::from((event, ctx));

        debug!(
            "Executing hook agent '{}' (max_turns: {}): {}",
            agent_type, max_turns, prompt
        );

        // 콜백이 설정된 경우 동기적으로 실행
        if let Some(ref handler) = self.handlers.agent_handler {
            let request = AgentRequest {
                agent_type: agent_type.to_string(),
                prompt: prompt.to_string(),
                max_turns,
                source_event: source,
                response_tx: None,
            };

            let response = handler(request).await;
            let duration = start.elapsed().as_millis() as u64;

            if response.success {
                let result = response.result.unwrap();
                HookResult::success(
                    format!(
                        "Agent '{}' completed in {} turns: {}",
                        agent_type, result.turns_used, result.content
                    ),
                    duration,
                )
            } else {
                HookResult::failure(
                    response.error.unwrap_or_else(|| "Agent failed".to_string()),
                    duration,
                )
            }
        }
        // 채널이 설정된 경우 fire-and-forget으로 전송
        else if let Some(ref tx) = self.handlers.agent_tx {
            let request = AgentRequest {
                agent_type: agent_type.to_string(),
                prompt: prompt.to_string(),
                max_turns,
                source_event: source,
                response_tx: None,
            };

            match tx.try_send(request) {
                Ok(_) => {
                    info!(
                        "Hook agent '{}' queued (max_turns: {}): {}",
                        agent_type, max_turns, prompt
                    );
                    let duration = start.elapsed().as_millis() as u64;
                    HookResult::success(
                        format!("Agent '{}' queued with prompt: {}", agent_type, prompt),
                        duration,
                    )
                }
                Err(e) => {
                    warn!("Failed to queue agent: {}", e);
                    let duration = start.elapsed().as_millis() as u64;
                    HookResult::failure(format!("Failed to queue agent: {}", e), duration)
                }
            }
        }
        // 핸들러가 설정되지 않은 경우 로깅만 수행
        else {
            info!(
                "Hook agent '{}' (no handler, max_turns: {}): {}",
                agent_type, max_turns, prompt
            );
            let duration = start.elapsed().as_millis() as u64;
            HookResult::success(
                format!("Agent '{}' logged (no handler): {}", agent_type, prompt),
                duration,
            )
        }
    }

    /// PreToolUse Hook 실행 및 블로킹 여부 확인
    pub async fn check_pre_tool_use(
        &self,
        tool_name: &str,
        input: serde_json::Value,
        ctx: &HookContext,
    ) -> Result<(), BlockReason> {
        let event = HookEvent::pre_tool_use(tool_name, input);
        let results = self.execute(&event, ctx).await;

        for result in results {
            if let HookOutcome::Blocked(reason) = result.outcome {
                return Err(reason);
            }
        }

        Ok(())
    }

    /// PostToolUse Hook 실행
    pub async fn run_post_tool_use(
        &self,
        tool_name: &str,
        input: serde_json::Value,
        output: &str,
        ctx: &HookContext,
    ) -> Vec<HookResult> {
        let event = HookEvent::post_tool_use(tool_name, input, output);
        self.execute(&event, ctx).await
    }

    /// SessionStart Hook 실행
    pub async fn run_session_start(&self, ctx: &HookContext) -> Vec<HookResult> {
        let event = HookEvent::session_start();
        self.execute(&event, ctx).await
    }

    /// SessionStop Hook 실행
    pub async fn run_session_stop(&self, ctx: &HookContext) -> Vec<HookResult> {
        let event = HookEvent::session_stop();
        self.execute(&event, ctx).await
    }
}

impl Default for HookExecutor {
    fn default() -> Self {
        Self::new(HookConfig::default())
    }
}

// ============================================================================
// 테스트
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hook::types::HookMatcher;

    fn test_context() -> HookContext {
        HookContext::new(".", "test-session")
    }

    #[test]
    fn test_hook_context_env() {
        let ctx = HookContext::new(".", "test").with_env("CUSTOM_VAR", "value");

        assert!(ctx.env.contains_key("CUSTOM_VAR"));
    }

    #[test]
    fn test_hook_executor_empty() {
        let executor = HookExecutor::default();
        assert!(executor.config().is_empty());
    }

    #[tokio::test]
    async fn test_execute_notify_action() {
        let mut config = HookConfig::new();
        config
            .pre_tool_use
            .push(HookMatcher::new("*").with_action(HookAction::notify("Test message")));

        let executor = HookExecutor::new(config);
        let event = HookEvent::pre_tool_use("Bash", serde_json::json!({}));
        let ctx = test_context();

        let results = executor.execute(&event, &ctx).await;
        assert_eq!(results.len(), 1);
        assert!(results[0].success);
    }

    #[tokio::test]
    async fn test_execute_command_action() {
        let cmd = if cfg!(windows) {
            "echo test"
        } else {
            "echo test"
        };

        let mut config = HookConfig::new();
        config
            .pre_tool_use
            .push(HookMatcher::new("Bash").with_action(HookAction::command(cmd)));

        let executor = HookExecutor::new(config);
        let event = HookEvent::pre_tool_use("Bash", serde_json::json!({}));
        let ctx = test_context();

        let results = executor.execute(&event, &ctx).await;
        assert_eq!(results.len(), 1);
        assert!(results[0].success);
        assert!(results[0].output.as_ref().unwrap().contains("test"));
    }

    #[tokio::test]
    async fn test_no_match() {
        let mut config = HookConfig::new();
        config
            .pre_tool_use
            .push(HookMatcher::new("Read").with_action(HookAction::notify("Should not run")));

        let executor = HookExecutor::new(config);
        let event = HookEvent::pre_tool_use("Bash", serde_json::json!({}));
        let ctx = test_context();

        let results = executor.execute(&event, &ctx).await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_blocking_command_failure() {
        // 의도적으로 실패하는 명령
        let cmd = if cfg!(windows) { "exit /b 1" } else { "exit 1" };

        let mut config = HookConfig::new();
        config
            .pre_tool_use
            .push(HookMatcher::new("Bash").with_action(HookAction::blocking_command(cmd)));

        let executor = HookExecutor::new(config);
        let ctx = test_context();

        let result = executor
            .check_pre_tool_use("Bash", serde_json::json!({}), &ctx)
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_prompt_action_no_handler() {
        let mut config = HookConfig::new();
        config
            .pre_tool_use
            .push(HookMatcher::new("*").with_action(HookAction::Prompt {
                prompt: "Test prompt".to_string(),
            }));

        let executor = HookExecutor::new(config);
        let event = HookEvent::pre_tool_use("Bash", serde_json::json!({}));
        let ctx = test_context();

        let results = executor.execute(&event, &ctx).await;
        assert_eq!(results.len(), 1);
        assert!(results[0].success);
        assert!(results[0].output.as_ref().unwrap().contains("no handler"));
    }

    #[tokio::test]
    async fn test_prompt_action_with_handler() {
        let mut config = HookConfig::new();
        config
            .pre_tool_use
            .push(HookMatcher::new("*").with_action(HookAction::Prompt {
                prompt: "Test prompt".to_string(),
            }));

        // 콜백 핸들러 생성
        let handler: PromptCallback = Arc::new(|req| {
            Box::pin(async move { PromptResponse::success(format!("Handled: {}", req.prompt), 10) })
        });

        let handlers = HookActionHandlers::new().with_prompt_handler(handler);
        let executor = HookExecutor::with_handlers(config, handlers);
        let event = HookEvent::pre_tool_use("Bash", serde_json::json!({}));
        let ctx = test_context();

        let results = executor.execute(&event, &ctx).await;
        assert_eq!(results.len(), 1);
        assert!(results[0].success);
        assert!(results[0].output.as_ref().unwrap().contains("Handled:"));
    }

    #[tokio::test]
    async fn test_agent_action_no_handler() {
        let mut config = HookConfig::new();
        config
            .pre_tool_use
            .push(HookMatcher::new("*").with_action(HookAction::Agent {
                agent: "explore".to_string(),
                prompt: "Search codebase".to_string(),
                max_turns: 5,
            }));

        let executor = HookExecutor::new(config);
        let event = HookEvent::pre_tool_use("Read", serde_json::json!({}));
        let ctx = test_context();

        let results = executor.execute(&event, &ctx).await;
        assert_eq!(results.len(), 1);
        assert!(results[0].success);
        assert!(results[0].output.as_ref().unwrap().contains("no handler"));
    }

    #[tokio::test]
    async fn test_agent_action_with_handler() {
        let mut config = HookConfig::new();
        config
            .pre_tool_use
            .push(HookMatcher::new("*").with_action(HookAction::Agent {
                agent: "explore".to_string(),
                prompt: "Search codebase".to_string(),
                max_turns: 5,
            }));

        // 콜백 핸들러 생성
        let handler: AgentCallback = Arc::new(|req| {
            Box::pin(async move {
                AgentResponse::success(
                    AgentResult {
                        content: format!("Found results for: {}", req.prompt),
                        turns_used: 2,
                        agent_id: "test-agent-123".to_string(),
                        affected_files: vec!["src/main.rs".to_string()],
                    },
                    50,
                )
            })
        });

        let handlers = HookActionHandlers::new().with_agent_handler(handler);
        let executor = HookExecutor::with_handlers(config, handlers);
        let event = HookEvent::pre_tool_use("Read", serde_json::json!({}));
        let ctx = test_context();

        let results = executor.execute(&event, &ctx).await;
        assert_eq!(results.len(), 1);
        assert!(results[0].success);
        assert!(results[0]
            .output
            .as_ref()
            .unwrap()
            .contains("completed in 2 turns"));
    }

    #[tokio::test]
    async fn test_prompt_action_with_channel() {
        let mut config = HookConfig::new();
        config
            .pre_tool_use
            .push(HookMatcher::new("*").with_action(HookAction::Prompt {
                prompt: "Queued prompt".to_string(),
            }));

        let (tx, mut rx) = mpsc::channel::<PromptRequest>(10);
        let handlers = HookActionHandlers::new().with_prompt_channel(tx);
        let executor = HookExecutor::with_handlers(config, handlers);
        let event = HookEvent::pre_tool_use("Bash", serde_json::json!({}));
        let ctx = test_context();

        let results = executor.execute(&event, &ctx).await;
        assert_eq!(results.len(), 1);
        assert!(results[0].success);
        assert!(results[0].output.as_ref().unwrap().contains("queued"));

        // 채널에서 요청 수신 확인
        let received = rx.try_recv();
        assert!(received.is_ok());
        assert_eq!(received.unwrap().prompt, "Queued prompt");
    }
}
