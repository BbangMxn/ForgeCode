//! Agent Lifecycle Management
//!
//! Agent의 실행 단계 및 생명주기 관리입니다.

use serde::{Deserialize, Serialize};
use std::fmt;

// ============================================================================
// AgentPhase - Agent 실행 단계
// ============================================================================

/// Agent 실행 단계
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentPhase {
    /// 초기화 중
    Initializing,

    /// Think 단계 (추론)
    Thinking,

    /// Plan 단계 (계획)
    Planning,

    /// Execute 단계 (실행)
    Executing,

    /// Reflect 단계 (반성)
    Reflecting,

    /// 대기 중 (사용자 입력 등)
    Waiting,

    /// 일시 정지
    Paused,

    /// 완료
    Completed,

    /// 실패
    Failed,

    /// 중단됨
    Aborted,
}

impl AgentPhase {
    /// 활성 상태 여부
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            AgentPhase::Thinking
                | AgentPhase::Planning
                | AgentPhase::Executing
                | AgentPhase::Reflecting
        )
    }

    /// 종료 상태 여부
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            AgentPhase::Completed | AgentPhase::Failed | AgentPhase::Aborted
        )
    }

    /// 다음 단계 가져오기 (표준 흐름)
    pub fn next(&self) -> Option<AgentPhase> {
        match self {
            AgentPhase::Initializing => Some(AgentPhase::Thinking),
            AgentPhase::Thinking => Some(AgentPhase::Planning),
            AgentPhase::Planning => Some(AgentPhase::Executing),
            AgentPhase::Executing => Some(AgentPhase::Reflecting),
            AgentPhase::Reflecting => Some(AgentPhase::Thinking), // 루프
            _ => None,
        }
    }
}

impl fmt::Display for AgentPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentPhase::Initializing => write!(f, "Initializing"),
            AgentPhase::Thinking => write!(f, "Thinking"),
            AgentPhase::Planning => write!(f, "Planning"),
            AgentPhase::Executing => write!(f, "Executing"),
            AgentPhase::Reflecting => write!(f, "Reflecting"),
            AgentPhase::Waiting => write!(f, "Waiting"),
            AgentPhase::Paused => write!(f, "Paused"),
            AgentPhase::Completed => write!(f, "Completed"),
            AgentPhase::Failed => write!(f, "Failed"),
            AgentPhase::Aborted => write!(f, "Aborted"),
        }
    }
}

// ============================================================================
// PhaseTransition - 단계 전이
// ============================================================================

/// 단계 전이 정보
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseTransition {
    /// 이전 단계
    pub from: AgentPhase,

    /// 다음 단계
    pub to: AgentPhase,

    /// 전이 이유
    pub reason: String,

    /// 타임스탬프
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl PhaseTransition {
    /// 새 전이 생성
    pub fn new(from: AgentPhase, to: AgentPhase, reason: impl Into<String>) -> Self {
        Self {
            from,
            to,
            reason: reason.into(),
            timestamp: chrono::Utc::now(),
        }
    }
}

// ============================================================================
// PhaseResult - 단계 실행 결과
// ============================================================================

/// 단계 실행 결과
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PhaseResult {
    /// 다음 단계로 진행
    Continue,

    /// 특정 단계로 점프
    JumpTo(AgentPhase),

    /// 현재 단계 재실행
    Retry { reason: String },

    /// 대기 (외부 입력 필요)
    Wait { prompt: String },

    /// 완료
    Complete { summary: String },

    /// 실패
    Fail { error: String },

    /// 중단
    Abort { reason: String },
}

impl PhaseResult {
    /// 계속 진행
    pub fn continue_() -> Self {
        PhaseResult::Continue
    }

    /// 완료
    pub fn complete(summary: impl Into<String>) -> Self {
        PhaseResult::Complete {
            summary: summary.into(),
        }
    }

    /// 실패
    pub fn fail(error: impl Into<String>) -> Self {
        PhaseResult::Fail {
            error: error.into(),
        }
    }

    /// 재시도
    pub fn retry(reason: impl Into<String>) -> Self {
        PhaseResult::Retry {
            reason: reason.into(),
        }
    }

    /// 대기
    pub fn wait(prompt: impl Into<String>) -> Self {
        PhaseResult::Wait {
            prompt: prompt.into(),
        }
    }
}

// ============================================================================
// LifecycleEvent - 생명주기 이벤트
// ============================================================================

/// Agent 생명주기 이벤트
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LifecycleEvent {
    /// 시작됨
    Started {
        session_id: String,
        agent_id: String,
    },

    /// 단계 시작
    PhaseStarted { phase: AgentPhase, turn: u32 },

    /// 단계 완료
    PhaseCompleted {
        phase: AgentPhase,
        turn: u32,
        duration_ms: u64,
    },

    /// 단계 전이
    PhaseTransitioned(PhaseTransition),

    /// 턴 완료
    TurnCompleted { turn: u32, duration_ms: u64 },

    /// Tool 호출
    ToolCalled {
        tool_name: String,
        tool_call_id: String,
    },

    /// Tool 완료
    ToolCompleted {
        tool_name: String,
        tool_call_id: String,
        success: bool,
        duration_ms: u64,
    },

    /// 일시 정지
    Paused { reason: String },

    /// 재개
    Resumed,

    /// 에러 발생
    Error { error: String, recoverable: bool },

    /// 완료
    Completed {
        summary: String,
        total_turns: u32,
        total_duration_ms: u64,
    },

    /// 중단
    Aborted { reason: String },
}

// ============================================================================
// LifecycleObserver - 생명주기 관찰자
// ============================================================================

/// 생명주기 이벤트 관찰자 트레이트
#[async_trait::async_trait]
pub trait LifecycleObserver: Send + Sync {
    /// 이벤트 처리
    async fn on_event(&self, event: &LifecycleEvent);
}

/// 로깅 관찰자
pub struct LoggingObserver;

#[async_trait::async_trait]
impl LifecycleObserver for LoggingObserver {
    async fn on_event(&self, event: &LifecycleEvent) {
        match event {
            LifecycleEvent::Started {
                session_id,
                agent_id,
            } => {
                tracing::info!("Agent started: session={}, agent={}", session_id, agent_id);
            }
            LifecycleEvent::PhaseStarted { phase, turn } => {
                tracing::debug!("Phase started: {} (turn {})", phase, turn);
            }
            LifecycleEvent::PhaseCompleted {
                phase,
                turn,
                duration_ms,
            } => {
                tracing::debug!(
                    "Phase completed: {} (turn {}, {}ms)",
                    phase,
                    turn,
                    duration_ms
                );
            }
            LifecycleEvent::TurnCompleted { turn, duration_ms } => {
                tracing::info!("Turn {} completed ({}ms)", turn, duration_ms);
            }
            LifecycleEvent::ToolCalled { tool_name, .. } => {
                tracing::debug!("Tool called: {}", tool_name);
            }
            LifecycleEvent::ToolCompleted {
                tool_name,
                success,
                duration_ms,
                ..
            } => {
                if *success {
                    tracing::debug!("Tool {} completed ({}ms)", tool_name, duration_ms);
                } else {
                    tracing::warn!("Tool {} failed ({}ms)", tool_name, duration_ms);
                }
            }
            LifecycleEvent::Error { error, recoverable } => {
                if *recoverable {
                    tracing::warn!("Recoverable error: {}", error);
                } else {
                    tracing::error!("Fatal error: {}", error);
                }
            }
            LifecycleEvent::Completed {
                summary,
                total_turns,
                total_duration_ms,
            } => {
                tracing::info!(
                    "Agent completed: {} turns, {}ms - {}",
                    total_turns,
                    total_duration_ms,
                    summary
                );
            }
            LifecycleEvent::Aborted { reason } => {
                tracing::warn!("Agent aborted: {}", reason);
            }
            _ => {}
        }
    }
}

/// 메트릭 수집 관찰자
pub struct MetricsObserver {
    /// 이벤트 수집용 채널
    event_tx: tokio::sync::mpsc::Sender<LifecycleEvent>,
}

impl MetricsObserver {
    /// 새 메트릭 관찰자 생성
    pub fn new(event_tx: tokio::sync::mpsc::Sender<LifecycleEvent>) -> Self {
        Self { event_tx }
    }
}

#[async_trait::async_trait]
impl LifecycleObserver for MetricsObserver {
    async fn on_event(&self, event: &LifecycleEvent) {
        let _ = self.event_tx.send(event.clone()).await;
    }
}
