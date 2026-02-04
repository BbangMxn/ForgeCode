//! Error types for forgecmd module
//!
//! ForgeCmdError는 PTY/Shell 실행 관련 세부 에러를 관리합니다.
//! forge_foundation::Error와의 변환을 지원합니다.

use forge_foundation::permission::PermissionAction;
use forge_foundation::Error as FoundationError;
use std::io;
use thiserror::Error;

/// Result type for forgecmd operations
pub type Result<T> = std::result::Result<T, ForgeCmdError>;

/// Errors that can occur during forgecmd operations
#[derive(Error, Debug)]
pub enum ForgeCmdError {
    /// PTY session is not started
    #[error("PTY session not started. Call start_session() first")]
    SessionNotStarted,

    /// PTY session is already running
    #[error("PTY session already running")]
    SessionAlreadyRunning,

    /// Failed to create PTY
    #[error("Failed to create PTY: {0}")]
    PtyCreationFailed(String),

    /// Failed to spawn shell process
    #[error("Failed to spawn shell: {0}")]
    ShellSpawnFailed(String),

    /// Command execution failed
    #[error("Command execution failed: {0}")]
    ExecutionFailed(String),

    /// Command timed out
    #[error("Command timed out after {0} seconds")]
    Timeout(u64),

    /// Forbidden command blocked
    #[error("Forbidden command blocked: {0}")]
    ForbiddenCommand(String),

    /// Permission denied by policy
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    /// Permission requires user approval
    #[error("Permission required for: {action:?}")]
    PermissionRequired {
        action: PermissionAction,
        description: String,
    },

    /// Invalid command syntax
    #[error("Invalid command: {0}")]
    InvalidCommand(String),

    /// Working directory error
    #[error("Working directory error: {0}")]
    WorkingDirectory(String),

    /// Environment variable error
    #[error("Environment variable error: {0}")]
    EnvironmentError(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// Storage/database error
    #[error("Storage error: {0}")]
    StorageError(String),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),
}

impl ForgeCmdError {
    /// Check if this error requires user interaction
    pub fn requires_user_action(&self) -> bool {
        matches!(self, Self::PermissionRequired { .. })
    }

    /// Check if this is a security-related error
    pub fn is_security_error(&self) -> bool {
        matches!(
            self,
            Self::ForbiddenCommand(_) | Self::PermissionDenied(_) | Self::PermissionRequired { .. }
        )
    }

    /// Get a user-friendly message
    pub fn user_message(&self) -> String {
        match self {
            Self::ForbiddenCommand(cmd) => {
                format!("Command '{}' is blocked for security reasons", cmd)
            }
            Self::PermissionDenied(reason) => {
                format!("Permission denied: {}", reason)
            }
            Self::PermissionRequired { description, .. } => {
                format!("User approval required: {}", description)
            }
            Self::Timeout(secs) => {
                format!("Command timed out after {} seconds", secs)
            }
            _ => self.to_string(),
        }
    }
}

// ============================================================================
// forge_foundation::Error 변환
// ============================================================================

impl From<ForgeCmdError> for FoundationError {
    fn from(err: ForgeCmdError) -> Self {
        match err {
            ForgeCmdError::SessionNotStarted => {
                FoundationError::Tool("PTY session not started".to_string())
            }
            ForgeCmdError::SessionAlreadyRunning => {
                FoundationError::Tool("PTY session already running".to_string())
            }
            ForgeCmdError::PtyCreationFailed(msg) => {
                FoundationError::Tool(format!("PTY creation failed: {}", msg))
            }
            ForgeCmdError::ShellSpawnFailed(msg) => {
                FoundationError::Tool(format!("Shell spawn failed: {}", msg))
            }
            ForgeCmdError::ExecutionFailed(msg) => FoundationError::ToolExecution {
                tool: "bash".to_string(),
                message: msg,
            },
            ForgeCmdError::Timeout(secs) => {
                FoundationError::Timeout(format!("Command timed out after {}s", secs))
            }
            ForgeCmdError::ForbiddenCommand(cmd) => {
                FoundationError::PermissionDenied(format!("Forbidden command: {}", cmd))
            }
            ForgeCmdError::PermissionDenied(reason) => FoundationError::PermissionDenied(reason),
            ForgeCmdError::PermissionRequired { description, .. } => {
                FoundationError::PermissionDenied(format!("Approval required: {}", description))
            }
            ForgeCmdError::InvalidCommand(msg) => FoundationError::InvalidInput(msg),
            ForgeCmdError::WorkingDirectory(msg) => {
                FoundationError::Tool(format!("Working directory: {}", msg))
            }
            ForgeCmdError::EnvironmentError(msg) => {
                FoundationError::Tool(format!("Environment: {}", msg))
            }
            ForgeCmdError::ConfigError(msg) => FoundationError::Config(msg),
            ForgeCmdError::StorageError(msg) => FoundationError::Storage(msg),
            ForgeCmdError::Io(e) => FoundationError::Io(e),
            ForgeCmdError::Internal(msg) => FoundationError::Internal(msg),
        }
    }
}

/// Command execution result with detailed info
#[derive(Debug, Clone)]
pub struct CommandResult {
    /// The command that was executed
    pub command: String,

    /// Exit code (None if process was killed)
    pub exit_code: Option<i32>,

    /// Standard output
    pub stdout: String,

    /// Standard error
    pub stderr: String,

    /// Execution duration in milliseconds
    pub duration_ms: u64,

    /// Whether output was truncated
    pub truncated: bool,
}

impl CommandResult {
    /// Check if command succeeded (exit code 0)
    pub fn success(&self) -> bool {
        self.exit_code == Some(0)
    }

    /// Get combined output (stdout + stderr)
    pub fn combined_output(&self) -> String {
        if self.stderr.is_empty() {
            self.stdout.clone()
        } else if self.stdout.is_empty() {
            self.stderr.clone()
        } else {
            format!("{}\n--- stderr ---\n{}", self.stdout, self.stderr)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_requires_user_action() {
        let err = ForgeCmdError::PermissionRequired {
            action: PermissionAction::Execute {
                command: "rm test".into(),
            },
            description: "Delete file".into(),
        };
        assert!(err.requires_user_action());

        let err = ForgeCmdError::ForbiddenCommand("rm -rf /".into());
        assert!(!err.requires_user_action());
    }

    #[test]
    fn test_error_is_security() {
        assert!(ForgeCmdError::ForbiddenCommand("test".into()).is_security_error());
        assert!(ForgeCmdError::PermissionDenied("test".into()).is_security_error());
        assert!(!ForgeCmdError::Timeout(60).is_security_error());
    }

    #[test]
    fn test_command_result() {
        let result = CommandResult {
            command: "ls".into(),
            exit_code: Some(0),
            stdout: "file.txt".into(),
            stderr: String::new(),
            duration_ms: 10,
            truncated: false,
        };
        assert!(result.success());
        assert_eq!(result.combined_output(), "file.txt");
    }
}
