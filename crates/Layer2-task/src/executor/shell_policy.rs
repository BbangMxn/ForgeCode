//! Shell Command Policy - CMD/Bash 명령어 수준 권한 제어
//!
//! PTY executor에서 실행되는 명령어를 검증하고 차단합니다.
//!
//! ## 기능
//! - 위험 명령어 차단 (rm -rf, format, shutdown 등)
//! - 경로 접근 제한 (시스템 디렉토리, 민감한 파일)
//! - 네트워크 명령어 제한 (curl, wget 등으로 외부 스크립트 실행 방지)
//! - Task별 커스텀 정책 지원
//!
//! ## 사용 예시
//! ```rust,ignore
//! let policy = ShellPolicy::default()
//!     .deny_commands(vec!["rm", "format"])
//!     .deny_paths(vec!["/etc", "C:\\Windows"])
//!     .allow_network(false);
//!
//! match policy.validate("rm -rf /home") {
//!     PolicyResult::Allow => { /* execute */ }
//!     PolicyResult::Deny(reason) => { /* block */ }
//!     PolicyResult::RequiresApproval(reason) => { /* ask user */ }
//! }
//! ```

use std::collections::HashSet;
use regex::Regex;
use tracing::warn;

/// 정책 검증 결과
#[derive(Debug, Clone, PartialEq)]
pub enum PolicyResult {
    /// 허용
    Allow,
    /// 거부 (이유 포함)
    Deny(String),
    /// 사용자 승인 필요
    RequiresApproval(String),
    /// 샌드박스에서 실행 필요
    Sandbox(String),
}

/// 명령어 위험 수준
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskLevel {
    /// 안전 (ls, pwd, echo 등)
    Safe = 0,
    /// 낮은 위험 (cat, grep 등)
    Low = 1,
    /// 중간 위험 (npm install, cargo build 등)
    Medium = 2,
    /// 높은 위험 (rm, git push 등)
    High = 3,
    /// 매우 위험 (rm -rf, format, shutdown 등)
    Critical = 4,
}

/// Shell 명령어 정책
#[derive(Debug, Clone)]
pub struct ShellPolicy {
    /// 차단할 명령어 목록
    denied_commands: HashSet<String>,
    /// 항상 허용할 명령어 목록 (화이트리스트)
    allowed_commands: HashSet<String>,
    /// 차단할 경로 패턴
    denied_paths: Vec<String>,
    /// 허용할 경로 패턴 (작업 디렉토리 등)
    allowed_paths: Vec<String>,
    /// 네트워크 명령어 허용 여부
    allow_network: bool,
    /// 파이프/리다이렉트 허용 여부
    allow_pipe_redirect: bool,
    /// 승인 필요 위험 수준 임계값
    approval_threshold: RiskLevel,
    /// 차단 위험 수준 임계값
    deny_threshold: RiskLevel,
    /// 사용자 정의 차단 패턴 (regex)
    custom_deny_patterns: Vec<String>,
}

impl Default for ShellPolicy {
    fn default() -> Self {
        Self {
            denied_commands: Self::default_denied_commands(),
            allowed_commands: Self::default_allowed_commands(),
            denied_paths: Self::default_denied_paths(),
            allowed_paths: Vec::new(),
            allow_network: true,
            allow_pipe_redirect: true,
            approval_threshold: RiskLevel::High,
            deny_threshold: RiskLevel::Critical,
            custom_deny_patterns: Vec::new(),
        }
    }
}

impl ShellPolicy {
    /// 새 정책 생성 (기본값)
    pub fn new() -> Self {
        Self::default()
    }

    /// 엄격한 정책 생성
    pub fn strict() -> Self {
        Self {
            denied_commands: Self::default_denied_commands(),
            allowed_commands: HashSet::new(), // 화이트리스트 비활성화
            denied_paths: Self::default_denied_paths(),
            allowed_paths: Vec::new(),
            allow_network: false,
            allow_pipe_redirect: false,
            approval_threshold: RiskLevel::Medium,
            deny_threshold: RiskLevel::High,
            custom_deny_patterns: Vec::new(),
        }
    }

    /// 개발용 관대한 정책
    pub fn permissive() -> Self {
        Self {
            denied_commands: Self::minimal_denied_commands(),
            allowed_commands: Self::default_allowed_commands(),
            denied_paths: Self::minimal_denied_paths(),
            allowed_paths: Vec::new(),
            allow_network: true,
            allow_pipe_redirect: true,
            approval_threshold: RiskLevel::Critical,
            deny_threshold: RiskLevel::Critical,
            custom_deny_patterns: Vec::new(),
        }
    }

    /// 기본 차단 명령어
    fn default_denied_commands() -> HashSet<String> {
        [
            // 시스템 파괴 명령어
            "rm -rf /",
            "rm -rf /*",
            "rm -rf ~",
            "rm -rf .",
            ":(){ :|:& };:", // Fork bomb
            "mkfs",
            "dd if=/dev/zero",
            "dd if=/dev/random",
            "> /dev/sda",
            // Windows 위험 명령어
            "format",
            "del /f /s /q c:\\",
            "rd /s /q c:\\",
            // 시스템 제어
            "shutdown",
            "reboot",
            "halt",
            "poweroff",
            "init 0",
            "init 6",
            // 권한 상승
            "chmod 777 /",
            "chown root",
            // 네트워크 공격
            "nc -e",
            "ncat -e",
            "bash -i >& /dev/tcp",
            "python -c 'import socket'",
            // 민감 파일 수정
            "passwd",
            "visudo",
            "/etc/shadow",
            "/etc/passwd",
            "authorized_keys",
            // 패키지 관리자 위험 동작
            "apt-get remove --purge",
            "yum remove",
            "pip uninstall -y",
            "npm uninstall -g",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    /// 최소 차단 명령어 (관대한 정책용)
    fn minimal_denied_commands() -> HashSet<String> {
        [
            "rm -rf /",
            "rm -rf /*",
            ":(){ :|:& };:",
            "mkfs",
            "format c:",
            "shutdown",
            "reboot",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    /// 기본 허용 명령어 (화이트리스트)
    fn default_allowed_commands() -> HashSet<String> {
        [
            // 탐색/읽기
            "ls", "dir", "pwd", "cd", "cat", "head", "tail", "less", "more",
            "find", "grep", "rg", "ag", "tree", "file", "stat", "wc",
            // 개발 도구
            "git", "cargo", "npm", "yarn", "pnpm", "node", "python", "python3",
            "pip", "pip3", "rustc", "rustup", "go", "java", "javac", "mvn", "gradle",
            "make", "cmake", "gcc", "g++", "clang",
            // 빌드/테스트
            "cargo build", "cargo test", "cargo run", "cargo check", "cargo fmt",
            "npm install", "npm run", "npm test", "npm start",
            "yarn install", "yarn build", "yarn test",
            "pytest", "jest", "mocha",
            // 유틸리티
            "echo", "printf", "date", "whoami", "hostname",
            "env", "which", "where", "type",
            "cp", "mv", "mkdir", "touch",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    /// 기본 차단 경로
    fn default_denied_paths() -> Vec<String> {
        vec![
            // Unix 시스템 경로
            "/etc".to_string(),
            "/boot".to_string(),
            "/root".to_string(),
            "/var/log".to_string(),
            "/usr/bin".to_string(),
            "/usr/sbin".to_string(),
            // Windows 시스템 경로
            "C:\\Windows".to_string(),
            "C:\\Windows\\System32".to_string(),
            "C:\\Program Files".to_string(),
            // 민감 파일
            ".ssh".to_string(),
            ".gnupg".to_string(),
            ".aws".to_string(),
            ".azure".to_string(),
            ".kube".to_string(),
            ".docker".to_string(),
            // 환경 설정
            ".env".to_string(),
            ".bashrc".to_string(),
            ".zshrc".to_string(),
            ".profile".to_string(),
        ]
    }

    /// 최소 차단 경로
    fn minimal_denied_paths() -> Vec<String> {
        vec![
            "/etc".to_string(),
            "/boot".to_string(),
            "C:\\Windows\\System32".to_string(),
            ".ssh".to_string(),
        ]
    }

    // Builder 패턴 메서드들

    /// 차단 명령어 추가
    pub fn deny_commands(mut self, commands: Vec<&str>) -> Self {
        for cmd in commands {
            self.denied_commands.insert(cmd.to_string());
        }
        self
    }

    /// 허용 명령어 추가
    pub fn allow_commands(mut self, commands: Vec<&str>) -> Self {
        for cmd in commands {
            self.allowed_commands.insert(cmd.to_string());
        }
        self
    }

    /// 차단 경로 추가
    pub fn deny_paths(mut self, paths: Vec<&str>) -> Self {
        for path in paths {
            self.denied_paths.push(path.to_string());
        }
        self
    }

    /// 허용 경로 추가 (작업 디렉토리)
    pub fn allow_paths(mut self, paths: Vec<&str>) -> Self {
        for path in paths {
            self.allowed_paths.push(path.to_string());
        }
        self
    }

    /// 네트워크 명령어 허용 여부 설정
    pub fn set_allow_network(mut self, allow: bool) -> Self {
        self.allow_network = allow;
        self
    }

    /// 파이프/리다이렉트 허용 여부 설정
    pub fn set_allow_pipe_redirect(mut self, allow: bool) -> Self {
        self.allow_pipe_redirect = allow;
        self
    }

    /// 승인 필요 임계값 설정
    pub fn set_approval_threshold(mut self, level: RiskLevel) -> Self {
        self.approval_threshold = level;
        self
    }

    /// 커스텀 차단 패턴 추가
    pub fn add_custom_deny_pattern(mut self, pattern: &str) -> Self {
        self.custom_deny_patterns.push(pattern.to_string());
        self
    }

    /// 명령어 검증
    pub fn validate(&self, command: &str) -> PolicyResult {
        let command_lower = command.to_lowercase();
        let command_trimmed = command.trim();

        // 1. 빈 명령어 허용
        if command_trimmed.is_empty() {
            return PolicyResult::Allow;
        }

        // 2. 명시적 차단 명령어 검사
        for denied in &self.denied_commands {
            if command_lower.contains(&denied.to_lowercase()) {
                warn!(
                    "Command blocked by policy: '{}' matches denied pattern '{}'",
                    command_trimmed, denied
                );
                return PolicyResult::Deny(format!(
                    "Command contains denied pattern: '{}'",
                    denied
                ));
            }
        }

        // 3. 커스텀 차단 패턴 검사
        for pattern in &self.custom_deny_patterns {
            if let Ok(re) = Regex::new(pattern) {
                if re.is_match(command) {
                    warn!(
                        "Command blocked by custom pattern: '{}' matches '{}'",
                        command_trimmed, pattern
                    );
                    return PolicyResult::Deny(format!(
                        "Command matches custom deny pattern: '{}'",
                        pattern
                    ));
                }
            }
        }

        // 4. 경로 검사
        if let Some(path_issue) = self.check_paths(command) {
            return path_issue;
        }

        // 5. 네트워크 명령어 검사
        if !self.allow_network {
            if let Some(network_issue) = self.check_network_commands(command) {
                return network_issue;
            }
        }

        // 6. 파이프/리다이렉트 검사
        if !self.allow_pipe_redirect {
            if let Some(pipe_issue) = self.check_pipe_redirect(command) {
                return pipe_issue;
            }
        }

        // 7. 위험 수준 평가
        let risk_level = self.assess_risk(command);

        if risk_level >= self.deny_threshold {
            return PolicyResult::Deny(format!(
                "Command risk level {:?} exceeds deny threshold {:?}",
                risk_level, self.deny_threshold
            ));
        }

        if risk_level >= self.approval_threshold {
            return PolicyResult::RequiresApproval(format!(
                "Command risk level {:?} requires approval (threshold: {:?})",
                risk_level, self.approval_threshold
            ));
        }

        // 8. 화이트리스트 검사 (있으면 즉시 허용)
        let base_command = self.extract_base_command(command);
        if self.allowed_commands.contains(&base_command) {
            return PolicyResult::Allow;
        }

        PolicyResult::Allow
    }

    /// 경로 검사
    fn check_paths(&self, command: &str) -> Option<PolicyResult> {
        let command_lower = command.to_lowercase();

        // 허용 경로 우선 검사
        for allowed in &self.allowed_paths {
            if command_lower.contains(&allowed.to_lowercase()) {
                return None; // 허용
            }
        }

        // 차단 경로 검사
        for denied in &self.denied_paths {
            let denied_lower = denied.to_lowercase();

            // 경로가 명령어에 포함되어 있는지 검사
            if command_lower.contains(&denied_lower) {
                // 읽기 명령어는 경고만
                if self.is_read_command(command) {
                    return Some(PolicyResult::RequiresApproval(format!(
                        "Reading from sensitive path: '{}'",
                        denied
                    )));
                }

                // 쓰기/삭제 명령어는 차단
                if self.is_write_command(command) || self.is_delete_command(command) {
                    return Some(PolicyResult::Deny(format!(
                        "Modifying sensitive path not allowed: '{}'",
                        denied
                    )));
                }
            }
        }

        None
    }

    /// 네트워크 명령어 검사
    fn check_network_commands(&self, command: &str) -> Option<PolicyResult> {
        let network_commands = [
            "curl", "wget", "fetch", "nc", "ncat", "netcat", "ssh", "scp",
            "rsync", "ftp", "sftp", "telnet",
        ];

        let base = self.extract_base_command(command);
        for net_cmd in &network_commands {
            if base == *net_cmd {
                return Some(PolicyResult::RequiresApproval(format!(
                    "Network command '{}' requires approval when network access is disabled",
                    net_cmd
                )));
            }
        }

        None
    }

    /// 파이프/리다이렉트 검사
    fn check_pipe_redirect(&self, command: &str) -> Option<PolicyResult> {
        // 파이프
        if command.contains(" | ") {
            return Some(PolicyResult::RequiresApproval(
                "Pipe operator not allowed in restricted mode".to_string()
            ));
        }

        // 리다이렉트
        if command.contains(" > ") || command.contains(" >> ")
            || command.contains(" < ") || command.contains(" 2>")
        {
            return Some(PolicyResult::RequiresApproval(
                "Redirect operator not allowed in restricted mode".to_string()
            ));
        }

        None
    }

    /// 위험 수준 평가
    fn assess_risk(&self, command: &str) -> RiskLevel {
        let command_lower = command.to_lowercase();
        let base = self.extract_base_command(command);

        // Critical 위험 명령어
        let critical_patterns = [
            "rm -rf", "rm -fr", "rm -r -f",
            ":()", "mkfs", "dd if=",
            "format c:", "del /f /s /q c:",
            "> /dev/sd", "chmod 777 /",
        ];
        for pattern in &critical_patterns {
            if command_lower.contains(pattern) {
                return RiskLevel::Critical;
            }
        }

        // High 위험 명령어
        let high_risk_commands = ["rm", "rmdir", "del", "rd", "unlink", "truncate"];
        if high_risk_commands.contains(&base.as_str()) {
            // -r, -f 플래그 확인
            if command_lower.contains(" -r") || command_lower.contains(" -f")
                || command_lower.contains("/s") || command_lower.contains("/q")
            {
                return RiskLevel::High;
            }
            return RiskLevel::Medium;
        }

        // Medium 위험 명령어
        let medium_risk_commands = [
            "git push", "git reset", "git checkout", "git clean",
            "npm publish", "cargo publish",
            "chmod", "chown", "chgrp",
            "kill", "pkill", "killall",
        ];
        for pattern in &medium_risk_commands {
            if command_lower.starts_with(pattern) || command_lower.contains(&format!(" {}", pattern)) {
                return RiskLevel::Medium;
            }
        }

        // Low 위험 명령어
        let low_risk_commands = [
            "mv", "cp", "mkdir", "touch",
            "git add", "git commit",
            "npm install", "yarn add",
        ];
        for pattern in &low_risk_commands {
            if command_lower.starts_with(pattern) || command_lower.contains(&format!(" {}", pattern)) {
                return RiskLevel::Low;
            }
        }

        RiskLevel::Safe
    }

    /// 기본 명령어 추출
    fn extract_base_command(&self, command: &str) -> String {
        command
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_lowercase()
    }

    /// 읽기 명령어인지 확인
    fn is_read_command(&self, command: &str) -> bool {
        let base = self.extract_base_command(command);
        let read_commands = ["cat", "head", "tail", "less", "more", "type", "find", "grep", "ls", "dir"];
        read_commands.contains(&base.as_str())
    }

    /// 쓰기 명령어인지 확인
    fn is_write_command(&self, command: &str) -> bool {
        let base = self.extract_base_command(command);
        let write_commands = ["echo", "printf", "tee", "touch", "cp", "mv"];
        write_commands.contains(&base.as_str())
            || command.contains(" > ")
            || command.contains(" >> ")
    }

    /// 삭제 명령어인지 확인
    fn is_delete_command(&self, command: &str) -> bool {
        let base = self.extract_base_command(command);
        let delete_commands = ["rm", "rmdir", "del", "rd", "unlink", "shred"];
        delete_commands.contains(&base.as_str())
    }
}

/// Task별 권한 정책 설정
#[derive(Debug, Clone)]
pub struct TaskShellPolicy {
    /// 기본 정책
    pub base_policy: ShellPolicy,
    /// Task ID별 커스텀 정책 (override)
    pub task_overrides: std::collections::HashMap<String, ShellPolicy>,
    /// 명령어 실행 히스토리 (감사용)
    pub enable_audit: bool,
}

impl Default for TaskShellPolicy {
    fn default() -> Self {
        Self {
            base_policy: ShellPolicy::default(),
            task_overrides: std::collections::HashMap::new(),
            enable_audit: true,
        }
    }
}

impl TaskShellPolicy {
    pub fn new() -> Self {
        Self::default()
    }

    /// Task별 정책 설정
    pub fn set_task_policy(&mut self, task_id: &str, policy: ShellPolicy) {
        self.task_overrides.insert(task_id.to_string(), policy);
    }

    /// Task ID로 정책 가져오기
    pub fn get_policy(&self, task_id: Option<&str>) -> &ShellPolicy {
        if let Some(id) = task_id {
            self.task_overrides.get(id).unwrap_or(&self.base_policy)
        } else {
            &self.base_policy
        }
    }

    /// 명령어 검증
    pub fn validate(&self, task_id: Option<&str>, command: &str) -> PolicyResult {
        let policy = self.get_policy(task_id);
        policy.validate(command)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dangerous_commands_blocked() {
        let policy = ShellPolicy::default();

        // 위험 명령어 차단
        assert!(matches!(
            policy.validate("rm -rf /"),
            PolicyResult::Deny(_)
        ));
        assert!(matches!(
            policy.validate("rm -rf /*"),
            PolicyResult::Deny(_)
        ));
        assert!(matches!(
            policy.validate(":(){ :|:& };:"),
            PolicyResult::Deny(_)
        ));
        assert!(matches!(
            policy.validate("format c:"),
            PolicyResult::Deny(_)
        ));
    }

    #[test]
    fn test_safe_commands_allowed() {
        let policy = ShellPolicy::default();

        // 안전한 명령어 허용
        assert!(matches!(policy.validate("ls"), PolicyResult::Allow));
        assert!(matches!(policy.validate("pwd"), PolicyResult::Allow));
        assert!(matches!(policy.validate("cargo build"), PolicyResult::Allow));
        assert!(matches!(policy.validate("npm install"), PolicyResult::Allow));
    }

    #[test]
    fn test_sensitive_paths() {
        let policy = ShellPolicy::default();

        // 민감 경로 쓰기 차단
        assert!(matches!(
            policy.validate("rm /etc/passwd"),
            PolicyResult::Deny(_)
        ));

        // 민감 경로 읽기는 승인 필요
        assert!(matches!(
            policy.validate("cat /etc/hosts"),
            PolicyResult::RequiresApproval(_)
        ));
    }

    #[test]
    fn test_risk_assessment() {
        let policy = ShellPolicy::default();

        assert_eq!(policy.assess_risk("ls"), RiskLevel::Safe);
        assert_eq!(policy.assess_risk("cat file.txt"), RiskLevel::Safe);
        assert_eq!(policy.assess_risk("mv a b"), RiskLevel::Low);
        assert_eq!(policy.assess_risk("rm file.txt"), RiskLevel::Medium);
        assert_eq!(policy.assess_risk("rm -r dir"), RiskLevel::High); // -r without -f
        assert_eq!(policy.assess_risk("rm -rf dir"), RiskLevel::Critical); // -rf is always critical
        assert_eq!(policy.assess_risk("rm -rf /"), RiskLevel::Critical);
    }

    #[test]
    fn test_strict_policy() {
        let policy = ShellPolicy::strict();

        // 네트워크 명령어 제한
        assert!(matches!(
            policy.validate("curl http://example.com"),
            PolicyResult::RequiresApproval(_)
        ));

        // 파이프 제한
        assert!(matches!(
            policy.validate("ls | grep foo"),
            PolicyResult::RequiresApproval(_)
        ));
    }

    #[test]
    fn test_permissive_policy() {
        let policy = ShellPolicy::permissive();

        // 대부분 허용
        assert!(matches!(
            policy.validate("curl http://example.com"),
            PolicyResult::Allow
        ));
        assert!(matches!(
            policy.validate("ls | grep foo"),
            PolicyResult::Allow
        ));

        // 하지만 극단적 위험은 여전히 차단
        assert!(matches!(
            policy.validate("rm -rf /"),
            PolicyResult::Deny(_)
        ));
    }

    #[test]
    fn test_custom_policy() {
        let policy = ShellPolicy::default()
            .deny_commands(vec!["my-dangerous-cmd"])
            .allow_paths(vec!["/tmp"])
            .set_allow_network(false);

        // 커스텀 차단
        assert!(matches!(
            policy.validate("my-dangerous-cmd --force"),
            PolicyResult::Deny(_)
        ));

        // /tmp 허용
        assert!(matches!(
            policy.validate("rm /tmp/test.txt"),
            PolicyResult::Allow
        ));
    }

    #[test]
    fn test_task_policy() {
        let mut task_policy = TaskShellPolicy::new();

        // 특정 Task에 엄격한 정책 적용
        task_policy.set_task_policy("sensitive-task", ShellPolicy::strict());

        // 기본 Task - 허용
        assert!(matches!(
            task_policy.validate(None, "curl http://example.com"),
            PolicyResult::Allow
        ));

        // 민감 Task - 승인 필요
        assert!(matches!(
            task_policy.validate(Some("sensitive-task"), "curl http://example.com"),
            PolicyResult::RequiresApproval(_)
        ));
    }
}
