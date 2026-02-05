//! Steering Queue
//!
//! Claude Code의 h2A 스타일 실시간 스티어링 시스템입니다.
//! 에이전트 실행 중 중단, 재개, 방향 전환을 지원합니다.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, RwLock};

// ============================================================================
// Steering Commands
// ============================================================================

/// 스티어링 명령
#[derive(Debug)]
pub enum SteeringCommand {
    /// 실행 일시 중단
    Pause,

    /// 실행 재개
    Resume,

    /// 실행 중단 (완전 종료)
    Stop { reason: String },

    /// 방향 전환 - 새로운 지시 주입
    Redirect { instruction: String },

    /// 우선순위 변경
    SetPriority { priority: u32 },

    /// 컨텍스트 추가 주입
    InjectContext { context: String },

    /// Tool 실행 취소 요청
    CancelTool { tool_call_id: String },

    /// 상태 조회 요청
    QueryStatus {
        response_tx: oneshot::Sender<AgentStatus>,
    },
}

/// Agent 상태
#[derive(Debug, Clone)]
pub struct AgentStatus {
    /// 현재 상태
    pub state: AgentState,

    /// 현재 턴
    pub current_turn: u32,

    /// 실행 중인 Tool (있으면)
    pub active_tool: Option<String>,

    /// 총 토큰 사용량
    pub total_tokens: u64,

    /// 경과 시간 (ms)
    pub elapsed_ms: u64,
}

/// Agent 상태 enum
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentState {
    /// 대기 중
    Idle,
    /// 실행 중
    Running,
    /// 일시 중단
    Paused,
    /// Tool 실행 중
    ExecutingTool,
    /// LLM 응답 대기 중
    WaitingForLlm,
    /// 완료
    Completed,
    /// 에러로 중단
    Error,
}

// ============================================================================
// Steering Queue
// ============================================================================

/// 스티어링 큐
///
/// 비동기적으로 Agent에 명령을 전달하고 상태를 관리합니다.
pub struct SteeringQueue {
    /// 명령 송신자
    command_tx: mpsc::Sender<SteeringCommand>,

    /// 명령 수신자 (Agent가 소유)
    command_rx: Arc<RwLock<mpsc::Receiver<SteeringCommand>>>,

    /// 현재 상태
    state: Arc<RwLock<AgentState>>,

    /// 일시 중단 플래그
    paused: Arc<AtomicBool>,

    /// 중단 플래그
    stopped: Arc<AtomicBool>,

    /// 중단 이유
    stop_reason: Arc<RwLock<Option<String>>>,

    /// 주입된 지시사항 큐
    injected_instructions: Arc<RwLock<Vec<String>>>,

    /// 주입된 컨텍스트
    injected_context: Arc<RwLock<Vec<String>>>,

    /// 현재 턴
    current_turn: Arc<AtomicU64>,

    /// 시작 시간
    start_time: std::time::Instant,
}

impl SteeringQueue {
    /// 새 스티어링 큐 생성
    pub fn new() -> Self {
        let (command_tx, command_rx) = mpsc::channel(100);

        Self {
            command_tx,
            command_rx: Arc::new(RwLock::new(command_rx)),
            state: Arc::new(RwLock::new(AgentState::Idle)),
            paused: Arc::new(AtomicBool::new(false)),
            stopped: Arc::new(AtomicBool::new(false)),
            stop_reason: Arc::new(RwLock::new(None)),
            injected_instructions: Arc::new(RwLock::new(Vec::new())),
            injected_context: Arc::new(RwLock::new(Vec::new())),
            current_turn: Arc::new(AtomicU64::new(0)),
            start_time: std::time::Instant::now(),
        }
    }

    /// 핸들 생성 (외부에서 명령 전송용)
    pub fn handle(&self) -> SteeringHandle {
        SteeringHandle {
            command_tx: self.command_tx.clone(),
            paused: self.paused.clone(),
            stopped: self.stopped.clone(),
            state: self.state.clone(),
        }
    }

    /// 체커 생성 (Agent 내부에서 상태 확인용)
    pub fn checker(&self) -> SteeringChecker {
        SteeringChecker {
            command_rx: self.command_rx.clone(),
            paused: self.paused.clone(),
            stopped: self.stopped.clone(),
            stop_reason: self.stop_reason.clone(),
            injected_instructions: self.injected_instructions.clone(),
            injected_context: self.injected_context.clone(),
            state: self.state.clone(),
            current_turn: self.current_turn.clone(),
            start_time: self.start_time,
        }
    }
}

impl Default for SteeringQueue {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Steering Handle (외부 인터페이스)
// ============================================================================

/// 스티어링 핸들
///
/// 외부에서 Agent에 명령을 전송하기 위한 인터페이스입니다.
#[derive(Clone)]
pub struct SteeringHandle {
    command_tx: mpsc::Sender<SteeringCommand>,
    paused: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
    state: Arc<RwLock<AgentState>>,
}

impl SteeringHandle {
    /// 일시 중단
    pub async fn pause(&self) -> Result<(), SteeringError> {
        self.paused.store(true, Ordering::SeqCst);
        self.command_tx
            .send(SteeringCommand::Pause)
            .await
            .map_err(|_| SteeringError::ChannelClosed)?;
        Ok(())
    }

    /// 재개
    pub async fn resume(&self) -> Result<(), SteeringError> {
        self.paused.store(false, Ordering::SeqCst);
        self.command_tx
            .send(SteeringCommand::Resume)
            .await
            .map_err(|_| SteeringError::ChannelClosed)?;
        Ok(())
    }

    /// 중단
    pub async fn stop(&self, reason: impl Into<String>) -> Result<(), SteeringError> {
        self.stopped.store(true, Ordering::SeqCst);
        self.command_tx
            .send(SteeringCommand::Stop {
                reason: reason.into(),
            })
            .await
            .map_err(|_| SteeringError::ChannelClosed)?;
        Ok(())
    }

    /// 방향 전환 (새 지시 주입)
    pub async fn redirect(&self, instruction: impl Into<String>) -> Result<(), SteeringError> {
        self.command_tx
            .send(SteeringCommand::Redirect {
                instruction: instruction.into(),
            })
            .await
            .map_err(|_| SteeringError::ChannelClosed)?;
        Ok(())
    }

    /// 컨텍스트 주입
    pub async fn inject_context(&self, context: impl Into<String>) -> Result<(), SteeringError> {
        self.command_tx
            .send(SteeringCommand::InjectContext {
                context: context.into(),
            })
            .await
            .map_err(|_| SteeringError::ChannelClosed)?;
        Ok(())
    }

    /// Tool 실행 취소
    pub async fn cancel_tool(&self, tool_call_id: impl Into<String>) -> Result<(), SteeringError> {
        self.command_tx
            .send(SteeringCommand::CancelTool {
                tool_call_id: tool_call_id.into(),
            })
            .await
            .map_err(|_| SteeringError::ChannelClosed)?;
        Ok(())
    }

    /// 상태 조회
    pub async fn query_status(&self) -> Result<AgentStatus, SteeringError> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(SteeringCommand::QueryStatus { response_tx })
            .await
            .map_err(|_| SteeringError::ChannelClosed)?;
        response_rx
            .await
            .map_err(|_| SteeringError::ResponseTimeout)
    }

    /// 일시 중단 상태인지
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    /// 중단 상태인지
    pub fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }

    /// 현재 상태
    pub async fn state(&self) -> AgentState {
        *self.state.read().await
    }
}

// ============================================================================
// Steering Checker (Agent 내부 인터페이스)
// ============================================================================

/// 스티어링 체커
///
/// Agent 내부에서 스티어링 상태를 확인하고 명령을 처리합니다.
pub struct SteeringChecker {
    command_rx: Arc<RwLock<mpsc::Receiver<SteeringCommand>>>,
    paused: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
    stop_reason: Arc<RwLock<Option<String>>>,
    injected_instructions: Arc<RwLock<Vec<String>>>,
    injected_context: Arc<RwLock<Vec<String>>>,
    state: Arc<RwLock<AgentState>>,
    current_turn: Arc<AtomicU64>,
    start_time: std::time::Instant,
}

impl SteeringChecker {
    /// 중단 요청됐는지 확인
    pub fn should_stop(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }

    /// 일시 중단됐는지 확인
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    /// 중단 이유 가져오기
    pub async fn stop_reason(&self) -> Option<String> {
        self.stop_reason.read().await.clone()
    }

    /// 대기 중인 명령 처리
    pub async fn process_commands(&self) -> Vec<SteeringCommand> {
        let mut commands = Vec::new();
        let mut rx = self.command_rx.write().await;

        while let Ok(cmd) = rx.try_recv() {
            match cmd {
                SteeringCommand::Pause => {
                    self.paused.store(true, Ordering::SeqCst);
                    self.set_state(AgentState::Paused).await;
                    commands.push(SteeringCommand::Pause);
                }
                SteeringCommand::Resume => {
                    self.paused.store(false, Ordering::SeqCst);
                    self.set_state(AgentState::Running).await;
                    commands.push(SteeringCommand::Resume);
                }
                SteeringCommand::Stop { reason } => {
                    self.stopped.store(true, Ordering::SeqCst);
                    *self.stop_reason.write().await = Some(reason.clone());
                    commands.push(SteeringCommand::Stop { reason });
                }
                SteeringCommand::Redirect { instruction } => {
                    self.injected_instructions
                        .write()
                        .await
                        .push(instruction.clone());
                    commands.push(SteeringCommand::Redirect { instruction });
                }
                SteeringCommand::InjectContext { context } => {
                    self.injected_context.write().await.push(context.clone());
                    commands.push(SteeringCommand::InjectContext { context });
                }
                SteeringCommand::QueryStatus { response_tx } => {
                    let status = self.create_status().await;
                    let _ = response_tx.send(status);
                    // Don't add to commands - consumed by send
                }
                SteeringCommand::SetPriority { priority } => {
                    commands.push(SteeringCommand::SetPriority { priority });
                }
                SteeringCommand::CancelTool { tool_call_id } => {
                    commands.push(SteeringCommand::CancelTool { tool_call_id });
                }
            }
        }

        commands
    }

    /// 일시 중단 시 대기
    pub async fn wait_if_paused(&self) {
        while self.is_paused() && !self.should_stop() {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            self.process_commands().await;
        }
    }

    /// 주입된 지시사항 가져오기 및 클리어
    pub async fn take_injected_instructions(&self) -> Vec<String> {
        let mut instructions = self.injected_instructions.write().await;
        std::mem::take(&mut *instructions)
    }

    /// 주입된 컨텍스트 가져오기 및 클리어
    pub async fn take_injected_context(&self) -> Vec<String> {
        let mut context = self.injected_context.write().await;
        std::mem::take(&mut *context)
    }

    /// 상태 설정
    pub async fn set_state(&self, state: AgentState) {
        *self.state.write().await = state;
    }

    /// 현재 턴 설정
    pub fn set_turn(&self, turn: u32) {
        self.current_turn.store(turn as u64, Ordering::Relaxed);
    }

    /// 상태 정보 생성
    async fn create_status(&self) -> AgentStatus {
        AgentStatus {
            state: *self.state.read().await,
            current_turn: self.current_turn.load(Ordering::Relaxed) as u32,
            active_tool: None, // TODO: Track active tool
            total_tokens: 0,   // TODO: Track tokens
            elapsed_ms: self.start_time.elapsed().as_millis() as u64,
        }
    }
}

// ============================================================================
// Errors
// ============================================================================

/// 스티어링 에러
#[derive(Debug, thiserror::Error)]
pub enum SteeringError {
    #[error("Channel closed")]
    ChannelClosed,

    #[error("Response timeout")]
    ResponseTimeout,

    #[error("Agent not running")]
    NotRunning,
}

// ============================================================================
// Convenience Trait
// ============================================================================

/// 스티어링 가능한 Agent 트레이트
#[allow(async_fn_in_trait)]
pub trait Steerable {
    /// 스티어링 체커 가져오기
    fn steering(&self) -> &SteeringChecker;

    /// 실행 계속 가능 여부
    fn can_continue(&self) -> bool {
        !self.steering().should_stop()
    }

    /// 체크포인트 - 중단/일시중단 확인
    async fn checkpoint(&self) -> Result<(), SteeringError> {
        let steering = self.steering();

        // 일시 중단 대기
        steering.wait_if_paused().await;

        // 중단 확인
        if steering.should_stop() {
            return Err(SteeringError::NotRunning);
        }

        Ok(())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_steering_pause_resume() {
        let queue = SteeringQueue::new();
        let handle = queue.handle();
        let checker = queue.checker();

        assert!(!checker.is_paused());

        handle.pause().await.unwrap();
        checker.process_commands().await;
        assert!(checker.is_paused());

        handle.resume().await.unwrap();
        checker.process_commands().await;
        assert!(!checker.is_paused());
    }

    #[tokio::test]
    async fn test_steering_stop() {
        let queue = SteeringQueue::new();
        let handle = queue.handle();
        let checker = queue.checker();

        assert!(!checker.should_stop());

        handle.stop("User requested").await.unwrap();
        checker.process_commands().await;

        assert!(checker.should_stop());
        assert_eq!(
            checker.stop_reason().await,
            Some("User requested".to_string())
        );
    }

    #[tokio::test]
    async fn test_steering_redirect() {
        let queue = SteeringQueue::new();
        let handle = queue.handle();
        let checker = queue.checker();

        handle.redirect("Focus on tests").await.unwrap();
        handle.redirect("Also check docs").await.unwrap();
        checker.process_commands().await;

        let instructions = checker.take_injected_instructions().await;
        assert_eq!(instructions.len(), 2);
        assert_eq!(instructions[0], "Focus on tests");
        assert_eq!(instructions[1], "Also check docs");

        // Should be empty after take
        let instructions = checker.take_injected_instructions().await;
        assert!(instructions.is_empty());
    }

    #[tokio::test]
    async fn test_handle_clone() {
        let queue = SteeringQueue::new();
        let handle1 = queue.handle();
        let handle2 = handle1.clone();
        let checker = queue.checker();

        handle1.pause().await.unwrap();
        checker.process_commands().await;

        // Both handles should see the same state
        assert!(handle1.is_paused());
        assert!(handle2.is_paused());
    }
}
