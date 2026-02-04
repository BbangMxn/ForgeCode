//! Benchmark Runner
//!
//! 벤치마크 실행기

use super::metrics::{AgentMetrics, MetricsCollector};
use super::scenario::{ExpectedOutcome, Scenario, ScenarioResult, TestCaseResult};
use crate::runtime::{AgentRuntime, RuntimeContext};
use crate::variant::AgentRegistry;
use forge_foundation::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

/// 벤치마크 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkConfig {
    /// 작업 디렉토리
    pub working_dir: PathBuf,

    /// 병렬 실행 여부
    pub parallel: bool,

    /// 타임아웃 (초)
    pub timeout_secs: u64,

    /// 상세 로깅
    pub verbose: bool,

    /// 실패 시 중단
    pub fail_fast: bool,

    /// 반복 횟수 (평균 계산용)
    pub iterations: u32,

    /// 워밍업 실행 횟수
    pub warmup_runs: u32,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            working_dir: PathBuf::from("."),
            parallel: false,
            timeout_secs: 300,
            verbose: false,
            fail_fast: false,
            iterations: 1,
            warmup_runs: 0,
        }
    }
}

/// 벤치마크 결과
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    /// Agent ID
    pub agent_id: String,

    /// 시나리오 결과들
    pub scenario_results: Vec<ScenarioResult>,

    /// 종합 메트릭
    pub aggregate_metrics: AgentMetrics,

    /// 실행 시간
    pub execution_time_ms: u64,

    /// 성공률
    pub success_rate: f32,

    /// 메타데이터
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Agent 비교 결과
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonResult {
    /// 비교된 Agent ID들
    pub agent_ids: Vec<String>,

    /// 각 Agent의 결과
    pub results: HashMap<String, BenchmarkResult>,

    /// 순위 (종합 점수 기준)
    pub ranking: Vec<(String, f32)>,

    /// 카테고리별 우승자
    pub winners_by_category: HashMap<String, String>,

    /// 상세 비교
    pub detailed_comparison: HashMap<String, Vec<(String, f32)>>,
}

/// 벤치마크 실행기
pub struct BenchmarkRunner {
    /// 설정
    config: BenchmarkConfig,

    /// Agent 레지스트리
    registry: AgentRegistry,

    /// 메트릭 수집기
    metrics_collector: MetricsCollector,
}

impl BenchmarkRunner {
    /// 새 실행기 생성
    pub fn new(registry: AgentRegistry, config: BenchmarkConfig) -> Self {
        Self {
            config,
            registry,
            metrics_collector: MetricsCollector::new(),
        }
    }

    /// 기본 설정으로 생성
    pub fn with_default_config(registry: AgentRegistry) -> Self {
        Self::new(registry, BenchmarkConfig::default())
    }

    /// 단일 Agent 벤치마크 실행
    pub async fn run_agent(
        &mut self,
        agent_id: &str,
        scenarios: &[Scenario],
    ) -> Result<BenchmarkResult> {
        let start = Instant::now();
        let mut scenario_results = Vec::new();
        let mut total_passed = 0;
        let mut total_tests = 0;

        // Agent 생성
        let mut agent = self.registry.create(agent_id, None)?;

        // 워밍업
        for _ in 0..self.config.warmup_runs {
            if let Some(scenario) = scenarios.first() {
                if let Some(case) = scenario.test_cases.first() {
                    let _ = self.run_test_case(&mut *agent, scenario, case).await;
                }
            }
        }

        // 시나리오 실행
        for scenario in scenarios {
            let scenario_result = self.run_scenario(&mut *agent, scenario).await?;
            total_passed += scenario_result.passed;
            total_tests += scenario_result.total_tests;
            scenario_results.push(scenario_result);

            if self.config.fail_fast
                && scenario_results
                    .last()
                    .map(|r| r.failed > 0)
                    .unwrap_or(false)
            {
                break;
            }
        }

        // 종합 메트릭 계산
        let aggregate_metrics = self.calculate_aggregate_metrics(agent_id, &scenario_results);

        let success_rate = if total_tests > 0 {
            total_passed as f32 / total_tests as f32
        } else {
            0.0
        };

        Ok(BenchmarkResult {
            agent_id: agent_id.to_string(),
            scenario_results,
            aggregate_metrics,
            execution_time_ms: start.elapsed().as_millis() as u64,
            success_rate,
            metadata: HashMap::new(),
        })
    }

    /// 시나리오 실행
    async fn run_scenario(
        &mut self,
        agent: &mut dyn AgentRuntime,
        scenario: &Scenario,
    ) -> Result<ScenarioResult> {
        let start = Instant::now();
        let mut test_results = Vec::new();
        let mut passed = 0;

        for case in &scenario.test_cases {
            for _ in 0..self.config.iterations {
                let result = self.run_test_case(agent, scenario, case).await?;
                if result.passed {
                    passed += 1;
                }
                test_results.push(result);
            }
        }

        let total_tests = scenario.test_cases.len() * self.config.iterations as usize;

        Ok(ScenarioResult {
            scenario_id: scenario.id.clone(),
            total_tests,
            passed,
            failed: total_tests - passed,
            test_results,
            total_duration_ms: start.elapsed().as_millis() as u64,
        })
    }

    /// 단일 테스트 케이스 실행
    async fn run_test_case(
        &mut self,
        agent: &mut dyn AgentRuntime,
        scenario: &Scenario,
        case: &super::scenario::TestCase,
    ) -> Result<TestCaseResult> {
        let start = Instant::now();

        // 메트릭 수집 시작
        self.metrics_collector
            .start(agent.metadata().id.clone(), &scenario.id, &case.id);

        // 컨텍스트 생성
        let session_id = format!("bench-{}-{}", scenario.id, case.id);
        let mut ctx = RuntimeContext::new(&session_id, &self.config.working_dir);

        // 프롬프트 설정
        ctx.add_user_message(&case.prompt);

        // Agent 초기화
        agent.initialize(&mut ctx).await?;

        // 실행 (간소화된 버전)
        let max_turns = case.max_turns.unwrap_or(agent.config().max_turns);
        let mut actual_output = String::new();
        let mut turns_used = 0;

        while turns_used < max_turns && !ctx.is_complete() {
            let turn_start = Instant::now();

            // Think
            let think = agent.think(&mut ctx).await?;
            ctx.set_think_output(think);

            // Execute
            let exec = agent.execute(&mut ctx).await?;
            actual_output = exec.output.clone();
            ctx.set_execute_output(exec);

            turns_used += 1;
            ctx.increment_turn();

            let turn_duration = turn_start.elapsed().as_millis() as u64;
            self.metrics_collector.record_turn(turn_duration);

            if agent.should_stop(&ctx) {
                break;
            }
        }

        // 결과 검증
        let passed = self.verify_outcome(&actual_output, &case.expected);

        // 메트릭 수집 완료
        let _ = self.metrics_collector.finish();

        let failure_reason = if passed {
            None
        } else {
            Some("Expected outcome not matched".to_string())
        };

        Ok(TestCaseResult {
            case_id: case.id.clone(),
            passed,
            duration_ms: start.elapsed().as_millis() as u64,
            turns_used,
            failure_reason,
            actual_output,
        })
    }

    /// 결과 검증
    fn verify_outcome(&self, output: &str, expected: &[ExpectedOutcome]) -> bool {
        if expected.is_empty() {
            return true;
        }

        for expectation in expected {
            let matched = match expectation {
                ExpectedOutcome::Contains(s) => output.contains(s),
                ExpectedOutcome::Matches(pattern) => regex::Regex::new(pattern)
                    .map(|re| re.is_match(output))
                    .unwrap_or(false),
                ExpectedOutcome::FileCreated(_) => true, // 실제로는 파일 확인 필요
                ExpectedOutcome::FileModified(_) => true,
                ExpectedOutcome::CommandExecuted(_) => true,
                ExpectedOutcome::Custom(_) => true,
            };

            if !matched {
                return false;
            }
        }

        true
    }

    /// 종합 메트릭 계산
    fn calculate_aggregate_metrics(
        &self,
        agent_id: &str,
        results: &[ScenarioResult],
    ) -> AgentMetrics {
        let mut metrics = AgentMetrics::new(agent_id);

        let total_duration: u64 = results.iter().map(|r| r.total_duration_ms).sum();
        let total_turns: u32 = results
            .iter()
            .flat_map(|r| &r.test_results)
            .map(|t| t.turns_used)
            .sum();
        let total_tests: u32 = results.iter().map(|r| r.total_tests as u32).sum();
        let total_passed: u32 = results.iter().map(|r| r.passed as u32).sum();

        metrics.performance.total_duration_ms = total_duration;
        metrics.performance.turns_used = total_turns;

        if total_tests > 0 {
            metrics.quality.accuracy = total_passed as f32 / total_tests as f32;
        }

        metrics
    }

    /// 여러 Agent 비교 벤치마크
    pub async fn compare(
        &mut self,
        agent_ids: &[&str],
        scenarios: &[Scenario],
    ) -> Result<ComparisonResult> {
        let mut results = HashMap::new();

        for agent_id in agent_ids {
            let result = self.run_agent(agent_id, scenarios).await?;
            results.insert(agent_id.to_string(), result);
        }

        // 순위 계산
        let mut ranking: Vec<_> = results
            .iter()
            .map(|(id, r)| (id.clone(), r.aggregate_metrics.overall_score()))
            .collect();
        ranking.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // 카테고리별 우승자 (시나리오 카테고리 기준)
        let mut winners_by_category = HashMap::new();
        for scenario in scenarios {
            let mut best_agent = String::new();
            let mut best_score = 0.0f32;

            for (agent_id, result) in &results {
                let scenario_score = result
                    .scenario_results
                    .iter()
                    .find(|r| r.scenario_id == scenario.id)
                    .map(|r| r.passed as f32 / r.total_tests.max(1) as f32)
                    .unwrap_or(0.0);

                if scenario_score > best_score {
                    best_score = scenario_score;
                    best_agent = agent_id.clone();
                }
            }

            if !best_agent.is_empty() {
                winners_by_category.insert(scenario.category.clone(), best_agent);
            }
        }

        Ok(ComparisonResult {
            agent_ids: agent_ids.iter().map(|s| s.to_string()).collect(),
            results,
            ranking,
            winners_by_category,
            detailed_comparison: HashMap::new(),
        })
    }
}
