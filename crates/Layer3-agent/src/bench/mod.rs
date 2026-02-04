//! # Agent Benchmark Framework
//!
//! Agent 변형들을 테스트하고 비교하는 벤치마크 프레임워크입니다.
//!
//! ## 기능
//!
//! - **Scenario**: 테스트 시나리오 정의
//! - **Metrics**: 성능 지표 수집
//! - **Runner**: 벤치마크 실행
//! - **Report**: 결과 리포트 생성

mod metrics;
mod report;
mod runner;
mod scenario;

pub use metrics::{
    AgentMetrics, CostMetrics, MetricType, MetricsCollector, PerformanceMetrics, QualityMetrics,
};
pub use report::{BenchmarkReport, ReportFormat, ReportGenerator};
pub use runner::{BenchmarkConfig, BenchmarkResult, BenchmarkRunner, ComparisonResult};
pub use scenario::{
    DifficultyLevel, ExpectedOutcome, Scenario, ScenarioBuilder, ScenarioResult, TestCase,
};
