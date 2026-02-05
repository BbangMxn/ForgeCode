//! ReAct Agent
//!
//! ReAct (Reasoning + Acting) 패턴 Agent입니다.
//! Thought → Action → Observation 루프를 반복합니다.

#![allow(dead_code)]

use super::registry::{AgentVariantInfo, BuiltinVariant, StrategiesInfo, VariantCategory};
use crate::runtime::{
    AgentCapability, AgentMetadata, AgentRuntime, ExecuteOutput, PlanOutput,
    RuntimeConfig, RuntimeContext, ThinkOutput,
};
use crate::strategy::{
    AdaptiveExecution, PlanningContext, PlanningStrategy, ReActPlanning, ReActReasoning,
    ReasoningContext, ReasoningStrategy, SlidingWindowMemory,
};
use async_trait::async_trait;
use forge_foundation::Result;
use std::collections::HashSet;

/// ReAct Agent
///
/// Reasoning + Acting 패턴:
/// 1. Thought: 현재 상황 분석
/// 2. Action: Tool 호출 결정
/// 3. Observation: 결과 관찰
/// 4. 반복 (완료될 때까지)
pub struct ReActAgent {
    metadata: AgentMetadata,
    config: RuntimeConfig,
    reasoning: ReActReasoning,
    planning: ReActPlanning,
    memory: SlidingWindowMemory,
    execution: AdaptiveExecution,
}

impl ReActAgent {
    /// 새 ReActAgent 생성
    pub fn new() -> Self {
        Self::create(RuntimeConfig::default())
    }

    /// 최대 반복 횟수 설정
    pub fn with_max_iterations(mut self, max: u32) -> Self {
        self.reasoning = ReActReasoning::new().with_max_iterations(max);
        self.planning = ReActPlanning::new().with_max_iterations(max);
        self
    }
}

impl Default for ReActAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl BuiltinVariant for ReActAgent {
    fn variant_info() -> AgentVariantInfo {
        AgentVariantInfo {
            id: "react".to_string(),
            name: "ReAct Agent".to_string(),
            version: "1.0.0".to_string(),
            description: "Reasoning + Acting pattern with thought-action-observation loop"
                .to_string(),
            category: VariantCategory::Reasoning,
            recommended_for: vec![
                "complex-reasoning".to_string(),
                "multi-step-tasks".to_string(),
                "tool-heavy-tasks".to_string(),
                "research".to_string(),
            ],
            default_config: RuntimeConfig {
                enable_think: true,
                enable_plan: true,
                enable_reflect: false,
                max_turns: 20,
                ..RuntimeConfig::default()
            },
            strategies: StrategiesInfo {
                reasoning: "react".to_string(),
                planning: "react".to_string(),
                memory: "sliding-window".to_string(),
                execution: "adaptive".to_string(),
            },
            is_builtin: true,
        }
    }

    fn create(config: RuntimeConfig) -> Self {
        let mut capabilities = HashSet::new();
        capabilities.insert(AgentCapability::MultiTurn);
        capabilities.insert(AgentCapability::ToolUse);
        capabilities.insert(AgentCapability::Planning);
        capabilities.insert(AgentCapability::Streaming);
        capabilities.insert(AgentCapability::ParallelToolExecution);

        let metadata = AgentMetadata {
            id: "react".to_string(),
            name: "ReAct Agent".to_string(),
            version: "1.0.0".to_string(),
            description: "Reasoning + Acting pattern with thought-action-observation loop"
                .to_string(),
            author: Some("ForgeCode".to_string()),
            capabilities,
            recommended_for: vec![
                "complex-reasoning".to_string(),
                "multi-step-tasks".to_string(),
            ],
            default_model: None,
            min_context_size: Some(16_000),
            extra: serde_json::Value::Null,
        };

        Self {
            metadata,
            config,
            reasoning: ReActReasoning::new(),
            planning: ReActPlanning::new(),
            memory: SlidingWindowMemory::new(100_000),
            execution: AdaptiveExecution::new(4),
        }
    }
}

#[async_trait]
impl AgentRuntime for ReActAgent {
    fn metadata(&self) -> &AgentMetadata {
        &self.metadata
    }

    fn config(&self) -> &RuntimeConfig {
        &self.config
    }

    fn set_config(&mut self, config: RuntimeConfig) {
        self.config = config;
    }

    async fn think(&self, ctx: &mut RuntimeContext) -> Result<ThinkOutput> {
        let task = ctx.last_user_input().unwrap_or("");
        let history: Vec<String> = ctx.messages().iter().map(|m| m.content.clone()).collect();

        let reasoning_ctx = ReasoningContext {
            task,
            history: &history,
            available_info: std::collections::HashMap::new(),
            max_steps: 10,
        };

        let output = self.reasoning.reason(&reasoning_ctx).await?;

        // ReAct 형식으로 Think 출력
        let mut think = ThinkOutput::new(&output.conclusion).with_confidence(output.confidence);

        for step in output.steps {
            think = think.with_suggestion(step.content);
        }

        Ok(think)
    }

    async fn plan(&self, ctx: &mut RuntimeContext) -> Result<PlanOutput> {
        let think_output = ctx.last_think_output();
        let goal = think_output
            .map(|t| t.reasoning.as_str())
            .unwrap_or(ctx.last_user_input().unwrap_or(""));

        let planning_ctx = PlanningContext {
            goal,
            reasoning: think_output.map(|t| t.reasoning.as_str()),
            available_tools: &["read".to_string(), "write".to_string(), "bash".to_string()],
            constraints: vec![],
            max_steps: 10,
        };

        let plan = self.planning.plan(&planning_ctx).await?;

        // ReAct는 한 번에 하나의 액션만 계획
        let mut output = PlanOutput::new(&plan.summary);
        output.estimated_steps = 1; // 한 번에 하나씩

        Ok(output)
    }

    async fn execute(&self, ctx: &mut RuntimeContext) -> Result<ExecuteOutput> {
        let plan = ctx.last_plan_output();

        let output = ExecuteOutput::new().with_output(format!(
            "Executed action from plan: {}",
            plan.map(|p| p.summary.as_str()).unwrap_or("No plan")
        ));

        // ReAct는 관찰 후 다음 사이클로
        // 완료 조건은 think에서 "finish" 액션을 결정할 때

        Ok(output)
    }

    fn should_stop(&self, ctx: &RuntimeContext) -> bool {
        // 최대 턴 초과
        if ctx.current_turn() >= self.config.max_turns {
            return true;
        }

        // 완료 표시됨
        if ctx.is_complete() {
            return true;
        }

        // Think에서 finish 액션 결정됨
        if let Some(think) = ctx
            .turn_history()
            .last()
            .and_then(|t| t.think_output.as_ref())
        {
            if think.reasoning.to_lowercase().contains("finish[") {
                return true;
            }
        }

        false
    }
}
