//! Configuration for forgecmd module

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;

/// PTY terminal size configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PtySize {
    /// Number of rows
    pub rows: u16,
    /// Number of columns
    pub cols: u16,
}

impl Default for PtySize {
    fn default() -> Self {
        Self { rows: 24, cols: 80 }
    }
}

/// Permission rule for allow/deny/ask patterns
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRule {
    /// Pattern to match (supports wildcards: *)
    pub pattern: String,

    /// Scope for allow rules: "always", "session", "once"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,

    /// Reason for deny rules
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,

    /// Risk score for ask rules (0-10)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk: Option<u8>,
}

impl PermissionRule {
    /// Create a new allow rule
    pub fn allow(pattern: impl Into<String>, scope: &str) -> Self {
        Self {
            pattern: pattern.into(),
            scope: Some(scope.to_string()),
            reason: None,
            risk: None,
        }
    }

    /// Create a new deny rule
    pub fn deny(pattern: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            pattern: pattern.into(),
            scope: None,
            reason: Some(reason.into()),
            risk: None,
        }
    }

    /// Create a new ask rule
    pub fn ask(pattern: impl Into<String>, risk: u8) -> Self {
        Self {
            pattern: pattern.into(),
            scope: None,
            reason: None,
            risk: Some(risk),
        }
    }
}

/// Permission rules configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRules {
    /// Patterns that are always allowed
    #[serde(default)]
    pub allow: Vec<PermissionRule>,

    /// Patterns that are always denied
    #[serde(default)]
    pub deny: Vec<PermissionRule>,

    /// Patterns that require user confirmation
    #[serde(default)]
    pub ask: Vec<PermissionRule>,
}

/// Risk threshold configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskThresholds {
    /// Risk score below which commands are auto-approved
    pub auto_approve: u8,

    /// Risk score below which session approval is sufficient
    pub session_approve: u8,

    /// Risk score below which user must approve each time
    pub always_ask: u8,

    /// Risk score at or above which commands are blocked
    pub block: u8,
}

impl Default for RiskThresholds {
    fn default() -> Self {
        Self {
            auto_approve: 2,
            session_approve: 5,
            always_ask: 7,
            block: 8,
        }
    }
}

/// Main configuration for forgecmd
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForgeCmdConfig {
    /// Shell to use (default: platform-specific)
    #[serde(default = "default_shell")]
    pub shell: String,

    /// Shell arguments
    #[serde(default = "default_shell_args")]
    pub shell_args: Vec<String>,

    /// PTY size
    #[serde(default)]
    pub pty_size: PtySize,

    /// Command timeout in seconds (default: 60)
    #[serde(default = "default_timeout")]
    pub timeout: u64,

    /// Maximum output size in bytes before truncation
    #[serde(default = "default_max_output")]
    pub max_output_size: usize,

    /// Permission rules
    #[serde(default)]
    pub rules: PermissionRules,

    /// Risk thresholds
    #[serde(default)]
    pub risk_thresholds: RiskThresholds,

    /// Environment variables to block (supports wildcards)
    #[serde(default = "default_blocked_env")]
    pub blocked_env_vars: HashSet<String>,

    /// Allowed working directories (supports wildcards)
    #[serde(default)]
    pub allowed_paths: Vec<String>,

    /// Enable command history tracking
    #[serde(default = "default_true")]
    pub track_history: bool,

    /// Enable ANSI escape stripping for stored output
    #[serde(default = "default_true")]
    pub strip_ansi: bool,
}

fn default_shell() -> String {
    if cfg!(windows) {
        "cmd.exe".to_string()
    } else {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())
    }
}

fn default_shell_args() -> Vec<String> {
    if cfg!(windows) {
        vec![]
    } else {
        vec!["-i".to_string()] // Interactive mode
    }
}

fn default_timeout() -> u64 {
    60
}

fn default_max_output() -> usize {
    100_000 // 100KB
}

fn default_blocked_env() -> HashSet<String> {
    [
        "AWS_*",
        "AZURE_*",
        "GCP_*",
        "GOOGLE_*",
        "*_SECRET",
        "*_TOKEN",
        "*_KEY",
        "*_PASSWORD",
        "GITHUB_TOKEN",
        "OPENAI_API_KEY",
        "ANTHROPIC_API_KEY",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

fn default_true() -> bool {
    true
}

impl Default for ForgeCmdConfig {
    fn default() -> Self {
        Self {
            shell: default_shell(),
            shell_args: default_shell_args(),
            pty_size: PtySize::default(),
            timeout: default_timeout(),
            max_output_size: default_max_output(),
            rules: PermissionRules::default(),
            risk_thresholds: RiskThresholds::default(),
            blocked_env_vars: default_blocked_env(),
            allowed_paths: vec![],
            track_history: true,
            strip_ansi: true,
        }
    }
}

impl ForgeCmdConfig {
    /// Create a new config with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Create config for development (more permissive)
    pub fn development() -> Self {
        let mut config = Self::default();
        config.risk_thresholds.auto_approve = 4;
        config.risk_thresholds.session_approve = 6;
        config
    }

    /// Create config for production (more restrictive)
    pub fn production() -> Self {
        let mut config = Self::default();
        config.risk_thresholds.auto_approve = 1;
        config.risk_thresholds.session_approve = 3;
        config.risk_thresholds.always_ask = 5;
        config
    }

    /// Add an allow rule
    pub fn allow(mut self, pattern: impl Into<String>, scope: &str) -> Self {
        self.rules.allow.push(PermissionRule::allow(pattern, scope));
        self
    }

    /// Add a deny rule
    pub fn deny(mut self, pattern: impl Into<String>, reason: impl Into<String>) -> Self {
        self.rules.deny.push(PermissionRule::deny(pattern, reason));
        self
    }

    /// Add an ask rule
    pub fn ask(mut self, pattern: impl Into<String>, risk: u8) -> Self {
        self.rules.ask.push(PermissionRule::ask(pattern, risk));
        self
    }

    /// Set working directory restrictions
    pub fn allowed_paths(mut self, paths: Vec<String>) -> Self {
        self.allowed_paths = paths;
        self
    }

    /// Check if a path is allowed
    pub fn is_path_allowed(&self, path: &PathBuf) -> bool {
        if self.allowed_paths.is_empty() {
            return true; // No restrictions
        }

        let path_str = path.to_string_lossy();
        for allowed in &self.allowed_paths {
            if pattern_matches(allowed, &path_str) {
                return true;
            }
        }
        false
    }

    /// Check if an environment variable should be blocked
    pub fn is_env_blocked(&self, var_name: &str) -> bool {
        for pattern in &self.blocked_env_vars {
            if pattern_matches(pattern, var_name) {
                return true;
            }
        }
        false
    }
}

/// Simple wildcard pattern matching
pub fn pattern_matches(pattern: &str, value: &str) -> bool {
    if pattern == "*" || pattern == "**" {
        return true;
    }

    if pattern.starts_with('*') && pattern.ends_with('*') {
        let middle = &pattern[1..pattern.len() - 1];
        return value.contains(middle);
    }

    if pattern.starts_with('*') {
        let suffix = &pattern[1..];
        return value.ends_with(suffix);
    }

    if pattern.ends_with('*') {
        let prefix = &pattern[..pattern.len() - 1];
        return value.starts_with(prefix);
    }

    // Contains wildcard in middle
    if let Some(pos) = pattern.find('*') {
        let prefix = &pattern[..pos];
        let suffix = &pattern[pos + 1..];
        return value.starts_with(prefix) && value.ends_with(suffix);
    }

    pattern == value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_matches() {
        assert!(pattern_matches("*", "anything"));
        assert!(pattern_matches("git *", "git status"));
        assert!(pattern_matches("* --help", "ls --help"));
        assert!(pattern_matches("*_TOKEN", "GITHUB_TOKEN"));
        assert!(pattern_matches("AWS_*", "AWS_SECRET_KEY"));
        assert!(!pattern_matches("git *", "cargo build"));
    }

    #[test]
    fn test_env_blocked() {
        let config = ForgeCmdConfig::default();
        assert!(config.is_env_blocked("AWS_SECRET_KEY"));
        assert!(config.is_env_blocked("GITHUB_TOKEN"));
        assert!(config.is_env_blocked("MY_PASSWORD"));
        assert!(!config.is_env_blocked("PATH"));
        assert!(!config.is_env_blocked("HOME"));
    }

    #[test]
    fn test_config_builder() {
        let config = ForgeCmdConfig::new()
            .allow("git status", "always")
            .deny("rm -rf /", "Dangerous")
            .ask("git push *", 5);

        assert_eq!(config.rules.allow.len(), 1);
        assert_eq!(config.rules.deny.len(), 1);
        assert_eq!(config.rules.ask.len(), 1);
    }
}
