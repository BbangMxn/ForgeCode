//! Planning Mode - 계획 먼저, 실행 나중
//!
//! Claude Code Superpowers 스타일의 지능적인 계획 수립:
//! 1. 요청 분석
//! 2. 계획 수립 (단계별)
//! 3. 사용자 확인
//! 4. 실행
//!
//! ## 사용 예시
//!
//! ```text
//! User: "Add authentication to this API"
//!
//! Agent Planning:
//! ┌─────────────────────────────────────────────────┐
//! │ 📋 PLAN: Add Authentication                     │
//! ├─────────────────────────────────────────────────┤
//! │ 1. [READ] Analyze current API structure         │
//! │ 2. [READ] Check existing auth patterns          │
//! │ 3. [WRITE] Create auth middleware               │
//! │ 4. [EDIT] Update routes to use middleware       │
//! │ 5. [WRITE] Add auth tests                       │
//! │ 6. [BASH] Run tests to verify                   │
//! └─────────────────────────────────────────────────┘
//! Proceed? [Y/n]
//! ```

use serde::{Deserialize, Serialize};
use std::fmt;

/// 계획 단계
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    /// 단계 번호
    pub step: usize,
    /// 사용할 도구
    pub tool: String,
    /// 설명
    pub description: String,
    /// 예상 파일들
    pub files: Vec<String>,
    /// 선행 조건 (다른 단계 번호)
    pub depends_on: Vec<usize>,
    /// 예상 소요 시간 (초)
    pub estimated_seconds: Option<u32>,
    /// 위험도 (0-10)
    pub risk_level: u8,
    /// 완료 여부
    pub completed: bool,
    /// 결과 (완료 후)
    pub result: Option<StepResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    pub success: bool,
    pub output: String,
    pub duration_ms: u64,
}

impl PlanStep {
    pub fn new(step: usize, tool: &str, description: &str) -> Self {
        Self {
            step,
            tool: tool.to_string(),
            description: description.to_string(),
            files: Vec::new(),
            depends_on: Vec::new(),
            estimated_seconds: None,
            risk_level: 0,
            completed: false,
            result: None,
        }
    }

    pub fn with_files(mut self, files: Vec<String>) -> Self {
        self.files = files;
        self
    }

    pub fn with_depends_on(mut self, deps: Vec<usize>) -> Self {
        self.depends_on = deps;
        self
    }

    pub fn with_risk(mut self, risk: u8) -> Self {
        self.risk_level = risk.min(10);
        self
    }

    pub fn with_estimated_time(mut self, seconds: u32) -> Self {
        self.estimated_seconds = Some(seconds);
        self
    }

    pub fn mark_completed(&mut self, success: bool, output: String, duration_ms: u64) {
        self.completed = true;
        self.result = Some(StepResult {
            success,
            output,
            duration_ms,
        });
    }

    /// 도구 아이콘
    fn tool_icon(&self) -> &str {
        match self.tool.as_str() {
            "read" => "📖",
            "write" => "✏️",
            "edit" => "🔧",
            "bash" => "⚡",
            "glob" => "🔍",
            "grep" => "🔎",
            "task_spawn" => "🚀",
            "task_wait" => "⏳",
            "task_logs" => "📋",
            "task_stop" => "🛑",
            _ => "🔹",
        }
    }

    /// 상태 아이콘
    fn status_icon(&self) -> &str {
        if self.completed {
            if let Some(ref result) = self.result {
                if result.success { "✅" } else { "❌" }
            } else {
                "✅"
            }
        } else {
            "⬜"
        }
    }
}

impl fmt::Display for PlanStep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {}. [{}] {}",
            self.status_icon(),
            self.step,
            self.tool.to_uppercase(),
            self.description
        )?;
        
        if !self.files.is_empty() {
            write!(f, " ({})", self.files.join(", "))?;
        }
        
        if self.risk_level > 5 {
            write!(f, " ⚠️")?;
        }
        
        Ok(())
    }
}

/// 실행 계획
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPlan {
    /// 계획 제목
    pub title: String,
    /// 계획 설명
    pub description: String,
    /// 단계들
    pub steps: Vec<PlanStep>,
    /// 계획 모드
    pub mode: PlanMode,
    /// 총 예상 시간 (초)
    pub estimated_total_seconds: Option<u32>,
    /// 전체 위험도
    pub overall_risk: RiskLevel,
    /// 생성 시간
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PlanMode {
    /// 빠른 실행 (검증 최소화)
    Fast,
    /// 표준 실행
    Standard,
    /// 철저한 실행 (전체 테스트)
    Thorough,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

impl ExecutionPlan {
    pub fn new(title: &str) -> Self {
        Self {
            title: title.to_string(),
            description: String::new(),
            steps: Vec::new(),
            mode: PlanMode::Standard,
            estimated_total_seconds: None,
            overall_risk: RiskLevel::Low,
            created_at: chrono::Utc::now(),
        }
    }

    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = desc.to_string();
        self
    }

    pub fn with_mode(mut self, mode: PlanMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn add_step(&mut self, step: PlanStep) {
        self.steps.push(step);
        self.recalculate();
    }

    fn recalculate(&mut self) {
        // 총 시간 계산
        self.estimated_total_seconds = Some(
            self.steps
                .iter()
                .filter_map(|s| s.estimated_seconds)
                .sum()
        );

        // 위험도 계산
        let max_risk = self.steps.iter().map(|s| s.risk_level).max().unwrap_or(0);
        self.overall_risk = match max_risk {
            0..=2 => RiskLevel::Low,
            3..=5 => RiskLevel::Medium,
            6..=8 => RiskLevel::High,
            _ => RiskLevel::Critical,
        };
    }

    /// 다음 실행 가능한 단계
    pub fn next_executable(&self) -> Option<&PlanStep> {
        self.steps.iter().find(|s| {
            !s.completed && s.depends_on.iter().all(|dep| {
                self.steps.get(*dep - 1).map(|d| d.completed).unwrap_or(false)
            })
        })
    }

    /// 진행률 (0.0 - 1.0)
    pub fn progress(&self) -> f32 {
        if self.steps.is_empty() {
            return 0.0;
        }
        let completed = self.steps.iter().filter(|s| s.completed).count();
        completed as f32 / self.steps.len() as f32
    }

    /// 모든 단계 완료 여부
    pub fn is_complete(&self) -> bool {
        self.steps.iter().all(|s| s.completed)
    }

    /// 실패한 단계 여부
    pub fn has_failures(&self) -> bool {
        self.steps.iter().any(|s| {
            s.result.as_ref().map(|r| !r.success).unwrap_or(false)
        })
    }

    /// 계획을 문자열로 포맷
    pub fn format_plan(&self) -> String {
        let mut output = String::new();
        
        // 헤더
        let width = 55;
        output.push_str(&format!("┌{}┐\n", "─".repeat(width)));
        output.push_str(&format!("│ 📋 PLAN: {:<width$}│\n", 
            truncate(&self.title, width - 11), width = width - 11));
        
        if !self.description.is_empty() {
            output.push_str(&format!("│ {:<width$}│\n", 
                truncate(&self.description, width - 2), width = width - 2));
        }
        
        output.push_str(&format!("├{}┤\n", "─".repeat(width)));
        
        // 단계들
        for step in &self.steps {
            let line = format!("{} {}. [{}] {}", 
                step.status_icon(),
                step.step,
                step.tool.to_uppercase(),
                truncate(&step.description, 35)
            );
            output.push_str(&format!("│ {:<width$}│\n", line, width = width - 2));
        }
        
        // 푸터
        output.push_str(&format!("├{}┤\n", "─".repeat(width)));
        
        let mode_str = match self.mode {
            PlanMode::Fast => "⚡ Fast",
            PlanMode::Standard => "📊 Standard",
            PlanMode::Thorough => "🔬 Thorough",
        };
        
        let risk_str = match self.overall_risk {
            RiskLevel::Low => "🟢 Low",
            RiskLevel::Medium => "🟡 Medium",
            RiskLevel::High => "🟠 High",
            RiskLevel::Critical => "🔴 Critical",
        };
        
        let progress = (self.progress() * 100.0) as u32;
        let stats = format!("Mode: {} | Risk: {} | Progress: {}%", mode_str, risk_str, progress);
        output.push_str(&format!("│ {:<width$}│\n", stats, width = width - 2));
        output.push_str(&format!("└{}┘\n", "─".repeat(width)));
        
        output
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

/// 계획 생성 도우미
pub struct PlanBuilder {
    plan: ExecutionPlan,
    step_counter: usize,
}

impl PlanBuilder {
    pub fn new(title: &str) -> Self {
        Self {
            plan: ExecutionPlan::new(title),
            step_counter: 0,
        }
    }

    pub fn description(mut self, desc: &str) -> Self {
        self.plan.description = desc.to_string();
        self
    }

    pub fn mode(mut self, mode: PlanMode) -> Self {
        self.plan.mode = mode;
        self
    }

    pub fn read(mut self, desc: &str) -> Self {
        self.step_counter += 1;
        self.plan.add_step(PlanStep::new(self.step_counter, "read", desc));
        self
    }

    pub fn write(mut self, desc: &str) -> Self {
        self.step_counter += 1;
        self.plan.add_step(PlanStep::new(self.step_counter, "write", desc).with_risk(3));
        self
    }

    pub fn edit(mut self, desc: &str) -> Self {
        self.step_counter += 1;
        self.plan.add_step(PlanStep::new(self.step_counter, "edit", desc).with_risk(4));
        self
    }

    pub fn bash(mut self, desc: &str) -> Self {
        self.step_counter += 1;
        self.plan.add_step(PlanStep::new(self.step_counter, "bash", desc).with_risk(5));
        self
    }

    pub fn task_spawn(mut self, desc: &str) -> Self {
        self.step_counter += 1;
        self.plan.add_step(PlanStep::new(self.step_counter, "task_spawn", desc).with_risk(4));
        self
    }

    pub fn task_wait(mut self, desc: &str) -> Self {
        self.step_counter += 1;
        self.plan.add_step(PlanStep::new(self.step_counter, "task_wait", desc));
        self
    }

    pub fn custom(mut self, tool: &str, desc: &str, risk: u8) -> Self {
        self.step_counter += 1;
        self.plan.add_step(PlanStep::new(self.step_counter, tool, desc).with_risk(risk));
        self
    }

    pub fn build(self) -> ExecutionPlan {
        self.plan
    }
}

/// 계획 프롬프트 생성
pub fn planning_prompt() -> &'static str {
    r#"## Planning Mode

When working on complex tasks, first create a plan before executing:

### Plan Format:
```
PLAN: [Title]
Description: [Brief description]

Steps:
1. [TOOL] Description
2. [TOOL] Description (depends on: 1)
3. [TOOL] Description
...

Mode: [Fast/Standard/Thorough]
Risk: [Low/Medium/High]
```

### When to Plan:
- Multi-file changes
- Feature implementations
- Refactoring tasks
- Bug fixes requiring investigation

### Plan Rules:
1. Read/analyze BEFORE writing/editing
2. Identify dependencies between steps
3. Estimate risk for each step
4. Choose appropriate mode:
   - Fast: Skip verification for simple changes
   - Standard: Basic verification after changes
   - Thorough: Full test suite after changes
"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_builder() {
        let plan = PlanBuilder::new("Add Authentication")
            .description("Implement JWT authentication")
            .mode(PlanMode::Thorough)
            .read("Analyze current API structure")
            .read("Check existing auth patterns")
            .write("Create auth middleware")
            .edit("Update routes to use middleware")
            .write("Add auth tests")
            .bash("Run tests to verify")
            .build();

        assert_eq!(plan.steps.len(), 6);
        assert_eq!(plan.mode, PlanMode::Thorough);
        assert!(!plan.is_complete());
    }

    #[test]
    fn test_plan_progress() {
        let mut plan = PlanBuilder::new("Test")
            .read("Step 1")
            .write("Step 2")
            .build();

        assert_eq!(plan.progress(), 0.0);

        plan.steps[0].mark_completed(true, "done".to_string(), 100);
        assert_eq!(plan.progress(), 0.5);

        plan.steps[1].mark_completed(true, "done".to_string(), 200);
        assert_eq!(plan.progress(), 1.0);
        assert!(plan.is_complete());
    }

    #[test]
    fn test_plan_format() {
        let plan = PlanBuilder::new("Quick Test")
            .read("Read file")
            .edit("Make changes")
            .build();

        let formatted = plan.format_plan();
        assert!(formatted.contains("PLAN: Quick Test"));
        assert!(formatted.contains("READ"));
        assert!(formatted.contains("EDIT"));
    }
}
