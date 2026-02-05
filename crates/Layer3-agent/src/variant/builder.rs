//! Agent Builder
//!
//! 전략 조합으로 커스텀 Agent를 생성하는 빌더입니다.

#![allow(dead_code)]

use crate::runtime::{
    AgentCapability, AgentMetadata, AgentRuntime, ExecuteOutput, PlanOutput,
    RuntimeConfig, RuntimeContext, ThinkOutput,
};
use crate::strategy::{
    AdaptiveExecution, ExecutionStrategy, MemoryStrategy, PlanningContext, PlanningStrategy,
    ReasoningContext, ReasoningStrategy, SimplePlanning, SimpleReasoning, SlidingWindowMemory,
};
use async_trait::async_trait;
use forge_foundation::Result;
use std::collections::HashSet;

// ============================================================================
// ComposableAgent - 조합 가능한 Agent
// ============================================================================

/// 전략 조합으로 구성된 Agent
pub struct ComposableAgent {
    metadata: AgentMetadata,
    config: RuntimeConfig,
    reasoning: Box<dyn ReasoningStrategy>,
    planning: Box<dyn PlanningStrategy>,
    memory: Box<dyn MemoryStrategy>,
    execution: Box<dyn ExecutionStrategy>,
}

impl ComposableAgent {
    /// 새 ComposableAgent 생성
    pub fn new(
        reasoning: Box<dyn ReasoningStrategy>,
        planning: Box<dyn PlanningStrategy>,
        memory: Box<dyn MemoryStrategy>,
        execution: Box<dyn ExecutionStrategy>,
    ) -> Self {
        let id = format!(
            "composable-{}-{}-{}",
            reasoning.name(),
            planning.name(),
            execution.name()
        );

        let mut capabilities = HashSet::new();
        capabilities.insert(AgentCapability::MultiTurn);
        capabilities.insert(AgentCapability::ToolUse);

        if execution.max_parallelism() > 1 {
            capabilities.insert(AgentCapability::ParallelToolExecution);
        }

        let metadata = AgentMetadata {
            id: id.clone(),
            name: format!(
                "Composable Agent ({}/{}/{})",
                reasoning.name(),
                planning.name(),
                execution.name()
            ),
            version: "0.1.0".to_string(),
            description: format!(
                "Custom agent with {} reasoning, {} planning, {} execution",
                reasoning.name(),
                planning.name(),
                execution.name()
            ),
            author: None,
            capabilities,
            recommended_for: vec!["general-purpose".to_string()],
            default_model: None,
            min_context_size: None,
            extra: serde_json::Value::Null,
        };

        Self {
            metadata,
            config: RuntimeConfig::default(),
            reasoning,
            planning,
            memory,
            execution,
        }
    }
}

#[async_trait]
impl AgentRuntime for ComposableAgent {
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
        let task = ctx.last_user_input().unwrap_or("No input");
        let history: Vec<String> = ctx.messages().iter().map(|m| m.content.clone()).collect();

        let reasoning_ctx = ReasoningContext {
            task,
            history: &history,
            available_info: std::collections::HashMap::new(),
            max_steps: 10,
        };

        let output = self.reasoning.reason(&reasoning_ctx).await?;

        Ok(ThinkOutput::new(&output.conclusion).with_confidence(output.confidence))
    }

    async fn plan(&self, ctx: &mut RuntimeContext) -> Result<PlanOutput> {
        let think_output = ctx.last_think_output().cloned();
        let goal = think_output
            .as_ref()
            .map(|t| t.reasoning.as_str())
            .unwrap_or("No goal");

        let planning_ctx = PlanningContext {
            goal,
            reasoning: think_output.as_ref().map(|t| t.reasoning.as_str()),
            available_tools: &[],
            constraints: vec![],
            max_steps: 20,
        };

        let plan = self.planning.plan(&planning_ctx).await?;

        let mut output = PlanOutput::new(&plan.summary);
        output.estimated_steps = plan.steps.len() as u32;
        output.can_parallelize = plan.can_parallelize;

        Ok(output)
    }

    async fn execute(&self, ctx: &mut RuntimeContext) -> Result<ExecuteOutput> {
        // 실제 구현에서는 plan에서 생성된 액션들을 execution 전략으로 실행
        let _plan = ctx.last_plan_output();

        // 단순 구현: 실행 완료로 표시
        Ok(ExecuteOutput::new()
            .with_output("Execution completed")
            .mark_complete())
    }
}

// ============================================================================
// AgentBuilder - Agent 빌더
// ============================================================================

/// Agent 빌더
///
/// 전략 조합으로 커스텀 Agent를 생성합니다.
pub struct AgentBuilder {
    reasoning: Option<Box<dyn ReasoningStrategy>>,
    planning: Option<Box<dyn PlanningStrategy>>,
    memory: Option<Box<dyn MemoryStrategy>>,
    execution: Option<Box<dyn ExecutionStrategy>>,
    config: RuntimeConfig,
}

impl AgentBuilder {
    /// 새 빌더 생성
    pub fn new() -> Self {
        Self {
            reasoning: None,
            planning: None,
            memory: None,
            execution: None,
            config: RuntimeConfig::default(),
        }
    }

    /// 추론 전략 설정
    pub fn with_reasoning<R: ReasoningStrategy + 'static>(mut self, strategy: R) -> Self {
        self.reasoning = Some(Box::new(strategy));
        self
    }

    /// 계획 전략 설정
    pub fn with_planning<P: PlanningStrategy + 'static>(mut self, strategy: P) -> Self {
        self.planning = Some(Box::new(strategy));
        self
    }

    /// 메모리 전략 설정
    pub fn with_memory<M: MemoryStrategy + 'static>(mut self, strategy: M) -> Self {
        self.memory = Some(Box::new(strategy));
        self
    }

    /// 실행 전략 설정
    pub fn with_execution<E: ExecutionStrategy + 'static>(mut self, strategy: E) -> Self {
        self.execution = Some(Box::new(strategy));
        self
    }

    /// 런타임 설정
    pub fn with_config(mut self, config: RuntimeConfig) -> Self {
        self.config = config;
        self
    }

    /// Agent 생성
    pub fn build(self) -> ComposableAgent {
        let reasoning = self
            .reasoning
            .unwrap_or_else(|| Box::new(SimpleReasoning::new()));
        let planning = self
            .planning
            .unwrap_or_else(|| Box::new(SimplePlanning::new()));
        let memory = self
            .memory
            .unwrap_or_else(|| Box::new(SlidingWindowMemory::new(100_000)));
        let execution = self
            .execution
            .unwrap_or_else(|| Box::new(AdaptiveExecution::new(4)));

        let mut agent = ComposableAgent::new(reasoning, planning, memory, execution);
        agent.config = self.config;
        agent
    }
}

impl Default for AgentBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Preset Builders
// ============================================================================

impl AgentBuilder {
    /// 빠른 실행용 프리셋
    pub fn fast() -> Self {
        use crate::strategy::{SequentialExecution, SimplePlanning, SimpleReasoning};

        Self::new()
            .with_reasoning(SimpleReasoning::new())
            .with_planning(SimplePlanning::new())
            .with_execution(SequentialExecution::new())
            .with_config(RuntimeConfig::fast())
    }

    /// 심층 분석용 프리셋
    pub fn thorough() -> Self {
        use crate::strategy::{HierarchicalPlanning, ParallelExecution, TreeOfThought};

        Self::new()
            .with_reasoning(TreeOfThought::new())
            .with_planning(HierarchicalPlanning::new())
            .with_execution(ParallelExecution::new(4))
            .with_config(RuntimeConfig::thorough())
    }

    /// 코딩 작업용 프리셋
    pub fn coding() -> Self {
        use crate::strategy::{
            AdaptiveExecution, ChainOfThought, HierarchicalPlanning, SummarizingMemory,
        };

        Self::new()
            .with_reasoning(ChainOfThought::new())
            .with_planning(HierarchicalPlanning::new())
            .with_memory(SummarizingMemory::new(128_000))
            .with_execution(AdaptiveExecution::new(4))
    }
}
