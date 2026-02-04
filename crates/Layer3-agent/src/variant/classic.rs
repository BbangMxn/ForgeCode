//! Classic Agent
//!
//! 기본 Agent 구현입니다. 단순 추론과 순차 실행을 사용합니다.

use super::registry::{AgentVariantInfo, BuiltinVariant, StrategiesInfo, VariantCategory};
use crate::runtime::{
    AgentCapability, AgentMetadata, AgentRuntime, ExecuteOutput, PlanOutput, ReflectOutput,
    RuntimeConfig, RuntimeContext, ThinkOutput,
};
use crate::strategy::{
    ExecutionStrategy, MemoryStrategy, PlanningContext, PlanningStrategy, ReasoningContext,
    ReasoningStrategy, SequentialExecution, SimplePlanning, SimpleReasoning, SlidingWindowMemory,
};
use async_trait::async_trait;
use forge_foundation::Result;
use std::collections::HashSet;

/// Classic Agent
///
/// 가장 기본적인 Agent입니다:
/// - Simple Reasoning: 직접적인 추론
/// - Simple Planning: 선형 계획
/// - Sliding Window Memory: 최근 메시지만 유지
/// - Sequential Execution: 순차 실행
pub struct ClassicAgent {
    metadata: AgentMetadata,
    config: RuntimeConfig,
    reasoning: SimpleReasoning,
    planning: SimplePlanning,
    memory: SlidingWindowMemory,
    execution: SequentialExecution,
}

impl ClassicAgent {
    /// 새 ClassicAgent 생성
    pub fn new() -> Self {
        Self::create(RuntimeConfig::default())
    }

    /// 메모리 크기 설정
    pub fn with_memory_size(mut self, max_tokens: usize) -> Self {
        self.memory = SlidingWindowMemory::new(max_tokens);
        self
    }
}

impl Default for ClassicAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl BuiltinVariant for ClassicAgent {
    fn variant_info() -> AgentVariantInfo {
        AgentVariantInfo {
            id: "classic".to_string(),
            name: "Classic Agent".to_string(),
            version: "1.0.0".to_string(),
            description: "Basic agent with simple reasoning and sequential execution".to_string(),
            category: VariantCategory::Standard,
            recommended_for: vec![
                "simple-tasks".to_string(),
                "quick-responses".to_string(),
                "low-latency".to_string(),
            ],
            default_config: RuntimeConfig {
                enable_think: false,
                enable_plan: false,
                enable_reflect: false,
                ..RuntimeConfig::default()
            },
            strategies: StrategiesInfo {
                reasoning: "simple".to_string(),
                planning: "simple".to_string(),
                memory: "sliding-window".to_string(),
                execution: "sequential".to_string(),
            },
            is_builtin: true,
        }
    }

    fn create(config: RuntimeConfig) -> Self {
        let mut capabilities = HashSet::new();
        capabilities.insert(AgentCapability::MultiTurn);
        capabilities.insert(AgentCapability::ToolUse);
        capabilities.insert(AgentCapability::Streaming);

        let metadata = AgentMetadata {
            id: "classic".to_string(),
            name: "Classic Agent".to_string(),
            version: "1.0.0".to_string(),
            description: "Basic agent with simple reasoning and sequential execution".to_string(),
            author: Some("ForgeCode".to_string()),
            capabilities,
            recommended_for: vec!["simple-tasks".to_string(), "quick-responses".to_string()],
            default_model: None,
            min_context_size: Some(8_000),
            extra: serde_json::Value::Null,
        };

        Self {
            metadata,
            config,
            reasoning: SimpleReasoning::new(),
            planning: SimplePlanning::new(),
            memory: SlidingWindowMemory::new(100_000),
            execution: SequentialExecution::new(),
        }
    }
}

#[async_trait]
impl AgentRuntime for ClassicAgent {
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
            max_steps: 5,
        };

        let output = self.reasoning.reason(&reasoning_ctx).await?;

        Ok(ThinkOutput::new(&output.conclusion).with_confidence(output.confidence))
    }

    async fn plan(&self, ctx: &mut RuntimeContext) -> Result<PlanOutput> {
        let think_output = ctx.last_think_output();
        let goal = think_output
            .map(|t| t.reasoning.as_str())
            .unwrap_or(ctx.last_user_input().unwrap_or(""));

        let planning_ctx = PlanningContext {
            goal,
            reasoning: think_output.map(|t| t.reasoning.as_str()),
            available_tools: &[],
            constraints: vec![],
            max_steps: 10,
        };

        let plan = self.planning.plan(&planning_ctx).await?;

        Ok(PlanOutput::new(&plan.summary))
    }

    async fn execute(&self, ctx: &mut RuntimeContext) -> Result<ExecuteOutput> {
        // Classic agent는 단순 실행
        let output = ExecuteOutput::new()
            .with_output("Task executed")
            .mark_complete();

        if ctx.current_turn() > 0 {
            ctx.mark_complete("Task completed");
        }

        Ok(output)
    }
}
