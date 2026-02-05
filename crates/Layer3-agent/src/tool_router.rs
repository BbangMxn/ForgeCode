//! Tool Router - 지능적인 도구 선택
//!
//! LLM이 적절한 도구를 선택하도록 돕는 시스템:
//! - 명령어 패턴 분석
//! - 컨텍스트 기반 추천
//! - 자동 라우팅 힌트
//!
//! ## 핵심 규칙
//!
//! ### bash vs task_spawn
//! ```text
//! bash: 즉시 완료되는 명령어
//! - ls, cat, grep, find
//! - cargo build, cargo test, cargo --version
//! - git status, git diff, git log
//! - npm install, pip install
//!
//! task_spawn: 장시간 실행/서버/watch
//! - cargo run (서버)
//! - npm start, npm run dev
//! - python -m http.server
//! - tail -f, watch
//! - docker run (detached)
//! ```

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// 도구 선택 힌트
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolHint {
    /// 추천 도구
    pub recommended_tool: String,
    /// 이유
    pub reason: String,
    /// 신뢰도 (0.0 - 1.0)
    pub confidence: f32,
    /// 대안 도구들
    pub alternatives: Vec<String>,
}

/// 명령어 타입
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandType {
    /// 즉시 완료 (bash 적합)
    Instant,
    /// 장시간 실행 (task_spawn 적합)
    LongRunning,
    /// 서버/데몬 (task_spawn 필수)
    Server,
    /// 대화형 (task_spawn + PTY)
    Interactive,
    /// 위험/파괴적
    Destructive,
    /// 알 수 없음
    Unknown,
}

/// Tool Router
pub struct ToolRouter {
    /// 장시간 실행 패턴
    long_running_patterns: Vec<CommandPattern>,
    /// 서버 패턴
    server_patterns: Vec<CommandPattern>,
    /// 즉시 완료 패턴
    instant_patterns: Vec<CommandPattern>,
    /// 대화형 패턴
    interactive_patterns: Vec<CommandPattern>,
}

#[derive(Debug, Clone)]
struct CommandPattern {
    pattern: String,
    pattern_type: PatternType,
}

#[derive(Debug, Clone, Copy)]
enum PatternType {
    Prefix,      // 명령어 시작
    Contains,    // 포함
    Suffix,      // 끝
    Exact,       // 정확히 일치
}

impl Default for ToolRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRouter {
    pub fn new() -> Self {
        Self {
            long_running_patterns: vec![
                // Build with watch
                pattern_prefix("cargo watch"),
                pattern_prefix("npm run watch"),
                pattern_prefix("tsc --watch"),
                pattern_prefix("webpack --watch"),
                // Tests with watch
                pattern_prefix("cargo test --watch"),
                pattern_prefix("npm test --watch"),
                pattern_prefix("pytest --watch"),
                // File watching
                pattern_prefix("tail -f"),
                pattern_prefix("watch "),
                pattern_prefix("fswatch"),
                // Long builds
                pattern_contains("--release"),
            ],
            server_patterns: vec![
                // Rust servers
                pattern_prefix("cargo run"),
                pattern_contains("actix"),
                pattern_contains("axum"),
                pattern_contains("rocket"),
                // Node.js servers
                pattern_prefix("npm start"),
                pattern_prefix("npm run dev"),
                pattern_prefix("npm run serve"),
                pattern_prefix("node "),
                pattern_prefix("nodemon"),
                pattern_prefix("ts-node"),
                pattern_prefix("bun run"),
                pattern_prefix("deno run"),
                // Python servers
                pattern_prefix("python -m http"),
                pattern_prefix("python manage.py runserver"),
                pattern_prefix("flask run"),
                pattern_prefix("uvicorn"),
                pattern_prefix("gunicorn"),
                pattern_prefix("django"),
                // Go servers
                pattern_prefix("go run"),
                // Docker
                pattern_prefix("docker run"),
                pattern_prefix("docker-compose up"),
                pattern_prefix("docker compose up"),
                // Database
                pattern_prefix("mongod"),
                pattern_prefix("redis-server"),
                pattern_prefix("postgres"),
                pattern_prefix("mysql"),
            ],
            instant_patterns: vec![
                // File operations
                pattern_prefix("ls"),
                pattern_prefix("dir"),
                pattern_prefix("cat "),
                pattern_prefix("head "),
                pattern_prefix("tail "),
                pattern_prefix("grep "),
                pattern_prefix("find "),
                pattern_prefix("wc "),
                pattern_prefix("pwd"),
                pattern_prefix("cd "),
                pattern_prefix("mkdir "),
                pattern_prefix("rm "),
                pattern_prefix("cp "),
                pattern_prefix("mv "),
                pattern_prefix("touch "),
                // Version checks
                pattern_suffix("--version"),
                pattern_suffix("-v"),
                pattern_suffix("-V"),
                // Status checks
                pattern_prefix("git status"),
                pattern_prefix("git diff"),
                pattern_prefix("git log"),
                pattern_prefix("git branch"),
                pattern_prefix("git show"),
                // Cargo quick commands
                pattern_prefix("cargo --version"),
                pattern_prefix("cargo check"),
                pattern_prefix("cargo fmt"),
                pattern_prefix("cargo clippy"),
                pattern_exact("cargo build"),
                pattern_exact("cargo test"),
                // NPM quick commands
                pattern_prefix("npm --version"),
                pattern_prefix("npm list"),
                pattern_prefix("npm outdated"),
                pattern_exact("npm install"),
                pattern_exact("npm ci"),
                // Python quick commands
                pattern_prefix("python --version"),
                pattern_prefix("pip list"),
                pattern_prefix("pip show"),
                pattern_exact("pip install"),
                // Other
                pattern_prefix("echo "),
                pattern_prefix("date"),
                pattern_prefix("whoami"),
                pattern_prefix("hostname"),
                pattern_prefix("env"),
                pattern_prefix("printenv"),
            ],
            interactive_patterns: vec![
                pattern_prefix("vim "),
                pattern_prefix("nvim "),
                pattern_prefix("nano "),
                pattern_prefix("emacs "),
                pattern_exact("python"),
                pattern_exact("node"),
                pattern_exact("irb"),
                pattern_prefix("ssh "),
                pattern_prefix("mysql -u"),
                pattern_prefix("psql "),
                pattern_prefix("redis-cli"),
                pattern_prefix("mongo "),
            ],
        }
    }

    /// 명령어 분석
    pub fn analyze(&self, command: &str) -> CommandType {
        let cmd = command.trim().to_lowercase();

        // 대화형 체크 (우선)
        if self.matches_patterns(&cmd, &self.interactive_patterns) {
            return CommandType::Interactive;
        }

        // 서버 체크
        if self.matches_patterns(&cmd, &self.server_patterns) {
            return CommandType::Server;
        }

        // 장시간 실행 체크
        if self.matches_patterns(&cmd, &self.long_running_patterns) {
            return CommandType::LongRunning;
        }

        // 즉시 완료 체크
        if self.matches_patterns(&cmd, &self.instant_patterns) {
            return CommandType::Instant;
        }

        // 휴리스틱: 파이프나 리다이렉션이 있으면 보통 즉시 완료
        if cmd.contains(" | ") || cmd.contains(" > ") || cmd.contains(" < ") {
            return CommandType::Instant;
        }

        CommandType::Unknown
    }

    /// 도구 추천
    pub fn recommend_tool(&self, command: &str) -> ToolHint {
        let cmd_type = self.analyze(command);

        match cmd_type {
            CommandType::Instant => ToolHint {
                recommended_tool: "bash".to_string(),
                reason: "Quick command that completes immediately".to_string(),
                confidence: 0.9,
                alternatives: vec![],
            },
            CommandType::Server => ToolHint {
                recommended_tool: "task_spawn".to_string(),
                reason: "Server/daemon that runs continuously. Use task_spawn with mode='pty' for proper signal handling.".to_string(),
                confidence: 0.95,
                alternatives: vec!["bash".to_string()],
            },
            CommandType::LongRunning => ToolHint {
                recommended_tool: "task_spawn".to_string(),
                reason: "Long-running process. Use task_spawn to run in background and task_logs to check progress.".to_string(),
                confidence: 0.85,
                alternatives: vec!["bash".to_string()],
            },
            CommandType::Interactive => ToolHint {
                recommended_tool: "task_spawn".to_string(),
                reason: "Interactive command requires PTY. Use task_spawn with mode='pty' and task_send for input.".to_string(),
                confidence: 0.9,
                alternatives: vec![],
            },
            CommandType::Destructive => ToolHint {
                recommended_tool: "bash".to_string(),
                reason: "Destructive command - will require permission confirmation.".to_string(),
                confidence: 0.8,
                alternatives: vec![],
            },
            CommandType::Unknown => ToolHint {
                recommended_tool: "bash".to_string(),
                reason: "Unknown command type. Try bash first; if it needs background execution, use task_spawn.".to_string(),
                confidence: 0.5,
                alternatives: vec!["task_spawn".to_string()],
            },
        }
    }

    /// 시스템 프롬프트용 도구 선택 가이드라인 생성
    pub fn tool_selection_guide(&self) -> String {
        r#"## Tool Selection Guide

### When to use `bash`:
- Quick commands: ls, cat, grep, find, echo
- Version checks: --version, -v
- Build commands: cargo build, npm install, pip install
- Git commands: git status, git diff, git log
- One-shot scripts

### When to use `task_spawn`:
- Servers: cargo run, npm start, python -m http.server
- Watch modes: cargo watch, npm run dev, tsc --watch
- Background processes: docker run -d
- Long-running tasks that need monitoring

### Decision Flow:
1. Will it complete in < 30 seconds? → Use `bash`
2. Is it a server or daemon? → Use `task_spawn` (mode: pty)
3. Does it need background execution? → Use `task_spawn`
4. Will you need to send input later? → Use `task_spawn` (mode: pty)
5. Otherwise → Use `bash`

### After task_spawn:
- Use `task_wait` to wait for specific output
- Use `task_logs` to check progress
- Use `task_send` to send input (PTY mode)
- Use `task_stop` to terminate
"#.to_string()
    }

    fn matches_patterns(&self, cmd: &str, patterns: &[CommandPattern]) -> bool {
        patterns.iter().any(|p| match p.pattern_type {
            PatternType::Prefix => cmd.starts_with(&p.pattern),
            PatternType::Contains => cmd.contains(&p.pattern),
            PatternType::Suffix => cmd.ends_with(&p.pattern),
            PatternType::Exact => cmd == p.pattern,
        })
    }
}

// Pattern builders
fn pattern_prefix(s: &str) -> CommandPattern {
    CommandPattern {
        pattern: s.to_lowercase(),
        pattern_type: PatternType::Prefix,
    }
}

fn pattern_contains(s: &str) -> CommandPattern {
    CommandPattern {
        pattern: s.to_lowercase(),
        pattern_type: PatternType::Contains,
    }
}

fn pattern_suffix(s: &str) -> CommandPattern {
    CommandPattern {
        pattern: s.to_lowercase(),
        pattern_type: PatternType::Suffix,
    }
}

fn pattern_exact(s: &str) -> CommandPattern {
    CommandPattern {
        pattern: s.to_lowercase(),
        pattern_type: PatternType::Exact,
    }
}

/// 글로벌 Tool Router
static TOOL_ROUTER: std::sync::OnceLock<ToolRouter> = std::sync::OnceLock::new();

pub fn tool_router() -> &'static ToolRouter {
    TOOL_ROUTER.get_or_init(ToolRouter::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instant_commands() {
        let router = ToolRouter::new();

        assert_eq!(router.analyze("ls -la"), CommandType::Instant);
        assert_eq!(router.analyze("cat file.txt"), CommandType::Instant);
        assert_eq!(router.analyze("git status"), CommandType::Instant);
        assert_eq!(router.analyze("cargo --version"), CommandType::Instant);
        assert_eq!(router.analyze("cargo build"), CommandType::Instant);
        assert_eq!(router.analyze("npm install"), CommandType::Instant);
    }

    #[test]
    fn test_server_commands() {
        let router = ToolRouter::new();

        assert_eq!(router.analyze("cargo run"), CommandType::Server);
        assert_eq!(router.analyze("npm start"), CommandType::Server);
        assert_eq!(router.analyze("npm run dev"), CommandType::Server);
        assert_eq!(router.analyze("python -m http.server"), CommandType::Server);
        assert_eq!(router.analyze("docker run nginx"), CommandType::Server);
    }

    #[test]
    fn test_long_running_commands() {
        let router = ToolRouter::new();

        assert_eq!(router.analyze("cargo watch -x test"), CommandType::LongRunning);
        assert_eq!(router.analyze("tail -f /var/log/syslog"), CommandType::LongRunning);
        assert_eq!(router.analyze("npm run watch"), CommandType::LongRunning);
    }

    #[test]
    fn test_interactive_commands() {
        let router = ToolRouter::new();

        assert_eq!(router.analyze("vim file.txt"), CommandType::Interactive);
        assert_eq!(router.analyze("python"), CommandType::Interactive);
        assert_eq!(router.analyze("ssh user@host"), CommandType::Interactive);
    }

    #[test]
    fn test_recommendations() {
        let router = ToolRouter::new();

        let hint = router.recommend_tool("ls -la");
        assert_eq!(hint.recommended_tool, "bash");
        assert!(hint.confidence > 0.8);

        let hint = router.recommend_tool("npm start");
        assert_eq!(hint.recommended_tool, "task_spawn");
        assert!(hint.confidence > 0.9);
    }
}
