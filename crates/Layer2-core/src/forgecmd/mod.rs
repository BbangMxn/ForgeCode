//! ForgeCmd - PTY-based shell environment for LLM agents
//!
//! This module provides a secure, permission-controlled shell environment
//! for AI agents to execute commands. Unlike Claude Code's stateless approach,
//! ForgeCmd maintains persistent PTY sessions with full terminal emulation.
//!
//! ## Features
//!
//! - **PTY Support**: Full pseudo-terminal for interactive commands (vim, htop, etc.)
//! - **Permission Control**: 5-level risk classification with Layer1 integration
//! - **Command Tracking**: Full history with timing, output, and risk analysis
//! - **Security**: Forbidden command detection, environment filtering
//!
//! ## Quick Start
//!
//! ```rust,ignore
//! use forge_tool::forgecmd::{ForgeCmd, ForgeCmdConfig};
//! use forge_foundation::permission::PermissionService;
//! use std::sync::Arc;
//!
//! // Create with default config
//! let permission_service = Arc::new(PermissionService::new());
//! let forge_cmd = ForgeCmd::new(permission_service)?;
//!
//! // Execute a command
//! let result = forge_cmd.execute("ls -la").await?;
//! println!("Exit code: {:?}", result.exit_code);
//! println!("Output: {}", result.stdout);
//! ```
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                        ForgeCmd                              │
//! │  ┌─────────────┐  ┌──────────────┐  ┌────────────────────┐  │
//! │  │   filter    │  │  permission  │  │      tracker       │  │
//! │  │  (Risk)     │──│  (Layer1)    │──│   (History)        │  │
//! │  └─────────────┘  └──────────────┘  └────────────────────┘  │
//! │         │                │                    │              │
//! │         └────────────────┼────────────────────┘              │
//! │                          ▼                                   │
//! │                    ┌──────────┐                              │
//! │                    │  shell   │                              │
//! │                    │  (PTY)   │                              │
//! │                    └──────────┘                              │
//! └─────────────────────────────────────────────────────────────┘
//! ```

// Submodules
pub mod config;
pub mod error;
pub mod filter;
pub mod permission;
pub mod shell;
pub mod tracker;

// Re-exports
pub use config::{ForgeCmdConfig, PermissionRule, PermissionRules, PtySize, RiskThresholds};
pub use error::{CommandResult, ForgeCmdError};
pub use filter::{CommandCategory, CommandFilter, PermissionDecision, RiskAnalysis};
pub use permission::{CheckResult, ConfirmOption, ConfirmationPrompt, PermissionChecker};
pub use shell::{execute_simple, PtySession, SpawnedCommand};
pub use tracker::{CommandRecord, CommandTracker, ExecutionStatus, TrackerStats};

use forge_foundation::permission::{
    categories, register, PermissionDef, PermissionScope, PermissionService,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

// Track if permissions have been registered (only register once)
static PERMISSIONS_REGISTERED: AtomicBool = AtomicBool::new(false);

/// Register forgecmd permissions with Layer1 PermissionRegistry
///
/// This should be called once during application initialization.
/// It's safe to call multiple times - subsequent calls are no-ops.
pub fn register_permissions() {
    if PERMISSIONS_REGISTERED.swap(true, Ordering::SeqCst) {
        return; // Already registered
    }

    // Basic command execution
    register(
        PermissionDef::new("forgecmd.execute", categories::EXECUTE)
            .risk_level(5)
            .description("Execute shell command in PTY session"),
    );

    // Read-only commands (low risk)
    register(
        PermissionDef::new("forgecmd.readonly", categories::EXECUTE)
            .risk_level(1)
            .description("Execute read-only command (ls, cat, pwd)"),
    );

    // Safe write commands
    register(
        PermissionDef::new("forgecmd.write", categories::EXECUTE)
            .risk_level(4)
            .description("Execute write command (mkdir, touch, git add)"),
    );

    // Caution commands (need confirmation)
    register(
        PermissionDef::new("forgecmd.caution", categories::EXECUTE)
            .risk_level(6)
            .description("Execute command requiring caution (rm, mv, git push)")
            .requires_confirmation(true),
    );

    // Interactive programs
    register(
        PermissionDef::new("forgecmd.interactive", categories::EXECUTE)
            .risk_level(7)
            .description("Run interactive program (vim, htop, python REPL)")
            .requires_confirmation(true),
    );

    // Dangerous commands
    register(
        PermissionDef::new("forgecmd.dangerous", categories::EXECUTE)
            .risk_level(9)
            .description("Execute potentially destructive command")
            .requires_confirmation(true),
    );

    // Forbidden commands (always blocked)
    register(
        PermissionDef::new("forgecmd.forbidden", categories::EXECUTE)
            .risk_level(10)
            .description("Forbidden command (rm -rf /, fork bomb)"),
    );
}

/// Get the permission name for a command category
pub fn permission_name_for_category(category: CommandCategory) -> &'static str {
    match category {
        CommandCategory::ReadOnly => "forgecmd.readonly",
        CommandCategory::SafeWrite => "forgecmd.write",
        CommandCategory::Caution => "forgecmd.caution",
        CommandCategory::Dangerous => "forgecmd.dangerous",
        CommandCategory::Forbidden => "forgecmd.forbidden",
        CommandCategory::Interactive => "forgecmd.interactive",
        CommandCategory::Unknown => "forgecmd.execute",
    }
}

/// ForgeCmd - Main entry point for command execution
///
/// Combines all components (filter, permission, tracker, shell) into
/// a unified interface for secure command execution.
pub struct ForgeCmd {
    /// Permission checker (integrates with Layer1)
    permission_checker: PermissionChecker,

    /// Command tracker
    tracker: CommandTracker,

    /// PTY session (lazy initialized)
    session: Option<PtySession>,

    /// Configuration
    config: ForgeCmdConfig,

    /// Current working directory
    working_dir: PathBuf,

    /// Session ID
    session_id: String,
}

impl ForgeCmd {
    /// Create a new ForgeCmd instance
    pub fn new(permission_service: Arc<PermissionService>) -> Result<Self, ForgeCmdError> {
        // Register permissions with Layer1 (idempotent)
        register_permissions();

        let config = ForgeCmdConfig::default();
        let working_dir = std::env::current_dir().map_err(|e| {
            ForgeCmdError::WorkingDirectory(format!("Failed to get working directory: {}", e))
        })?;
        let session_id = generate_session_id();

        Ok(Self {
            permission_checker: PermissionChecker::new(
                Arc::clone(&permission_service),
                config.clone(),
            ),
            tracker: CommandTracker::new(&session_id),
            session: None,
            config,
            working_dir,
            session_id,
        })
    }

    /// Create with custom configuration
    pub fn with_config(
        permission_service: Arc<PermissionService>,
        config: ForgeCmdConfig,
    ) -> Result<Self, ForgeCmdError> {
        let working_dir = std::env::current_dir().map_err(|e| {
            ForgeCmdError::WorkingDirectory(format!("Failed to get working directory: {}", e))
        })?;
        let session_id = generate_session_id();

        Ok(Self {
            permission_checker: PermissionChecker::new(
                Arc::clone(&permission_service),
                config.clone(),
            ),
            tracker: CommandTracker::new(&session_id),
            session: None,
            config,
            working_dir,
            session_id,
        })
    }

    /// Execute a command with permission checks
    ///
    /// This is the main entry point for command execution.
    /// Returns an error if permission is denied or confirmation is required.
    pub async fn execute(&mut self, command: &str) -> Result<CommandResult, ForgeCmdError> {
        // 1. Check permission
        let check_result = self.permission_checker.check_permission(command)?;

        match check_result {
            CheckResult::Allowed { .. } => {
                // Permission granted, execute
                self.execute_internal(command).await
            }
            CheckResult::NeedsConfirmation { analysis } => {
                // Confirmation required
                Err(ForgeCmdError::PermissionRequired {
                    action: forge_foundation::permission::PermissionAction::Execute {
                        command: command.to_string(),
                    },
                    description: analysis.reason.unwrap_or_else(|| {
                        format!(
                            "Command requires confirmation (risk: {})",
                            analysis.risk_score
                        )
                    }),
                })
            }
            CheckResult::Denied { reason } => {
                // Denied
                let _record_id = self.tracker.record_denied(
                    command,
                    self.working_dir.to_string_lossy().as_ref(),
                    &reason,
                );
                Err(ForgeCmdError::PermissionDenied(reason))
            }
        }
    }

    /// Execute with explicit user confirmation
    ///
    /// Use this when the user has already approved the command.
    pub async fn execute_with_confirmation(
        &mut self,
        command: &str,
        scope: PermissionScope,
    ) -> Result<CommandResult, ForgeCmdError> {
        // Grant permission based on scope
        self.permission_checker.grant(command, scope);

        // Execute
        self.execute_internal(command).await
    }

    /// Execute without permission checks (use with caution!)
    ///
    /// Only use this for commands that are known to be safe.
    pub async fn execute_unchecked(
        &mut self,
        command: &str,
    ) -> Result<CommandResult, ForgeCmdError> {
        self.execute_internal(command).await
    }

    /// Internal execution (after permission checks)
    async fn execute_internal(&mut self, command: &str) -> Result<CommandResult, ForgeCmdError> {
        let analysis = self.permission_checker.analyze(command);
        let working_dir_str = self.working_dir.to_string_lossy().to_string();

        // Start tracking
        let record_id = self.tracker.start(command, &working_dir_str, &analysis);

        // Execute based on category
        let result = match analysis.category {
            CommandCategory::Interactive => {
                // Use PTY for interactive commands
                self.execute_with_pty(command).await
            }
            _ => {
                // Use simple execution for non-interactive
                self.execute_simple(command).await
            }
        };

        // Update tracker
        match &result {
            Ok(cmd_result) => {
                let exit_code = cmd_result.exit_code.unwrap_or(-1);
                if cmd_result.success() {
                    self.tracker.complete_success(
                        &record_id,
                        exit_code,
                        &cmd_result.stdout,
                        &cmd_result.stderr,
                    );
                } else {
                    self.tracker.complete_failed(
                        &record_id,
                        exit_code,
                        &cmd_result.stdout,
                        &cmd_result.stderr,
                    );
                }
            }
            Err(e) => {
                if matches!(e, ForgeCmdError::Timeout { .. }) {
                    self.tracker.complete_timeout(&record_id, "", "");
                } else {
                    self.tracker.mark_denied(&record_id, &e.to_string());
                }
            }
        }

        result
    }

    /// Execute using PTY session
    async fn execute_with_pty(&mut self, command: &str) -> Result<CommandResult, ForgeCmdError> {
        // Initialize session if needed
        if self.session.is_none() {
            self.session = Some(PtySession::new(
                self.config.clone(),
                self.working_dir.clone(),
            )?);
        }

        let session = self.session.as_mut().unwrap();
        session.execute(command)
    }

    /// Execute using simple process spawn
    async fn execute_simple(&self, command: &str) -> Result<CommandResult, ForgeCmdError> {
        execute_simple(
            command,
            &self.working_dir,
            Duration::from_secs(self.config.timeout),
        )
    }

    /// Check if a command would be allowed (without executing)
    pub fn check(&mut self, command: &str) -> Result<CheckResult, ForgeCmdError> {
        self.permission_checker.check_permission(command)
    }

    /// Get risk analysis for a command
    pub fn analyze(&self, command: &str) -> RiskAnalysis {
        self.permission_checker.analyze(command)
    }

    /// Build a confirmation prompt for the user
    pub fn build_prompt(&self, command: &str) -> ConfirmationPrompt {
        let analysis = self.permission_checker.analyze(command);
        permission::build_confirmation_prompt(command, &analysis)
    }

    /// Grant permission for a command
    pub fn grant(&mut self, command: &str, scope: PermissionScope) {
        self.permission_checker.grant(command, scope);
    }

    /// Grant permission for a pattern
    pub fn grant_pattern(&mut self, pattern: &str, scope: PermissionScope) {
        self.permission_checker.grant_pattern(pattern, scope);
    }

    /// Deny a command
    pub fn deny(&mut self, command: &str) {
        self.permission_checker.deny(command);
    }

    /// Set working directory
    pub fn set_working_dir(&mut self, dir: PathBuf) -> Result<(), ForgeCmdError> {
        if !dir.exists() {
            return Err(ForgeCmdError::WorkingDirectory(format!(
                "Directory does not exist: {}",
                dir.display()
            )));
        }
        self.working_dir = dir.clone();
        if let Some(ref mut session) = self.session {
            session.set_working_dir(dir);
        }
        Ok(())
    }

    /// Get current working directory
    pub fn working_dir(&self) -> &PathBuf {
        &self.working_dir
    }

    /// Get command history
    pub fn history(&self) -> Vec<CommandRecord> {
        self.tracker.get_all()
    }

    /// Get recent commands
    pub fn recent_commands(&self, count: usize) -> Vec<CommandRecord> {
        self.tracker.get_recent(count)
    }

    /// Get tracker statistics
    pub fn stats(&self) -> TrackerStats {
        self.tracker.stats()
    }

    /// Search command history
    pub fn search_history(&self, pattern: &str) -> Vec<CommandRecord> {
        self.tracker.search(pattern)
    }

    /// Get session ID
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Clear session permissions
    pub fn clear_session_permissions(&mut self) {
        self.permission_checker.clear_session();
    }

    /// Update configuration
    pub fn set_config(&mut self, config: ForgeCmdConfig) {
        self.config = config.clone();
        self.permission_checker.set_config(config);
    }

    /// Get current configuration
    pub fn config(&self) -> &ForgeCmdConfig {
        &self.config
    }

    /// Close the session
    pub fn close(&mut self) {
        if let Some(ref mut session) = self.session {
            session.close();
        }
        self.session = None;
    }
}

impl Drop for ForgeCmd {
    fn drop(&mut self) {
        self.close();
    }
}

/// Generate a unique session ID
fn generate_session_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);

    format!("forgecmd-{:x}", timestamp)
}

/// Builder for ForgeCmd with fluent API
pub struct ForgeCmdBuilder {
    permission_service: Option<Arc<PermissionService>>,
    config: ForgeCmdConfig,
    working_dir: Option<PathBuf>,
}

impl ForgeCmdBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            permission_service: None,
            config: ForgeCmdConfig::default(),
            working_dir: None,
        }
    }

    /// Set permission service
    pub fn permission_service(mut self, service: Arc<PermissionService>) -> Self {
        self.permission_service = Some(service);
        self
    }

    /// Set configuration
    pub fn config(mut self, config: ForgeCmdConfig) -> Self {
        self.config = config;
        self
    }

    /// Set working directory
    pub fn working_dir(mut self, dir: PathBuf) -> Self {
        self.working_dir = Some(dir);
        self
    }

    /// Set shell
    pub fn shell(mut self, shell: &str) -> Self {
        self.config.shell = shell.to_string();
        self
    }

    /// Set timeout in seconds
    pub fn timeout(mut self, timeout_secs: u64) -> Self {
        self.config.timeout = timeout_secs;
        self
    }

    /// Set PTY size
    pub fn pty_size(mut self, rows: u16, cols: u16) -> Self {
        self.config.pty_size = PtySize { rows, cols };
        self
    }

    /// Build ForgeCmd instance
    pub fn build(self) -> Result<ForgeCmd, ForgeCmdError> {
        let permission_service = self
            .permission_service
            .unwrap_or_else(|| Arc::new(PermissionService::new()));

        let working_dir = self
            .working_dir
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

        let session_id = generate_session_id();

        Ok(ForgeCmd {
            permission_checker: PermissionChecker::new(
                Arc::clone(&permission_service),
                self.config.clone(),
            ),
            tracker: CommandTracker::new(&session_id),
            session: None,
            config: self.config,
            working_dir,
            session_id,
        })
    }
}

impl Default for ForgeCmdBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_forge_cmd() -> ForgeCmd {
        ForgeCmdBuilder::new().build().unwrap()
    }

    #[test]
    fn test_builder() {
        let cmd = ForgeCmdBuilder::new()
            .shell("bash")
            .timeout(10)
            .pty_size(24, 80)
            .build()
            .unwrap();

        assert_eq!(cmd.config().shell, "bash");
        assert_eq!(cmd.config().timeout, 10);
    }

    #[test]
    fn test_check_readonly() {
        let mut cmd = create_forge_cmd();
        let result = cmd.check("ls -la").unwrap();
        assert!(result.is_allowed());
    }

    #[test]
    fn test_check_forbidden() {
        let mut cmd = create_forge_cmd();
        let result = cmd.check("rm -rf /").unwrap();
        assert!(result.is_denied());
    }

    #[test]
    fn test_analyze() {
        let cmd = create_forge_cmd();

        let analysis = cmd.analyze("ls -la");
        assert_eq!(analysis.category, CommandCategory::ReadOnly);
        assert_eq!(analysis.risk_score, 0);

        let analysis = cmd.analyze("git reset --hard");
        assert_eq!(analysis.category, CommandCategory::Dangerous);
        assert!(analysis.risk_score >= 7);
    }

    #[test]
    fn test_build_prompt() {
        let cmd = create_forge_cmd();
        let prompt = cmd.build_prompt("rm -rf ./build");

        assert!(!prompt.command.is_empty());
        assert!(prompt.risk_score > 0);
    }

    #[tokio::test]
    async fn test_execute_simple() {
        let mut cmd = create_forge_cmd();
        let result = cmd.execute("echo hello").await.unwrap();

        assert!(result.success());
        assert!(result.stdout.contains("hello"));
    }

    #[test]
    fn test_session_id() {
        let cmd = create_forge_cmd();
        assert!(cmd.session_id().starts_with("forgecmd-"));
    }
}
