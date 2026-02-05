//! Runtime Context
//!
//! Agent 실행 중 공유되는 상태와 컨텍스트입니다.

#![allow(dead_code)]

use super::output::{ExecuteOutput, PlanOutput, ReflectOutput, ThinkOutput};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

// ============================================================================
// TurnInfo - 턴 정보
// ============================================================================

/// 개별 턴 정보
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnInfo {
    /// 턴 번호
    pub turn: u32,

    /// Think 출력
    pub think_output: Option<ThinkOutput>,

    /// Plan 출력
    pub plan_output: Option<PlanOutput>,

    /// Execute 출력
    pub execute_output: Option<ExecuteOutput>,

    /// Reflect 출력
    pub reflect_output: Option<ReflectOutput>,

    /// 시작 시간
    pub started_at: DateTime<Utc>,

    /// 완료 시간
    pub completed_at: Option<DateTime<Utc>>,

    /// 소요 시간 (ms)
    pub duration_ms: Option<u64>,
}

impl TurnInfo {
    /// 새 턴 생성
    pub fn new(turn: u32) -> Self {
        Self {
            turn,
            think_output: None,
            plan_output: None,
            execute_output: None,
            reflect_output: None,
            started_at: Utc::now(),
            completed_at: None,
            duration_ms: None,
        }
    }

    /// 턴 완료 처리
    pub fn complete(&mut self) {
        self.completed_at = Some(Utc::now());
        self.duration_ms = Some((Utc::now() - self.started_at).num_milliseconds() as u64);
    }
}

// ============================================================================
// ContextSnapshot - 컨텍스트 스냅샷
// ============================================================================

/// 컨텍스트 스냅샷 (핸드오프, 복원용)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSnapshot {
    /// 스냅샷 ID
    pub id: String,

    /// 생성 시간
    pub created_at: DateTime<Utc>,

    /// 현재 턴
    pub current_turn: u32,

    /// 메시지 히스토리 (직렬화된)
    pub messages: Vec<serde_json::Value>,

    /// 마지막 사용자 입력
    pub last_user_input: Option<String>,

    /// 마지막 응답
    pub last_response: Option<String>,

    /// 사용자 정의 데이터
    pub custom_data: HashMap<String, serde_json::Value>,

    /// 토큰 사용량
    pub token_usage: TokenUsage,
}

/// 토큰 사용량
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    /// 입력 토큰
    pub input_tokens: u64,
    /// 출력 토큰
    pub output_tokens: u64,
    /// 총 토큰
    pub total_tokens: u64,
}

// ============================================================================
// RuntimeContext - 런타임 컨텍스트
// ============================================================================

/// Agent 런타임 컨텍스트
///
/// 실행 중 모든 상태와 히스토리를 관리합니다.
pub struct RuntimeContext {
    /// 세션 ID
    session_id: String,

    /// 작업 디렉토리
    working_dir: PathBuf,

    /// 현재 턴
    current_turn: u32,

    /// 턴 히스토리
    turn_history: Vec<TurnInfo>,

    /// 현재 턴 정보
    current_turn_info: TurnInfo,

    /// 메시지 히스토리 (LLM 대화)
    messages: Vec<Message>,

    /// 시스템 프롬프트
    system_prompt: Option<String>,

    /// 마지막 사용자 입력
    last_user_input: Option<String>,

    /// 완료 여부
    is_complete: bool,

    /// 완료 이유
    completion_reason: Option<String>,

    /// 토큰 사용량
    token_usage: TokenUsage,

    /// 사용자 정의 데이터
    custom_data: HashMap<String, serde_json::Value>,

    /// 시작 시간
    started_at: DateTime<Utc>,
}

/// 메시지 타입
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// 역할
    pub role: MessageRole,
    /// 내용
    pub content: String,
    /// 생성 시간
    pub created_at: DateTime<Utc>,
    /// Tool 호출 (assistant인 경우)
    pub tool_calls: Option<Vec<ToolCallInfo>>,
    /// Tool 결과 (tool인 경우)
    pub tool_call_id: Option<String>,
}

/// 메시지 역할
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

/// Tool 호출 정보
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallInfo {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

impl RuntimeContext {
    /// 새 컨텍스트 생성
    pub fn new(session_id: impl Into<String>, working_dir: impl Into<PathBuf>) -> Self {
        Self {
            session_id: session_id.into(),
            working_dir: working_dir.into(),
            current_turn: 0,
            turn_history: Vec::new(),
            current_turn_info: TurnInfo::new(0),
            messages: Vec::new(),
            system_prompt: None,
            last_user_input: None,
            is_complete: false,
            completion_reason: None,
            token_usage: TokenUsage::default(),
            custom_data: HashMap::new(),
            started_at: Utc::now(),
        }
    }

    // ========== Getters ==========

    /// 세션 ID
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// 작업 디렉토리
    pub fn working_dir(&self) -> &PathBuf {
        &self.working_dir
    }

    /// 현재 턴
    pub fn current_turn(&self) -> u32 {
        self.current_turn
    }

    /// 완료 여부
    pub fn is_complete(&self) -> bool {
        self.is_complete
    }

    /// 메시지 목록
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// 시스템 프롬프트
    pub fn system_prompt(&self) -> Option<&str> {
        self.system_prompt.as_deref()
    }

    /// 마지막 사용자 입력
    pub fn last_user_input(&self) -> Option<&str> {
        self.last_user_input.as_deref()
    }

    /// 토큰 사용량
    pub fn token_usage(&self) -> &TokenUsage {
        &self.token_usage
    }

    /// 턴 히스토리
    pub fn turn_history(&self) -> &[TurnInfo] {
        &self.turn_history
    }

    /// 마지막 Think 출력
    pub fn last_think_output(&self) -> Option<&ThinkOutput> {
        self.current_turn_info.think_output.as_ref()
    }

    /// 마지막 Plan 출력
    pub fn last_plan_output(&self) -> Option<&PlanOutput> {
        self.current_turn_info.plan_output.as_ref()
    }

    /// 마지막 Execute 출력
    pub fn last_execute_output(&self) -> Option<&ExecuteOutput> {
        self.current_turn_info.execute_output.as_ref()
    }

    /// 마지막 Reflect 출력
    pub fn last_reflect_output(&self) -> Option<&ReflectOutput> {
        self.current_turn_info.reflect_output.as_ref()
    }

    // ========== Setters ==========

    /// 시스템 프롬프트 설정
    pub fn set_system_prompt(&mut self, prompt: impl Into<String>) {
        self.system_prompt = Some(prompt.into());
    }

    /// 사용자 메시지 추가
    pub fn add_user_message(&mut self, content: impl Into<String>) {
        let content = content.into();
        self.last_user_input = Some(content.clone());
        self.messages.push(Message {
            role: MessageRole::User,
            content,
            created_at: Utc::now(),
            tool_calls: None,
            tool_call_id: None,
        });
    }

    /// Assistant 메시지 추가
    pub fn add_assistant_message(&mut self, content: impl Into<String>) {
        self.messages.push(Message {
            role: MessageRole::Assistant,
            content: content.into(),
            created_at: Utc::now(),
            tool_calls: None,
            tool_call_id: None,
        });
    }

    /// Tool 호출과 함께 Assistant 메시지 추가
    pub fn add_assistant_with_tools(
        &mut self,
        content: impl Into<String>,
        tool_calls: Vec<ToolCallInfo>,
    ) {
        self.messages.push(Message {
            role: MessageRole::Assistant,
            content: content.into(),
            created_at: Utc::now(),
            tool_calls: Some(tool_calls),
            tool_call_id: None,
        });
    }

    /// Tool 결과 추가
    pub fn add_tool_result(&mut self, tool_call_id: impl Into<String>, content: impl Into<String>) {
        self.messages.push(Message {
            role: MessageRole::Tool,
            content: content.into(),
            created_at: Utc::now(),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
        });
    }

    /// Think 출력 설정
    pub fn set_think_output(&mut self, output: ThinkOutput) {
        self.current_turn_info.think_output = Some(output);
    }

    /// Plan 출력 설정
    pub fn set_plan_output(&mut self, output: PlanOutput) {
        self.current_turn_info.plan_output = Some(output);
    }

    /// Execute 출력 설정
    pub fn set_execute_output(&mut self, output: ExecuteOutput) {
        self.current_turn_info.execute_output = Some(output);
    }

    /// Reflect 출력 설정
    pub fn set_reflect_output(&mut self, output: ReflectOutput) {
        self.current_turn_info.reflect_output = Some(output);
    }

    /// 턴 증가
    pub fn increment_turn(&mut self) {
        self.current_turn_info.complete();
        self.turn_history.push(self.current_turn_info.clone());
        self.current_turn += 1;
        self.current_turn_info = TurnInfo::new(self.current_turn);
    }

    /// 완료 표시
    pub fn mark_complete(&mut self, reason: impl Into<String>) {
        self.is_complete = true;
        self.completion_reason = Some(reason.into());
    }

    /// 토큰 사용량 추가
    pub fn add_token_usage(&mut self, input: u64, output: u64) {
        self.token_usage.input_tokens += input;
        self.token_usage.output_tokens += output;
        self.token_usage.total_tokens += input + output;
    }

    /// 사용자 정의 데이터 설정
    pub fn set_custom_data(&mut self, key: impl Into<String>, value: serde_json::Value) {
        self.custom_data.insert(key.into(), value);
    }

    /// 사용자 정의 데이터 조회
    pub fn get_custom_data(&self, key: &str) -> Option<&serde_json::Value> {
        self.custom_data.get(key)
    }

    // ========== Snapshot ==========

    /// 스냅샷 생성
    pub fn create_snapshot(&self) -> ContextSnapshot {
        ContextSnapshot {
            id: uuid::Uuid::new_v4().to_string(),
            created_at: Utc::now(),
            current_turn: self.current_turn,
            messages: self
                .messages
                .iter()
                .map(|m| serde_json::to_value(m).unwrap_or_default())
                .collect(),
            last_user_input: self.last_user_input.clone(),
            last_response: self
                .messages
                .iter()
                .rfind(|m| m.role == MessageRole::Assistant)
                .map(|m| m.content.clone()),
            custom_data: self.custom_data.clone(),
            token_usage: self.token_usage.clone(),
        }
    }

    /// 스냅샷에서 복원
    pub fn restore_from_snapshot(&mut self, snapshot: ContextSnapshot) {
        self.current_turn = snapshot.current_turn;
        self.messages = snapshot
            .messages
            .iter()
            .filter_map(|v| serde_json::from_value(v.clone()).ok())
            .collect();
        self.last_user_input = snapshot.last_user_input;
        self.custom_data = snapshot.custom_data;
        self.token_usage = snapshot.token_usage;
    }
}
