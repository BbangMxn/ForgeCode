//! Benchmark Metrics
//!
//! Agent 성능 지표 수집 및 관리

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 메트릭 타입
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MetricType {
    // 성능 메트릭
    Latency,
    Throughput,
    TurnsUsed,

    // 품질 메트릭
    Accuracy,
    Completeness,
    Correctness,

    // 비용 메트릭
    InputTokens,
    OutputTokens,
    TotalTokens,
    EstimatedCost,
}

/// 성능 메트릭
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    /// 총 소요 시간 (ms)
    pub total_duration_ms: u64,

    /// 평균 턴 시간 (ms)
    pub avg_turn_duration_ms: f64,

    /// 최소 턴 시간 (ms)
    pub min_turn_duration_ms: u64,

    /// 최대 턴 시간 (ms)
    pub max_turn_duration_ms: u64,

    /// 사용한 턴 수
    pub turns_used: u32,

    /// Tool 호출 수
    pub tool_calls: u32,

    /// 병렬 Tool 호출 수
    pub parallel_tool_calls: u32,

    /// 첫 응답까지 시간 (ms)
    pub time_to_first_response_ms: u64,
}

/// 품질 메트릭
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QualityMetrics {
    /// 정확도 (0.0 ~ 1.0)
    pub accuracy: f32,

    /// 완성도 (0.0 ~ 1.0)
    pub completeness: f32,

    /// 정확성 (0.0 ~ 1.0)
    pub correctness: f32,

    /// 예상 결과 매칭 수
    pub expected_matches: u32,

    /// 예상 결과 총 수
    pub expected_total: u32,

    /// 성공한 액션 비율
    pub action_success_rate: f32,

    /// 재시도 횟수
    pub retry_count: u32,
}

/// 비용 메트릭
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CostMetrics {
    /// 입력 토큰 수
    pub input_tokens: u64,

    /// 출력 토큰 수
    pub output_tokens: u64,

    /// 총 토큰 수
    pub total_tokens: u64,

    /// 추정 비용 (USD)
    pub estimated_cost_usd: f64,

    /// 모델별 토큰 사용량
    pub tokens_by_model: HashMap<String, u64>,
}

impl CostMetrics {
    /// 비용 계산 (기본 가격)
    pub fn calculate_cost(&mut self) {
        // Claude 3.5 Sonnet 기준 (대략적인 가격)
        let input_cost_per_1k = 0.003;
        let output_cost_per_1k = 0.015;

        self.estimated_cost_usd = (self.input_tokens as f64 / 1000.0) * input_cost_per_1k
            + (self.output_tokens as f64 / 1000.0) * output_cost_per_1k;
    }
}

/// 종합 Agent 메트릭
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentMetrics {
    /// Agent ID
    pub agent_id: String,

    /// 시나리오 ID
    pub scenario_id: String,

    /// 테스트 케이스 ID
    pub test_case_id: String,

    /// 성능 메트릭
    pub performance: PerformanceMetrics,

    /// 품질 메트릭
    pub quality: QualityMetrics,

    /// 비용 메트릭
    pub cost: CostMetrics,

    /// 수집 시간
    pub collected_at: Option<DateTime<Utc>>,

    /// 추가 메타데이터
    pub metadata: HashMap<String, serde_json::Value>,
}

impl AgentMetrics {
    /// 새 메트릭 생성
    pub fn new(agent_id: impl Into<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
            collected_at: Some(Utc::now()),
            ..Default::default()
        }
    }

    /// 종합 점수 계산 (0.0 ~ 1.0)
    pub fn overall_score(&self) -> f32 {
        // 가중치 기반 종합 점수
        let quality_weight = 0.5;
        let performance_weight = 0.3;
        let cost_weight = 0.2;

        let quality_score =
            (self.quality.accuracy + self.quality.completeness + self.quality.correctness) / 3.0;

        // 성능 점수: 턴 수가 적을수록 좋음 (최대 50턴 기준)
        let performance_score = 1.0 - (self.performance.turns_used.min(50) as f32 / 50.0);

        // 비용 점수: 토큰 수가 적을수록 좋음 (최대 100k 기준)
        let cost_score = 1.0 - (self.cost.total_tokens.min(100_000) as f32 / 100_000.0);

        quality_score * quality_weight
            + performance_score * performance_weight
            + cost_score * cost_weight
    }
}

/// 메트릭 수집기
pub struct MetricsCollector {
    /// 수집된 메트릭들
    metrics: Vec<AgentMetrics>,

    /// 현재 수집 중인 메트릭
    current: Option<AgentMetrics>,

    /// 턴별 시간 기록
    turn_durations: Vec<u64>,

    /// 시작 시간
    start_time: Option<std::time::Instant>,
}

impl MetricsCollector {
    /// 새 수집기 생성
    pub fn new() -> Self {
        Self {
            metrics: Vec::new(),
            current: None,
            turn_durations: Vec::new(),
            start_time: None,
        }
    }

    /// 수집 시작
    pub fn start(
        &mut self,
        agent_id: impl Into<String>,
        scenario_id: impl Into<String>,
        test_case_id: impl Into<String>,
    ) {
        let mut metrics = AgentMetrics::new(agent_id);
        metrics.scenario_id = scenario_id.into();
        metrics.test_case_id = test_case_id.into();

        self.current = Some(metrics);
        self.turn_durations.clear();
        self.start_time = Some(std::time::Instant::now());
    }

    /// 턴 완료 기록
    pub fn record_turn(&mut self, duration_ms: u64) {
        self.turn_durations.push(duration_ms);

        if let Some(ref mut metrics) = self.current {
            metrics.performance.turns_used += 1;
        }
    }

    /// Tool 호출 기록
    pub fn record_tool_call(&mut self, parallel: bool) {
        if let Some(ref mut metrics) = self.current {
            metrics.performance.tool_calls += 1;
            if parallel {
                metrics.performance.parallel_tool_calls += 1;
            }
        }
    }

    /// 토큰 사용량 기록
    pub fn record_tokens(&mut self, input: u64, output: u64) {
        if let Some(ref mut metrics) = self.current {
            metrics.cost.input_tokens += input;
            metrics.cost.output_tokens += output;
            metrics.cost.total_tokens += input + output;
        }
    }

    /// 품질 점수 기록
    pub fn record_quality(&mut self, accuracy: f32, completeness: f32, correctness: f32) {
        if let Some(ref mut metrics) = self.current {
            metrics.quality.accuracy = accuracy;
            metrics.quality.completeness = completeness;
            metrics.quality.correctness = correctness;
        }
    }

    /// 예상 결과 매칭 기록
    pub fn record_expected_match(&mut self, matched: bool) {
        if let Some(ref mut metrics) = self.current {
            metrics.quality.expected_total += 1;
            if matched {
                metrics.quality.expected_matches += 1;
            }
        }
    }

    /// 수집 완료
    pub fn finish(&mut self) -> Option<AgentMetrics> {
        let mut metrics = self.current.take()?;

        // 총 시간 계산
        if let Some(start) = self.start_time.take() {
            metrics.performance.total_duration_ms = start.elapsed().as_millis() as u64;
        }

        // 턴 시간 통계 계산
        if !self.turn_durations.is_empty() {
            metrics.performance.avg_turn_duration_ms =
                self.turn_durations.iter().sum::<u64>() as f64 / self.turn_durations.len() as f64;
            metrics.performance.min_turn_duration_ms =
                *self.turn_durations.iter().min().unwrap_or(&0);
            metrics.performance.max_turn_duration_ms =
                *self.turn_durations.iter().max().unwrap_or(&0);
        }

        // 비용 계산
        metrics.cost.calculate_cost();

        // 품질 점수 계산
        if metrics.quality.expected_total > 0 {
            metrics.quality.accuracy =
                metrics.quality.expected_matches as f32 / metrics.quality.expected_total as f32;
        }

        self.metrics.push(metrics.clone());

        Some(metrics)
    }

    /// 모든 수집된 메트릭
    pub fn all_metrics(&self) -> &[AgentMetrics] {
        &self.metrics
    }

    /// 메트릭 초기화
    pub fn clear(&mut self) {
        self.metrics.clear();
        self.current = None;
        self.turn_durations.clear();
        self.start_time = None;
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}
