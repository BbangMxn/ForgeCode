//! Parallel Execution Support for Agent
//!
//! 도구 실행을 병렬화하여 Agent 성능을 향상시킵니다.
//! **안전성 시스템(Layer1 Permission)과 통합**되어 위험한 명령은 자동 차단됩니다.
//!
//! ## 전략
//!
//! 1. **독립 도구 병렬화**: 서로 의존성이 없는 도구들을 동시 실행
//! 2. **의존성 그래프**: 도구 간 의존성을 분석하여 최적 실행 순서 결정
//! 3. **Task 시스템 통합**: 장시간 실행 도구는 Task 시스템으로 위임
//! 4. **안전성 검사**: Layer1 security 모듈과 연동하여 위험 명령 차단
//!
//! ## 실행 모드
//!
//! ```text
//! ┌─────────────┬────────────────────┬─────────────────────┐
//! │   도구 유형  │     실행 방식       │       특징          │
//! ├─────────────┼────────────────────┼─────────────────────┤
//! │ read, glob  │ 직접 실행 (병렬)    │ 빠름, IO bound      │
//! │ grep        │ 직접 실행 (병렬)    │ 빠름, CPU bound     │
//! │ write, edit │ 순차 실행          │ 파일 충돌 방지       │
//! │ bash(safe)  │ 직접 실행          │ 자동 승인 (ls 등)   │
//! │ bash(long)  │ Task 시스템        │ 로그 추적, 타임아웃  │
//! │ bash(pty)   │ Task+PTY           │ 대화형 (vim 등)     │
//! │ bash(danger)│ 확인 필요          │ rm, mv 등           │
//! │ bash(forbid)│ 차단               │ rm -rf / 등         │
//! └─────────────┴────────────────────┴─────────────────────┘
//! ```
//!
//! ## 예시
//!
//! ```text
//! LLM Response: [read(a.rs), read(b.rs), grep("TODO"), write(c.rs)]
//!
//! 의존성 분석:
//! - read(a.rs), read(b.rs), grep("TODO") → 독립적 (병렬 가능)
//! - write(c.rs) → read 결과에 의존할 수 있음 (순차 실행)
//!
//! 실행 계획:
//! Phase 1 (병렬): [read(a.rs), read(b.rs), grep("TODO")]
//! Phase 2 (순차): [write(c.rs)]
//! ```

use forge_foundation::permission::security::{
    analyzer as command_analyzer, path_analyzer, CommandRisk,
};
use forge_provider::ToolCall;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// 도구 의존성 유형
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyType {
    /// 완전 독립 (병렬 실행 가능)
    Independent,
    /// 읽기 전용 (다른 읽기와 병렬 가능)
    ReadOnly,
    /// 쓰기 (순차 실행 필요)
    Write,
    /// 상태 변경 (이전 작업 완료 후 실행)
    StateMutating,
}

/// 실행 방식
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionStrategy {
    /// Agent에서 직접 실행 (빠른 도구)
    Direct,
    /// Task 시스템으로 위임 (장시간 실행, 로그 추적 필요)
    Task,
    /// Task + PTY (대화형 명령)
    TaskPty,
    /// 사용자 확인 필요 (위험한 명령)
    RequiresConfirmation,
    /// 차단 (금지된 명령)
    Blocked,
}

/// 도구 분류기
pub struct ToolClassifier {
    /// 읽기 전용 도구
    read_only_tools: HashSet<&'static str>,
    /// 쓰기 도구
    write_tools: HashSet<&'static str>,
    /// 상태 변경 도구
    state_mutating_tools: HashSet<&'static str>,
    /// PTY가 필요한 명령어 패턴
    pty_commands: Vec<&'static str>,
    /// 장시간 실행 명령어 패턴
    long_running_commands: Vec<&'static str>,
}

impl Default for ToolClassifier {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolClassifier {
    pub fn new() -> Self {
        let mut read_only = HashSet::new();
        read_only.insert("read");
        read_only.insert("glob");
        read_only.insert("grep");
        read_only.insert("task_status");
        read_only.insert("task_list");
        read_only.insert("task_logs");

        let mut write = HashSet::new();
        write.insert("write");
        write.insert("edit");

        let mut state_mutating = HashSet::new();
        state_mutating.insert("bash");
        state_mutating.insert("task_spawn");
        state_mutating.insert("task_stop");
        state_mutating.insert("task_send");

        // PTY가 필요한 대화형 명령어 - Layer1 security의 interactive_commands() 참조
        let pty_commands = vec![
            "vim", "nvim", "vi", "nano", "emacs",
            "htop", "top", "less", "more",
            "ssh", "telnet",
            "python", "python3", "node", "irb", // REPL
            "psql", "mysql", "sqlite3", "mongosh", "redis-cli", // DB clients
        ];

        // 장시간 실행될 수 있는 명령어
        let long_running_commands = vec![
            "npm install", "npm run", "yarn",
            "cargo build", "cargo test", "cargo run",
            "make", "cmake",
            "docker build", "docker run",
            "git clone", "git fetch", "git pull",
            "pip install", "pip3 install",
            "go build", "go test",
            "mvn", "gradle",
            "pytest", "jest", "mocha",
        ];

        Self {
            read_only_tools: read_only,
            write_tools: write,
            state_mutating_tools: state_mutating,
            pty_commands,
            long_running_commands,
        }
    }

    /// 도구 의존성 유형 분류
    pub fn classify(&self, tool_name: &str) -> DependencyType {
        if self.read_only_tools.contains(tool_name) {
            DependencyType::ReadOnly
        } else if self.write_tools.contains(tool_name) {
            DependencyType::Write
        } else if self.state_mutating_tools.contains(tool_name) {
            DependencyType::StateMutating
        } else {
            // 알 수 없는 도구는 안전하게 상태 변경으로 취급
            DependencyType::StateMutating
        }
    }

    /// bash 명령어의 실행 전략 결정
    ///
    /// Layer1 security 모듈의 CommandAnalyzer를 사용하여 안전성을 먼저 검사합니다.
    pub fn classify_bash_command(&self, command: &str) -> ExecutionStrategy {
        // 1. 먼저 Layer1 security 모듈로 안전성 검사
        let analysis = command_analyzer().analyze(command);

        match analysis.risk {
            // 금지된 명령어는 차단
            CommandRisk::Forbidden => return ExecutionStrategy::Blocked,
            // 위험한 명령어는 확인 필요
            CommandRisk::Dangerous => return ExecutionStrategy::RequiresConfirmation,
            // 대화형 명령어는 PTY로
            CommandRisk::Interactive => return ExecutionStrategy::TaskPty,
            // 안전한 명령어, 주의 필요 명령어, 알 수 없는 명령어는 아래에서 처리
            CommandRisk::Safe | CommandRisk::Caution | CommandRisk::Unknown => {}
        }

        let cmd_lower = command.to_lowercase();
        let first_word = cmd_lower.split_whitespace().next().unwrap_or("");

        // 2. PTY가 필요한 대화형 명령어 (security에서 놓친 것들)
        for pty_cmd in &self.pty_commands {
            if first_word == *pty_cmd || cmd_lower.starts_with(pty_cmd) {
                return ExecutionStrategy::TaskPty;
            }
        }

        // 3. 장시간 실행 명령어
        for long_cmd in &self.long_running_commands {
            if cmd_lower.starts_with(long_cmd) {
                return ExecutionStrategy::Task;
            }
        }

        // 4. 파이프라인이나 복잡한 명령어는 Task로
        if command.contains('|') || command.contains("&&") || command.contains("||") {
            // 단, 간단한 파이프라인은 직접 실행
            if command.len() < 100 {
                return ExecutionStrategy::Direct;
            }
            return ExecutionStrategy::Task;
        }

        // 5. 주의가 필요한 명령어는 확인 요청
        if analysis.risk == CommandRisk::Caution {
            return ExecutionStrategy::RequiresConfirmation;
        }

        // 6. 알 수 없는 명령어는 확인 요청 (안전 우선)
        if analysis.risk == CommandRisk::Unknown {
            return ExecutionStrategy::RequiresConfirmation;
        }

        // 기본: 직접 실행 (Safe인 경우)
        ExecutionStrategy::Direct
    }

    /// 경로의 민감도 검사
    pub fn check_path_sensitivity(&self, path: &str) -> Option<u8> {
        let sensitivity = path_analyzer().sensitivity_score(path);
        if sensitivity > 0 {
            Some(sensitivity)
        } else {
            None
        }
    }

    /// 명령어가 금지되었는지 확인
    pub fn is_command_forbidden(&self, command: &str) -> bool {
        command_analyzer().is_forbidden(command)
    }

    /// 명령어가 안전한지 확인
    pub fn is_command_safe(&self, command: &str) -> bool {
        command_analyzer().is_safe(command)
    }

    /// 도구와 인자를 분석하여 실행 전략 결정
    pub fn determine_strategy(&self, tool_name: &str, args: &Value) -> ExecutionStrategy {
        match tool_name {
            "bash" => {
                let command = args
                    .get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                self.classify_bash_command(command)
            }
            // 읽기 도구들은 항상 직접 실행
            "read" | "glob" | "grep" => ExecutionStrategy::Direct,
            // 쓰기 도구들도 직접 실행 (빠름)
            "write" | "edit" => ExecutionStrategy::Direct,
            // Task 관련 도구들은 Task 시스템 사용
            "task_spawn" | "task_stop" | "task_send" => ExecutionStrategy::Task,
            // 알 수 없는 도구는 직접 실행
            _ => ExecutionStrategy::Direct,
        }
    }
}

/// 실행 계획의 단일 단계
#[derive(Debug, Clone)]
pub struct ExecutionPhase {
    /// 이 단계에서 병렬 실행할 도구들의 인덱스
    pub tool_indices: Vec<usize>,
    /// 병렬 실행 가능 여부
    pub parallel: bool,
}

/// 실행 계획
#[derive(Debug, Clone)]
pub struct ExecutionPlan {
    /// 실행 단계들
    pub phases: Vec<ExecutionPhase>,
    /// 원본 도구 호출들
    pub tool_calls: Vec<ToolCall>,
}

impl ExecutionPlan {
    /// 총 단계 수
    pub fn phase_count(&self) -> usize {
        self.phases.len()
    }

    /// 병렬 실행 가능한 도구 수
    pub fn parallelizable_count(&self) -> usize {
        self.phases
            .iter()
            .filter(|p| p.parallel && p.tool_indices.len() > 1)
            .map(|p| p.tool_indices.len())
            .sum()
    }
}

/// 실행 계획 생성기
pub struct ExecutionPlanner {
    classifier: ToolClassifier,
}

impl Default for ExecutionPlanner {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecutionPlanner {
    pub fn new() -> Self {
        Self {
            classifier: ToolClassifier::new(),
        }
    }

    /// 도구 호출 목록에서 실행 계획 생성
    ///
    /// 의존성을 분석하여 병렬 실행 가능한 그룹을 식별합니다.
    pub fn plan(&self, tool_calls: Vec<ToolCall>) -> ExecutionPlan {
        if tool_calls.is_empty() {
            return ExecutionPlan {
                phases: vec![],
                tool_calls,
            };
        }

        // 각 도구의 의존성 유형 분류
        let classifications: Vec<DependencyType> = tool_calls
            .iter()
            .map(|tc| self.classifier.classify(&tc.name))
            .collect();

        // 파일 경로별 접근 추적 (쓰기 충돌 감지)
        let mut file_access: HashMap<String, Vec<usize>> = HashMap::new();

        for (i, tc) in tool_calls.iter().enumerate() {
            if let Some(path) = self.extract_file_path(&tc.arguments) {
                file_access.entry(path).or_default().push(i);
            }
        }

        // 실행 단계 생성
        let mut phases = Vec::new();
        let mut processed = vec![false; tool_calls.len()];

        // Phase 1: 독립적인 읽기 전용 도구들 (병렬 실행)
        let read_only_indices: Vec<usize> = classifications
            .iter()
            .enumerate()
            .filter(|(_, &dep)| dep == DependencyType::ReadOnly)
            .map(|(i, _)| i)
            .collect();

        if !read_only_indices.is_empty() {
            // 같은 파일에 대한 쓰기가 없는 읽기만 병렬 실행
            let safe_reads: Vec<usize> = read_only_indices
                .into_iter()
                .filter(|&i| {
                    if let Some(path) = self.extract_file_path(&tool_calls[i].arguments) {
                        // 이 파일에 대한 쓰기 도구가 없는지 확인
                        !file_access.get(&path).map_or(false, |indices| {
                            indices.iter().any(|&j| {
                                j != i && classifications[j] == DependencyType::Write
                            })
                        })
                    } else {
                        true
                    }
                })
                .collect();

            if !safe_reads.is_empty() {
                for &i in &safe_reads {
                    processed[i] = true;
                }
                phases.push(ExecutionPhase {
                    tool_indices: safe_reads,
                    parallel: true,
                });
            }
        }

        // Phase 2+: 나머지 도구들 (순차 실행)
        for (i, _) in tool_calls.iter().enumerate() {
            if !processed[i] {
                phases.push(ExecutionPhase {
                    tool_indices: vec![i],
                    parallel: false,
                });
                processed[i] = true;
            }
        }

        ExecutionPlan { phases, tool_calls }
    }

    /// 도구 인자에서 파일 경로 추출
    fn extract_file_path(&self, args: &Value) -> Option<String> {
        // 일반적인 파일 경로 필드명들
        args.get("file_path")
            .or_else(|| args.get("path"))
            .or_else(|| args.get("filename"))
            .and_then(|v| v.as_str())
            .map(String::from)
    }
}

/// 병렬 실행 통계
#[derive(Debug, Default, Clone)]
pub struct ParallelExecutionStats {
    /// 총 도구 호출 수
    pub total_tools: usize,
    /// 병렬 실행된 도구 수
    pub parallel_executed: usize,
    /// 순차 실행된 도구 수
    pub sequential_executed: usize,
    /// 총 실행 단계 수
    pub phase_count: usize,
    /// 예상 시간 절감 (ms)
    pub estimated_time_saved_ms: u64,
}

impl ParallelExecutionStats {
    /// 병렬화 효율성 (0.0 ~ 1.0)
    pub fn parallelization_ratio(&self) -> f64 {
        if self.total_tools == 0 {
            return 0.0;
        }
        self.parallel_executed as f64 / self.total_tools as f64
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tool_call(name: &str, path: Option<&str>) -> ToolCall {
        let args = match path {
            Some(p) => serde_json::json!({ "file_path": p }),
            None => serde_json::json!({}),
        };
        ToolCall {
            id: format!("call_{}", name),
            name: name.to_string(),
            arguments: args,
        }
    }

    #[test]
    fn test_classifier() {
        let classifier = ToolClassifier::new();

        assert_eq!(classifier.classify("read"), DependencyType::ReadOnly);
        assert_eq!(classifier.classify("glob"), DependencyType::ReadOnly);
        assert_eq!(classifier.classify("write"), DependencyType::Write);
        assert_eq!(classifier.classify("bash"), DependencyType::StateMutating);
        assert_eq!(classifier.classify("unknown"), DependencyType::StateMutating);
    }

    #[test]
    fn test_plan_all_reads() {
        let planner = ExecutionPlanner::new();

        let calls = vec![
            make_tool_call("read", Some("a.rs")),
            make_tool_call("read", Some("b.rs")),
            make_tool_call("glob", None),
        ];

        let plan = planner.plan(calls);

        // 모든 읽기 작업은 하나의 병렬 단계로 묶여야 함
        assert_eq!(plan.phase_count(), 1);
        assert_eq!(plan.phases[0].tool_indices.len(), 3);
        assert!(plan.phases[0].parallel);
    }

    #[test]
    fn test_plan_read_write_dependency() {
        let planner = ExecutionPlanner::new();

        let calls = vec![
            make_tool_call("read", Some("a.rs")),
            make_tool_call("write", Some("a.rs")), // 같은 파일에 쓰기
            make_tool_call("read", Some("b.rs")),
        ];

        let plan = planner.plan(calls);

        // a.rs 읽기는 쓰기와 충돌하므로 순차 실행
        // b.rs 읽기만 병렬 가능
        assert!(plan.phase_count() >= 2);
    }

    #[test]
    fn test_plan_mixed() {
        let planner = ExecutionPlanner::new();

        let calls = vec![
            make_tool_call("read", Some("a.rs")),
            make_tool_call("bash", None),
            make_tool_call("glob", None),
        ];

        let plan = planner.plan(calls);

        // read와 glob은 병렬 가능, bash는 순차
        assert_eq!(plan.parallelizable_count(), 2);
    }

    #[test]
    fn test_empty_plan() {
        let planner = ExecutionPlanner::new();
        let plan = planner.plan(vec![]);

        assert_eq!(plan.phase_count(), 0);
    }

    #[test]
    fn test_security_integration_forbidden() {
        let classifier = ToolClassifier::new();

        // 금지된 명령어
        assert_eq!(
            classifier.classify_bash_command("rm -rf /"),
            ExecutionStrategy::Blocked
        );
        assert_eq!(
            classifier.classify_bash_command(":(){ :|:& };:"),
            ExecutionStrategy::Blocked
        );
    }

    #[test]
    fn test_security_integration_dangerous() {
        let classifier = ToolClassifier::new();

        // 위험한 명령어 - 확인 필요
        assert_eq!(
            classifier.classify_bash_command("rm file.txt"),
            ExecutionStrategy::RequiresConfirmation
        );
        assert_eq!(
            classifier.classify_bash_command("git push --force"),
            ExecutionStrategy::RequiresConfirmation
        );
    }

    #[test]
    fn test_security_integration_safe() {
        let classifier = ToolClassifier::new();

        // 안전한 명령어 - 직접 실행
        assert_eq!(
            classifier.classify_bash_command("ls -la"),
            ExecutionStrategy::Direct
        );
        assert_eq!(
            classifier.classify_bash_command("pwd"),
            ExecutionStrategy::Direct
        );
        assert_eq!(
            classifier.classify_bash_command("git status"),
            ExecutionStrategy::Direct
        );
    }

    #[test]
    fn test_security_integration_interactive() {
        let classifier = ToolClassifier::new();

        // 대화형 명령어 - PTY 필요
        assert_eq!(
            classifier.classify_bash_command("vim file.txt"),
            ExecutionStrategy::TaskPty
        );
        assert_eq!(
            classifier.classify_bash_command("htop"),
            ExecutionStrategy::TaskPty
        );
    }

    #[test]
    fn test_path_sensitivity() {
        let classifier = ToolClassifier::new();

        // 민감한 경로
        assert!(classifier.check_path_sensitivity("/home/user/.env").is_some());
        assert!(classifier.check_path_sensitivity("/home/user/.ssh/id_rsa").is_some());

        // 일반 경로
        assert!(classifier.check_path_sensitivity("/home/user/code/main.rs").is_none());
    }

    #[test]
    fn test_forbidden_check() {
        let classifier = ToolClassifier::new();

        assert!(classifier.is_command_forbidden("rm -rf /"));
        assert!(!classifier.is_command_forbidden("ls -la"));
    }

    #[test]
    fn test_safe_check() {
        let classifier = ToolClassifier::new();

        assert!(classifier.is_command_safe("ls -la"));
        assert!(classifier.is_command_safe("git status"));
        assert!(!classifier.is_command_safe("rm file.txt"));
    }
}
