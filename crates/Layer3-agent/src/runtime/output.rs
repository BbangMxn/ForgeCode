//! Agent Phase Outputs
//!
//! 각 Agent 단계(Think, Plan, Execute, Reflect)의 출력 타입입니다.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// ThinkOutput - Think 단계 출력
// ============================================================================

/// Think 단계 출력
///
/// 상황 분석, 추론 과정, 다음 단계 제안을 포함합니다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkOutput {
    /// 추론 과정 (Chain-of-Thought, Tree-of-Thought 등)
    pub reasoning: String,

    /// 현재 상황 분석
    pub analysis: Option<String>,

    /// 핵심 인사이트
    pub insights: Vec<String>,

    /// 다음 단계 제안
    pub suggestions: Vec<String>,

    /// 필요한 정보/도구
    pub requirements: Vec<String>,

    /// 확신도 (0.0 ~ 1.0)
    pub confidence: f32,

    /// 추가 메타데이터
    pub metadata: HashMap<String, serde_json::Value>,
}

impl ThinkOutput {
    /// 새 ThinkOutput 생성
    pub fn new(reasoning: impl Into<String>) -> Self {
        Self {
            reasoning: reasoning.into(),
            analysis: None,
            insights: Vec::new(),
            suggestions: Vec::new(),
            requirements: Vec::new(),
            confidence: 0.5,
            metadata: HashMap::new(),
        }
    }

    /// 분석 추가
    pub fn with_analysis(mut self, analysis: impl Into<String>) -> Self {
        self.analysis = Some(analysis.into());
        self
    }

    /// 인사이트 추가
    pub fn with_insight(mut self, insight: impl Into<String>) -> Self {
        self.insights.push(insight.into());
        self
    }

    /// 제안 추가
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestions.push(suggestion.into());
        self
    }

    /// 확신도 설정
    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }
}

impl Default for ThinkOutput {
    fn default() -> Self {
        Self::new("")
    }
}

// ============================================================================
// PlanOutput - Plan 단계 출력
// ============================================================================

/// Plan 단계 출력
///
/// 실행할 액션들의 계획입니다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanOutput {
    /// 계획 요약
    pub summary: String,

    /// 실행할 액션 목록
    pub actions: Vec<ActionItem>,

    /// 예상 단계 수
    pub estimated_steps: u32,

    /// 병렬 실행 가능 여부
    pub can_parallelize: bool,

    /// 대체 계획 (실패 시)
    pub fallback: Option<Box<PlanOutput>>,

    /// 추가 메타데이터
    pub metadata: HashMap<String, serde_json::Value>,
}

impl PlanOutput {
    /// 새 PlanOutput 생성
    pub fn new(summary: impl Into<String>) -> Self {
        Self {
            summary: summary.into(),
            actions: Vec::new(),
            estimated_steps: 0,
            can_parallelize: false,
            fallback: None,
            metadata: HashMap::new(),
        }
    }

    /// Think 출력에서 생성
    pub fn from_think(think: Option<ThinkOutput>) -> Self {
        let think = think.unwrap_or_default();
        let mut plan = Self::new(&think.reasoning);

        for suggestion in think.suggestions {
            plan.actions
                .push(ActionItem::new(ActionType::General, suggestion));
        }

        plan.estimated_steps = plan.actions.len() as u32;
        plan
    }

    /// 액션 추가
    pub fn with_action(mut self, action: ActionItem) -> Self {
        self.actions.push(action);
        self.estimated_steps = self.actions.len() as u32;
        self
    }

    /// 병렬 실행 설정
    pub fn parallelizable(mut self) -> Self {
        self.can_parallelize = true;
        self
    }
}

/// 액션 항목
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionItem {
    /// 액션 ID
    pub id: String,

    /// 액션 타입
    pub action_type: ActionType,

    /// 설명
    pub description: String,

    /// Tool 요청 (Tool 액션인 경우)
    pub tool_request: Option<ToolRequest>,

    /// 의존하는 액션 ID 목록
    pub depends_on: Vec<String>,

    /// 우선순위 (높을수록 먼저)
    pub priority: i32,

    /// 예상 소요 시간 (초)
    pub estimated_duration: Option<u32>,
}

impl ActionItem {
    /// 새 ActionItem 생성
    pub fn new(action_type: ActionType, description: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string()[..8].to_string(),
            action_type,
            description: description.into(),
            tool_request: None,
            depends_on: Vec::new(),
            priority: 0,
            estimated_duration: None,
        }
    }

    /// Tool 요청 액션 생성
    pub fn tool(
        name: impl Into<String>,
        args: serde_json::Value,
        description: impl Into<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string()[..8].to_string(),
            action_type: ActionType::ToolCall,
            description: description.into(),
            tool_request: Some(ToolRequest {
                tool_name: name.into(),
                arguments: args,
            }),
            depends_on: Vec::new(),
            priority: 0,
            estimated_duration: None,
        }
    }

    /// 의존성 추가
    pub fn depends_on(mut self, action_id: impl Into<String>) -> Self {
        self.depends_on.push(action_id.into());
        self
    }

    /// 우선순위 설정
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }
}

/// 액션 타입
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionType {
    /// Tool 호출
    ToolCall,
    /// 사용자 질문
    AskUser,
    /// 서브에이전트 생성
    SpawnAgent,
    /// 대기
    Wait,
    /// 일반 액션
    General,
    /// 완료
    Complete,
}

/// Tool 요청
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRequest {
    /// Tool 이름
    pub tool_name: String,
    /// 인자
    pub arguments: serde_json::Value,
}

// ============================================================================
// ExecuteOutput - Execute 단계 출력
// ============================================================================

/// Execute 단계 출력
///
/// 실행 결과입니다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteOutput {
    /// 실행된 액션 수
    pub actions_executed: u32,

    /// 성공한 액션 수
    pub actions_succeeded: u32,

    /// 실패한 액션 수
    pub actions_failed: u32,

    /// 각 액션의 결과
    pub results: Vec<ActionResult>,

    /// 최종 출력 (사용자에게 표시)
    pub output: String,

    /// 생성/수정된 파일
    pub affected_files: Vec<String>,

    /// 완료 여부
    pub is_complete: bool,

    /// 다음 액션 필요 여부
    pub needs_more_actions: bool,

    /// 추가 메타데이터
    pub metadata: HashMap<String, serde_json::Value>,
}

impl ExecuteOutput {
    /// 새 ExecuteOutput 생성
    pub fn new() -> Self {
        Self {
            actions_executed: 0,
            actions_succeeded: 0,
            actions_failed: 0,
            results: Vec::new(),
            output: String::new(),
            affected_files: Vec::new(),
            is_complete: false,
            needs_more_actions: false,
            metadata: HashMap::new(),
        }
    }

    /// 결과 추가
    pub fn with_result(mut self, result: ActionResult) -> Self {
        self.actions_executed += 1;
        if result.success {
            self.actions_succeeded += 1;
        } else {
            self.actions_failed += 1;
        }
        self.results.push(result);
        self
    }

    /// 출력 설정
    pub fn with_output(mut self, output: impl Into<String>) -> Self {
        self.output = output.into();
        self
    }

    /// 완료 표시
    pub fn mark_complete(mut self) -> Self {
        self.is_complete = true;
        self
    }
}

impl Default for ExecuteOutput {
    fn default() -> Self {
        Self::new()
    }
}

/// 액션 실행 결과
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    /// 액션 ID
    pub action_id: String,

    /// 성공 여부
    pub success: bool,

    /// 출력
    pub output: String,

    /// 에러 (실패 시)
    pub error: Option<String>,

    /// 소요 시간 (ms)
    pub duration_ms: u64,
}

impl ActionResult {
    /// 성공 결과 생성
    pub fn success(
        action_id: impl Into<String>,
        output: impl Into<String>,
        duration_ms: u64,
    ) -> Self {
        Self {
            action_id: action_id.into(),
            success: true,
            output: output.into(),
            error: None,
            duration_ms,
        }
    }

    /// 실패 결과 생성
    pub fn failure(
        action_id: impl Into<String>,
        error: impl Into<String>,
        duration_ms: u64,
    ) -> Self {
        Self {
            action_id: action_id.into(),
            success: false,
            output: String::new(),
            error: Some(error.into()),
            duration_ms,
        }
    }
}

// ============================================================================
// ReflectOutput - Reflect 단계 출력
// ============================================================================

/// Reflect 단계 출력
///
/// 실행 결과에 대한 자기 평가입니다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflectOutput {
    /// 성공 여부 평가
    pub success_assessment: bool,

    /// 품질 점수 (0.0 ~ 1.0)
    pub quality_score: f32,

    /// 평가 설명
    pub assessment: String,

    /// 잘된 점
    pub positives: Vec<String>,

    /// 개선할 점
    pub improvements: Vec<String>,

    /// 다음 시도에 대한 제안
    pub next_suggestions: Vec<String>,

    /// 다시 시도 필요 여부
    pub should_retry: bool,

    /// 재시도 시 다른 접근 방식 필요 여부
    pub needs_different_approach: bool,

    /// 추가 메타데이터
    pub metadata: HashMap<String, serde_json::Value>,
}

impl ReflectOutput {
    /// 새 ReflectOutput 생성
    pub fn new(assessment: impl Into<String>) -> Self {
        Self {
            success_assessment: true,
            quality_score: 0.5,
            assessment: assessment.into(),
            positives: Vec::new(),
            improvements: Vec::new(),
            next_suggestions: Vec::new(),
            should_retry: false,
            needs_different_approach: false,
            metadata: HashMap::new(),
        }
    }

    /// Execute 출력에서 생성
    pub fn from_execute(execute: Option<&ExecuteOutput>) -> Self {
        match execute {
            Some(exec) => {
                let success = exec.actions_failed == 0;
                let score = if exec.actions_executed > 0 {
                    exec.actions_succeeded as f32 / exec.actions_executed as f32
                } else {
                    0.5
                };

                Self::new(format!(
                    "Executed {} actions: {} succeeded, {} failed",
                    exec.actions_executed, exec.actions_succeeded, exec.actions_failed
                ))
                .with_success(success)
                .with_quality_score(score)
            }
            None => Self::new("No execution output to reflect on"),
        }
    }

    /// 성공 여부 설정
    pub fn with_success(mut self, success: bool) -> Self {
        self.success_assessment = success;
        self
    }

    /// 품질 점수 설정
    pub fn with_quality_score(mut self, score: f32) -> Self {
        self.quality_score = score.clamp(0.0, 1.0);
        self
    }

    /// 잘된 점 추가
    pub fn with_positive(mut self, positive: impl Into<String>) -> Self {
        self.positives.push(positive.into());
        self
    }

    /// 개선점 추가
    pub fn with_improvement(mut self, improvement: impl Into<String>) -> Self {
        self.improvements.push(improvement.into());
        self
    }

    /// 재시도 필요 표시
    pub fn needs_retry(mut self) -> Self {
        self.should_retry = true;
        self
    }
}

impl Default for ReflectOutput {
    fn default() -> Self {
        Self::new("Default reflection")
    }
}
