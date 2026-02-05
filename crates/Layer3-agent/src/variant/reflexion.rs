//! Reflexion Agent
//!
//! Reflexion 패턴 Agent입니다.
//! 실행 후 자기 반성을 통해 개선합니다.

#![allow(dead_code)]

use super::registry::{AgentVariantInfo, BuiltinVariant, StrategiesInfo, VariantCategory};
use crate::runtime::{
    AgentCapability, AgentMetadata, AgentRuntime, ExecuteOutput, PlanOutput, ReflectOutput,
    RuntimeConfig, RuntimeContext, ThinkOutput,
};
use crate::strategy::{
    AdaptiveExecution, ChainOfThought, HierarchicalPlanning, PlanningContext, PlanningStrategy,
    ReasoningContext, ReasoningStrategy, SummarizingMemory,
};
use async_trait::async_trait;
use forge_foundation::Result;
use std::collections::HashSet;

/// Reflexion Agent
///
/// 자기 반성 패턴:
/// 1. Think: 상황 분석
/// 2. Plan: 계획 수립
/// 3. Execute: 실행
/// 4. Reflect: 결과 평가 및 개선점 도출
/// 5. 필요시 재시도 (개선된 접근으로)
pub struct ReflexionAgent {
    metadata: AgentMetadata,
    config: RuntimeConfig,
    reasoning: ChainOfThought,
    planning: HierarchicalPlanning,
    memory: SummarizingMemory,
    execution: AdaptiveExecution,
    /// 이전 시도들의 반성 내용
    reflections: Vec<String>,
    /// 최대 재시도 횟수
    max_retries: u32,
}

impl ReflexionAgent {
    /// 새 ReflexionAgent 생성
    pub fn new() -> Self {
        Self::create(RuntimeConfig::default())
    }

    /// 최대 재시도 횟수 설정
    pub fn with_max_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
    }

    /// 이전 반성 내용 추가
    pub fn add_reflection(&mut self, reflection: impl Into<String>) {
        self.reflections.push(reflection.into());
    }
}

impl Default for ReflexionAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl BuiltinVariant for ReflexionAgent {
    fn variant_info() -> AgentVariantInfo {
        AgentVariantInfo {
            id: "reflexion".to_string(),
            name: "Reflexion Agent".to_string(),
            version: "1.0.0".to_string(),
            description: "Self-reflection pattern that learns from mistakes and improves"
                .to_string(),
            category: VariantCategory::Reasoning,
            recommended_for: vec![
                "difficult-problems".to_string(),
                "iterative-improvement".to_string(),
                "debugging".to_string(),
                "code-review".to_string(),
            ],
            default_config: RuntimeConfig {
                enable_think: true,
                enable_plan: true,
                enable_reflect: true, // Reflect 활성화
                max_turns: 30,
                ..RuntimeConfig::default()
            },
            strategies: StrategiesInfo {
                reasoning: "chain-of-thought".to_string(),
                planning: "hierarchical".to_string(),
                memory: "summarizing".to_string(),
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
        capabilities.insert(AgentCapability::Reflection);
        capabilities.insert(AgentCapability::Streaming);
        capabilities.insert(AgentCapability::Memory);

        let metadata = AgentMetadata {
            id: "reflexion".to_string(),
            name: "Reflexion Agent".to_string(),
            version: "1.0.0".to_string(),
            description: "Self-reflection pattern that learns from mistakes and improves"
                .to_string(),
            author: Some("ForgeCode".to_string()),
            capabilities,
            recommended_for: vec![
                "difficult-problems".to_string(),
                "iterative-improvement".to_string(),
            ],
            default_model: None,
            min_context_size: Some(32_000),
            extra: serde_json::Value::Null,
        };

        Self {
            metadata,
            config,
            reasoning: ChainOfThought::new().verbose(true),
            planning: HierarchicalPlanning::new(),
            memory: SummarizingMemory::new(100_000),
            execution: AdaptiveExecution::new(4),
            reflections: Vec::new(),
            max_retries: 3,
        }
    }
}

#[async_trait]
impl AgentRuntime for ReflexionAgent {
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

        // 이전 반성 내용을 포함
        let mut available_info = std::collections::HashMap::new();
        if !self.reflections.is_empty() {
            available_info.insert(
                "previous_reflections".to_string(),
                self.reflections.join("\n---\n"),
            );
        }

        let reasoning_ctx = ReasoningContext {
            task,
            history: &history,
            available_info,
            max_steps: 10,
        };

        let output = self.reasoning.reason(&reasoning_ctx).await?;

        let mut think = ThinkOutput::new(&output.conclusion).with_confidence(output.confidence);

        // 이전 반성이 있으면 참고했음을 표시
        if !self.reflections.is_empty() {
            think = think.with_insight(format!(
                "Considered {} previous reflection(s) for improvement",
                self.reflections.len()
            ));
        }

        Ok(think)
    }

    async fn plan(&self, ctx: &mut RuntimeContext) -> Result<PlanOutput> {
        let think_output = ctx.last_think_output();
        let goal = think_output
            .map(|t| t.reasoning.as_str())
            .unwrap_or(ctx.last_user_input().unwrap_or(""));

        // 이전 실패에서 배운 제약 조건 추가
        let constraints: Vec<String> = self
            .reflections
            .iter()
            .filter(|r| r.contains("avoid") || r.contains("should not"))
            .cloned()
            .collect();

        let planning_ctx = PlanningContext {
            goal,
            reasoning: think_output.map(|t| t.reasoning.as_str()),
            available_tools: &[
                "read".to_string(),
                "write".to_string(),
                "edit".to_string(),
                "bash".to_string(),
            ],
            constraints,
            max_steps: 20,
        };

        let plan = self.planning.plan(&planning_ctx).await?;

        let mut output = PlanOutput::new(&plan.summary);
        output.estimated_steps = plan.steps.len() as u32;
        output.can_parallelize = plan.can_parallelize;

        Ok(output)
    }

    async fn execute(&self, ctx: &mut RuntimeContext) -> Result<ExecuteOutput> {
        let plan = ctx.last_plan_output();

        let output = ExecuteOutput::new().with_output(format!(
            "Executed plan: {}",
            plan.map(|p| p.summary.as_str()).unwrap_or("No plan")
        ));

        Ok(output)
    }

    async fn reflect(&self, ctx: &mut RuntimeContext) -> Result<ReflectOutput> {
        let execute_output = ctx.last_execute_output();

        // 실행 결과 평가
        let success = execute_output
            .map(|e| e.is_complete && e.actions_failed == 0)
            .unwrap_or(false);

        let quality_score = execute_output
            .map(|e| {
                if e.actions_executed > 0 {
                    e.actions_succeeded as f32 / e.actions_executed as f32
                } else {
                    0.5
                }
            })
            .unwrap_or(0.5);

        let mut reflect = ReflectOutput::new(if success {
            "Task completed successfully"
        } else {
            "Task needs improvement"
        })
        .with_success(success)
        .with_quality_score(quality_score);

        // 실패한 경우 개선점 도출
        if !success {
            reflect = reflect
                .with_improvement("Consider alternative approaches")
                .needs_retry();
        }

        // 잘된 점과 개선점 기록
        if let Some(exec) = execute_output {
            if exec.actions_succeeded > 0 {
                reflect = reflect
                    .with_positive(format!("{} action(s) succeeded", exec.actions_succeeded));
            }
            if exec.actions_failed > 0 {
                reflect = reflect.with_improvement(format!(
                    "{} action(s) failed - analyze and fix",
                    exec.actions_failed
                ));
            }
        }

        Ok(reflect)
    }

    fn should_stop(&self, ctx: &RuntimeContext) -> bool {
        // 최대 턴 초과
        if ctx.current_turn() >= self.config.max_turns {
            return true;
        }

        // 완료됨
        if ctx.is_complete() {
            return true;
        }

        // 마지막 반성에서 성공으로 판단
        if let Some(turn) = ctx.turn_history().last() {
            if let Some(reflect) = &turn.reflect_output {
                if reflect.success_assessment && !reflect.should_retry {
                    return true;
                }
            }
        }

        // 최대 재시도 횟수 초과
        let retry_count = ctx
            .turn_history()
            .iter()
            .filter(|t| {
                t.reflect_output
                    .as_ref()
                    .map(|r| r.should_retry)
                    .unwrap_or(false)
            })
            .count() as u32;

        if retry_count >= self.max_retries {
            return true;
        }

        false
    }
}
