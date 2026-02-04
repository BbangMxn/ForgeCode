//! Agent Runtime Traits
//!
//! 모든 Agent 변형이 구현해야 하는 핵심 트레이트들입니다.

use super::context::RuntimeContext;
use super::output::{ExecuteOutput, PlanOutput, ReflectOutput, ThinkOutput};
use async_trait::async_trait;
use forge_foundation::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::Duration;

// ============================================================================
// AgentCapability - Agent가 지원하는 기능
// ============================================================================

/// Agent가 지원하는 기능 플래그
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentCapability {
    /// 멀티턴 대화 지원
    MultiTurn,

    /// Tool 사용 지원
    ToolUse,

    /// 병렬 Tool 실행
    ParallelToolExecution,

    /// 계획 수립 (Planning)
    Planning,

    /// 자기 반성 (Reflection)
    Reflection,

    /// 메모리/RAG 지원
    Memory,

    /// 코드 실행
    CodeExecution,

    /// 파일 시스템 접근
    FileSystem,

    /// 웹 검색
    WebSearch,

    /// 비전 (이미지 분석)
    Vision,

    /// 스트리밍 출력
    Streaming,

    /// 중단/재개 지원
    Pausable,

    /// 핸드오프 지원
    Handoff,

    /// 서브에이전트 생성
    SubAgentSpawn,
}

// ============================================================================
// AgentMetadata - Agent 메타데이터
// ============================================================================

/// Agent의 메타데이터
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMetadata {
    /// 고유 식별자 (예: "claude-react-v1", "reflexion-cot")
    pub id: String,

    /// 표시 이름
    pub name: String,

    /// 버전
    pub version: String,

    /// 설명
    pub description: String,

    /// 작성자
    pub author: Option<String>,

    /// 지원 기능
    pub capabilities: HashSet<AgentCapability>,

    /// 추천 사용 사례
    pub recommended_for: Vec<String>,

    /// 기본 모델 (없으면 설정에서 상속)
    pub default_model: Option<String>,

    /// 최소 요구 컨텍스트 크기
    pub min_context_size: Option<usize>,

    /// 추가 메타데이터
    pub extra: serde_json::Value,
}

impl AgentMetadata {
    /// 새 메타데이터 생성
    pub fn new(id: impl Into<String>, name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            version: version.into(),
            description: String::new(),
            author: None,
            capabilities: HashSet::new(),
            recommended_for: Vec::new(),
            default_model: None,
            min_context_size: None,
            extra: serde_json::Value::Null,
        }
    }

    /// 설명 추가
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// 기능 추가
    pub fn with_capability(mut self, cap: AgentCapability) -> Self {
        self.capabilities.insert(cap);
        self
    }

    /// 여러 기능 추가
    pub fn with_capabilities(mut self, caps: impl IntoIterator<Item = AgentCapability>) -> Self {
        self.capabilities.extend(caps);
        self
    }

    /// 추천 사용 사례 추가
    pub fn recommended_for(mut self, use_case: impl Into<String>) -> Self {
        self.recommended_for.push(use_case.into());
        self
    }

    /// 기능 지원 여부 확인
    pub fn has_capability(&self, cap: AgentCapability) -> bool {
        self.capabilities.contains(&cap)
    }
}

// ============================================================================
// RuntimeConfig - 런타임 설정
// ============================================================================

/// Agent 런타임 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    /// 최대 턴 수
    pub max_turns: u32,

    /// 턴 타임아웃
    pub turn_timeout: Duration,

    /// 전체 타임아웃
    pub total_timeout: Duration,

    /// Think 단계 활성화
    pub enable_think: bool,

    /// Plan 단계 활성화
    pub enable_plan: bool,

    /// Reflect 단계 활성화
    pub enable_reflect: bool,

    /// 병렬 Tool 실행 허용
    pub allow_parallel_tools: bool,

    /// 최대 병렬 Tool 수
    pub max_parallel_tools: usize,

    /// 자동 핸드오프 활성화
    pub auto_handoff: bool,

    /// 스트리밍 활성화
    pub streaming: bool,

    /// 디버그 모드
    pub debug: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            max_turns: 50,
            turn_timeout: Duration::from_secs(120),
            total_timeout: Duration::from_secs(1800),
            enable_think: true,
            enable_plan: true,
            enable_reflect: false,
            allow_parallel_tools: true,
            max_parallel_tools: 4,
            auto_handoff: true,
            streaming: true,
            debug: false,
        }
    }
}

impl RuntimeConfig {
    /// 빠른 실행용 설정
    pub fn fast() -> Self {
        Self {
            max_turns: 20,
            turn_timeout: Duration::from_secs(60),
            total_timeout: Duration::from_secs(600),
            enable_think: false,
            enable_plan: false,
            enable_reflect: false,
            ..Default::default()
        }
    }

    /// 심층 분석용 설정
    pub fn thorough() -> Self {
        Self {
            max_turns: 100,
            turn_timeout: Duration::from_secs(300),
            total_timeout: Duration::from_secs(3600),
            enable_think: true,
            enable_plan: true,
            enable_reflect: true,
            ..Default::default()
        }
    }
}

// ============================================================================
// RuntimeHooks - 런타임 훅
// ============================================================================

/// 런타임 훅 (각 단계에서 호출)
#[async_trait]
pub trait RuntimeHooks: Send + Sync {
    /// Think 단계 전
    async fn before_think(&self, _ctx: &RuntimeContext) -> Result<()> {
        Ok(())
    }

    /// Think 단계 후
    async fn after_think(&self, _ctx: &RuntimeContext, _output: &ThinkOutput) -> Result<()> {
        Ok(())
    }

    /// Plan 단계 전
    async fn before_plan(&self, _ctx: &RuntimeContext) -> Result<()> {
        Ok(())
    }

    /// Plan 단계 후
    async fn after_plan(&self, _ctx: &RuntimeContext, _output: &PlanOutput) -> Result<()> {
        Ok(())
    }

    /// Execute 단계 전
    async fn before_execute(&self, _ctx: &RuntimeContext) -> Result<()> {
        Ok(())
    }

    /// Execute 단계 후
    async fn after_execute(&self, _ctx: &RuntimeContext, _output: &ExecuteOutput) -> Result<()> {
        Ok(())
    }

    /// Reflect 단계 전
    async fn before_reflect(&self, _ctx: &RuntimeContext) -> Result<()> {
        Ok(())
    }

    /// Reflect 단계 후
    async fn after_reflect(&self, _ctx: &RuntimeContext, _output: &ReflectOutput) -> Result<()> {
        Ok(())
    }

    /// 턴 완료 시
    async fn on_turn_complete(&self, _ctx: &RuntimeContext, _turn: u32) -> Result<()> {
        Ok(())
    }

    /// 에러 발생 시
    async fn on_error(
        &self,
        _ctx: &RuntimeContext,
        _error: &forge_foundation::Error,
    ) -> Result<()> {
        Ok(())
    }
}

/// 기본 훅 (no-op)
pub struct DefaultHooks;

impl RuntimeHooks for DefaultHooks {}

// ============================================================================
// AgentRuntime - 핵심 트레이트
// ============================================================================

/// 모든 Agent 변형이 구현해야 하는 핵심 트레이트
///
/// Agent 실행 흐름:
/// 1. `think()`: 현재 상황 분석 및 추론
/// 2. `plan()`: 실행 계획 수립
/// 3. `execute()`: 계획 실행 (Tool 호출 등)
/// 4. `reflect()`: 결과 평가 및 다음 단계 결정
///
/// 각 단계는 선택적이며, RuntimeConfig로 활성화/비활성화 가능합니다.
#[async_trait]
pub trait AgentRuntime: Send + Sync {
    /// Agent 메타데이터 반환
    fn metadata(&self) -> &AgentMetadata;

    /// 런타임 설정 반환
    fn config(&self) -> &RuntimeConfig;

    /// 런타임 설정 변경
    fn set_config(&mut self, config: RuntimeConfig);

    /// 초기화 (세션 시작 시 호출)
    async fn initialize(&mut self, ctx: &mut RuntimeContext) -> Result<()> {
        let _ = ctx;
        Ok(())
    }

    /// Think 단계: 현재 상황 분석 및 추론
    ///
    /// Chain-of-Thought, Tree-of-Thought 등의 추론 전략 적용
    async fn think(&self, ctx: &mut RuntimeContext) -> Result<ThinkOutput>;

    /// Plan 단계: 실행 계획 수립
    ///
    /// 필요한 액션들을 계획하고 순서 결정
    async fn plan(&self, ctx: &mut RuntimeContext) -> Result<PlanOutput> {
        // 기본 구현: think 결과를 바로 실행 계획으로 변환
        let think_output = ctx.last_think_output().cloned();
        Ok(PlanOutput::from_think(think_output))
    }

    /// Execute 단계: 계획 실행
    ///
    /// Tool 호출, 코드 실행 등 실제 작업 수행
    async fn execute(&self, ctx: &mut RuntimeContext) -> Result<ExecuteOutput>;

    /// Reflect 단계: 결과 평가 및 다음 단계 결정
    ///
    /// Reflexion 패턴 등 자기 평가 전략 적용
    async fn reflect(&self, ctx: &mut RuntimeContext) -> Result<ReflectOutput> {
        // 기본 구현: 단순 평가
        let execute_output = ctx.last_execute_output();
        Ok(ReflectOutput::from_execute(execute_output))
    }

    /// 종료 조건 확인
    fn should_stop(&self, ctx: &RuntimeContext) -> bool {
        // 기본: 최대 턴 수 초과하면 종료
        ctx.current_turn() >= self.config().max_turns
    }

    /// 정리 (세션 종료 시 호출)
    async fn cleanup(&mut self, ctx: &mut RuntimeContext) -> Result<()> {
        let _ = ctx;
        Ok(())
    }

    /// 중단 (사용자 요청 시)
    async fn abort(&mut self, ctx: &mut RuntimeContext) -> Result<()> {
        let _ = ctx;
        Ok(())
    }

    /// 일시 정지
    async fn pause(&mut self, ctx: &mut RuntimeContext) -> Result<()> {
        let _ = ctx;
        Ok(())
    }

    /// 재개
    async fn resume(&mut self, ctx: &mut RuntimeContext) -> Result<()> {
        let _ = ctx;
        Ok(())
    }

    /// 스냅샷 생성 (핸드오프용)
    fn snapshot(&self, ctx: &RuntimeContext) -> Result<serde_json::Value> {
        let _ = ctx;
        Ok(serde_json::Value::Null)
    }

    /// 스냅샷에서 복원
    async fn restore(
        &mut self,
        ctx: &mut RuntimeContext,
        snapshot: serde_json::Value,
    ) -> Result<()> {
        let _ = (ctx, snapshot);
        Ok(())
    }
}

// ============================================================================
// AgentRuntimeExt - 확장 트레이트
// ============================================================================

/// AgentRuntime 확장 메서드
#[async_trait]
pub trait AgentRuntimeExt: AgentRuntime {
    /// 전체 실행 루프
    async fn run_loop(
        &mut self,
        ctx: &mut RuntimeContext,
        hooks: Option<&dyn RuntimeHooks>,
    ) -> Result<()> {
        let hooks = hooks.unwrap_or(&DefaultHooks as &dyn RuntimeHooks);
        let config = self.config().clone();

        self.initialize(ctx).await?;

        while !self.should_stop(ctx) {
            // Think
            if config.enable_think {
                hooks.before_think(ctx).await?;
                let think_out = self.think(ctx).await?;
                ctx.set_think_output(think_out.clone());
                hooks.after_think(ctx, &think_out).await?;
            }

            // Plan
            if config.enable_plan {
                hooks.before_plan(ctx).await?;
                let plan_out = self.plan(ctx).await?;
                ctx.set_plan_output(plan_out.clone());
                hooks.after_plan(ctx, &plan_out).await?;
            }

            // Execute
            hooks.before_execute(ctx).await?;
            let exec_out = self.execute(ctx).await?;
            ctx.set_execute_output(exec_out.clone());
            hooks.after_execute(ctx, &exec_out).await?;

            // Reflect
            if config.enable_reflect {
                hooks.before_reflect(ctx).await?;
                let reflect_out = self.reflect(ctx).await?;
                ctx.set_reflect_output(reflect_out.clone());
                hooks.after_reflect(ctx, &reflect_out).await?;
            }

            ctx.increment_turn();
            hooks.on_turn_complete(ctx, ctx.current_turn()).await?;

            // 완료 조건 확인
            if ctx.is_complete() {
                break;
            }
        }

        self.cleanup(ctx).await?;
        Ok(())
    }
}

// 모든 AgentRuntime에 대해 자동 구현
impl<T: AgentRuntime + ?Sized> AgentRuntimeExt for T {}
