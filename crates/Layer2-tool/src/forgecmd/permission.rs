//! Permission checker for forgecmd
//!
//! This module integrates forgecmd with forge-foundation's PermissionService,
//! bridging the command filter's risk analysis with the Layer1 permission system.

use crate::forgecmd::config::ForgeCmdConfig;
use crate::forgecmd::error::ForgeCmdError;
use crate::forgecmd::filter::{CommandCategory, CommandFilter, PermissionDecision, RiskAnalysis};
use forge_foundation::permission::{
    Permission, PermissionAction, PermissionScope, PermissionService, PermissionStatus,
};
use std::collections::HashMap;
use std::sync::Arc;

/// Tool name used for permission checks
pub const TOOL_NAME: &str = "forgecmd";

/// Permission checker integrating CommandFilter with PermissionService
pub struct PermissionChecker {
    /// Reference to Layer1 PermissionService
    permission_service: Arc<PermissionService>,

    /// Command filter for risk analysis
    filter: CommandFilter,

    /// Configuration
    config: ForgeCmdConfig,

    /// Session-approved command patterns (cached for performance)
    session_patterns: HashMap<String, bool>,
}

impl PermissionChecker {
    /// Create a new permission checker
    pub fn new(permission_service: Arc<PermissionService>, config: ForgeCmdConfig) -> Self {
        Self {
            permission_service,
            filter: CommandFilter::new(),
            config,
            session_patterns: HashMap::new(),
        }
    }

    /// Create with default configuration
    pub fn with_service(permission_service: Arc<PermissionService>) -> Self {
        Self::new(permission_service, ForgeCmdConfig::default())
    }

    /// Check if a command is permitted to execute
    ///
    /// Returns Ok(()) if permitted, Err with appropriate error otherwise
    pub fn check_permission(&mut self, command: &str) -> Result<CheckResult, ForgeCmdError> {
        let analysis = self.filter.analyze(command, &self.config);

        // 1. Forbidden commands are always blocked
        if analysis.category == CommandCategory::Forbidden {
            return Ok(CheckResult::Denied {
                reason: analysis
                    .reason
                    .unwrap_or_else(|| "Forbidden command".to_string()),
            });
        }

        // 2. Check session patterns first (fast path)
        if let Some(&allowed) = self.session_patterns.get(command) {
            if allowed {
                return Ok(CheckResult::Allowed {
                    scope: PermissionScope::Session,
                });
            }
        }

        // 3. Check Layer1 PermissionService
        let action = PermissionAction::Execute {
            command: command.to_string(),
        };
        let status = self.permission_service.check(TOOL_NAME, &action);

        match status {
            PermissionStatus::Granted => {
                return Ok(CheckResult::Allowed {
                    scope: PermissionScope::Permanent,
                });
            }
            PermissionStatus::AutoApproved => {
                return Ok(CheckResult::Allowed {
                    scope: PermissionScope::Permanent,
                });
            }
            PermissionStatus::Denied => {
                return Ok(CheckResult::Denied {
                    reason: "Denied by permission settings".to_string(),
                });
            }
            PermissionStatus::Unknown => {
                // Continue with risk-based analysis
            }
        }

        // 4. Pattern-based permission check
        let decision =
            crate::forgecmd::filter::decide_permission(&analysis, &self.config.risk_thresholds);

        match decision {
            PermissionDecision::Allow => Ok(CheckResult::Allowed {
                scope: PermissionScope::Once,
            }),

            PermissionDecision::AllowSession => {
                // Check if we have session approval for similar patterns
                if self.has_session_pattern_approval(&analysis) {
                    Ok(CheckResult::Allowed {
                        scope: PermissionScope::Session,
                    })
                } else {
                    Ok(CheckResult::NeedsConfirmation { analysis })
                }
            }

            PermissionDecision::AskUser => Ok(CheckResult::NeedsConfirmation { analysis }),

            PermissionDecision::Deny(reason) => Ok(CheckResult::Denied { reason }),
        }
    }

    /// Grant permission for a command
    pub fn grant(&mut self, command: &str, scope: PermissionScope) {
        let action = PermissionAction::Execute {
            command: command.to_string(),
        };

        match scope {
            PermissionScope::Once => {
                // No caching needed for one-time permissions
            }
            PermissionScope::Session => {
                // Cache in session patterns
                self.session_patterns.insert(command.to_string(), true);
                // Also grant in Layer1
                self.permission_service.grant_session(TOOL_NAME, action);
            }
            PermissionScope::Permanent => {
                // Permanent grants go through Layer1
                let permission = Permission {
                    tool_name: TOOL_NAME.to_string(),
                    action,
                    scope: PermissionScope::Permanent,
                };
                self.permission_service.grant(permission);
            }
        }
    }

    /// Grant session permission for a pattern (e.g., "git *")
    pub fn grant_pattern(&mut self, pattern: &str, scope: PermissionScope) {
        self.session_patterns.insert(pattern.to_string(), true);

        // If permanent, we'd need to save this to config
        // For now, only session patterns are supported
        if scope == PermissionScope::Session {
            // Pattern is already stored in session_patterns
        }
    }

    /// Deny a command
    pub fn deny(&mut self, command: &str) {
        self.session_patterns.insert(command.to_string(), false);
    }

    /// Clear session permissions
    pub fn clear_session(&mut self) {
        self.session_patterns.clear();
        self.permission_service.clear_session();
    }

    /// Get risk analysis for a command
    pub fn analyze(&self, command: &str) -> RiskAnalysis {
        self.filter.analyze(command, &self.config)
    }

    /// Check if command matches any session-approved pattern
    fn has_session_pattern_approval(&self, analysis: &RiskAnalysis) -> bool {
        if let Some(ref pattern) = analysis.matched_rule {
            self.session_patterns.get(pattern).copied().unwrap_or(false)
        } else {
            false
        }
    }

    /// Check if a command is forbidden (always blocked)
    pub fn is_forbidden(&self, command: &str) -> Option<String> {
        self.filter.is_forbidden(command)
    }

    /// Update configuration
    pub fn set_config(&mut self, config: ForgeCmdConfig) {
        self.config = config;
    }

    /// Get current configuration
    pub fn config(&self) -> &ForgeCmdConfig {
        &self.config
    }
}

/// Result of permission check
#[derive(Debug, Clone)]
pub enum CheckResult {
    /// Command is allowed
    Allowed { scope: PermissionScope },

    /// Command needs user confirmation
    NeedsConfirmation { analysis: RiskAnalysis },

    /// Command is denied
    Denied { reason: String },
}

impl CheckResult {
    /// Check if the result allows execution
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed { .. })
    }

    /// Check if user confirmation is needed
    pub fn needs_confirmation(&self) -> bool {
        matches!(self, Self::NeedsConfirmation { .. })
    }

    /// Check if the command is denied
    pub fn is_denied(&self) -> bool {
        matches!(self, Self::Denied { .. })
    }

    /// Get denial reason if denied
    pub fn denial_reason(&self) -> Option<&str> {
        match self {
            Self::Denied { reason } => Some(reason),
            _ => None,
        }
    }
}

/// Build a confirmation prompt for user
pub fn build_confirmation_prompt(command: &str, analysis: &RiskAnalysis) -> ConfirmationPrompt {
    let risk_indicator = match analysis.risk_score {
        0..=2 => "🟢",
        3..=4 => "🟡",
        5..=6 => "🟠",
        7..=8 => "🔴",
        _ => "⛔",
    };

    let category_desc = match analysis.category {
        CommandCategory::ReadOnly => "Read-only operation",
        CommandCategory::SafeWrite => "Safe write operation",
        CommandCategory::Caution => "Requires caution",
        CommandCategory::Dangerous => "Dangerous operation",
        CommandCategory::Forbidden => "Forbidden operation",
        CommandCategory::Interactive => "Interactive program",
        CommandCategory::Unknown => "Unknown operation",
    };

    ConfirmationPrompt {
        command: command.to_string(),
        risk_indicator: risk_indicator.to_string(),
        risk_score: analysis.risk_score,
        category: category_desc.to_string(),
        reason: analysis.reason.clone(),
        options: vec![
            ConfirmOption::AllowOnce,
            ConfirmOption::AllowSession,
            ConfirmOption::Deny,
        ],
    }
}

/// Confirmation prompt for user
#[derive(Debug, Clone)]
pub struct ConfirmationPrompt {
    /// The command to execute
    pub command: String,

    /// Risk indicator emoji
    pub risk_indicator: String,

    /// Risk score (0-10)
    pub risk_score: u8,

    /// Category description
    pub category: String,

    /// Reason for risk level
    pub reason: Option<String>,

    /// Available options
    pub options: Vec<ConfirmOption>,
}

impl ConfirmationPrompt {
    /// Format as display string
    pub fn display(&self) -> String {
        let mut output = format!(
            "{} Risk: {}/10 ({})\n",
            self.risk_indicator, self.risk_score, self.category
        );

        output.push_str(&format!("Command: {}\n", self.command));

        if let Some(ref reason) = self.reason {
            output.push_str(&format!("Reason: {}\n", reason));
        }

        output.push_str("\nOptions:\n");
        output.push_str("  [y] Allow once\n");
        output.push_str("  [s] Allow for session\n");
        output.push_str("  [n] Deny\n");

        output
    }
}

/// Confirmation options
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmOption {
    AllowOnce,
    AllowSession,
    AllowPermanent,
    Deny,
}

impl ConfirmOption {
    /// Parse from user input
    pub fn from_input(input: &str) -> Option<Self> {
        match input.trim().to_lowercase().as_str() {
            "y" | "yes" | "1" => Some(Self::AllowOnce),
            "s" | "session" | "2" => Some(Self::AllowSession),
            "a" | "always" | "permanent" | "3" => Some(Self::AllowPermanent),
            "n" | "no" | "deny" | "0" => Some(Self::Deny),
            _ => None,
        }
    }

    /// Convert to PermissionScope
    pub fn to_scope(self) -> Option<PermissionScope> {
        match self {
            Self::AllowOnce => Some(PermissionScope::Once),
            Self::AllowSession => Some(PermissionScope::Session),
            Self::AllowPermanent => Some(PermissionScope::Permanent),
            Self::Deny => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_checker() -> PermissionChecker {
        let service = Arc::new(PermissionService::new());
        PermissionChecker::with_service(service)
    }

    #[test]
    fn test_forbidden_commands_blocked() {
        let mut checker = create_checker();

        let result = checker.check_permission("rm -rf /").unwrap();
        assert!(result.is_denied());
    }

    #[test]
    fn test_readonly_commands_allowed() {
        let mut checker = create_checker();

        let result = checker.check_permission("ls -la").unwrap();
        assert!(result.is_allowed());
    }

    #[test]
    fn test_dangerous_commands_need_confirmation() {
        let mut checker = create_checker();

        let result = checker.check_permission("git reset --hard").unwrap();
        assert!(result.needs_confirmation() || result.is_denied());
    }

    #[test]
    fn test_session_grant() {
        let mut checker = create_checker();

        // First check - needs confirmation
        let result = checker.check_permission("npm install lodash").unwrap();
        assert!(result.needs_confirmation() || result.is_allowed());

        // Grant session permission
        checker.grant("npm install lodash", PermissionScope::Session);

        // Second check - should be allowed
        let result = checker.check_permission("npm install lodash").unwrap();
        assert!(result.is_allowed());
    }

    #[test]
    fn test_confirmation_prompt() {
        let checker = create_checker();
        let analysis = checker.analyze("rm -r ./build");

        let prompt = build_confirmation_prompt("rm -r ./build", &analysis);
        assert!(!prompt.command.is_empty());
        assert!(prompt.risk_score > 0);
    }

    #[test]
    fn test_confirm_option_parsing() {
        assert_eq!(
            ConfirmOption::from_input("y"),
            Some(ConfirmOption::AllowOnce)
        );
        assert_eq!(
            ConfirmOption::from_input("s"),
            Some(ConfirmOption::AllowSession)
        );
        assert_eq!(ConfirmOption::from_input("n"), Some(ConfirmOption::Deny));
        assert_eq!(ConfirmOption::from_input("invalid"), None);
    }
}
