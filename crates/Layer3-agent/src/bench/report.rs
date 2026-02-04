//! Benchmark Report
//!
//! 벤치마크 결과 리포트 생성

use super::metrics::AgentMetrics;
use super::runner::{BenchmarkResult, ComparisonResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 리포트 형식
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportFormat {
    /// 텍스트 (터미널 출력용)
    Text,
    /// JSON
    Json,
    /// Markdown
    Markdown,
    /// HTML
    Html,
}

/// 벤치마크 리포트
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkReport {
    /// 리포트 제목
    pub title: String,

    /// 생성 시간
    pub generated_at: chrono::DateTime<chrono::Utc>,

    /// 요약
    pub summary: ReportSummary,

    /// Agent별 결과
    pub agent_results: HashMap<String, AgentReportSection>,

    /// 비교 결과 (여러 Agent 비교 시)
    pub comparison: Option<ComparisonReportSection>,

    /// 권장 사항
    pub recommendations: Vec<String>,
}

/// 리포트 요약
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportSummary {
    /// 테스트된 Agent 수
    pub agents_tested: usize,

    /// 실행된 시나리오 수
    pub scenarios_run: usize,

    /// 총 테스트 케이스 수
    pub total_test_cases: usize,

    /// 전체 성공률
    pub overall_success_rate: f32,

    /// 총 실행 시간 (ms)
    pub total_execution_time_ms: u64,

    /// 최고 성능 Agent
    pub best_performing_agent: Option<String>,
}

/// Agent별 리포트 섹션
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentReportSection {
    /// Agent ID
    pub agent_id: String,

    /// Agent 이름
    pub agent_name: String,

    /// 종합 점수
    pub overall_score: f32,

    /// 성능 요약
    pub performance_summary: String,

    /// 품질 요약
    pub quality_summary: String,

    /// 비용 요약
    pub cost_summary: String,

    /// 강점
    pub strengths: Vec<String>,

    /// 약점
    pub weaknesses: Vec<String>,

    /// 메트릭
    pub metrics: AgentMetrics,
}

/// 비교 리포트 섹션
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonReportSection {
    /// 순위
    pub ranking: Vec<RankingEntry>,

    /// 카테고리별 우승자
    pub category_winners: HashMap<String, String>,

    /// 상세 비교 테이블
    pub comparison_table: Vec<ComparisonRow>,
}

/// 순위 항목
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankingEntry {
    pub rank: usize,
    pub agent_id: String,
    pub score: f32,
    pub success_rate: f32,
}

/// 비교 테이블 행
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonRow {
    pub metric: String,
    pub values: HashMap<String, String>,
    pub best: String,
}

/// 리포트 생성기
pub struct ReportGenerator;

impl ReportGenerator {
    /// 단일 Agent 결과에서 리포트 생성
    pub fn from_result(result: &BenchmarkResult) -> BenchmarkReport {
        let mut agent_results = HashMap::new();

        let section = AgentReportSection {
            agent_id: result.agent_id.clone(),
            agent_name: result.agent_id.clone(), // 실제로는 메타데이터에서
            overall_score: result.aggregate_metrics.overall_score(),
            performance_summary: format!(
                "Total time: {}ms, Turns: {}",
                result.aggregate_metrics.performance.total_duration_ms,
                result.aggregate_metrics.performance.turns_used
            ),
            quality_summary: format!(
                "Accuracy: {:.1}%",
                result.aggregate_metrics.quality.accuracy * 100.0
            ),
            cost_summary: format!(
                "Tokens: {}, Est. cost: ${:.4}",
                result.aggregate_metrics.cost.total_tokens,
                result.aggregate_metrics.cost.estimated_cost_usd
            ),
            strengths: vec![],
            weaknesses: vec![],
            metrics: result.aggregate_metrics.clone(),
        };

        agent_results.insert(result.agent_id.clone(), section);

        let summary = ReportSummary {
            agents_tested: 1,
            scenarios_run: result.scenario_results.len(),
            total_test_cases: result.scenario_results.iter().map(|r| r.total_tests).sum(),
            overall_success_rate: result.success_rate,
            total_execution_time_ms: result.execution_time_ms,
            best_performing_agent: Some(result.agent_id.clone()),
        };

        BenchmarkReport {
            title: format!("Benchmark Report: {}", result.agent_id),
            generated_at: chrono::Utc::now(),
            summary,
            agent_results,
            comparison: None,
            recommendations: Self::generate_recommendations(result),
        }
    }

    /// 비교 결과에서 리포트 생성
    pub fn from_comparison(comparison: &ComparisonResult) -> BenchmarkReport {
        let mut agent_results = HashMap::new();
        let mut total_tests = 0;
        let mut total_time = 0;

        for (agent_id, result) in &comparison.results {
            total_tests += result
                .scenario_results
                .iter()
                .map(|r| r.total_tests)
                .sum::<usize>();
            total_time += result.execution_time_ms;

            let section = AgentReportSection {
                agent_id: agent_id.clone(),
                agent_name: agent_id.clone(),
                overall_score: result.aggregate_metrics.overall_score(),
                performance_summary: format!(
                    "Total time: {}ms, Turns: {}",
                    result.aggregate_metrics.performance.total_duration_ms,
                    result.aggregate_metrics.performance.turns_used
                ),
                quality_summary: format!(
                    "Accuracy: {:.1}%",
                    result.aggregate_metrics.quality.accuracy * 100.0
                ),
                cost_summary: format!(
                    "Tokens: {}, Est. cost: ${:.4}",
                    result.aggregate_metrics.cost.total_tokens,
                    result.aggregate_metrics.cost.estimated_cost_usd
                ),
                strengths: vec![],
                weaknesses: vec![],
                metrics: result.aggregate_metrics.clone(),
            };

            agent_results.insert(agent_id.clone(), section);
        }

        let best_agent = comparison.ranking.first().map(|(id, _)| id.clone());
        let overall_success = comparison
            .results
            .values()
            .map(|r| r.success_rate)
            .sum::<f32>()
            / comparison.results.len().max(1) as f32;

        let ranking: Vec<RankingEntry> = comparison
            .ranking
            .iter()
            .enumerate()
            .map(|(i, (id, score))| {
                let success_rate = comparison
                    .results
                    .get(id)
                    .map(|r| r.success_rate)
                    .unwrap_or(0.0);
                RankingEntry {
                    rank: i + 1,
                    agent_id: id.clone(),
                    score: *score,
                    success_rate,
                }
            })
            .collect();

        let comparison_section = ComparisonReportSection {
            ranking,
            category_winners: comparison.winners_by_category.clone(),
            comparison_table: vec![],
        };

        let summary = ReportSummary {
            agents_tested: comparison.agent_ids.len(),
            scenarios_run: comparison
                .results
                .values()
                .next()
                .map(|r| r.scenario_results.len())
                .unwrap_or(0),
            total_test_cases: total_tests,
            overall_success_rate: overall_success,
            total_execution_time_ms: total_time,
            best_performing_agent: best_agent,
        };

        BenchmarkReport {
            title: "Agent Comparison Report".to_string(),
            generated_at: chrono::Utc::now(),
            summary,
            agent_results,
            comparison: Some(comparison_section),
            recommendations: vec![],
        }
    }

    /// 권장 사항 생성
    fn generate_recommendations(result: &BenchmarkResult) -> Vec<String> {
        let mut recommendations = Vec::new();

        // 성공률 기반 권장
        if result.success_rate < 0.5 {
            recommendations
                .push("Consider using a more capable agent variant for this task type".to_string());
        }

        // 성능 기반 권장
        if result.aggregate_metrics.performance.turns_used > 30 {
            recommendations.push(
                "High turn count detected. Consider using a more efficient planning strategy"
                    .to_string(),
            );
        }

        // 비용 기반 권장
        if result.aggregate_metrics.cost.total_tokens > 50_000 {
            recommendations
                .push("High token usage. Consider using summarizing memory strategy".to_string());
        }

        recommendations
    }

    /// 리포트를 특정 형식으로 출력
    pub fn render(report: &BenchmarkReport, format: ReportFormat) -> String {
        match format {
            ReportFormat::Text => Self::render_text(report),
            ReportFormat::Json => serde_json::to_string_pretty(report).unwrap_or_default(),
            ReportFormat::Markdown => Self::render_markdown(report),
            ReportFormat::Html => Self::render_html(report),
        }
    }

    /// 텍스트 형식 렌더링
    fn render_text(report: &BenchmarkReport) -> String {
        let mut output = String::new();

        output.push_str(&format!(
            "╔═══════════════════════════════════════════════════════════╗\n"
        ));
        output.push_str(&format!("║ {} ║\n", report.title));
        output.push_str(&format!(
            "╚═══════════════════════════════════════════════════════════╝\n\n"
        ));

        output.push_str("SUMMARY\n");
        output.push_str(&format!(
            "  Agents Tested: {}\n",
            report.summary.agents_tested
        ));
        output.push_str(&format!(
            "  Scenarios Run: {}\n",
            report.summary.scenarios_run
        ));
        output.push_str(&format!(
            "  Total Tests: {}\n",
            report.summary.total_test_cases
        ));
        output.push_str(&format!(
            "  Success Rate: {:.1}%\n",
            report.summary.overall_success_rate * 100.0
        ));
        output.push_str(&format!(
            "  Execution Time: {}ms\n",
            report.summary.total_execution_time_ms
        ));

        if let Some(best) = &report.summary.best_performing_agent {
            output.push_str(&format!("  Best Agent: {}\n", best));
        }

        output.push_str("\n");

        for (id, section) in &report.agent_results {
            output.push_str(&format!("AGENT: {}\n", id));
            output.push_str(&format!("  Score: {:.2}\n", section.overall_score));
            output.push_str(&format!("  Performance: {}\n", section.performance_summary));
            output.push_str(&format!("  Quality: {}\n", section.quality_summary));
            output.push_str(&format!("  Cost: {}\n", section.cost_summary));
            output.push_str("\n");
        }

        if !report.recommendations.is_empty() {
            output.push_str("RECOMMENDATIONS\n");
            for rec in &report.recommendations {
                output.push_str(&format!("  • {}\n", rec));
            }
        }

        output
    }

    /// Markdown 형식 렌더링
    fn render_markdown(report: &BenchmarkReport) -> String {
        let mut output = String::new();

        output.push_str(&format!("# {}\n\n", report.title));
        output.push_str(&format!("*Generated: {}*\n\n", report.generated_at));

        output.push_str("## Summary\n\n");
        output.push_str(&format!("| Metric | Value |\n"));
        output.push_str(&format!("|--------|-------|\n"));
        output.push_str(&format!(
            "| Agents Tested | {} |\n",
            report.summary.agents_tested
        ));
        output.push_str(&format!(
            "| Scenarios Run | {} |\n",
            report.summary.scenarios_run
        ));
        output.push_str(&format!(
            "| Total Tests | {} |\n",
            report.summary.total_test_cases
        ));
        output.push_str(&format!(
            "| Success Rate | {:.1}% |\n",
            report.summary.overall_success_rate * 100.0
        ));
        output.push_str(&format!(
            "| Execution Time | {}ms |\n\n",
            report.summary.total_execution_time_ms
        ));

        for (id, section) in &report.agent_results {
            output.push_str(&format!("## Agent: {}\n\n", id));
            output.push_str(&format!(
                "**Overall Score:** {:.2}\n\n",
                section.overall_score
            ));
            output.push_str(&format!(
                "- **Performance:** {}\n",
                section.performance_summary
            ));
            output.push_str(&format!("- **Quality:** {}\n", section.quality_summary));
            output.push_str(&format!("- **Cost:** {}\n\n", section.cost_summary));
        }

        if !report.recommendations.is_empty() {
            output.push_str("## Recommendations\n\n");
            for rec in &report.recommendations {
                output.push_str(&format!("- {}\n", rec));
            }
        }

        output
    }

    /// HTML 형식 렌더링
    fn render_html(report: &BenchmarkReport) -> String {
        format!(
            r#"<!DOCTYPE html>
<html>
<head>
    <title>{}</title>
    <style>
        body {{ font-family: Arial, sans-serif; margin: 20px; }}
        table {{ border-collapse: collapse; width: 100%; }}
        th, td {{ border: 1px solid #ddd; padding: 8px; text-align: left; }}
        th {{ background-color: #4CAF50; color: white; }}
        .score {{ font-size: 24px; font-weight: bold; }}
    </style>
</head>
<body>
    <h1>{}</h1>
    <p>Generated: {}</p>
    <h2>Summary</h2>
    <p>Success Rate: {:.1}%</p>
</body>
</html>"#,
            report.title,
            report.title,
            report.generated_at,
            report.summary.overall_success_rate * 100.0
        )
    }
}
