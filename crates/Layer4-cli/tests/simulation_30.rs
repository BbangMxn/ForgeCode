//! 30회 시나리오 시뮬레이션 테스트
//!
//! 실제 Agent 동작을 검증하는 30가지 시나리오

use std::collections::HashMap;
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};

/// 시뮬레이션 결과
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationResult {
    pub scenario_id: String,
    pub description: String,
    pub success: bool,
    pub duration_ms: u64,
    pub error: Option<String>,
    pub response_preview: String,
}

/// 시뮬레이션 시나리오
#[derive(Debug, Clone)]
pub struct SimScenario {
    pub id: String,
    pub prompt: String,
    pub description: String,
    pub category: String,
    pub expected_keywords: Vec<String>,
}

impl SimScenario {
    pub fn new(id: &str, prompt: &str, desc: &str, category: &str) -> Self {
        Self {
            id: id.to_string(),
            prompt: prompt.to_string(),
            description: desc.to_string(),
            category: category.to_string(),
            expected_keywords: vec![],
        }
    }

    pub fn with_keywords(mut self, keywords: Vec<&str>) -> Self {
        self.expected_keywords = keywords.into_iter().map(String::from).collect();
        self
    }
}

/// 30가지 테스트 시나리오 정의
pub fn get_simulation_scenarios() -> Vec<SimScenario> {
    vec![
        // === 카테고리 1: 기본 대화 (5개) ===
        SimScenario::new(
            "conv-01",
            "Hello, introduce yourself briefly",
            "Basic greeting and self-introduction",
            "conversation"
        ).with_keywords(vec!["hello", "assist", "help"]),

        SimScenario::new(
            "conv-02",
            "What can you help me with?",
            "Capability explanation",
            "conversation"
        ).with_keywords(vec!["code", "help", "assist"]),

        SimScenario::new(
            "conv-03",
            "Say hello in exactly one word",
            "Concise response test",
            "conversation"
        ).with_keywords(vec!["hello", "hi", "hey"]),

        SimScenario::new(
            "conv-04",
            "What programming languages do you know?",
            "Knowledge scope test",
            "conversation"
        ).with_keywords(vec!["python", "rust", "javascript"]),

        SimScenario::new(
            "conv-05",
            "Explain what a function is in one sentence",
            "Concise explanation test",
            "conversation"
        ).with_keywords(vec!["function", "code", "reusable"]),

        // === 카테고리 2: 코드 생성 (5개) ===
        SimScenario::new(
            "gen-01",
            "Write a Python function to check if a number is prime",
            "Prime number checker",
            "code_generation"
        ).with_keywords(vec!["def", "prime", "return"]),

        SimScenario::new(
            "gen-02",
            "Write a Rust function to reverse a string",
            "String reversal in Rust",
            "code_generation"
        ).with_keywords(vec!["fn", "String", "reverse"]),

        SimScenario::new(
            "gen-03",
            "Write a JavaScript function to filter even numbers from an array",
            "Array filtering",
            "code_generation"
        ).with_keywords(vec!["function", "filter", "return"]),

        SimScenario::new(
            "gen-04",
            "Write a simple Python class for a Stack data structure",
            "Stack implementation",
            "code_generation"
        ).with_keywords(vec!["class", "push", "pop"]),

        SimScenario::new(
            "gen-05",
            "Write a TypeScript interface for a User with id, name, and email",
            "TypeScript interface",
            "code_generation"
        ).with_keywords(vec!["interface", "User", "string"]),

        // === 카테고리 3: 코드 설명 (5개) ===
        SimScenario::new(
            "explain-01",
            "Explain what 'fn main() { println!(\"Hello\"); }' does in Rust",
            "Basic Rust explanation",
            "explanation"
        ).with_keywords(vec!["main", "print", "output"]),

        SimScenario::new(
            "explain-02",
            "What does 'git status' command do?",
            "Git command explanation",
            "explanation"
        ).with_keywords(vec!["git", "status", "changes"]),

        SimScenario::new(
            "explain-03",
            "Explain async/await in JavaScript briefly",
            "Async concept explanation",
            "explanation"
        ).with_keywords(vec!["async", "await", "promise"]),

        SimScenario::new(
            "explain-04",
            "What is the difference between let and const in JavaScript?",
            "Variable declaration explanation",
            "explanation"
        ).with_keywords(vec!["let", "const", "reassign"]),

        SimScenario::new(
            "explain-05",
            "What does the cargo build command do?",
            "Cargo explanation",
            "explanation"
        ).with_keywords(vec!["cargo", "build", "compile"]),

        // === 카테고리 4: 디버깅 조언 (5개) ===
        SimScenario::new(
            "debug-01",
            "How do I fix a 'null pointer exception' in Java?",
            "NPE debugging advice",
            "debugging"
        ).with_keywords(vec!["null", "check", "initialize"]),

        SimScenario::new(
            "debug-02",
            "Why might 'cargo build' fail with missing dependencies?",
            "Cargo dependency issue",
            "debugging"
        ).with_keywords(vec!["cargo", "dependency", "toml"]),

        SimScenario::new(
            "debug-03",
            "How to debug a Python import error?",
            "Python import debugging",
            "debugging"
        ).with_keywords(vec!["import", "path", "module"]),

        SimScenario::new(
            "debug-04",
            "What causes 'index out of bounds' errors?",
            "Array bounds debugging",
            "debugging"
        ).with_keywords(vec!["index", "array", "bounds"]),

        SimScenario::new(
            "debug-05",
            "How to fix 'permission denied' when running a script?",
            "Permission error debugging",
            "debugging"
        ).with_keywords(vec!["permission", "chmod", "execute"]),

        // === 카테고리 5: 아키텍처/설계 (5개) ===
        SimScenario::new(
            "arch-01",
            "What is the MVC pattern? Explain briefly",
            "MVC pattern explanation",
            "architecture"
        ).with_keywords(vec!["model", "view", "controller"]),

        SimScenario::new(
            "arch-02",
            "When should I use microservices vs monolith?",
            "Architecture decision",
            "architecture"
        ).with_keywords(vec!["microservice", "monolith", "scale"]),

        SimScenario::new(
            "arch-03",
            "What is dependency injection?",
            "DI explanation",
            "architecture"
        ).with_keywords(vec!["dependency", "inject", "decouple"]),

        SimScenario::new(
            "arch-04",
            "Explain the repository pattern in one paragraph",
            "Repository pattern",
            "architecture"
        ).with_keywords(vec!["repository", "data", "abstract"]),

        SimScenario::new(
            "arch-05",
            "What is a REST API?",
            "REST API explanation",
            "architecture"
        ).with_keywords(vec!["rest", "api", "http"]),

        // === 카테고리 6: 도구 사용 (5개) ===
        SimScenario::new(
            "tool-01",
            "List files in the current directory",
            "Directory listing",
            "tool_usage"
        ).with_keywords(vec!["file", "list", "directory"]),

        SimScenario::new(
            "tool-02",
            "What is the current working directory?",
            "PWD query",
            "tool_usage"
        ).with_keywords(vec!["directory", "path", "current"]),

        SimScenario::new(
            "tool-03",
            "Search for the word 'main' in Rust files",
            "Code search",
            "tool_usage"
        ).with_keywords(vec!["search", "main", "rust"]),

        SimScenario::new(
            "tool-04",
            "Show the first 5 lines of Cargo.toml",
            "File reading",
            "tool_usage"
        ).with_keywords(vec!["cargo", "toml", "package"]),

        SimScenario::new(
            "tool-05",
            "Check if git is installed",
            "Tool availability check",
            "tool_usage"
        ).with_keywords(vec!["git", "version", "installed"]),
    ]
}

/// 단일 시나리오 실행 (모의)
pub fn run_mock_scenario(scenario: &SimScenario) -> SimulationResult {
    let start = Instant::now();

    // 모의 응답 생성 (실제로는 Agent 호출)
    let mock_response = format!(
        "This is a mock response for scenario '{}': {}",
        scenario.id, scenario.description
    );

    // 키워드 검증
    let success = scenario.expected_keywords.is_empty() ||
        scenario.expected_keywords.iter().any(|kw|
            mock_response.to_lowercase().contains(&kw.to_lowercase())
        );

    SimulationResult {
        scenario_id: scenario.id.clone(),
        description: scenario.description.clone(),
        success,
        duration_ms: start.elapsed().as_millis() as u64,
        error: if success { None } else { Some("Keyword not found".to_string()) },
        response_preview: mock_response.chars().take(100).collect(),
    }
}

/// 시뮬레이션 요약
#[derive(Debug, Serialize)]
pub struct SimulationSummary {
    pub total_scenarios: usize,
    pub successful: usize,
    pub failed: usize,
    pub total_duration_ms: u64,
    pub by_category: HashMap<String, (usize, usize)>, // (pass, fail)
}

/// 모든 시나리오 실행
pub fn run_all_simulations() -> (Vec<SimulationResult>, SimulationSummary) {
    let scenarios = get_simulation_scenarios();
    let start = Instant::now();

    let results: Vec<SimulationResult> = scenarios.iter()
        .map(|s| run_mock_scenario(s))
        .collect();

    let successful = results.iter().filter(|r| r.success).count();
    let failed = results.len() - successful;

    let mut by_category: HashMap<String, (usize, usize)> = HashMap::new();
    for (scenario, result) in scenarios.iter().zip(results.iter()) {
        let entry = by_category.entry(scenario.category.clone()).or_insert((0, 0));
        if result.success {
            entry.0 += 1;
        } else {
            entry.1 += 1;
        }
    }

    let summary = SimulationSummary {
        total_scenarios: results.len(),
        successful,
        failed,
        total_duration_ms: start.elapsed().as_millis() as u64,
        by_category,
    };

    (results, summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scenario_count() {
        let scenarios = get_simulation_scenarios();
        assert_eq!(scenarios.len(), 30, "Should have exactly 30 scenarios");
    }

    #[test]
    fn test_categories() {
        let scenarios = get_simulation_scenarios();
        let categories: std::collections::HashSet<_> = scenarios.iter()
            .map(|s| s.category.as_str())
            .collect();

        assert!(categories.contains("conversation"));
        assert!(categories.contains("code_generation"));
        assert!(categories.contains("explanation"));
        assert!(categories.contains("debugging"));
        assert!(categories.contains("architecture"));
        assert!(categories.contains("tool_usage"));
    }

    #[test]
    fn test_run_mock_simulations() {
        let (results, summary) = run_all_simulations();

        println!("\n=== Simulation Results ===");
        println!("Total: {} scenarios", summary.total_scenarios);
        println!("Success: {}, Failed: {}", summary.successful, summary.failed);
        println!("Duration: {}ms", summary.total_duration_ms);
        println!("\nBy Category:");
        for (cat, (pass, fail)) in &summary.by_category {
            println!("  {}: {} pass, {} fail", cat, pass, fail);
        }

        assert_eq!(results.len(), 30);
    }
}
