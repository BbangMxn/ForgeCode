//! ReAct Pattern Implementation
//!
//! 연구 기반: "ReAct: Synergizing Reasoning and Acting in Language Models" (Yao et al.)
//! AI Agentic Programming Survey (2025) - Prompt Engineering & Reasoning Strategies
//!
//! ## ReAct 패턴
//! ```text
//! Question → Thought → Action → Observation → Thought → ... → Answer
//! ```
//!
//! ## 핵심 원리
//! - **Thought**: 현재 상황 분석 및 다음 행동 계획
//! - **Action**: 도구 실행
//! - **Observation**: 결과 관찰 및 해석
//! - 이 과정을 목표 달성까지 반복

use std::time::Instant;

/// ReAct 단계 유형
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReactStep {
    /// 사고 단계 - 상황 분석 및 계획
    Thought,
    /// 행동 단계 - 도구 실행
    Action,
    /// 관찰 단계 - 결과 해석
    Observation,
    /// 최종 답변
    Answer,
}

/// ReAct 트레이스 항목
#[derive(Debug, Clone)]
pub struct ReactTrace {
    pub step: ReactStep,
    pub content: String,
    pub timestamp: Instant,
}

impl ReactTrace {
    pub fn thought(content: impl Into<String>) -> Self {
        Self {
            step: ReactStep::Thought,
            content: content.into(),
            timestamp: Instant::now(),
        }
    }

    pub fn action(content: impl Into<String>) -> Self {
        Self {
            step: ReactStep::Action,
            content: content.into(),
            timestamp: Instant::now(),
        }
    }

    pub fn observation(content: impl Into<String>) -> Self {
        Self {
            step: ReactStep::Observation,
            content: content.into(),
            timestamp: Instant::now(),
        }
    }

    pub fn answer(content: impl Into<String>) -> Self {
        Self {
            step: ReactStep::Answer,
            content: content.into(),
            timestamp: Instant::now(),
        }
    }
}

/// ReAct 프롬프트 생성기
#[derive(Debug, Clone)]
pub struct ReactPromptBuilder {
    /// 초기 지시문
    instruction: String,
    /// 예시 (few-shot)
    examples: Vec<ReactExample>,
    /// 도구 설명
    tool_descriptions: Vec<String>,
}

/// ReAct 예시 (few-shot learning용)
#[derive(Debug, Clone)]
pub struct ReactExample {
    pub question: String,
    pub traces: Vec<ReactTrace>,
}

impl Default for ReactPromptBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ReactPromptBuilder {
    pub fn new() -> Self {
        Self {
            instruction: Self::default_instruction(),
            examples: Vec::new(),
            tool_descriptions: Vec::new(),
        }
    }

    /// 기본 ReAct 지시문
    fn default_instruction() -> String {
        r#"You are an expert coding assistant that solves problems step by step.

For each step, use the following format:
- Thought: Analyze the current situation and plan your next action
- Action: Execute a tool to gather information or make changes
- Observation: Interpret the results of the action

Continue this process until you have enough information to provide the final answer.

Guidelines:
1. Think before you act - analyze what information you need
2. Use tools efficiently - avoid redundant calls
3. Learn from observations - adjust your approach based on results
4. Be concise in thoughts but thorough in actions"#
            .to_string()
    }

    /// 커스텀 지시문 설정
    pub fn with_instruction(mut self, instruction: impl Into<String>) -> Self {
        self.instruction = instruction.into();
        self
    }

    /// 예시 추가
    pub fn with_example(mut self, example: ReactExample) -> Self {
        self.examples.push(example);
        self
    }

    /// 도구 설명 추가
    pub fn with_tool(mut self, tool_desc: impl Into<String>) -> Self {
        self.tool_descriptions.push(tool_desc.into());
        self
    }

    /// 전체 프롬프트 생성
    pub fn build(&self) -> String {
        let mut prompt = self.instruction.clone();

        // 도구 설명 추가
        if !self.tool_descriptions.is_empty() {
            prompt.push_str("\n\nAvailable Tools:\n");
            for tool in &self.tool_descriptions {
                prompt.push_str(&format!("- {}\n", tool));
            }
        }

        // 예시 추가
        if !self.examples.is_empty() {
            prompt.push_str("\n\nExamples:\n");
            for (i, example) in self.examples.iter().enumerate() {
                prompt.push_str(&format!("\n--- Example {} ---\n", i + 1));
                prompt.push_str(&format!("Question: {}\n\n", example.question));
                for trace in &example.traces {
                    let label = match trace.step {
                        ReactStep::Thought => "Thought",
                        ReactStep::Action => "Action",
                        ReactStep::Observation => "Observation",
                        ReactStep::Answer => "Answer",
                    };
                    prompt.push_str(&format!("{}: {}\n", label, trace.content));
                }
            }
        }

        prompt
    }
}

/// ReAct 실행 추적기
#[derive(Debug)]
pub struct ReactTracer {
    traces: Vec<ReactTrace>,
    current_step: ReactStep,
    max_steps: usize,
}

impl Default for ReactTracer {
    fn default() -> Self {
        Self::new()
    }
}

impl ReactTracer {
    pub fn new() -> Self {
        Self {
            traces: Vec::with_capacity(50),
            current_step: ReactStep::Thought,
            max_steps: 20,
        }
    }

    pub fn with_max_steps(mut self, max: usize) -> Self {
        self.max_steps = max;
        self
    }

    /// 새 트레이스 추가
    pub fn add(&mut self, trace: ReactTrace) {
        self.current_step = trace.step.clone();
        self.traces.push(trace);
    }

    /// Thought 추가
    pub fn thought(&mut self, content: impl Into<String>) {
        self.add(ReactTrace::thought(content));
    }

    /// Action 추가
    pub fn action(&mut self, tool_name: &str, input: &str) {
        self.add(ReactTrace::action(format!("{}({})", tool_name, input)));
    }

    /// Observation 추가
    pub fn observation(&mut self, content: impl Into<String>) {
        self.add(ReactTrace::observation(content));
    }

    /// Answer 추가
    pub fn answer(&mut self, content: impl Into<String>) {
        self.add(ReactTrace::answer(content));
    }

    /// 현재 단계
    pub fn current_step(&self) -> &ReactStep {
        &self.current_step
    }

    /// 총 단계 수
    pub fn step_count(&self) -> usize {
        self.traces.len()
    }

    /// 최대 단계 도달 여부
    pub fn is_max_reached(&self) -> bool {
        self.traces.len() >= self.max_steps
    }

    /// 완료 여부 (Answer 도달)
    pub fn is_complete(&self) -> bool {
        self.current_step == ReactStep::Answer
    }

    /// 트레이스 포맷 (디버깅/로깅용)
    pub fn format_trace(&self) -> String {
        let mut output = String::with_capacity(self.traces.len() * 100);
        for (i, trace) in self.traces.iter().enumerate() {
            let label = match trace.step {
                ReactStep::Thought => "💭 Thought",
                ReactStep::Action => "⚡ Action",
                ReactStep::Observation => "👁️ Observation",
                ReactStep::Answer => "✅ Answer",
            };
            output.push_str(&format!("[{}] {}: {}\n", i + 1, label, trace.content));
        }
        output
    }

    /// 통계 요약
    pub fn summary(&self) -> ReactSummary {
        let thoughts = self.traces.iter().filter(|t| t.step == ReactStep::Thought).count();
        let actions = self.traces.iter().filter(|t| t.step == ReactStep::Action).count();
        let observations = self.traces.iter().filter(|t| t.step == ReactStep::Observation).count();
        let has_answer = self.traces.iter().any(|t| t.step == ReactStep::Answer);

        ReactSummary {
            total_steps: self.traces.len(),
            thoughts,
            actions,
            observations,
            completed: has_answer,
        }
    }

    /// 트레이스 초기화
    pub fn reset(&mut self) {
        self.traces.clear();
        self.current_step = ReactStep::Thought;
    }
}

/// ReAct 실행 요약
#[derive(Debug, Clone)]
pub struct ReactSummary {
    pub total_steps: usize,
    pub thoughts: usize,
    pub actions: usize,
    pub observations: usize,
    pub completed: bool,
}

/// ReAct 기반 추론 강화 프롬프트
pub fn enhance_with_react(base_prompt: &str) -> String {
    format!(
        r#"{}

When solving problems, follow the ReAct pattern:
1. **Thought**: Before taking any action, explicitly state what you're thinking:
   - What is the current state?
   - What information do I need?
   - What's my plan?

2. **Action**: Execute the appropriate tool

3. **Observation**: After each action, reflect on the results:
   - Did it succeed?
   - What did I learn?
   - Do I need to adjust my approach?

This structured reasoning helps ensure thorough and accurate problem-solving."#,
        base_prompt
    )
}

/// 코딩 에이전트용 ReAct 예시 생성
pub fn coding_react_example() -> ReactExample {
    ReactExample {
        question: "Fix the compilation error in src/main.rs".to_string(),
        traces: vec![
            ReactTrace::thought(
                "I need to first understand the compilation error. Let me read the error message and the relevant code."
            ),
            ReactTrace::action("Read(src/main.rs, lines 1-50)"),
            ReactTrace::observation(
                "The file contains a function `process_data` that calls `unwrap()` on line 25. The error mentions 'cannot find value `config`'."
            ),
            ReactTrace::thought(
                "The error indicates `config` is not defined. I need to check if there's a missing import or if the variable needs to be defined."
            ),
            ReactTrace::action("Grep('config', src/)"),
            ReactTrace::observation(
                "Found config definition in src/config.rs. It needs to be imported."
            ),
            ReactTrace::thought(
                "I should add the import statement at the top of main.rs"
            ),
            ReactTrace::action("Edit(src/main.rs, add 'use crate::config::Config;' after line 1)"),
            ReactTrace::observation("Edit successful."),
            ReactTrace::action("Bash(cargo check)"),
            ReactTrace::observation("Build succeeded with no errors."),
            ReactTrace::answer(
                "Fixed the compilation error by adding the missing import `use crate::config::Config;` to src/main.rs."
            ),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_react_tracer() {
        let mut tracer = ReactTracer::new();
        
        tracer.thought("I need to check the file");
        tracer.action("Read", "src/main.rs");
        tracer.observation("File contains 100 lines");
        tracer.answer("The file is valid");
        
        let summary = tracer.summary();
        assert_eq!(summary.total_steps, 4);
        assert_eq!(summary.thoughts, 1);
        assert_eq!(summary.actions, 1);
        assert_eq!(summary.observations, 1);
        assert!(summary.completed);
    }

    #[test]
    fn test_react_prompt_builder() {
        let prompt = ReactPromptBuilder::new()
            .with_tool("Read: Read file contents")
            .with_tool("Edit: Edit file contents")
            .with_example(coding_react_example())
            .build();
        
        assert!(prompt.contains("Thought"));
        assert!(prompt.contains("Action"));
        assert!(prompt.contains("Observation"));
        assert!(prompt.contains("Read:"));
    }
}
