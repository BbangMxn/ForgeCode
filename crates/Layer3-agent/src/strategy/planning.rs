//! Planning Strategies
//!
//! 다양한 계획 수립 전략 구현입니다.
//!
//! - Simple Planning: 단순 선형 계획
//! - Hierarchical Planning: 계층적 계획
//! - ReAct Planning: 반응형 계획

use async_trait::async_trait;
use forge_foundation::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// Plan - 계획 타입
// ============================================================================

/// 실행 계획
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    /// 계획 ID
    pub id: String,

    /// 계획 요약
    pub summary: String,

    /// 계획 단계들
    pub steps: Vec<PlanStep>,

    /// 예상 총 시간 (초)
    pub estimated_duration: Option<u32>,

    /// 병렬 실행 가능 여부
    pub can_parallelize: bool,

    /// 계획 타입
    pub plan_type: String,

    /// 메타데이터
    pub metadata: HashMap<String, serde_json::Value>,
}

/// 계획 단계
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    /// 단계 ID
    pub id: String,

    /// 단계 번호
    pub order: u32,

    /// 설명
    pub description: String,

    /// 액션 타입
    pub action_type: PlanActionType,

    /// Tool 이름 (Tool 액션인 경우)
    pub tool_name: Option<String>,

    /// Tool 인자 (Tool 액션인 경우)
    pub tool_args: Option<serde_json::Value>,

    /// 의존하는 단계 ID들
    pub dependencies: Vec<String>,

    /// 하위 단계들 (계층적 계획용)
    pub sub_steps: Vec<PlanStep>,

    /// 예상 시간 (초)
    pub estimated_duration: Option<u32>,

    /// 조건 (조건부 실행용)
    pub condition: Option<String>,

    /// 우선순위
    pub priority: i32,
}

/// 계획 액션 타입
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanActionType {
    /// Tool 호출
    ToolCall,
    /// 사용자에게 질문
    AskUser,
    /// 서브에이전트 생성
    SpawnSubAgent,
    /// 대기
    Wait,
    /// 조건 분기
    Conditional,
    /// 반복
    Loop,
    /// 완료
    Complete,
}

impl Plan {
    /// 새 계획 생성
    pub fn new(summary: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string()[..8].to_string(),
            summary: summary.into(),
            steps: Vec::new(),
            estimated_duration: None,
            can_parallelize: false,
            plan_type: "unknown".to_string(),
            metadata: HashMap::new(),
        }
    }

    /// 단계 추가
    pub fn with_step(mut self, step: PlanStep) -> Self {
        self.steps.push(step);
        self
    }

    /// 병렬 실행 가능 설정
    pub fn parallelizable(mut self) -> Self {
        self.can_parallelize = true;
        self
    }

    /// 예상 시간 계산
    pub fn calculate_duration(&mut self) {
        let total: u32 = self.steps.iter().filter_map(|s| s.estimated_duration).sum();
        self.estimated_duration = Some(total);
    }

    /// 실행 순서 정렬 (의존성 기반)
    pub fn sort_by_dependencies(&mut self) {
        // 위상 정렬 (간단 버전)
        self.steps.sort_by(|a, b| {
            if a.dependencies.contains(&b.id) {
                std::cmp::Ordering::Greater
            } else if b.dependencies.contains(&a.id) {
                std::cmp::Ordering::Less
            } else {
                a.priority.cmp(&b.priority).reverse()
            }
        });

        // order 업데이트
        for (i, step) in self.steps.iter_mut().enumerate() {
            step.order = i as u32;
        }
    }
}

impl PlanStep {
    /// 새 단계 생성
    pub fn new(description: impl Into<String>, action_type: PlanActionType) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string()[..8].to_string(),
            order: 0,
            description: description.into(),
            action_type,
            tool_name: None,
            tool_args: None,
            dependencies: Vec::new(),
            sub_steps: Vec::new(),
            estimated_duration: None,
            condition: None,
            priority: 0,
        }
    }

    /// Tool 호출 단계 생성
    pub fn tool_call(
        tool_name: impl Into<String>,
        args: serde_json::Value,
        description: impl Into<String>,
    ) -> Self {
        let mut step = Self::new(description, PlanActionType::ToolCall);
        step.tool_name = Some(tool_name.into());
        step.tool_args = Some(args);
        step
    }

    /// 의존성 추가
    pub fn depends_on(mut self, step_id: impl Into<String>) -> Self {
        self.dependencies.push(step_id.into());
        self
    }

    /// 하위 단계 추가
    pub fn with_sub_step(mut self, sub: PlanStep) -> Self {
        self.sub_steps.push(sub);
        self
    }

    /// 우선순위 설정
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// 조건 설정
    pub fn with_condition(mut self, condition: impl Into<String>) -> Self {
        self.condition = Some(condition.into());
        self
    }
}

// ============================================================================
// PlanningStrategy - 계획 전략 트레이트
// ============================================================================

/// 계획 컨텍스트
pub struct PlanningContext<'a> {
    /// 목표
    pub goal: &'a str,
    /// 추론 결과
    pub reasoning: Option<&'a str>,
    /// 사용 가능한 Tool 목록
    pub available_tools: &'a [String],
    /// 제약 조건
    pub constraints: Vec<String>,
    /// 최대 단계 수
    pub max_steps: u32,
}

/// 계획 전략 트레이트
#[async_trait]
pub trait PlanningStrategy: Send + Sync {
    /// 전략 이름
    fn name(&self) -> &str;

    /// 전략 설명
    fn description(&self) -> &str;

    /// 계획 수립
    async fn plan(&self, ctx: &PlanningContext<'_>) -> Result<Plan>;

    /// 계획 프롬프트 생성
    fn build_prompt(&self, ctx: &PlanningContext<'_>) -> String;

    /// 계획 검증
    fn validate(&self, plan: &Plan) -> Result<()> {
        // 기본 검증: 순환 의존성 체크
        for step in &plan.steps {
            for dep in &step.dependencies {
                if step.id == *dep {
                    return Err(forge_foundation::Error::Config(format!(
                        "Circular dependency detected: {}",
                        step.id
                    )));
                }
            }
        }
        Ok(())
    }
}

// ============================================================================
// SimplePlanning - 단순 선형 계획
// ============================================================================

/// 단순 선형 계획
///
/// 순차적으로 실행할 단계들을 생성합니다.
pub struct SimplePlanning {
    max_steps: u32,
}

impl SimplePlanning {
    pub fn new() -> Self {
        Self { max_steps: 20 }
    }

    pub fn with_max_steps(mut self, max: u32) -> Self {
        self.max_steps = max;
        self
    }
}

impl Default for SimplePlanning {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PlanningStrategy for SimplePlanning {
    fn name(&self) -> &str {
        "simple"
    }

    fn description(&self) -> &str {
        "Linear sequential planning"
    }

    async fn plan(&self, ctx: &PlanningContext<'_>) -> Result<Plan> {
        // 실제 구현에서는 LLM 호출
        let mut plan = Plan::new(format!("Plan for: {}", ctx.goal));
        plan.plan_type = "simple".to_string();

        // 예시 단계들
        plan = plan
            .with_step(PlanStep::new("Analyze the goal", PlanActionType::ToolCall))
            .with_step(PlanStep::new(
                "Execute main action",
                PlanActionType::ToolCall,
            ))
            .with_step(PlanStep::new("Verify result", PlanActionType::ToolCall));

        Ok(plan)
    }

    fn build_prompt(&self, ctx: &PlanningContext<'_>) -> String {
        let tools = ctx.available_tools.join(", ");

        format!(
            r#"Create a simple step-by-step plan to achieve the following goal.

Goal: {}

Available tools: {}

Maximum steps: {}

For each step, specify:
1. What action to take
2. Which tool to use (if applicable)
3. Expected outcome

Output the plan as a numbered list."#,
            ctx.goal, tools, self.max_steps
        )
    }
}

// ============================================================================
// HierarchicalPlanning - 계층적 계획
// ============================================================================

/// 계층적 계획
///
/// 고수준 계획을 세부 계획으로 분해합니다.
pub struct HierarchicalPlanning {
    max_depth: u32,
    max_steps_per_level: u32,
}

impl HierarchicalPlanning {
    pub fn new() -> Self {
        Self {
            max_depth: 3,
            max_steps_per_level: 5,
        }
    }

    pub fn with_max_depth(mut self, depth: u32) -> Self {
        self.max_depth = depth;
        self
    }

    pub fn with_max_steps_per_level(mut self, steps: u32) -> Self {
        self.max_steps_per_level = steps;
        self
    }
}

impl Default for HierarchicalPlanning {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PlanningStrategy for HierarchicalPlanning {
    fn name(&self) -> &str {
        "hierarchical"
    }

    fn description(&self) -> &str {
        "Hierarchical planning with high-level goals broken into sub-tasks"
    }

    async fn plan(&self, ctx: &PlanningContext<'_>) -> Result<Plan> {
        // 실제 구현에서는 LLM 호출로 계층적 분해
        let mut plan = Plan::new(format!("Hierarchical plan for: {}", ctx.goal));
        plan.plan_type = "hierarchical".to_string();

        // 고수준 단계와 하위 단계들
        let high_level = PlanStep::new("Phase 1: Preparation", PlanActionType::ToolCall)
            .with_sub_step(PlanStep::new(
                "Gather information",
                PlanActionType::ToolCall,
            ))
            .with_sub_step(PlanStep::new(
                "Analyze requirements",
                PlanActionType::ToolCall,
            ));

        let execution = PlanStep::new("Phase 2: Execution", PlanActionType::ToolCall)
            .depends_on(&high_level.id)
            .with_sub_step(PlanStep::new("Implement changes", PlanActionType::ToolCall))
            .with_sub_step(PlanStep::new("Test changes", PlanActionType::ToolCall));

        plan = plan
            .with_step(high_level)
            .with_step(execution)
            .with_step(PlanStep::new(
                "Phase 3: Verification",
                PlanActionType::Complete,
            ));

        Ok(plan)
    }

    fn build_prompt(&self, ctx: &PlanningContext<'_>) -> String {
        let tools = ctx.available_tools.join(", ");

        format!(
            r#"Create a hierarchical plan to achieve the following goal.

Goal: {}

Available tools: {}

Create a plan with:
1. High-level phases (max {} per level)
2. Each phase broken down into specific steps
3. Maximum {} levels of hierarchy

Format:
Phase 1: [description]
  1.1 [sub-step]
  1.2 [sub-step]
Phase 2: [description] (depends on Phase 1)
  2.1 [sub-step]
..."#,
            ctx.goal, tools, self.max_steps_per_level, self.max_depth
        )
    }
}

// ============================================================================
// ReActPlanning - ReAct 스타일 반응형 계획
// ============================================================================

/// ReAct 스타일 반응형 계획
///
/// 각 단계 실행 후 관찰 결과에 따라 다음 계획을 조정합니다.
pub struct ReActPlanning {
    max_iterations: u32,
}

impl ReActPlanning {
    pub fn new() -> Self {
        Self { max_iterations: 10 }
    }

    pub fn with_max_iterations(mut self, max: u32) -> Self {
        self.max_iterations = max;
        self
    }
}

impl Default for ReActPlanning {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PlanningStrategy for ReActPlanning {
    fn name(&self) -> &str {
        "react"
    }

    fn description(&self) -> &str {
        "Reactive planning that adapts based on observations"
    }

    async fn plan(&self, ctx: &PlanningContext<'_>) -> Result<Plan> {
        // ReAct는 동적이므로 초기 계획만 생성
        let mut plan = Plan::new(format!("ReAct plan for: {}", ctx.goal));
        plan.plan_type = "react".to_string();

        // 첫 번째 Thought-Action 쌍만 계획
        plan = plan.with_step(
            PlanStep::new("Initial thought and action", PlanActionType::ToolCall)
                .with_condition("Observe result and plan next step"),
        );

        Ok(plan)
    }

    fn build_prompt(&self, ctx: &PlanningContext<'_>) -> String {
        let tools = ctx.available_tools.join(", ");

        format!(
            r#"You are operating in ReAct mode. Plan ONE action at a time.

Goal: {}

Available tools: {}

Current step format:
Thought: [your reasoning about what to do next]
Action: [tool_name]
Action Input: [input for the tool]

After each action, you will receive an Observation.
Then plan the next action based on the observation.

Maximum iterations: {}

Plan your first action now."#,
            ctx.goal, tools, self.max_iterations
        )
    }
}

// ============================================================================
// 유틸리티
// ============================================================================

/// 계획 전략 팩토리
pub fn create_planning_strategy(name: &str) -> Box<dyn PlanningStrategy> {
    match name {
        "simple" => Box::new(SimplePlanning::new()),
        "hierarchical" => Box::new(HierarchicalPlanning::new()),
        "react" => Box::new(ReActPlanning::new()),
        _ => Box::new(SimplePlanning::new()),
    }
}
