//! Tree Search Agent
//!
//! Tree-of-Thought 기반 탐색 Agent입니다.
//! 여러 추론 경로를 탐색하고 최적의 경로를 선택합니다.

use super::registry::{AgentVariantInfo, BuiltinVariant, StrategiesInfo, VariantCategory};
use crate::runtime::{
    AgentCapability, AgentMetadata, AgentRuntime, ExecuteOutput, PlanOutput, ReflectOutput,
    RuntimeConfig, RuntimeContext, ThinkOutput,
};
use crate::strategy::{
    HierarchicalPlanning, ParallelExecution, PlanningContext, PlanningStrategy, RAGMemory,
    ReasoningContext, ReasoningStrategy, TreeOfThought,
};
use async_trait::async_trait;
use forge_foundation::Result;
use std::collections::HashSet;

/// Tree Search Agent
///
/// Tree-of-Thought 패턴:
/// 1. 여러 초기 접근 방식 생성
/// 2. 각 접근 방식 평가
/// 3. 가장 유망한 경로 확장
/// 4. 최적의 해결책 선택
pub struct TreeSearchAgent {
    metadata: AgentMetadata,
    config: RuntimeConfig,
    reasoning: TreeOfThought,
    planning: HierarchicalPlanning,
    memory: RAGMemory,
    execution: ParallelExecution,
}

impl TreeSearchAgent {
    /// 새 TreeSearchAgent 생성
    pub fn new() -> Self {
        Self::create(RuntimeConfig::default())
    }

    /// 분기 계수 설정
    pub fn with_branching_factor(mut self, factor: u32) -> Self {
        self.reasoning = TreeOfThought::new().with_branching_factor(factor);
        self
    }

    /// 최대 깊이 설정
    pub fn with_max_depth(mut self, depth: u32) -> Self {
        self.reasoning = TreeOfThought::new().with_max_depth(depth);
        self
    }

    /// 병렬 실행 수 설정
    pub fn with_parallelism(mut self, max: usize) -> Self {
        self.execution = ParallelExecution::new(max);
        self
    }
}

impl Default for TreeSearchAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl BuiltinVariant for TreeSearchAgent {
    fn variant_info() -> AgentVariantInfo {
        AgentVariantInfo {
            id: "tree-search".to_string(),
            name: "Tree Search Agent".to_string(),
            version: "1.0.0".to_string(),
            description: "Tree-of-Thought based agent that explores multiple reasoning paths"
                .to_string(),
            category: VariantCategory::Reasoning,
            recommended_for: vec![
                "complex-problems".to_string(),
                "creative-tasks".to_string(),
                "exploration".to_string(),
                "architecture-design".to_string(),
                "algorithm-design".to_string(),
            ],
            default_config: RuntimeConfig {
                enable_think: true,
                enable_plan: true,
                enable_reflect: true,
                max_turns: 50,
                allow_parallel_tools: true,
                max_parallel_tools: 4,
                ..RuntimeConfig::default()
            },
            strategies: StrategiesInfo {
                reasoning: "tree-of-thought".to_string(),
                planning: "hierarchical".to_string(),
                memory: "rag".to_string(),
                execution: "parallel".to_string(),
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
        capabilities.insert(AgentCapability::Memory);
        capabilities.insert(AgentCapability::ParallelToolExecution);
        capabilities.insert(AgentCapability::Streaming);

        let metadata = AgentMetadata {
            id: "tree-search".to_string(),
            name: "Tree Search Agent".to_string(),
            version: "1.0.0".to_string(),
            description: "Tree-of-Thought based agent that explores multiple reasoning paths"
                .to_string(),
            author: Some("ForgeCode".to_string()),
            capabilities,
            recommended_for: vec!["complex-problems".to_string(), "creative-tasks".to_string()],
            default_model: Some("claude-sonnet-4-20250514".to_string()), // 추론에 강한 모델 추천
            min_context_size: Some(64_000),
            extra: serde_json::Value::Null,
        };

        Self {
            metadata,
            config,
            reasoning: TreeOfThought::new()
                .with_branching_factor(3)
                .with_max_depth(4),
            planning: HierarchicalPlanning::new().with_max_depth(3),
            memory: RAGMemory::new(128_000),
            execution: ParallelExecution::new(4),
        }
    }
}

#[async_trait]
impl AgentRuntime for TreeSearchAgent {
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
            max_steps: 15,
        };

        let output = self.reasoning.reason(&reasoning_ctx).await?;

        let mut think = ThinkOutput::new(&output.conclusion).with_confidence(output.confidence);

        // 탐색한 경로들 기록
        for step in &output.steps {
            think = think.with_suggestion(step.content.clone());
        }

        // 대안적 결론도 기록
        for alt in &output.alternatives {
            think = think.with_insight(format!(
                "Alternative (confidence {:.2}): {}",
                alt.confidence, alt.conclusion
            ));
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
            available_tools: &[
                "read".to_string(),
                "write".to_string(),
                "edit".to_string(),
                "bash".to_string(),
                "grep".to_string(),
                "glob".to_string(),
            ],
            constraints: vec![],
            max_steps: 30,
        };

        let plan = self.planning.plan(&planning_ctx).await?;

        let mut output = PlanOutput::new(&plan.summary);
        output.estimated_steps = plan.steps.len() as u32;
        output.can_parallelize = true; // Tree search는 병렬 탐색 지원

        Ok(output)
    }

    async fn execute(&self, ctx: &mut RuntimeContext) -> Result<ExecuteOutput> {
        let plan = ctx.last_plan_output();

        // Tree search agent는 병렬 실행 활용
        let output = ExecuteOutput::new().with_output(format!(
            "Executed plan with parallel exploration: {}",
            plan.map(|p| p.summary.as_str()).unwrap_or("No plan")
        ));

        Ok(output)
    }

    async fn reflect(&self, ctx: &mut RuntimeContext) -> Result<ReflectOutput> {
        let execute_output = ctx.last_execute_output();
        let think_output = ctx.last_think_output();

        // 선택한 경로의 품질 평가
        let confidence = think_output.map(|t| t.confidence).unwrap_or(0.5);

        let success = execute_output.map(|e| e.is_complete).unwrap_or(false);

        let mut reflect = ReflectOutput::new(if success {
            format!("Selected path succeeded with confidence {:.2}", confidence)
        } else {
            "Selected path needs refinement - consider alternatives".to_string()
        })
        .with_success(success)
        .with_quality_score(confidence);

        // 대안 경로 고려 제안
        if !success {
            if let Some(think) = think_output {
                if !think.insights.is_empty() {
                    reflect = reflect.with_improvement(
                        "Consider alternative paths identified during exploration",
                    );
                }
            }
            reflect = reflect.needs_retry();
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

        // 높은 확신도로 완료
        if let Some(turn) = ctx.turn_history().last() {
            if let Some(think) = &turn.think_output {
                if think.confidence >= 0.9 {
                    if let Some(exec) = &turn.execute_output {
                        if exec.is_complete {
                            return true;
                        }
                    }
                }
            }
        }

        false
    }

    fn snapshot(&self, ctx: &RuntimeContext) -> Result<serde_json::Value> {
        // Tree search 상태 스냅샷
        Ok(serde_json::json!({
            "current_turn": ctx.current_turn(),
            "token_usage": ctx.token_usage(),
            "turn_count": ctx.turn_history().len(),
        }))
    }
}
