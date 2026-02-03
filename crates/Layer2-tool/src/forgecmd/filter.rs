//! Command filtering and risk analysis for forgecmd
//!
//! This module provides:
//! - Forbidden command detection (always blocked)
//! - Command categorization (ReadOnly, SafeWrite, Caution, etc.)
//! - Risk score calculation (0-10)
//! - Pattern-based allow/deny rules

use crate::forgecmd::config::{pattern_matches, ForgeCmdConfig, RiskThresholds};
use regex::Regex;
use std::collections::HashSet;

/// Command category for risk assessment
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandCategory {
    /// Read-only commands - auto approve (ls, cat, pwd)
    ReadOnly,

    /// Safe write operations - session approve (mkdir, touch, git add)
    SafeWrite,

    /// Caution required - ask each time (rm, mv, git push)
    Caution,

    /// Dangerous commands - block by default (rm -rf, git reset --hard)
    Dangerous,

    /// Forbidden commands - always block (rm -rf /, fork bomb)
    Forbidden,

    /// Interactive programs - special handling (vim, htop)
    Interactive,

    /// Unknown - requires analysis
    Unknown,
}

/// Risk analysis result
#[derive(Debug, Clone)]
pub struct RiskAnalysis {
    /// Command category
    pub category: CommandCategory,

    /// Risk score (0-10)
    pub risk_score: u8,

    /// Whether confirmation is required
    pub requires_confirmation: bool,

    /// Reason for the risk level
    pub reason: Option<String>,

    /// Matched rule (if any)
    pub matched_rule: Option<String>,
}

impl RiskAnalysis {
    fn new(category: CommandCategory, risk_score: u8) -> Self {
        Self {
            category,
            risk_score,
            requires_confirmation: risk_score >= 5,
            reason: None,
            matched_rule: None,
        }
    }

    fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    fn with_rule(mut self, rule: impl Into<String>) -> Self {
        self.matched_rule = Some(rule.into());
        self
    }
}

/// Command filter for security checks
pub struct CommandFilter {
    /// Forbidden patterns (always blocked)
    forbidden_patterns: Vec<String>,

    /// Known dangerous patterns
    dangerous_patterns: Vec<String>,

    /// Read-only commands (safe)
    readonly_commands: HashSet<String>,

    /// Safe write commands
    safe_write_commands: HashSet<String>,

    /// Interactive programs
    interactive_programs: HashSet<String>,

    /// Compiled regex for complex patterns
    forbidden_regex: Vec<Regex>,
}

impl Default for CommandFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandFilter {
    /// Create a new command filter with default rules
    pub fn new() -> Self {
        Self {
            forbidden_patterns: default_forbidden_patterns(),
            dangerous_patterns: default_dangerous_patterns(),
            readonly_commands: default_readonly_commands(),
            safe_write_commands: default_safe_write_commands(),
            interactive_programs: default_interactive_programs(),
            forbidden_regex: compile_forbidden_regex(),
        }
    }

    /// Check if a command is forbidden (always blocked)
    pub fn is_forbidden(&self, command: &str) -> Option<String> {
        let cmd_lower = command.to_lowercase();
        let cmd_trimmed = command.trim();

        // Check forbidden patterns
        for pattern in &self.forbidden_patterns {
            if pattern_matches(pattern, cmd_trimmed) || pattern_matches(pattern, &cmd_lower) {
                return Some(format!("Matches forbidden pattern: {}", pattern));
            }
        }

        // Check forbidden regex
        for regex in &self.forbidden_regex {
            if regex.is_match(cmd_trimmed) {
                return Some(format!("Matches forbidden regex: {}", regex.as_str()));
            }
        }

        // Special checks for critical commands
        if is_dangerous_rm(cmd_trimmed) {
            return Some("Dangerous root deletion command".to_string());
        }

        if self.is_fork_bomb(cmd_trimmed) {
            return Some("Fork bomb detected".to_string());
        }

        if self.is_disk_wipe(cmd_trimmed) {
            return Some("Disk wipe command detected".to_string());
        }

        None
    }

    /// Analyze command risk
    pub fn analyze(&self, command: &str, config: &ForgeCmdConfig) -> RiskAnalysis {
        let cmd_trimmed = command.trim();

        // 1. Check forbidden first
        if let Some(reason) = self.is_forbidden(cmd_trimmed) {
            return RiskAnalysis::new(CommandCategory::Forbidden, 10).with_reason(reason);
        }

        // 2. Check deny rules
        for rule in &config.rules.deny {
            if pattern_matches(&rule.pattern, cmd_trimmed) {
                let reason = rule
                    .reason
                    .clone()
                    .unwrap_or_else(|| "Denied by rule".to_string());
                return RiskAnalysis::new(CommandCategory::Dangerous, 9)
                    .with_reason(reason)
                    .with_rule(&rule.pattern);
            }
        }

        // 3. Check allow rules
        for rule in &config.rules.allow {
            if pattern_matches(&rule.pattern, cmd_trimmed) {
                let risk = match rule.scope.as_deref() {
                    Some("always") => 0,
                    Some("session") => 2,
                    _ => 3,
                };
                return RiskAnalysis::new(CommandCategory::ReadOnly, risk)
                    .with_reason("Allowed by rule")
                    .with_rule(&rule.pattern);
            }
        }

        // 4. Check ask rules
        for rule in &config.rules.ask {
            if pattern_matches(&rule.pattern, cmd_trimmed) {
                let risk = rule.risk.unwrap_or(6);
                return RiskAnalysis::new(CommandCategory::Caution, risk)
                    .with_reason("Requires confirmation")
                    .with_rule(&rule.pattern);
            }
        }

        // 5. Categorize by command type
        self.categorize_command(cmd_trimmed)
    }

    /// Categorize a command based on built-in rules
    fn categorize_command(&self, command: &str) -> RiskAnalysis {
        let first_word = extract_first_word(command);

        // Check read-only
        if self.readonly_commands.contains(first_word) {
            return RiskAnalysis::new(CommandCategory::ReadOnly, 0)
                .with_reason("Read-only command");
        }

        // Check safe write
        if self.safe_write_commands.contains(first_word) {
            return RiskAnalysis::new(CommandCategory::SafeWrite, 3)
                .with_reason("Safe write command");
        }

        // Check interactive
        if self.interactive_programs.contains(first_word) {
            return RiskAnalysis::new(CommandCategory::Interactive, 5)
                .with_reason("Interactive program");
        }

        // Check dangerous patterns
        for pattern in &self.dangerous_patterns {
            if pattern_matches(pattern, command) {
                return RiskAnalysis::new(CommandCategory::Dangerous, 8)
                    .with_reason(format!("Matches dangerous pattern: {}", pattern));
            }
        }

        // Check specific commands
        match first_word {
            // File operations - medium risk
            "rm" => {
                let risk = if command.contains("-r") || command.contains("-f") {
                    7
                } else {
                    5
                };
                RiskAnalysis::new(CommandCategory::Caution, risk).with_reason("File deletion")
            }
            "mv" | "cp" => {
                RiskAnalysis::new(CommandCategory::Caution, 4).with_reason("File operation")
            }
            "chmod" | "chown" => {
                RiskAnalysis::new(CommandCategory::Caution, 5).with_reason("Permission change")
            }

            // Git operations
            "git" => self.analyze_git_command(command),

            // Package managers
            "npm" | "yarn" | "pnpm" | "cargo" | "pip" => {
                self.analyze_package_command(first_word, command)
            }

            // Network
            "curl" | "wget" => {
                if command.contains("|") && (command.contains("sh") || command.contains("bash")) {
                    RiskAnalysis::new(CommandCategory::Dangerous, 9)
                        .with_reason("Remote code execution")
                } else {
                    RiskAnalysis::new(CommandCategory::Caution, 5).with_reason("Network request")
                }
            }

            // Elevated privileges
            "sudo" | "su" | "doas" => {
                RiskAnalysis::new(CommandCategory::Dangerous, 8).with_reason("Elevated privileges")
            }

            // Default - unknown
            _ => RiskAnalysis::new(CommandCategory::Unknown, 4).with_reason("Unknown command"),
        }
    }

    /// Analyze git-specific commands
    fn analyze_git_command(&self, command: &str) -> RiskAnalysis {
        let parts: Vec<&str> = command.split_whitespace().collect();
        let subcommand = parts.get(1).map(|s| *s).unwrap_or("");

        match subcommand {
            // Safe read operations
            "status" | "log" | "diff" | "show" | "branch" | "remote" | "fetch" => {
                RiskAnalysis::new(CommandCategory::ReadOnly, 0).with_reason("Git read operation")
            }

            // Safe write operations
            "add" | "commit" | "stash" | "tag" => {
                RiskAnalysis::new(CommandCategory::SafeWrite, 2).with_reason("Git write operation")
            }

            // Caution
            "push" => {
                if command.contains("--force") && !command.contains("--force-with-lease") {
                    RiskAnalysis::new(CommandCategory::Dangerous, 8)
                        .with_reason("Force push overwrites remote")
                } else {
                    RiskAnalysis::new(CommandCategory::Caution, 5).with_reason("Git push")
                }
            }
            "pull" | "merge" | "rebase" => {
                RiskAnalysis::new(CommandCategory::Caution, 4).with_reason("Git merge operation")
            }

            // Dangerous
            "reset" => {
                if command.contains("--hard") {
                    RiskAnalysis::new(CommandCategory::Dangerous, 8)
                        .with_reason("Hard reset loses changes")
                } else {
                    RiskAnalysis::new(CommandCategory::Caution, 5).with_reason("Git reset")
                }
            }
            "clean" => {
                if command.contains("-f") {
                    RiskAnalysis::new(CommandCategory::Dangerous, 7)
                        .with_reason("Clean deletes untracked files")
                } else {
                    RiskAnalysis::new(CommandCategory::Caution, 4).with_reason("Git clean")
                }
            }
            "checkout" => {
                if command.contains("--") {
                    RiskAnalysis::new(CommandCategory::Caution, 5)
                        .with_reason("Checkout discards changes")
                } else {
                    RiskAnalysis::new(CommandCategory::SafeWrite, 2).with_reason("Git checkout")
                }
            }

            _ => RiskAnalysis::new(CommandCategory::Caution, 4).with_reason("Git operation"),
        }
    }

    /// Analyze package manager commands
    fn analyze_package_command(&self, manager: &str, command: &str) -> RiskAnalysis {
        let is_install = command.contains("install")
            || command.contains("add")
            || command.contains("update")
            || command.contains("upgrade");

        let is_run = command.contains("run") || command.contains("exec");

        let is_publish = command.contains("publish");

        if is_publish {
            RiskAnalysis::new(CommandCategory::Dangerous, 8)
                .with_reason(format!("{} publish", manager))
        } else if is_install {
            RiskAnalysis::new(CommandCategory::Caution, 5)
                .with_reason(format!("{} install", manager))
        } else if is_run {
            RiskAnalysis::new(CommandCategory::Caution, 4).with_reason(format!("{} run", manager))
        } else {
            RiskAnalysis::new(CommandCategory::SafeWrite, 3)
                .with_reason(format!("{} operation", manager))
        }
    }

    // === Private helper methods ===

    fn is_fork_bomb(&self, command: &str) -> bool {
        // Classic bash fork bomb: :(){ :|:& };:
        command.contains(":|:") || command.contains(":&};:")
    }

    fn is_disk_wipe(&self, command: &str) -> bool {
        let cmd = command.to_lowercase();
        (cmd.contains("dd ") && cmd.contains("/dev/"))
            || cmd.contains("mkfs")
            || cmd.contains("> /dev/sd")
            || cmd.contains("> /dev/nvme")
    }
}

/// Determine permission decision based on risk analysis and thresholds
pub fn decide_permission(
    analysis: &RiskAnalysis,
    thresholds: &RiskThresholds,
) -> PermissionDecision {
    match analysis.category {
        CommandCategory::Forbidden => PermissionDecision::Deny(
            analysis
                .reason
                .clone()
                .unwrap_or_else(|| "Forbidden command".to_string()),
        ),

        CommandCategory::Dangerous => {
            if analysis.risk_score >= thresholds.block {
                PermissionDecision::Deny(
                    analysis
                        .reason
                        .clone()
                        .unwrap_or_else(|| "Dangerous command blocked".to_string()),
                )
            } else {
                PermissionDecision::AskUser
            }
        }

        _ => {
            if analysis.risk_score <= thresholds.auto_approve {
                PermissionDecision::Allow
            } else if analysis.risk_score <= thresholds.session_approve {
                PermissionDecision::AllowSession
            } else if analysis.risk_score <= thresholds.always_ask {
                PermissionDecision::AskUser
            } else {
                PermissionDecision::Deny("Risk too high".to_string())
            }
        }
    }
}

/// Permission decision result
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionDecision {
    /// Allow immediately
    Allow,
    /// Allow for this session
    AllowSession,
    /// Ask user for confirmation
    AskUser,
    /// Deny with reason
    Deny(String),
}

// === Default lists ===

fn default_forbidden_patterns() -> Vec<String> {
    // Note: These are exact match patterns (no wildcards for safety)
    // Wildcard patterns in pattern_matches could match unintended commands
    vec![
        ":(){ :|:& };:".to_string(), // Fork bomb
        "chmod -R 777 /".to_string(),
    ]
}

/// Check if command is a dangerous root deletion
fn is_dangerous_rm(command: &str) -> bool {
    let cmd = command.trim().to_lowercase();

    // Must be rm command with -r or -f flags
    if !cmd.starts_with("rm ") {
        return false;
    }
    if !cmd.contains("-r") && !cmd.contains("-f") {
        return false;
    }

    // Extract the path argument(s)
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    for part in parts.iter().skip(1) {
        // Skip flags
        if part.starts_with('-') {
            continue;
        }

        // Check for dangerous root paths
        let dangerous_paths = ["/", "/*", "~", "~/", "$HOME", "$HOME/"];
        if dangerous_paths.contains(part) {
            return true;
        }

        // Allow /tmp and /var/tmp explicitly
        if part.starts_with("/tmp") || part.starts_with("/var/tmp") {
            continue;
        }

        // Block paths that are just root-level directories
        // e.g., /home, /etc, /usr (but not /home/user/project)
        if *part == "/" || *part == "/*" {
            return true;
        }
    }

    false
}

fn default_dangerous_patterns() -> Vec<String> {
    vec![
        "git reset --hard".to_string(),
        "git clean -fd".to_string(),
        "git push --force".to_string(),
        "git stash drop".to_string(),
        "git stash clear".to_string(),
        "DROP TABLE".to_string(),
        "DROP DATABASE".to_string(),
        "TRUNCATE TABLE".to_string(),
        "curl * | sh".to_string(),
        "curl * | bash".to_string(),
        "wget * | sh".to_string(),
        "wget * | bash".to_string(),
    ]
}

fn default_readonly_commands() -> HashSet<String> {
    [
        "ls", "ll", "la", "dir", "cat", "less", "more", "head", "tail", "pwd", "echo", "printf",
        "which", "whereis", "type", "file", "stat", "wc", "grep", "egrep", "fgrep", "rg", "ag",
        "find", "fd", "tree", "df", "du", "free", "top", "ps", "date", "cal", "whoami", "id",
        "uname", "hostname", "env", "printenv", "history",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

fn default_safe_write_commands() -> HashSet<String> {
    [
        "mkdir", "touch", "ln", "cd", "pushd", "popd", "source", "export", "alias", "unalias",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

fn default_interactive_programs() -> HashSet<String> {
    [
        "vim",
        "nvim",
        "vi",
        "nano",
        "emacs",
        "code",
        "htop",
        "btop",
        "top",
        "less",
        "more",
        "man",
        "python",
        "python3",
        "node",
        "irb",
        "ghci",
        "psql",
        "mysql",
        "sqlite3",
        "redis-cli",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

fn compile_forbidden_regex() -> Vec<Regex> {
    let patterns = [
        // Match rm -rf with root path (/ or /*) but NOT paths like /tmp, /home, etc.
        // Uses negative lookahead simulation via explicit exclusion
        r"rm\s+(-[rf]+\s+)+(/\*?|~/?|\$HOME/?)$",
        r">\s*/dev/(sd|nvme|hd)[a-z]",
        r"dd\s+.*of=/dev/(sd|nvme|hd)",
    ];

    patterns.iter().filter_map(|p| Regex::new(p).ok()).collect()
}

/// Extract the first word from a command
fn extract_first_word(command: &str) -> &str {
    command.split_whitespace().next().unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_forbidden_commands() {
        let filter = CommandFilter::new();

        assert!(filter.is_forbidden("rm -rf /").is_some());
        assert!(filter.is_forbidden("rm -rf /*").is_some());
        assert!(filter.is_forbidden(":(){ :|:& };:").is_some());

        // Should not be forbidden
        let result = filter.is_forbidden("rm -rf /tmp/test");
        assert!(
            result.is_none(),
            "rm -rf /tmp/test should be allowed but got: {:?}",
            result
        );
        assert!(filter.is_forbidden("ls -la").is_none());
    }

    #[test]
    fn test_command_categories() {
        let filter = CommandFilter::new();
        let config = ForgeCmdConfig::default();

        let analysis = filter.analyze("ls -la", &config);
        assert_eq!(analysis.category, CommandCategory::ReadOnly);
        assert_eq!(analysis.risk_score, 0);

        let analysis = filter.analyze("mkdir test", &config);
        assert_eq!(analysis.category, CommandCategory::SafeWrite);

        let analysis = filter.analyze("rm file.txt", &config);
        assert_eq!(analysis.category, CommandCategory::Caution);
    }

    #[test]
    fn test_git_commands() {
        let filter = CommandFilter::new();
        let config = ForgeCmdConfig::default();

        let analysis = filter.analyze("git status", &config);
        assert_eq!(analysis.category, CommandCategory::ReadOnly);

        let analysis = filter.analyze("git push --force", &config);
        assert_eq!(analysis.category, CommandCategory::Dangerous);

        let analysis = filter.analyze("git push --force-with-lease", &config);
        assert_eq!(analysis.category, CommandCategory::Caution);
    }

    #[test]
    fn test_permission_decision() {
        let thresholds = RiskThresholds::default();

        let analysis = RiskAnalysis::new(CommandCategory::ReadOnly, 0);
        assert_eq!(
            decide_permission(&analysis, &thresholds),
            PermissionDecision::Allow
        );

        let analysis = RiskAnalysis::new(CommandCategory::Caution, 6);
        assert_eq!(
            decide_permission(&analysis, &thresholds),
            PermissionDecision::AskUser
        );

        let analysis = RiskAnalysis::new(CommandCategory::Forbidden, 10);
        assert!(matches!(
            decide_permission(&analysis, &thresholds),
            PermissionDecision::Deny(_)
        ));
    }
}
