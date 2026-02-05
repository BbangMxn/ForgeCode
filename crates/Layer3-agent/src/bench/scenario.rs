//! Benchmark Scenarios
//!
//! 테스트 시나리오 정의 및 관리

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 난이도 레벨
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DifficultyLevel {
    Easy,
    Medium,
    Hard,
    Expert,
}

impl std::fmt::Display for DifficultyLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DifficultyLevel::Easy => write!(f, "Easy"),
            DifficultyLevel::Medium => write!(f, "Medium"),
            DifficultyLevel::Hard => write!(f, "Hard"),
            DifficultyLevel::Expert => write!(f, "Expert"),
        }
    }
}

/// 예상 결과
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExpectedOutcome {
    /// 특정 문자열 포함
    Contains(String),
    /// 정규식 매치
    Matches(String),
    /// 파일 생성됨
    FileCreated(String),
    /// 파일 수정됨
    FileModified(String),
    /// 명령 실행됨
    CommandExecuted(String),
    /// 커스텀 검증 함수
    Custom(String), // 함수 이름
}

/// 테스트 케이스
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCase {
    /// 케이스 ID
    pub id: String,

    /// 설명
    pub description: String,

    /// 입력 프롬프트
    pub prompt: String,

    /// 초기 컨텍스트 (파일 내용 등)
    pub context: HashMap<String, String>,

    /// 예상 결과들
    pub expected: Vec<ExpectedOutcome>,

    /// 최대 허용 턴 수
    pub max_turns: Option<u32>,

    /// 최대 허용 시간 (초)
    pub max_duration_secs: Option<u64>,

    /// 난이도
    pub difficulty: DifficultyLevel,

    /// 태그
    pub tags: Vec<String>,
}

impl TestCase {
    /// 새 테스트 케이스 생성
    pub fn new(id: impl Into<String>, prompt: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            description: String::new(),
            prompt: prompt.into(),
            context: HashMap::new(),
            expected: Vec::new(),
            max_turns: None,
            max_duration_secs: None,
            difficulty: DifficultyLevel::Medium,
            tags: Vec::new(),
        }
    }

    /// 설명 추가
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// 컨텍스트 추가
    pub fn with_context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context.insert(key.into(), value.into());
        self
    }

    /// 예상 결과 추가
    pub fn expect(mut self, outcome: ExpectedOutcome) -> Self {
        self.expected.push(outcome);
        self
    }

    /// 난이도 설정
    pub fn with_difficulty(mut self, level: DifficultyLevel) -> Self {
        self.difficulty = level;
        self
    }

    /// 태그 추가
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// 최대 턴 수 설정
    pub fn with_max_turns(mut self, turns: u32) -> Self {
        self.max_turns = Some(turns);
        self
    }

    /// 최대 시간 설정
    pub fn with_max_duration(mut self, secs: u64) -> Self {
        self.max_duration_secs = Some(secs);
        self
    }
}

/// 테스트 시나리오
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scenario {
    /// 시나리오 ID
    pub id: String,

    /// 이름
    pub name: String,

    /// 설명
    pub description: String,

    /// 카테고리
    pub category: String,

    /// 테스트 케이스들
    pub test_cases: Vec<TestCase>,

    /// 시나리오 전체 설정
    pub setup: Option<String>,

    /// 정리 명령
    pub teardown: Option<String>,

    /// 메타데이터
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Scenario {
    /// 새 시나리오 생성
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: String::new(),
            category: "general".to_string(),
            test_cases: Vec::new(),
            setup: None,
            teardown: None,
            metadata: HashMap::new(),
        }
    }

    /// 테스트 케이스 추가
    pub fn with_test_case(mut self, case: TestCase) -> Self {
        self.test_cases.push(case);
        self
    }

    /// 카테고리 설정
    pub fn with_category(mut self, category: impl Into<String>) -> Self {
        self.category = category.into();
        self
    }

    /// 설명 추가
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }
}

/// 시나리오 빌더
pub struct ScenarioBuilder {
    scenario: Scenario,
}

impl ScenarioBuilder {
    /// 새 빌더 생성
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            scenario: Scenario::new(id, name),
        }
    }

    /// 설명 추가
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.scenario.description = desc.into();
        self
    }

    /// 카테고리 설정
    pub fn category(mut self, category: impl Into<String>) -> Self {
        self.scenario.category = category.into();
        self
    }

    /// 테스트 케이스 추가
    pub fn test_case(mut self, case: TestCase) -> Self {
        self.scenario.test_cases.push(case);
        self
    }

    /// 빌드
    pub fn build(self) -> Scenario {
        self.scenario
    }
}

/// 시나리오 실행 결과
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioResult {
    /// 시나리오 ID
    pub scenario_id: String,

    /// 총 테스트 수
    pub total_tests: usize,

    /// 통과한 테스트 수
    pub passed: usize,

    /// 실패한 테스트 수
    pub failed: usize,

    /// 각 테스트 케이스 결과
    pub test_results: Vec<TestCaseResult>,

    /// 총 소요 시간 (ms)
    pub total_duration_ms: u64,
}

/// 개별 테스트 케이스 결과
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCaseResult {
    /// 케이스 ID
    pub case_id: String,

    /// 성공 여부
    pub passed: bool,

    /// 소요 시간 (ms)
    pub duration_ms: u64,

    /// 사용한 턴 수
    pub turns_used: u32,

    /// 실패 이유 (실패 시)
    pub failure_reason: Option<String>,

    /// 실제 출력
    pub actual_output: String,
}

// ============================================================================
// 내장 시나리오
// ============================================================================

/// 코딩 시나리오 모음
pub fn coding_scenarios() -> Vec<Scenario> {
    vec![
        ScenarioBuilder::new("coding-basic", "Basic Coding Tasks")
            .description("Simple coding tasks like fixing bugs and adding features")
            .category("coding")
            .test_case(
                TestCase::new("fix-typo", "Fix the typo in the function name 'calcualte'")
                    .with_description("Simple typo fix")
                    .with_difficulty(DifficultyLevel::Easy)
                    .expect(ExpectedOutcome::Contains("calculate".to_string()))
                    .with_tag("refactor"),
            )
            .test_case(
                TestCase::new(
                    "add-function",
                    "Add a function that calculates the sum of an array",
                )
                .with_description("Simple function addition")
                .with_difficulty(DifficultyLevel::Easy)
                .expect(ExpectedOutcome::Contains("sum".to_string()))
                .with_tag("feature"),
            )
            .build(),
        ScenarioBuilder::new("coding-refactor", "Code Refactoring")
            .description("Code refactoring and improvement tasks")
            .category("coding")
            .test_case(
                TestCase::new(
                    "extract-method",
                    "Extract the repeated code into a separate method",
                )
                .with_difficulty(DifficultyLevel::Medium)
                .with_tag("refactor"),
            )
            .build(),
    ]
}

/// 분석 시나리오 모음
pub fn analysis_scenarios() -> Vec<Scenario> {
    vec![
        ScenarioBuilder::new("analysis-codebase", "Codebase Analysis")
            .description("Tasks involving understanding and analyzing code")
            .category("analysis")
            .test_case(
                TestCase::new("find-bugs", "Find potential bugs in the given code")
                    .with_difficulty(DifficultyLevel::Medium)
                    .with_tag("analysis"),
            )
            .test_case(
                TestCase::new("explain-code", "Explain what this function does")
                    .with_difficulty(DifficultyLevel::Easy)
                    .with_tag("explanation"),
            )
            .build(),
    ]
}
