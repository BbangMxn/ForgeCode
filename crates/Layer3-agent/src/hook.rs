//! Agent Hook System
//!
//! Claude Code/Gemini CLI 스타일의 확장 가능한 Hook 시스템입니다.
//! Agent 실행의 각 단계에서 커스텀 로직을 주입할 수 있습니다.

use async_trait::async_trait;
use forge_foundation::Result;
use forge_provider::ToolCall;
use std::sync::Arc;

use crate::history::MessageHistory;

// ============================================================================
// Hook Types
// ============================================================================

/// Hook 실행 결과
#[derive(Debug, Clone)]
pub enum HookResult {
    /// 계속 진행
    Continue,

    /// 실행 중단 (사용자 요청 등)
    Stop { reason: String },

    /// 실행 차단 (권한 부족 등) - 합성 응답 반환
    Block { response: String },

    /// 입력 수정 후 계속
    ModifyAndContinue { modified: String },
}

impl Default for HookResult {
    fn default() -> Self {
        Self::Continue
    }
}

/// Tool 실행 결과 정보
#[derive(Debug, Clone)]
pub struct ToolResult {
    /// Tool 호출 ID
    pub tool_call_id: String,

    /// Tool 이름
    pub tool_name: String,

    /// 실행 결과
    pub output: String,

    /// 성공 여부
    pub success: bool,

    /// 소요 시간 (ms)
    pub duration_ms: u64,
}

/// Agent 턴 정보
#[derive(Debug, Clone)]
pub struct TurnInfo {
    /// 현재 턴 번호
    pub turn: u32,

    /// 입력 토큰 수
    pub input_tokens: u32,

    /// 출력 토큰 수
    pub output_tokens: u32,

    /// 사용된 Tool 목록
    pub tools_used: Vec<String>,
}

// ============================================================================
// AgentHook Trait
// ============================================================================

/// Agent Hook 트레이트
///
/// Agent 실행의 각 단계에서 호출되는 훅입니다.
/// 모든 메서드는 기본 구현이 있어 필요한 것만 오버라이드하면 됩니다.
#[async_trait]
pub trait AgentHook: Send + Sync {
    /// Hook 이름 (디버깅/로깅용)
    fn name(&self) -> &str {
        "unnamed-hook"
    }

    /// Agent 루프 시작 전
    ///
    /// - 컨텍스트 초기화
    /// - 리소스 설정
    /// - 실행 차단 가능
    async fn before_agent(&self, _history: &MessageHistory) -> Result<HookResult> {
        Ok(HookResult::Continue)
    }

    /// Agent 루프 완료 후
    ///
    /// - 결과 검증
    /// - 정리 작업
    /// - 요약 생성
    async fn after_agent(
        &self,
        _history: &MessageHistory,
        _response: &str,
        _turn_info: &TurnInfo,
    ) -> Result<HookResult> {
        Ok(HookResult::Continue)
    }

    /// 각 턴 시작 전
    async fn before_turn(&self, _history: &MessageHistory, _turn: u32) -> Result<HookResult> {
        Ok(HookResult::Continue)
    }

    /// 각 턴 완료 후
    async fn after_turn(
        &self,
        _history: &MessageHistory,
        _turn: u32,
        _response: &str,
    ) -> Result<HookResult> {
        Ok(HookResult::Continue)
    }

    /// Tool 실행 전
    ///
    /// - 권한 확인
    /// - 입력 검증
    /// - 실행 차단 가능
    async fn before_tool(
        &self,
        _tool_call: &ToolCall,
        _history: &MessageHistory,
    ) -> Result<HookResult> {
        Ok(HookResult::Continue)
    }

    /// Tool 실행 후
    ///
    /// - 결과 검증
    /// - 로깅
    /// - 결과 수정 가능
    async fn after_tool(
        &self,
        _tool_call: &ToolCall,
        _result: &ToolResult,
        _history: &MessageHistory,
    ) -> Result<HookResult> {
        Ok(HookResult::Continue)
    }

    /// 컨텍스트 압축 전
    async fn before_compress(&self, _history: &MessageHistory) -> Result<HookResult> {
        Ok(HookResult::Continue)
    }

    /// 컨텍스트 압축 후
    async fn after_compress(
        &self,
        _history: &MessageHistory,
        _tokens_saved: usize,
    ) -> Result<HookResult> {
        Ok(HookResult::Continue)
    }

    /// 에러 발생 시
    async fn on_error(
        &self,
        _error: &forge_foundation::Error,
        _history: &MessageHistory,
    ) -> Result<HookResult> {
        Ok(HookResult::Continue)
    }
}

// ============================================================================
// HookManager
// ============================================================================

/// Hook 관리자
///
/// 여러 Hook을 등록하고 순서대로 실행합니다.
pub struct HookManager {
    hooks: Vec<Arc<dyn AgentHook>>,
}

impl HookManager {
    /// 새 HookManager 생성
    pub fn new() -> Self {
        Self { hooks: Vec::new() }
    }

    /// Hook 추가
    pub fn add<H: AgentHook + 'static>(&mut self, hook: H) {
        self.hooks.push(Arc::new(hook));
    }

    /// Arc로 감싼 Hook 추가
    pub fn add_arc(&mut self, hook: Arc<dyn AgentHook>) {
        self.hooks.push(hook);
    }

    /// 등록된 Hook 수
    pub fn len(&self) -> usize {
        self.hooks.len()
    }

    /// Hook이 비어있는지
    pub fn is_empty(&self) -> bool {
        self.hooks.is_empty()
    }

    /// before_agent 실행
    pub async fn run_before_agent(&self, history: &MessageHistory) -> Result<HookResult> {
        for hook in &self.hooks {
            match hook.before_agent(history).await? {
                HookResult::Continue => continue,
                result => return Ok(result),
            }
        }
        Ok(HookResult::Continue)
    }

    /// after_agent 실행
    pub async fn run_after_agent(
        &self,
        history: &MessageHistory,
        response: &str,
        turn_info: &TurnInfo,
    ) -> Result<HookResult> {
        for hook in &self.hooks {
            match hook.after_agent(history, response, turn_info).await? {
                HookResult::Continue => continue,
                result => return Ok(result),
            }
        }
        Ok(HookResult::Continue)
    }

    /// before_turn 실행
    pub async fn run_before_turn(&self, history: &MessageHistory, turn: u32) -> Result<HookResult> {
        for hook in &self.hooks {
            match hook.before_turn(history, turn).await? {
                HookResult::Continue => continue,
                result => return Ok(result),
            }
        }
        Ok(HookResult::Continue)
    }

    /// after_turn 실행
    pub async fn run_after_turn(
        &self,
        history: &MessageHistory,
        turn: u32,
        response: &str,
    ) -> Result<HookResult> {
        for hook in &self.hooks {
            match hook.after_turn(history, turn, response).await? {
                HookResult::Continue => continue,
                result => return Ok(result),
            }
        }
        Ok(HookResult::Continue)
    }

    /// before_tool 실행
    pub async fn run_before_tool(
        &self,
        tool_call: &ToolCall,
        history: &MessageHistory,
    ) -> Result<HookResult> {
        for hook in &self.hooks {
            match hook.before_tool(tool_call, history).await? {
                HookResult::Continue => continue,
                result => return Ok(result),
            }
        }
        Ok(HookResult::Continue)
    }

    /// after_tool 실행
    pub async fn run_after_tool(
        &self,
        tool_call: &ToolCall,
        result: &ToolResult,
        history: &MessageHistory,
    ) -> Result<HookResult> {
        for hook in &self.hooks {
            match hook.after_tool(tool_call, result, history).await? {
                HookResult::Continue => continue,
                res => return Ok(res),
            }
        }
        Ok(HookResult::Continue)
    }

    /// before_compress 실행
    pub async fn run_before_compress(&self, history: &MessageHistory) -> Result<HookResult> {
        for hook in &self.hooks {
            match hook.before_compress(history).await? {
                HookResult::Continue => continue,
                result => return Ok(result),
            }
        }
        Ok(HookResult::Continue)
    }

    /// after_compress 실행
    pub async fn run_after_compress(
        &self,
        history: &MessageHistory,
        tokens_saved: usize,
    ) -> Result<HookResult> {
        for hook in &self.hooks {
            match hook.after_compress(history, tokens_saved).await? {
                HookResult::Continue => continue,
                result => return Ok(result),
            }
        }
        Ok(HookResult::Continue)
    }

    /// on_error 실행
    pub async fn run_on_error(
        &self,
        error: &forge_foundation::Error,
        history: &MessageHistory,
    ) -> Result<HookResult> {
        for hook in &self.hooks {
            match hook.on_error(error, history).await? {
                HookResult::Continue => continue,
                result => return Ok(result),
            }
        }
        Ok(HookResult::Continue)
    }
}

impl Default for HookManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Built-in Hooks
// ============================================================================

/// 로깅 Hook
///
/// 모든 이벤트를 tracing으로 로깅합니다.
pub struct LoggingHook {
    log_tool_args: bool,
    log_tool_results: bool,
}

impl LoggingHook {
    pub fn new() -> Self {
        Self {
            log_tool_args: false,
            log_tool_results: false,
        }
    }

    pub fn with_tool_args(mut self) -> Self {
        self.log_tool_args = true;
        self
    }

    pub fn with_tool_results(mut self) -> Self {
        self.log_tool_results = true;
        self
    }
}

impl Default for LoggingHook {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AgentHook for LoggingHook {
    fn name(&self) -> &str {
        "logging"
    }

    async fn before_agent(&self, history: &MessageHistory) -> Result<HookResult> {
        tracing::info!(messages = history.len(), "Agent loop starting");
        Ok(HookResult::Continue)
    }

    async fn after_agent(
        &self,
        _history: &MessageHistory,
        response: &str,
        turn_info: &TurnInfo,
    ) -> Result<HookResult> {
        tracing::info!(
            turns = turn_info.turn,
            input_tokens = turn_info.input_tokens,
            output_tokens = turn_info.output_tokens,
            tools_used = ?turn_info.tools_used,
            response_len = response.len(),
            "Agent loop completed"
        );
        Ok(HookResult::Continue)
    }

    async fn before_tool(
        &self,
        tool_call: &ToolCall,
        _history: &MessageHistory,
    ) -> Result<HookResult> {
        if self.log_tool_args {
            tracing::debug!(
                tool = %tool_call.name,
                id = %tool_call.id,
                args = %tool_call.arguments,
                "Executing tool"
            );
        } else {
            tracing::debug!(
                tool = %tool_call.name,
                id = %tool_call.id,
                "Executing tool"
            );
        }
        Ok(HookResult::Continue)
    }

    async fn after_tool(
        &self,
        tool_call: &ToolCall,
        result: &ToolResult,
        _history: &MessageHistory,
    ) -> Result<HookResult> {
        if self.log_tool_results {
            tracing::debug!(
                tool = %tool_call.name,
                success = result.success,
                duration_ms = result.duration_ms,
                output_len = result.output.len(),
                "Tool completed"
            );
        } else {
            tracing::debug!(
                tool = %tool_call.name,
                success = result.success,
                duration_ms = result.duration_ms,
                "Tool completed"
            );
        }
        Ok(HookResult::Continue)
    }

    async fn on_error(
        &self,
        error: &forge_foundation::Error,
        _history: &MessageHistory,
    ) -> Result<HookResult> {
        tracing::error!(error = %error, "Agent error occurred");
        Ok(HookResult::Continue)
    }
}

/// Token 추적 Hook
///
/// 토큰 사용량을 추적하고 제한을 확인합니다.
pub struct TokenTrackingHook {
    /// 최대 입력 토큰
    max_input_tokens: Option<u32>,
    /// 최대 출력 토큰
    max_output_tokens: Option<u32>,
    /// 누적 입력 토큰
    total_input: std::sync::atomic::AtomicU32,
    /// 누적 출력 토큰
    total_output: std::sync::atomic::AtomicU32,
}

impl TokenTrackingHook {
    pub fn new() -> Self {
        Self {
            max_input_tokens: None,
            max_output_tokens: None,
            total_input: std::sync::atomic::AtomicU32::new(0),
            total_output: std::sync::atomic::AtomicU32::new(0),
        }
    }

    pub fn with_limits(mut self, max_input: u32, max_output: u32) -> Self {
        self.max_input_tokens = Some(max_input);
        self.max_output_tokens = Some(max_output);
        self
    }

    pub fn total_input(&self) -> u32 {
        self.total_input.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn total_output(&self) -> u32 {
        self.total_output.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn total(&self) -> u32 {
        self.total_input() + self.total_output()
    }
}

impl Default for TokenTrackingHook {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AgentHook for TokenTrackingHook {
    fn name(&self) -> &str {
        "token-tracking"
    }

    async fn after_turn(
        &self,
        _history: &MessageHistory,
        _turn: u32,
        _response: &str,
    ) -> Result<HookResult> {
        // 토큰 제한 확인
        if let Some(max) = self.max_input_tokens {
            if self.total_input() > max {
                return Ok(HookResult::Stop {
                    reason: format!(
                        "Input token limit exceeded: {} > {}",
                        self.total_input(),
                        max
                    ),
                });
            }
        }

        if let Some(max) = self.max_output_tokens {
            if self.total_output() > max {
                return Ok(HookResult::Stop {
                    reason: format!(
                        "Output token limit exceeded: {} > {}",
                        self.total_output(),
                        max
                    ),
                });
            }
        }

        Ok(HookResult::Continue)
    }

    async fn after_agent(
        &self,
        _history: &MessageHistory,
        _response: &str,
        turn_info: &TurnInfo,
    ) -> Result<HookResult> {
        self.total_input
            .fetch_add(turn_info.input_tokens, std::sync::atomic::Ordering::Relaxed);
        self.total_output.fetch_add(
            turn_info.output_tokens,
            std::sync::atomic::Ordering::Relaxed,
        );
        Ok(HookResult::Continue)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    struct TestHook {
        name: String,
        should_stop: bool,
    }

    impl TestHook {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
                should_stop: false,
            }
        }

        fn stopping(name: &str) -> Self {
            Self {
                name: name.to_string(),
                should_stop: true,
            }
        }
    }

    #[async_trait]
    impl AgentHook for TestHook {
        fn name(&self) -> &str {
            &self.name
        }

        async fn before_agent(&self, _history: &MessageHistory) -> Result<HookResult> {
            if self.should_stop {
                Ok(HookResult::Stop {
                    reason: "Test stop".to_string(),
                })
            } else {
                Ok(HookResult::Continue)
            }
        }
    }

    #[tokio::test]
    async fn test_hook_manager_continues() {
        let mut manager = HookManager::new();
        manager.add(TestHook::new("hook1"));
        manager.add(TestHook::new("hook2"));

        let history = MessageHistory::new();
        let result = manager.run_before_agent(&history).await.unwrap();

        assert!(matches!(result, HookResult::Continue));
    }

    #[tokio::test]
    async fn test_hook_manager_stops() {
        let mut manager = HookManager::new();
        manager.add(TestHook::new("hook1"));
        manager.add(TestHook::stopping("hook2"));
        manager.add(TestHook::new("hook3")); // Should not be reached

        let history = MessageHistory::new();
        let result = manager.run_before_agent(&history).await.unwrap();

        assert!(matches!(result, HookResult::Stop { .. }));
    }

    #[tokio::test]
    async fn test_logging_hook() {
        let hook = LoggingHook::new().with_tool_args().with_tool_results();

        let history = MessageHistory::new();
        let result = hook.before_agent(&history).await.unwrap();

        assert!(matches!(result, HookResult::Continue));
    }
}
