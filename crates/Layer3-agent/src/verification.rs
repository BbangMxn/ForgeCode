//! Verification Module - OpenCode 스타일 철저한 검증
//!
//! 변경 후 자동 검증:
//! - 빌드 확인
//! - 테스트 실행
//! - 타입 체크
//! - 린트 검사
//!
//! ## 검증 레벨
//!
//! - **None**: 검증 없음 (위험)
//! - **Quick**: 영향 받은 파일만 확인
//! - **Standard**: 빌드 + 영향 받은 테스트
//! - **Thorough**: 전체 테스트 스위트 + 린트

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// 검증 레벨
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum VerificationLevel {
    /// 검증 없음
    None,
    /// 빠른 검증 (타입 체크만)
    Quick,
    /// 표준 검증 (빌드 + 관련 테스트)
    #[default]
    Standard,
    /// 철저한 검증 (전체 테스트 + 린트)
    Thorough,
}

/// 검증 결과
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    /// 성공 여부
    pub success: bool,
    /// 검증 레벨
    pub level: VerificationLevel,
    /// 개별 검사 결과
    pub checks: Vec<CheckResult>,
    /// 총 소요 시간 (ms)
    pub duration_ms: u64,
    /// 요약
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    pub name: String,
    pub passed: bool,
    pub output: Option<String>,
    pub duration_ms: u64,
}

impl VerificationResult {
    pub fn new(level: VerificationLevel) -> Self {
        Self {
            success: true,
            level,
            checks: Vec::new(),
            duration_ms: 0,
            summary: String::new(),
        }
    }

    pub fn add_check(&mut self, check: CheckResult) {
        if !check.passed {
            self.success = false;
        }
        self.duration_ms += check.duration_ms;
        self.checks.push(check);
    }

    pub fn format_summary(&mut self) {
        let passed = self.checks.iter().filter(|c| c.passed).count();
        let total = self.checks.len();
        
        self.summary = if self.success {
            format!("✅ All {} checks passed ({:.1}s)", total, self.duration_ms as f64 / 1000.0)
        } else {
            let failed: Vec<_> = self.checks.iter()
                .filter(|c| !c.passed)
                .map(|c| c.name.as_str())
                .collect();
            format!("❌ {}/{} checks passed. Failed: {}", passed, total, failed.join(", "))
        };
    }
}

/// 프로젝트 타입별 검증 명령어
#[derive(Debug, Clone)]
pub struct ProjectVerifier {
    /// 프로젝트 타입
    project_type: ProjectType,
    /// 커스텀 명령어 (프로젝트별 오버라이드)
    custom_commands: HashMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectType {
    Rust,
    Node,
    Python,
    Go,
    Unknown,
}

impl ProjectVerifier {
    pub fn detect(working_dir: &Path) -> Self {
        let project_type = if working_dir.join("Cargo.toml").exists() {
            ProjectType::Rust
        } else if working_dir.join("package.json").exists() {
            ProjectType::Node
        } else if working_dir.join("pyproject.toml").exists() 
            || working_dir.join("setup.py").exists() 
        {
            ProjectType::Python
        } else if working_dir.join("go.mod").exists() {
            ProjectType::Go
        } else {
            ProjectType::Unknown
        };

        Self {
            project_type,
            custom_commands: HashMap::new(),
        }
    }

    /// 검증 명령어 목록 생성
    pub fn verification_commands(&self, level: VerificationLevel) -> Vec<VerificationCommand> {
        match self.project_type {
            ProjectType::Rust => self.rust_commands(level),
            ProjectType::Node => self.node_commands(level),
            ProjectType::Python => self.python_commands(level),
            ProjectType::Go => self.go_commands(level),
            ProjectType::Unknown => Vec::new(),
        }
    }

    fn rust_commands(&self, level: VerificationLevel) -> Vec<VerificationCommand> {
        match level {
            VerificationLevel::None => vec![],
            VerificationLevel::Quick => vec![
                VerificationCommand::new("check", "cargo check"),
            ],
            VerificationLevel::Standard => vec![
                VerificationCommand::new("check", "cargo check"),
                VerificationCommand::new("build", "cargo build"),
                VerificationCommand::new("test", "cargo test"),
            ],
            VerificationLevel::Thorough => vec![
                VerificationCommand::new("check", "cargo check"),
                VerificationCommand::new("build", "cargo build"),
                VerificationCommand::new("test", "cargo test"),
                VerificationCommand::new("clippy", "cargo clippy -- -D warnings"),
                VerificationCommand::new("fmt", "cargo fmt --check"),
            ],
        }
    }

    fn node_commands(&self, level: VerificationLevel) -> Vec<VerificationCommand> {
        match level {
            VerificationLevel::None => vec![],
            VerificationLevel::Quick => vec![
                VerificationCommand::new("typecheck", "npx tsc --noEmit"),
            ],
            VerificationLevel::Standard => vec![
                VerificationCommand::new("typecheck", "npx tsc --noEmit"),
                VerificationCommand::new("test", "npm test"),
            ],
            VerificationLevel::Thorough => vec![
                VerificationCommand::new("typecheck", "npx tsc --noEmit"),
                VerificationCommand::new("lint", "npm run lint"),
                VerificationCommand::new("test", "npm test"),
                VerificationCommand::new("build", "npm run build"),
            ],
        }
    }

    fn python_commands(&self, level: VerificationLevel) -> Vec<VerificationCommand> {
        match level {
            VerificationLevel::None => vec![],
            VerificationLevel::Quick => vec![
                VerificationCommand::new("typecheck", "mypy ."),
            ],
            VerificationLevel::Standard => vec![
                VerificationCommand::new("typecheck", "mypy ."),
                VerificationCommand::new("test", "pytest"),
            ],
            VerificationLevel::Thorough => vec![
                VerificationCommand::new("typecheck", "mypy ."),
                VerificationCommand::new("lint", "ruff check ."),
                VerificationCommand::new("format", "black --check ."),
                VerificationCommand::new("test", "pytest -v"),
            ],
        }
    }

    fn go_commands(&self, level: VerificationLevel) -> Vec<VerificationCommand> {
        match level {
            VerificationLevel::None => vec![],
            VerificationLevel::Quick => vec![
                VerificationCommand::new("build", "go build ./..."),
            ],
            VerificationLevel::Standard => vec![
                VerificationCommand::new("build", "go build ./..."),
                VerificationCommand::new("test", "go test ./..."),
            ],
            VerificationLevel::Thorough => vec![
                VerificationCommand::new("build", "go build ./..."),
                VerificationCommand::new("test", "go test -v ./..."),
                VerificationCommand::new("vet", "go vet ./..."),
                VerificationCommand::new("lint", "golangci-lint run"),
            ],
        }
    }

    /// 검증 프롬프트 생성
    pub fn verification_prompt(&self, level: VerificationLevel) -> String {
        let commands = self.verification_commands(level);
        
        if commands.is_empty() {
            return String::new();
        }

        let mut prompt = format!(
            "## Verification ({:?})\n\nAfter making changes, run these verification commands:\n\n",
            level
        );

        for cmd in &commands {
            prompt.push_str(&format!("- **{}**: `{}`\n", cmd.name, cmd.command));
        }

        prompt.push_str("\nIf any command fails, analyze the error and fix it before proceeding.\n");

        prompt
    }
}

#[derive(Debug, Clone)]
pub struct VerificationCommand {
    pub name: String,
    pub command: String,
    pub optional: bool,
    pub timeout_secs: u64,
}

impl VerificationCommand {
    pub fn new(name: &str, command: &str) -> Self {
        Self {
            name: name.to_string(),
            command: command.to_string(),
            optional: false,
            timeout_secs: 300, // 5분 기본
        }
    }

    pub fn optional(mut self) -> Self {
        self.optional = true;
        self
    }

    pub fn timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }
}

/// 변경된 파일 기반 영향 분석
pub struct ImpactAnalyzer;

impl ImpactAnalyzer {
    /// 변경된 파일이 영향을 미치는 범위 분석
    pub fn analyze_impact(changed_files: &[String]) -> ImpactReport {
        let mut report = ImpactReport::default();

        for file in changed_files {
            let path = Path::new(file);
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

            match ext {
                "rs" => {
                    report.needs_build = true;
                    report.needs_test = true;
                    if file.contains("lib.rs") || file.contains("mod.rs") {
                        report.scope = ImpactScope::Wide;
                    }
                }
                "ts" | "tsx" | "js" | "jsx" => {
                    report.needs_build = true;
                    report.needs_test = true;
                }
                "py" => {
                    report.needs_test = true;
                    if file.contains("__init__") {
                        report.scope = ImpactScope::Wide;
                    }
                }
                "toml" | "json" | "yaml" | "yml" => {
                    report.needs_build = true;
                    report.scope = ImpactScope::Wide;
                }
                "md" | "txt" => {
                    // 문서는 빌드/테스트 불필요
                }
                _ => {}
            }

            // 테스트 파일
            if file.contains("test") || file.contains("spec") {
                report.needs_test = true;
            }
        }

        report
    }

    /// 권장 검증 레벨
    pub fn recommended_level(report: &ImpactReport) -> VerificationLevel {
        match report.scope {
            ImpactScope::None => VerificationLevel::None,
            ImpactScope::Local => {
                if report.needs_test {
                    VerificationLevel::Standard
                } else {
                    VerificationLevel::Quick
                }
            }
            ImpactScope::Wide => VerificationLevel::Thorough,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ImpactReport {
    pub needs_build: bool,
    pub needs_test: bool,
    pub scope: ImpactScope,
    pub affected_modules: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ImpactScope {
    #[default]
    None,
    Local,
    Wide,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_commands() {
        let verifier = ProjectVerifier {
            project_type: ProjectType::Rust,
            custom_commands: HashMap::new(),
        };

        let quick = verifier.verification_commands(VerificationLevel::Quick);
        assert_eq!(quick.len(), 1);
        assert_eq!(quick[0].name, "check");

        let thorough = verifier.verification_commands(VerificationLevel::Thorough);
        assert_eq!(thorough.len(), 5);
    }

    #[test]
    fn test_impact_analysis() {
        let files = vec!["src/lib.rs".to_string(), "src/main.rs".to_string()];
        let report = ImpactAnalyzer::analyze_impact(&files);

        assert!(report.needs_build);
        assert!(report.needs_test);
        assert_eq!(report.scope, ImpactScope::Wide);
    }

    #[test]
    fn test_recommended_level() {
        let mut report = ImpactReport::default();
        assert_eq!(ImpactAnalyzer::recommended_level(&report), VerificationLevel::None);

        report.needs_test = true;
        report.scope = ImpactScope::Local;
        assert_eq!(ImpactAnalyzer::recommended_level(&report), VerificationLevel::Standard);

        report.scope = ImpactScope::Wide;
        assert_eq!(ImpactAnalyzer::recommended_level(&report), VerificationLevel::Thorough);
    }
}
