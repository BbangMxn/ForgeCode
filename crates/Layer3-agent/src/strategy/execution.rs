//! Execution Strategies
//!
//! 다양한 실행 전략 구현입니다.
//!
//! - Sequential: 순차 실행
//! - Parallel: 병렬 실행
//! - Adaptive: 상황에 따라 조절

use async_trait::async_trait;
use forge_foundation::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

// ============================================================================
// Execution Types
// ============================================================================

/// 실행 계획
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPlan {
    /// 실행할 액션들
    pub actions: Vec<ExecutionAction>,

    /// 병렬 실행 그룹 (같은 그룹은 동시 실행 가능)
    pub parallel_groups: Vec<Vec<usize>>,

    /// 예상 총 시간 (초)
    pub estimated_duration: u32,
}

/// 실행 액션
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionAction {
    /// 액션 ID
    pub id: String,

    /// Tool 이름
    pub tool_name: String,

    /// Tool 인자
    pub arguments: serde_json::Value,

    /// 의존하는 액션 ID들
    pub dependencies: Vec<String>,

    /// 타임아웃 (초)
    pub timeout: u32,

    /// 재시도 횟수
    pub max_retries: u32,

    /// 우선순위
    pub priority: i32,
}

impl ExecutionAction {
    /// 새 액션 생성
    pub fn new(tool_name: impl Into<String>, arguments: serde_json::Value) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string()[..8].to_string(),
            tool_name: tool_name.into(),
            arguments,
            dependencies: Vec::new(),
            timeout: 60,
            max_retries: 1,
            priority: 0,
        }
    }

    /// 의존성 추가
    pub fn depends_on(mut self, action_id: impl Into<String>) -> Self {
        self.dependencies.push(action_id.into());
        self
    }

    /// 타임아웃 설정
    pub fn with_timeout(mut self, timeout: u32) -> Self {
        self.timeout = timeout;
        self
    }

    /// 재시도 설정
    pub fn with_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
    }
}

/// 실행 결과
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    /// 액션 ID
    pub action_id: String,

    /// 성공 여부
    pub success: bool,

    /// 출력
    pub output: String,

    /// 에러 메시지 (실패 시)
    pub error: Option<String>,

    /// 소요 시간 (ms)
    pub duration_ms: u64,

    /// 재시도 횟수
    pub retries: u32,
}

impl ExecutionResult {
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
            retries: 0,
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
            retries: 0,
        }
    }
}

/// 전체 실행 결과
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BatchExecutionResult {
    /// 개별 결과들
    pub results: Vec<ExecutionResult>,

    /// 총 성공 수
    pub success_count: u32,

    /// 총 실패 수
    pub failure_count: u32,

    /// 총 소요 시간 (ms)
    pub total_duration_ms: u64,

    /// 병렬 실행 여부
    pub was_parallel: bool,
}

impl BatchExecutionResult {
    /// 새 결과 생성
    pub fn new() -> Self {
        Self::default()
    }

    /// 결과 추가
    pub fn add_result(&mut self, result: ExecutionResult) {
        if result.success {
            self.success_count += 1;
        } else {
            self.failure_count += 1;
        }
        self.total_duration_ms += result.duration_ms;
        self.results.push(result);
    }

    /// 모두 성공 여부
    pub fn all_succeeded(&self) -> bool {
        self.failure_count == 0
    }
}

// ============================================================================
// ExecutionStrategy - 실행 전략 트레이트
// ============================================================================

/// 실행 컨텍스트
pub struct ExecutionContext<'a> {
    /// 작업 디렉토리
    pub working_dir: &'a std::path::Path,

    /// 세션 ID
    pub session_id: &'a str,

    /// 환경 변수
    pub env: HashMap<String, String>,

    /// Tool 실행 함수
    pub executor: &'a dyn ToolExecutor,
}

/// Tool 실행자 트레이트
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    /// Tool 실행
    async fn execute(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
        timeout: Duration,
    ) -> Result<String>;
}

/// 실행 전략 트레이트
#[async_trait]
pub trait ExecutionStrategy: Send + Sync {
    /// 전략 이름
    fn name(&self) -> &str;

    /// 전략 설명
    fn description(&self) -> &str;

    /// 최대 병렬 실행 수
    fn max_parallelism(&self) -> usize;

    /// 실행 계획 생성 (의존성 분석)
    fn create_plan(&self, actions: Vec<ExecutionAction>) -> ExecutionPlan;

    /// 계획 실행
    async fn execute(
        &self,
        plan: &ExecutionPlan,
        ctx: &ExecutionContext<'_>,
    ) -> Result<BatchExecutionResult>;
}

// ============================================================================
// SequentialExecution - 순차 실행
// ============================================================================

/// 순차 실행 전략
pub struct SequentialExecution;

impl SequentialExecution {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SequentialExecution {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ExecutionStrategy for SequentialExecution {
    fn name(&self) -> &str {
        "sequential"
    }

    fn description(&self) -> &str {
        "Executes actions one at a time in order"
    }

    fn max_parallelism(&self) -> usize {
        1
    }

    fn create_plan(&self, actions: Vec<ExecutionAction>) -> ExecutionPlan {
        let parallel_groups: Vec<Vec<usize>> =
            actions.iter().enumerate().map(|(i, _)| vec![i]).collect();

        let estimated_duration = actions.iter().map(|a| a.timeout).sum();

        ExecutionPlan {
            actions,
            parallel_groups,
            estimated_duration,
        }
    }

    async fn execute(
        &self,
        plan: &ExecutionPlan,
        ctx: &ExecutionContext<'_>,
    ) -> Result<BatchExecutionResult> {
        let mut batch_result = BatchExecutionResult::new();
        batch_result.was_parallel = false;

        for action in &plan.actions {
            let start = Instant::now();
            let timeout = Duration::from_secs(action.timeout as u64);

            let mut retries = 0;
            let mut last_error = None;

            while retries <= action.max_retries {
                match ctx
                    .executor
                    .execute(&action.tool_name, action.arguments.clone(), timeout)
                    .await
                {
                    Ok(output) => {
                        let duration_ms = start.elapsed().as_millis() as u64;
                        let mut result = ExecutionResult::success(&action.id, output, duration_ms);
                        result.retries = retries;
                        batch_result.add_result(result);
                        last_error = None;
                        break;
                    }
                    Err(e) => {
                        last_error = Some(e.to_string());
                        retries += 1;
                    }
                }
            }

            if let Some(error) = last_error {
                let duration_ms = start.elapsed().as_millis() as u64;
                let mut result = ExecutionResult::failure(&action.id, error, duration_ms);
                result.retries = retries;
                batch_result.add_result(result);
            }
        }

        Ok(batch_result)
    }
}

// ============================================================================
// ParallelExecution - 병렬 실행
// ============================================================================

/// 병렬 실행 전략
pub struct ParallelExecution {
    max_concurrent: usize,
}

impl ParallelExecution {
    pub fn new(max_concurrent: usize) -> Self {
        Self { max_concurrent }
    }
}

impl Default for ParallelExecution {
    fn default() -> Self {
        Self::new(4)
    }
}

#[async_trait]
impl ExecutionStrategy for ParallelExecution {
    fn name(&self) -> &str {
        "parallel"
    }

    fn description(&self) -> &str {
        "Executes independent actions in parallel"
    }

    fn max_parallelism(&self) -> usize {
        self.max_concurrent
    }

    fn create_plan(&self, actions: Vec<ExecutionAction>) -> ExecutionPlan {
        // 의존성 기반으로 병렬 그룹 생성
        let mut groups: Vec<Vec<usize>> = Vec::new();
        let mut completed: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut remaining: Vec<(usize, &ExecutionAction)> = actions.iter().enumerate().collect();

        while !remaining.is_empty() {
            // 의존성이 모두 완료된 액션들을 찾음
            let ready: Vec<_> = remaining
                .iter()
                .filter(|(_, a)| a.dependencies.iter().all(|d| completed.contains(d)))
                .map(|(i, _)| *i)
                .take(self.max_concurrent)
                .collect();

            if ready.is_empty() {
                // 데드락 방지: 강제로 첫 번째 선택
                if let Some((i, _)) = remaining.first() {
                    groups.push(vec![*i]);
                    completed.insert(actions[*i].id.clone());
                    remaining.remove(0);
                }
                continue;
            }

            // 현재 그룹에 추가
            groups.push(ready.clone());

            // 완료 표시 및 remaining에서 제거
            for i in ready.iter().rev() {
                completed.insert(actions[*i].id.clone());
                remaining.retain(|(idx, _)| idx != i);
            }
        }

        let estimated_duration = groups
            .iter()
            .map(|g| g.iter().map(|i| actions[*i].timeout).max().unwrap_or(0))
            .sum();

        ExecutionPlan {
            actions,
            parallel_groups: groups,
            estimated_duration,
        }
    }

    async fn execute(
        &self,
        plan: &ExecutionPlan,
        ctx: &ExecutionContext<'_>,
    ) -> Result<BatchExecutionResult> {
        let mut batch_result = BatchExecutionResult::new();
        batch_result.was_parallel = true;

        for group in &plan.parallel_groups {
            let futures: Vec<_> = group
                .iter()
                .map(|&i| {
                    let action = &plan.actions[i];
                    let tool_name = action.tool_name.clone();
                    let arguments = action.arguments.clone();
                    let timeout = Duration::from_secs(action.timeout as u64);
                    let action_id = action.id.clone();

                    async move {
                        let start = Instant::now();
                        match ctx.executor.execute(&tool_name, arguments, timeout).await {
                            Ok(output) => {
                                let duration_ms = start.elapsed().as_millis() as u64;
                                ExecutionResult::success(action_id, output, duration_ms)
                            }
                            Err(e) => {
                                let duration_ms = start.elapsed().as_millis() as u64;
                                ExecutionResult::failure(action_id, e.to_string(), duration_ms)
                            }
                        }
                    }
                })
                .collect();

            let results = futures::future::join_all(futures).await;

            for result in results {
                batch_result.add_result(result);
            }
        }

        Ok(batch_result)
    }
}

// ============================================================================
// AdaptiveExecution - 적응형 실행
// ============================================================================

/// 적응형 실행 전략
///
/// 상황에 따라 순차/병렬을 자동으로 선택합니다.
pub struct AdaptiveExecution {
    max_concurrent: usize,
    parallel_threshold: usize, // 이 수 이상이면 병렬
}

impl AdaptiveExecution {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            max_concurrent,
            parallel_threshold: 3,
        }
    }

    pub fn with_threshold(mut self, threshold: usize) -> Self {
        self.parallel_threshold = threshold;
        self
    }
}

impl Default for AdaptiveExecution {
    fn default() -> Self {
        Self::new(4)
    }
}

#[async_trait]
impl ExecutionStrategy for AdaptiveExecution {
    fn name(&self) -> &str {
        "adaptive"
    }

    fn description(&self) -> &str {
        "Automatically chooses between sequential and parallel execution"
    }

    fn max_parallelism(&self) -> usize {
        self.max_concurrent
    }

    fn create_plan(&self, actions: Vec<ExecutionAction>) -> ExecutionPlan {
        // 독립적인 액션 수 계산
        let independent_count = actions.iter().filter(|a| a.dependencies.is_empty()).count();

        if independent_count >= self.parallel_threshold {
            // 병렬 계획
            ParallelExecution::new(self.max_concurrent).create_plan(actions)
        } else {
            // 순차 계획
            SequentialExecution::new().create_plan(actions)
        }
    }

    async fn execute(
        &self,
        plan: &ExecutionPlan,
        ctx: &ExecutionContext<'_>,
    ) -> Result<BatchExecutionResult> {
        // 병렬 그룹의 최대 크기로 판단
        let max_group_size = plan
            .parallel_groups
            .iter()
            .map(|g| g.len())
            .max()
            .unwrap_or(1);

        if max_group_size >= self.parallel_threshold {
            ParallelExecution::new(self.max_concurrent)
                .execute(plan, ctx)
                .await
        } else {
            SequentialExecution::new().execute(plan, ctx).await
        }
    }
}

// ============================================================================
// 유틸리티
// ============================================================================

/// 실행 전략 팩토리
pub fn create_execution_strategy(name: &str, max_concurrent: usize) -> Box<dyn ExecutionStrategy> {
    match name {
        "sequential" | "seq" => Box::new(SequentialExecution::new()),
        "parallel" | "par" => Box::new(ParallelExecution::new(max_concurrent)),
        "adaptive" | "auto" => Box::new(AdaptiveExecution::new(max_concurrent)),
        _ => Box::new(AdaptiveExecution::new(max_concurrent)),
    }
}

/// 간단한 Tool 실행자 (테스트용)
pub struct SimpleToolExecutor;

#[async_trait]
impl ToolExecutor for SimpleToolExecutor {
    async fn execute(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
        _timeout: Duration,
    ) -> Result<String> {
        // 실제 구현에서는 ToolRegistry를 통해 실행
        Ok(format!("Executed {} with {:?}", tool_name, arguments))
    }
}
