//! Task Orchestrator - Task 간 통신 및 조율
//!
//! 여러 Task를 조율하고 Task 간 통신을 지원합니다.
//!
//! ## 주요 기능
//!
//! - Task 간 메시지 전달
//! - Task 조건 대기 (출력 패턴, 상태 변경 등)
//! - Task 그룹 관리
//! - 상호작용 로그
//!
//! ## 사용 예시
//!
//! ```ignore
//! let orchestrator = TaskOrchestrator::new();
//!
//! // 서버 Task 시작
//! let server_id = orchestrator.spawn(Task::new("cargo run --bin server")).await?;
//!
//! // 서버 준비 대기
//! orchestrator.wait_for(server_id, WaitCondition::OutputContains("Listening on")).await?;
//!
//! // 테스트 Task 실행
//! let test_id = orchestrator.spawn(Task::new("cargo test")).await?;
//!
//! // 결과 수집
//! let test_result = orchestrator.wait_complete(test_id).await?;
//!
//! // 서버 종료
//! orchestrator.stop(server_id).await?;
//! ```

use crate::log::{LogEntry, LogLevel, TaskLogManager};
use crate::manager::TaskManager;
use crate::task::{Task, TaskId};
use chrono::{DateTime, Utc};
use forge_foundation::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::info;

// ============================================================================
// Task Group (Task들의 논리적 그룹)
// ============================================================================

/// Task Group ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskGroupId(pub uuid::Uuid);

impl TaskGroupId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

impl Default for TaskGroupId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TaskGroupId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "grp-{}", &self.0.to_string()[..8])
    }
}

/// Task Group - 관련 Task들의 논리적 그룹
#[derive(Debug)]
pub struct TaskGroup {
    /// Group ID
    pub id: TaskGroupId,

    /// Group name
    pub name: String,

    /// Member task IDs
    pub tasks: Vec<TaskId>,

    /// Created timestamp
    pub created_at: DateTime<Utc>,

    /// Metadata
    pub metadata: HashMap<String, String>,
}

impl TaskGroup {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: TaskGroupId::new(),
            name: name.into(),
            tasks: Vec::new(),
            created_at: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    pub fn add_task(&mut self, task_id: TaskId) {
        if !self.tasks.contains(&task_id) {
            self.tasks.push(task_id);
        }
    }

    pub fn remove_task(&mut self, task_id: TaskId) {
        self.tasks.retain(|id| *id != task_id);
    }
}

// ============================================================================
// Task Message (Task 간 통신 메시지)
// ============================================================================

/// Task 간 메시지 타입
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskMessage {
    /// 텍스트 메시지
    Text(String),

    /// JSON 데이터
    Json(serde_json::Value),

    /// 신호 (Signal)
    Signal(TaskSignal),

    /// 출력 스트림 데이터
    Output {
        level: LogLevel,
        content: String,
    },

    /// 상태 변경 알림
    StateChange {
        old_state: String,
        new_state: String,
    },

    /// 커스텀 이벤트
    Event {
        event_type: String,
        payload: serde_json::Value,
    },
}

/// Task 신호
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskSignal {
    /// 시작 신호
    Start,
    /// 정지 요청
    Stop,
    /// 일시 중지
    Pause,
    /// 재개
    Resume,
    /// 준비 완료 알림
    Ready,
    /// 종료 알림
    Finished,
    /// 에러 발생
    Error,
    /// Heartbeat
    Heartbeat,
}

// ============================================================================
// Wait Condition (Task 대기 조건)
// ============================================================================

/// Task 대기 조건
#[derive(Debug, Clone)]
pub enum WaitCondition {
    /// Task 완료 대기
    Complete,

    /// 특정 상태 대기
    State(String),

    /// 출력에 특정 문자열 포함 대기
    OutputContains(String),

    /// 출력에 정규식 매치 대기
    OutputMatches(String),

    /// 특정 신호 수신 대기
    Signal(TaskSignal),

    /// 지정 시간 대기 (최대 대기 시간)
    Timeout(Duration),

    /// 모든 조건 충족
    All(Vec<WaitCondition>),

    /// 하나라도 충족
    Any(Vec<WaitCondition>),
}

impl WaitCondition {
    pub fn output_contains(pattern: impl Into<String>) -> Self {
        Self::OutputContains(pattern.into())
    }

    pub fn output_matches(regex: impl Into<String>) -> Self {
        Self::OutputMatches(regex.into())
    }

    pub fn ready() -> Self {
        Self::Signal(TaskSignal::Ready)
    }

    pub fn timeout(duration: Duration) -> Self {
        Self::Timeout(duration)
    }
}

// ============================================================================
// Wait Result
// ============================================================================

/// 대기 결과
#[derive(Debug, Clone)]
pub enum WaitResult {
    /// 조건 충족
    Satisfied {
        /// 충족된 조건
        condition: String,
        /// 관련 데이터
        data: Option<String>,
    },

    /// 타임아웃
    Timeout,

    /// Task 에러
    Error(String),

    /// Task 취소
    Cancelled,
}

impl WaitResult {
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Satisfied { .. })
    }
}

// ============================================================================
// Interaction Log (Task 상호작용 로그)
// ============================================================================

/// Task 상호작용 액션
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InteractionAction {
    /// Task 생성
    Spawn,
    /// Task 종료
    Stop,
    /// Task 간 메시지 전송
    Send {
        to_task: TaskId,
        message_type: String,
    },
    /// Task 간 메시지 브로드캐스트
    Broadcast {
        message_type: String,
    },
    /// 대기 시작
    WaitStart {
        condition: String,
    },
    /// 대기 완료
    WaitComplete {
        result: String,
    },
    /// 그룹 생성
    GroupCreate {
        group_id: TaskGroupId,
    },
    /// 그룹 참가
    GroupJoin {
        group_id: TaskGroupId,
    },
    /// 신호 전송
    Signal(TaskSignal),
}

/// Task 상호작용 로그 엔트리
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionLog {
    /// 타임스탬프
    pub timestamp: DateTime<Utc>,

    /// 소스 Task ID
    pub from_task: Option<TaskId>,

    /// 대상 Task ID (있는 경우)
    pub to_task: Option<TaskId>,

    /// 액션
    pub action: InteractionAction,

    /// 추가 페이로드
    pub payload: Option<serde_json::Value>,

    /// 메모/설명
    pub note: Option<String>,
}

impl InteractionLog {
    pub fn new(from: Option<TaskId>, to: Option<TaskId>, action: InteractionAction) -> Self {
        Self {
            timestamp: Utc::now(),
            from_task: from,
            to_task: to,
            action,
            payload: None,
            note: None,
        }
    }

    pub fn with_payload(mut self, payload: serde_json::Value) -> Self {
        self.payload = Some(payload);
        self
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }

    /// 분석용 포맷
    pub fn format_for_analysis(&self) -> String {
        let from = self
            .from_task
            .map(|id| id.to_string())
            .unwrap_or_else(|| "system".to_string());
        let to = self
            .to_task
            .map(|id| id.to_string())
            .unwrap_or_else(|| "none".to_string());

        format!(
            "[{}] {} -> {}: {:?}{}",
            self.timestamp.format("%H:%M:%S%.3f"),
            from,
            to,
            self.action,
            self.note
                .as_ref()
                .map(|n| format!(" ({})", n))
                .unwrap_or_default()
        )
    }
}

// ============================================================================
// Task Channel (Task 간 통신 채널)
// ============================================================================

/// Task 간 통신 채널
struct TaskChannel {
    /// 메시지 송신자
    tx: mpsc::Sender<TaskMessage>,
    /// 메시지 수신자
    rx: Arc<RwLock<mpsc::Receiver<TaskMessage>>>,
}

impl TaskChannel {
    fn new(buffer_size: usize) -> Self {
        let (tx, rx) = mpsc::channel(buffer_size);
        Self {
            tx,
            rx: Arc::new(RwLock::new(rx)),
        }
    }
}

// ============================================================================
// Task Orchestrator
// ============================================================================

/// Task Orchestrator Configuration
#[derive(Debug, Clone)]
pub struct OrchestratorConfig {
    /// 채널 버퍼 크기
    pub channel_buffer_size: usize,

    /// 기본 대기 타임아웃
    pub default_wait_timeout: Duration,

    /// 상호작용 로그 최대 크기
    pub max_interaction_logs: usize,

    /// 출력 폴링 간격
    pub output_poll_interval: Duration,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            channel_buffer_size: 1000,
            default_wait_timeout: Duration::from_secs(300), // 5분
            max_interaction_logs: 10000,
            output_poll_interval: Duration::from_millis(100),
        }
    }
}

/// Task Orchestrator - Task 조율 및 통신 관리
pub struct TaskOrchestrator {
    /// Configuration
    config: OrchestratorConfig,

    /// Task Manager (실제 Task 실행)
    task_manager: Arc<TaskManager>,

    /// Task 그룹들
    groups: Arc<RwLock<HashMap<TaskGroupId, TaskGroup>>>,

    /// Task 간 채널들 (from_task, to_task) -> channel
    channels: Arc<RwLock<HashMap<(TaskId, TaskId), TaskChannel>>>,

    /// 브로드캐스트 채널들 (task_id -> broadcast_sender)
    broadcast_channels: Arc<RwLock<HashMap<TaskId, broadcast::Sender<TaskMessage>>>>,

    /// 상호작용 로그
    interaction_logs: Arc<RwLock<Vec<InteractionLog>>>,

    /// Log Manager 참조
    log_manager: Arc<TaskLogManager>,
}

impl TaskOrchestrator {
    /// 새 Orchestrator 생성
    pub async fn new(config: OrchestratorConfig) -> Self {
        let task_manager = TaskManager::new(Default::default()).await;
        let log_manager = task_manager.log_manager();

        Self {
            config,
            task_manager: Arc::new(task_manager),
            groups: Arc::new(RwLock::new(HashMap::new())),
            channels: Arc::new(RwLock::new(HashMap::new())),
            broadcast_channels: Arc::new(RwLock::new(HashMap::new())),
            interaction_logs: Arc::new(RwLock::new(Vec::new())),
            log_manager,
        }
    }

    /// 기본 설정으로 생성
    pub async fn default() -> Self {
        Self::new(OrchestratorConfig::default()).await
    }

    /// 기존 TaskManager와 함께 생성
    pub fn with_task_manager(task_manager: Arc<TaskManager>, config: OrchestratorConfig) -> Self {
        let log_manager = task_manager.log_manager();

        Self {
            config,
            task_manager,
            groups: Arc::new(RwLock::new(HashMap::new())),
            channels: Arc::new(RwLock::new(HashMap::new())),
            broadcast_channels: Arc::new(RwLock::new(HashMap::new())),
            interaction_logs: Arc::new(RwLock::new(Vec::new())),
            log_manager,
        }
    }

    // ========================================================================
    // Task Lifecycle
    // ========================================================================

    /// Task 시작
    pub async fn spawn(&self, task: Task) -> Result<TaskId> {
        let task_id = self.task_manager.submit(task).await;

        // 브로드캐스트 채널 생성
        let (tx, _) = broadcast::channel(self.config.channel_buffer_size);
        {
            let mut channels = self.broadcast_channels.write().await;
            channels.insert(task_id, tx);
        }

        // 상호작용 로그 기록
        self.log_interaction(InteractionLog::new(
            None,
            Some(task_id),
            InteractionAction::Spawn,
        ))
        .await;

        info!("Orchestrator spawned task: {}", task_id);
        Ok(task_id)
    }

    /// Task 정지
    pub async fn stop(&self, task_id: TaskId) -> Result<()> {
        self.task_manager.cancel(task_id).await?;

        // 브로드캐스트 채널 정리
        {
            let mut channels = self.broadcast_channels.write().await;
            channels.remove(&task_id);
        }

        // 상호작용 로그 기록
        self.log_interaction(InteractionLog::new(
            None,
            Some(task_id),
            InteractionAction::Stop,
        ))
        .await;

        info!("Orchestrator stopped task: {}", task_id);
        Ok(())
    }

    /// Task 상태 조회
    pub async fn status(&self, task_id: TaskId) -> Option<crate::manager::TaskStatus> {
        self.task_manager.get_status(task_id).await
    }

    // ========================================================================
    // Task Communication
    // ========================================================================

    /// Task 간 채널 생성
    pub async fn create_channel(&self, from: TaskId, to: TaskId) -> Result<()> {
        let mut channels = self.channels.write().await;
        if !channels.contains_key(&(from, to)) {
            channels.insert((from, to), TaskChannel::new(self.config.channel_buffer_size));
        }
        Ok(())
    }

    /// 특정 Task에 메시지 전송
    pub async fn send(&self, from: TaskId, to: TaskId, message: TaskMessage) -> Result<()> {
        // 채널이 없으면 생성
        self.create_channel(from, to).await?;

        let channels = self.channels.read().await;
        if let Some(channel) = channels.get(&(from, to)) {
            channel
                .tx
                .send(message.clone())
                .await
                .map_err(|e| Error::Task(format!("Failed to send message: {}", e)))?;
        }

        // 로그 기록
        let message_type = match &message {
            TaskMessage::Text(_) => "text",
            TaskMessage::Json(_) => "json",
            TaskMessage::Signal(s) => match s {
                TaskSignal::Ready => "signal:ready",
                TaskSignal::Stop => "signal:stop",
                _ => "signal:other",
            },
            TaskMessage::Output { .. } => "output",
            TaskMessage::StateChange { .. } => "state_change",
            TaskMessage::Event { .. } => "event",
        };

        self.log_interaction(InteractionLog::new(
            Some(from),
            Some(to),
            InteractionAction::Send {
                to_task: to,
                message_type: message_type.to_string(),
            },
        ))
        .await;

        Ok(())
    }

    /// 모든 Task에 브로드캐스트
    pub async fn broadcast(&self, from: TaskId, message: TaskMessage) -> Result<()> {
        let channels = self.broadcast_channels.read().await;

        if let Some(tx) = channels.get(&from) {
            // 브로드캐스트 (수신자가 없어도 에러 아님)
            let _ = tx.send(message.clone());
        }

        // 로그 기록
        let message_type = match &message {
            TaskMessage::Signal(s) => format!("signal:{:?}", s).to_lowercase(),
            _ => "broadcast".to_string(),
        };

        self.log_interaction(InteractionLog::new(
            Some(from),
            None,
            InteractionAction::Broadcast { message_type },
        ))
        .await;

        Ok(())
    }

    /// 브로드캐스트 구독
    pub async fn subscribe(&self, task_id: TaskId) -> Option<broadcast::Receiver<TaskMessage>> {
        let channels = self.broadcast_channels.read().await;
        channels.get(&task_id).map(|tx| tx.subscribe())
    }

    /// 메시지 수신 (from -> to 채널에서)
    pub async fn receive(
        &self,
        from: TaskId,
        to: TaskId,
        timeout: Option<Duration>,
    ) -> Result<Option<TaskMessage>> {
        let timeout = timeout.unwrap_or(self.config.default_wait_timeout);

        let channels = self.channels.read().await;
        if let Some(channel) = channels.get(&(from, to)) {
            let mut rx = channel.rx.write().await;
            match tokio::time::timeout(timeout, rx.recv()).await {
                Ok(Some(msg)) => Ok(Some(msg)),
                Ok(None) => Ok(None),
                Err(_) => Ok(None), // 타임아웃
            }
        } else {
            Ok(None)
        }
    }

    // ========================================================================
    // Wait Conditions
    // ========================================================================

    /// 조건이 충족될 때까지 대기
    pub async fn wait_for(
        &self,
        task_id: TaskId,
        condition: WaitCondition,
        timeout: Option<Duration>,
    ) -> Result<WaitResult> {
        let timeout = timeout.unwrap_or(self.config.default_wait_timeout);

        // 로그 기록
        self.log_interaction(InteractionLog::new(
            None,
            Some(task_id),
            InteractionAction::WaitStart {
                condition: format!("{:?}", condition),
            },
        ))
        .await;

        let result = tokio::time::timeout(timeout, self.wait_condition(task_id, condition.clone()))
            .await
            .unwrap_or(Ok(WaitResult::Timeout))?;

        // 완료 로그
        self.log_interaction(InteractionLog::new(
            None,
            Some(task_id),
            InteractionAction::WaitComplete {
                result: format!("{:?}", result),
            },
        ))
        .await;

        Ok(result)
    }

    /// 내부: 조건 대기 구현 (Box::pin 사용하여 재귀 허용)
    fn wait_condition(
        &self,
        task_id: TaskId,
        condition: WaitCondition,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<WaitResult>> + Send + '_>> {
        Box::pin(async move {
            match condition {
                WaitCondition::Complete => self.wait_complete_internal(task_id).await,

                WaitCondition::OutputContains(pattern) => {
                    self.wait_output_contains(task_id, &pattern).await
                }

                WaitCondition::OutputMatches(regex) => {
                    self.wait_output_matches(task_id, &regex).await
                }

                WaitCondition::Signal(signal) => self.wait_signal(task_id, signal).await,

                WaitCondition::State(state) => self.wait_state(task_id, &state).await,

                WaitCondition::Timeout(duration) => {
                    tokio::time::sleep(duration).await;
                    Ok(WaitResult::Timeout)
                }

                WaitCondition::All(conditions) => {
                    for cond in conditions {
                        let result = self.wait_condition(task_id, cond).await?;
                        if !result.is_success() {
                            return Ok(result);
                        }
                    }
                    Ok(WaitResult::Satisfied {
                        condition: "all".to_string(),
                        data: None,
                    })
                }

                WaitCondition::Any(conditions) => {
                    // 첫 번째 성공하는 조건 반환
                    for cond in conditions {
                        let result = self.wait_condition(task_id, cond).await?;
                        if result.is_success() {
                            return Ok(result);
                        }
                    }
                    Ok(WaitResult::Timeout)
                }
            }
        })
    }

    /// Task 완료 대기
    async fn wait_complete_internal(&self, task_id: TaskId) -> Result<WaitResult> {
        loop {
            if let Some(status) = self.task_manager.get_status(task_id).await {
                if !status.is_running {
                    return Ok(WaitResult::Satisfied {
                        condition: "complete".to_string(),
                        data: Some(format!("{:?}", status.state)),
                    });
                }
            } else {
                return Ok(WaitResult::Error("Task not found".to_string()));
            }
            tokio::time::sleep(self.config.output_poll_interval).await;
        }
    }

    /// 출력에 패턴 포함 대기
    async fn wait_output_contains(&self, task_id: TaskId, pattern: &str) -> Result<WaitResult> {
        loop {
            // 로그에서 패턴 검색
            if let Some(logs) = self.log_manager.get_buffer(&task_id.to_string()).await.map(|b| b.entries().cloned().collect::<Vec<_>>()) {
                for entry in logs {
                    if entry.content.contains(pattern) {
                        return Ok(WaitResult::Satisfied {
                            condition: format!("output_contains({})", pattern),
                            data: Some(entry.content.clone()),
                        });
                    }
                }
            }

            // Task 종료 확인
            if let Some(status) = self.task_manager.get_status(task_id).await {
                if !status.is_running {
                    return Ok(WaitResult::Error(
                        "Task finished without matching pattern".to_string(),
                    ));
                }
            }

            tokio::time::sleep(self.config.output_poll_interval).await;
        }
    }

    /// 출력에 정규식 매치 대기
    async fn wait_output_matches(&self, task_id: TaskId, regex_str: &str) -> Result<WaitResult> {
        let regex = regex::Regex::new(regex_str)
            .map_err(|e| Error::Task(format!("Invalid regex: {}", e)))?;

        loop {
            if let Some(logs) = self.log_manager.get_buffer(&task_id.to_string()).await.map(|b| b.entries().cloned().collect::<Vec<_>>()) {
                for entry in logs {
                    if regex.is_match(&entry.content) {
                        return Ok(WaitResult::Satisfied {
                            condition: format!("output_matches({})", regex_str),
                            data: Some(entry.content.clone()),
                        });
                    }
                }
            }

            if let Some(status) = self.task_manager.get_status(task_id).await {
                if !status.is_running {
                    return Ok(WaitResult::Error(
                        "Task finished without matching regex".to_string(),
                    ));
                }
            }

            tokio::time::sleep(self.config.output_poll_interval).await;
        }
    }

    /// 특정 신호 대기
    async fn wait_signal(&self, task_id: TaskId, expected: TaskSignal) -> Result<WaitResult> {
        if let Some(mut rx) = self.subscribe(task_id).await {
            loop {
                match rx.recv().await {
                    Ok(TaskMessage::Signal(signal)) if signal == expected => {
                        return Ok(WaitResult::Satisfied {
                            condition: format!("signal:{:?}", expected),
                            data: None,
                        });
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        return Ok(WaitResult::Error("Channel closed".to_string()));
                    }
                    _ => continue,
                }
            }
        } else {
            Ok(WaitResult::Error("No broadcast channel".to_string()))
        }
    }

    /// 특정 상태 대기
    async fn wait_state(&self, task_id: TaskId, expected_state: &str) -> Result<WaitResult> {
        loop {
            if let Some(status) = self.task_manager.get_status(task_id).await {
                let current_state = format!("{:?}", status.state);
                if current_state.contains(expected_state) {
                    return Ok(WaitResult::Satisfied {
                        condition: format!("state:{}", expected_state),
                        data: Some(current_state),
                    });
                }

                if !status.is_running && !current_state.contains(expected_state) {
                    return Ok(WaitResult::Error(format!(
                        "Task ended in state {} instead of {}",
                        current_state, expected_state
                    )));
                }
            }
            tokio::time::sleep(self.config.output_poll_interval).await;
        }
    }

    /// Task 완료 대기 및 결과 반환
    pub async fn wait_complete(&self, task_id: TaskId) -> Result<WaitResult> {
        self.wait_for(task_id, WaitCondition::Complete, None).await
    }

    // ========================================================================
    // Task Groups
    // ========================================================================

    /// 새 그룹 생성
    pub async fn create_group(&self, name: impl Into<String>) -> TaskGroupId {
        let group = TaskGroup::new(name);
        let id = group.id;

        let mut groups = self.groups.write().await;
        groups.insert(id, group);

        // 로그 기록
        self.log_interaction(InteractionLog::new(
            None,
            None,
            InteractionAction::GroupCreate { group_id: id },
        ))
        .await;

        id
    }

    /// Task를 그룹에 추가
    pub async fn add_to_group(&self, group_id: TaskGroupId, task_id: TaskId) -> Result<()> {
        let mut groups = self.groups.write().await;
        if let Some(group) = groups.get_mut(&group_id) {
            group.add_task(task_id);

            // 로그 기록
            drop(groups); // 락 해제
            self.log_interaction(InteractionLog::new(
                Some(task_id),
                None,
                InteractionAction::GroupJoin { group_id },
            ))
            .await;

            Ok(())
        } else {
            Err(Error::NotFound(format!("Group {} not found", group_id)))
        }
    }

    /// 그룹의 모든 Task 정지
    pub async fn stop_group(&self, group_id: TaskGroupId) -> Result<()> {
        let task_ids: Vec<TaskId> = {
            let groups = self.groups.read().await;
            groups
                .get(&group_id)
                .map(|g| g.tasks.clone())
                .unwrap_or_default()
        };

        for task_id in task_ids {
            let _ = self.stop(task_id).await;
        }

        Ok(())
    }

    /// 그룹의 모든 Task 완료 대기
    pub async fn wait_group_complete(&self, group_id: TaskGroupId) -> Result<Vec<WaitResult>> {
        let task_ids: Vec<TaskId> = {
            let groups = self.groups.read().await;
            groups
                .get(&group_id)
                .map(|g| g.tasks.clone())
                .unwrap_or_default()
        };

        let mut results = Vec::new();
        for task_id in task_ids {
            let result = self.wait_complete(task_id).await?;
            results.push(result);
        }

        Ok(results)
    }

    // ========================================================================
    // Interaction Logs
    // ========================================================================

    /// 상호작용 로그 기록
    async fn log_interaction(&self, log: InteractionLog) {
        let mut logs = self.interaction_logs.write().await;
        logs.push(log);

        // 최대 크기 제한
        let max_logs = self.config.max_interaction_logs;
        if logs.len() > max_logs {
            let drain_count = logs.len() - max_logs;
            logs.drain(0..drain_count);
        }
    }

    /// 상호작용 로그 조회
    pub async fn get_interaction_logs(&self) -> Vec<InteractionLog> {
        let logs = self.interaction_logs.read().await;
        logs.clone()
    }

    /// 특정 Task의 상호작용 로그 조회
    pub async fn get_task_interactions(&self, task_id: TaskId) -> Vec<InteractionLog> {
        let logs = self.interaction_logs.read().await;
        logs.iter()
            .filter(|log| log.from_task == Some(task_id) || log.to_task == Some(task_id))
            .cloned()
            .collect()
    }

    /// 상호작용 로그 분석용 텍스트 생성
    pub async fn format_interactions_for_analysis(&self) -> String {
        let logs = self.interaction_logs.read().await;
        logs.iter()
            .map(|l| l.format_for_analysis())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// 상호작용 로그 초기화
    pub async fn clear_interaction_logs(&self) {
        let mut logs = self.interaction_logs.write().await;
        logs.clear();
    }

    // ========================================================================
    // Utility Methods
    // ========================================================================

    /// Task의 로그 가져오기
    pub async fn get_task_logs(&self, task_id: TaskId) -> Option<Vec<LogEntry>> {
        self.log_manager.get_buffer(&task_id.to_string()).await.map(|b| b.entries().cloned().collect::<Vec<_>>())
    }

    /// Task Manager 접근
    pub fn task_manager(&self) -> &Arc<TaskManager> {
        &self.task_manager
    }

    /// Log Manager 접근
    pub fn log_manager(&self) -> &Arc<TaskLogManager> {
        &self.log_manager
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_group_id() {
        let id1 = TaskGroupId::new();
        let id2 = TaskGroupId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_task_message() {
        let msg = TaskMessage::Text("hello".to_string());
        assert!(matches!(msg, TaskMessage::Text(_)));

        let signal = TaskMessage::Signal(TaskSignal::Ready);
        assert!(matches!(signal, TaskMessage::Signal(TaskSignal::Ready)));
    }

    #[test]
    fn test_wait_condition() {
        let cond = WaitCondition::output_contains("Listening");
        assert!(matches!(cond, WaitCondition::OutputContains(_)));

        let cond = WaitCondition::ready();
        assert!(matches!(cond, WaitCondition::Signal(TaskSignal::Ready)));
    }

    #[test]
    fn test_interaction_log_format() {
        let log = InteractionLog::new(None, None, InteractionAction::Spawn);
        let formatted = log.format_for_analysis();
        assert!(formatted.contains("Spawn"));
    }

    #[test]
    fn test_task_group() {
        let mut group = TaskGroup::new("test-group");
        let task_id = TaskId::new();

        group.add_task(task_id);
        assert!(group.tasks.contains(&task_id));

        group.remove_task(task_id);
        assert!(!group.tasks.contains(&task_id));
    }
}
