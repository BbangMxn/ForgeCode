//! Error Recovery System
//!
//! Provides intelligent error recovery strategies for tool execution failures.
//! Enables the agent to automatically retry, use fallbacks, or ask for help.

use async_trait::async_trait;
use forge_foundation::{Error, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Types of errors that can occur during tool execution
#[derive(Debug, Clone)]
pub enum ToolError {
    /// File not found
    FileNotFound { path: String },

    /// Permission denied
    PermissionDenied { tool: String, action: String },

    /// Command failed
    CommandFailed {
        command: String,
        exit_code: i32,
        stderr: String,
    },

    /// Network error
    NetworkError { url: String, message: String },

    /// Timeout
    Timeout { tool: String, timeout_ms: u64 },

    /// Invalid input
    InvalidInput { tool: String, message: String },

    /// Resource not found
    ResourceNotFound {
        resource_type: String,
        identifier: String,
    },

    /// Edit conflict (old_string not found)
    EditConflict { path: String, old_string: String },

    /// Rate limit
    RateLimited { retry_after_secs: Option<u64> },

    /// Unknown error
    Unknown { message: String },
}

impl ToolError {
    /// Parse error from tool result
    pub fn from_error_string(tool_name: &str, error: &str) -> Self {
        let error_lower = error.to_lowercase();

        // File not found patterns
        if error_lower.contains("no such file")
            || error_lower.contains("file not found")
            || error_lower.contains("does not exist")
        {
            // Try to extract path
            if let Some(path) = Self::extract_path(error) {
                return Self::FileNotFound { path };
            }
        }

        // Permission denied
        if error_lower.contains("permission denied") || error_lower.contains("access denied") {
            return Self::PermissionDenied {
                tool: tool_name.to_string(),
                action: error.to_string(),
            };
        }

        // Edit conflict
        if error_lower.contains("could not find") && error_lower.contains("in file") {
            if let Some(path) = Self::extract_path(error) {
                return Self::EditConflict {
                    path,
                    old_string: Self::extract_quoted(error).unwrap_or_default(),
                };
            }
        }

        // Timeout
        if error_lower.contains("timeout") || error_lower.contains("timed out") {
            return Self::Timeout {
                tool: tool_name.to_string(),
                timeout_ms: 0,
            };
        }

        // Network errors
        if error_lower.contains("network")
            || error_lower.contains("connection")
            || error_lower.contains("dns")
        {
            return Self::NetworkError {
                url: Self::extract_url(error).unwrap_or_default(),
                message: error.to_string(),
            };
        }

        // Rate limiting
        if error_lower.contains("rate limit") || error_lower.contains("too many requests") {
            return Self::RateLimited {
                retry_after_secs: Self::extract_retry_after(error),
            };
        }

        // Default to unknown
        Self::Unknown {
            message: error.to_string(),
        }
    }

    /// Extract file path from error message
    fn extract_path(error: &str) -> Option<String> {
        // Look for common path patterns
        let patterns = [
            r#"["']([^"']+)["']"#, // Quoted paths
            r"`([^`]+)`",          // Backtick paths
            r"path[:\s]+(\S+)",    // "path: /foo/bar"
        ];

        for pattern in patterns {
            if let Ok(re) = regex::Regex::new(pattern) {
                if let Some(caps) = re.captures(error) {
                    if let Some(m) = caps.get(1) {
                        let path = m.as_str();
                        if path.contains('/') || path.contains('\\') {
                            return Some(path.to_string());
                        }
                    }
                }
            }
        }
        None
    }

    /// Extract quoted string from error
    fn extract_quoted(error: &str) -> Option<String> {
        if let Ok(re) = regex::Regex::new(r#"["']([^"']{1,100})["']"#) {
            if let Some(caps) = re.captures(error) {
                return caps.get(1).map(|m| m.as_str().to_string());
            }
        }
        None
    }

    /// Extract URL from error
    fn extract_url(error: &str) -> Option<String> {
        if let Ok(re) = regex::Regex::new(r"https?://[^\s]+") {
            if let Some(m) = re.find(error) {
                return Some(m.as_str().to_string());
            }
        }
        None
    }

    /// Extract retry-after seconds
    fn extract_retry_after(error: &str) -> Option<u64> {
        if let Ok(re) = regex::Regex::new(r"(\d+)\s*(?:seconds?|secs?)") {
            if let Some(caps) = re.captures(error) {
                if let Some(m) = caps.get(1) {
                    return m.as_str().parse().ok();
                }
            }
        }
        None
    }

    /// Get error category for logging
    pub fn category(&self) -> &'static str {
        match self {
            Self::FileNotFound { .. } => "file_not_found",
            Self::PermissionDenied { .. } => "permission_denied",
            Self::CommandFailed { .. } => "command_failed",
            Self::NetworkError { .. } => "network_error",
            Self::Timeout { .. } => "timeout",
            Self::InvalidInput { .. } => "invalid_input",
            Self::ResourceNotFound { .. } => "resource_not_found",
            Self::EditConflict { .. } => "edit_conflict",
            Self::RateLimited { .. } => "rate_limited",
            Self::Unknown { .. } => "unknown",
        }
    }
}

/// Action to take after error recovery analysis
#[derive(Debug, Clone)]
pub enum RecoveryAction {
    /// Retry with modified input
    Retry {
        modified_input: Value,
        reason: String,
    },

    /// Use a different tool
    UseFallback {
        tool: String,
        input: Value,
        reason: String,
    },

    /// Ask user for help
    AskUser { question: String, context: String },

    /// Wait and retry
    WaitAndRetry { delay_ms: u64, reason: String },

    /// Give up with explanation
    GiveUp {
        reason: String,
        suggestions: Vec<String>,
    },
}

/// Strategy for recovering from errors
#[async_trait]
pub trait RecoveryStrategy: Send + Sync {
    /// Name of this strategy
    fn name(&self) -> &str;

    /// Check if this strategy can handle the error
    fn can_handle(&self, error: &ToolError) -> bool;

    /// Attempt recovery
    async fn recover(
        &self,
        tool_name: &str,
        input: &Value,
        error: &ToolError,
        context: &RecoveryContext,
    ) -> Result<RecoveryAction>;

    /// Priority (higher = tried first)
    fn priority(&self) -> u8 {
        50
    }
}

/// Context available to recovery strategies
pub struct RecoveryContext {
    /// Working directory
    pub working_dir: String,

    /// Available tools
    pub available_tools: Vec<String>,

    /// Previous attempts for this operation
    pub attempt_count: usize,

    /// Maximum attempts allowed
    pub max_attempts: usize,

    /// Cached glob results for file search
    pub file_cache: HashMap<String, Vec<String>>,
}

impl RecoveryContext {
    pub fn new(working_dir: &str, available_tools: Vec<String>) -> Self {
        Self {
            working_dir: working_dir.to_string(),
            available_tools,
            attempt_count: 0,
            max_attempts: 3,
            file_cache: HashMap::new(),
        }
    }
}

/// Error recovery manager
pub struct ErrorRecovery {
    /// Registered strategies
    strategies: Vec<Box<dyn RecoveryStrategy>>,

    /// Maximum retry attempts per error
    max_retries: usize,
}

impl ErrorRecovery {
    /// Create with default strategies
    pub fn new() -> Self {
        let mut recovery = Self {
            strategies: Vec::new(),
            max_retries: 3,
        };

        // Register default strategies
        recovery.register(Box::new(FileNotFoundRecovery));
        recovery.register(Box::new(EditConflictRecovery));
        recovery.register(Box::new(RateLimitRecovery));
        recovery.register(Box::new(TimeoutRecovery));
        recovery.register(Box::new(PermissionDeniedRecovery));

        recovery
    }

    /// Register a recovery strategy
    pub fn register(&mut self, strategy: Box<dyn RecoveryStrategy>) {
        self.strategies.push(strategy);
        // Sort by priority (highest first)
        self.strategies
            .sort_by(|a, b| b.priority().cmp(&a.priority()));
    }

    /// Set maximum retries
    pub fn with_max_retries(mut self, max: usize) -> Self {
        self.max_retries = max;
        self
    }

    /// Attempt to recover from an error
    pub async fn handle_error(
        &self,
        tool_name: &str,
        input: &Value,
        error_message: &str,
        context: &mut RecoveryContext,
    ) -> RecoveryAction {
        // Parse the error
        let error = ToolError::from_error_string(tool_name, error_message);

        info!(
            "Handling {} error for tool '{}': {:?}",
            error.category(),
            tool_name,
            error
        );

        // Check attempt count
        if context.attempt_count >= context.max_attempts {
            return RecoveryAction::GiveUp {
                reason: format!("Maximum retry attempts ({}) exceeded", context.max_attempts),
                suggestions: vec![
                    "Check the error message for details".to_string(),
                    "Try a different approach".to_string(),
                ],
            };
        }

        // Find a strategy that can handle this error
        for strategy in &self.strategies {
            if strategy.can_handle(&error) {
                debug!("Trying recovery strategy: {}", strategy.name());

                match strategy.recover(tool_name, input, &error, context).await {
                    Ok(action) => {
                        info!(
                            "Recovery strategy '{}' suggested: {:?}",
                            strategy.name(),
                            action
                        );
                        context.attempt_count += 1;
                        return action;
                    }
                    Err(e) => {
                        warn!("Recovery strategy '{}' failed: {}", strategy.name(), e);
                        continue;
                    }
                }
            }
        }

        // No strategy could handle this
        RecoveryAction::GiveUp {
            reason: format!("No recovery strategy available for: {}", error_message),
            suggestions: self.generate_suggestions(&error),
        }
    }

    /// Generate helpful suggestions based on error type
    fn generate_suggestions(&self, error: &ToolError) -> Vec<String> {
        match error {
            ToolError::FileNotFound { path } => vec![
                format!("Check if '{}' exists", path),
                "Use glob to search for similar files".to_string(),
                "Verify the working directory".to_string(),
            ],
            ToolError::PermissionDenied { .. } => vec![
                "Check file permissions".to_string(),
                "Request elevated permissions".to_string(),
            ],
            ToolError::EditConflict { .. } => vec![
                "Re-read the file to get current content".to_string(),
                "Use a larger context for matching".to_string(),
            ],
            ToolError::NetworkError { .. } => vec![
                "Check network connectivity".to_string(),
                "Verify the URL is correct".to_string(),
            ],
            _ => vec!["Check the error details".to_string()],
        }
    }
}

impl Default for ErrorRecovery {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Built-in Recovery Strategies
// ============================================================================

/// Recovery for file not found errors
pub struct FileNotFoundRecovery;

#[async_trait]
impl RecoveryStrategy for FileNotFoundRecovery {
    fn name(&self) -> &str {
        "file_not_found"
    }

    fn can_handle(&self, error: &ToolError) -> bool {
        matches!(error, ToolError::FileNotFound { .. })
    }

    fn priority(&self) -> u8 {
        80
    }

    async fn recover(
        &self,
        tool_name: &str,
        input: &Value,
        error: &ToolError,
        context: &RecoveryContext,
    ) -> Result<RecoveryAction> {
        let ToolError::FileNotFound { path } = error else {
            return Err(Error::InvalidInput("Not a FileNotFound error".to_string()));
        };

        // Extract filename for search
        let filename = std::path::Path::new(path)
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or(path);

        // Suggest using glob to find similar files
        if context.available_tools.contains(&"glob".to_string()) {
            return Ok(RecoveryAction::UseFallback {
                tool: "glob".to_string(),
                input: json!({
                    "pattern": format!("**/*{}*", filename)
                }),
                reason: format!("File '{}' not found, searching for similar files", path),
            });
        }

        // If we have a cached result, suggest the closest match
        if let Some(similar_files) = context.file_cache.get(filename) {
            if let Some(closest) = similar_files.first() {
                let mut new_input = input.clone();
                if let Some(obj) = new_input.as_object_mut() {
                    obj.insert("path".to_string(), json!(closest));
                    obj.insert("file_path".to_string(), json!(closest));
                }
                return Ok(RecoveryAction::Retry {
                    modified_input: new_input,
                    reason: format!("Trying similar file: {}", closest),
                });
            }
        }

        Ok(RecoveryAction::AskUser {
            question: format!("File '{}' not found. What file did you mean?", path),
            context: format!("The {} tool couldn't find the specified file.", tool_name),
        })
    }
}

/// Recovery for edit conflicts
pub struct EditConflictRecovery;

#[async_trait]
impl RecoveryStrategy for EditConflictRecovery {
    fn name(&self) -> &str {
        "edit_conflict"
    }

    fn can_handle(&self, error: &ToolError) -> bool {
        matches!(error, ToolError::EditConflict { .. })
    }

    fn priority(&self) -> u8 {
        85
    }

    async fn recover(
        &self,
        _tool_name: &str,
        input: &Value,
        error: &ToolError,
        context: &RecoveryContext,
    ) -> Result<RecoveryAction> {
        let ToolError::EditConflict { path, old_string } = error else {
            return Err(Error::InvalidInput("Not an EditConflict error".to_string()));
        };

        // First, suggest re-reading the file
        if context.attempt_count == 0 && context.available_tools.contains(&"read".to_string()) {
            return Ok(RecoveryAction::UseFallback {
                tool: "read".to_string(),
                input: json!({ "file_path": path }),
                reason: format!(
                    "Could not find '{}...' in file. Re-reading to get current content.",
                    &old_string[..old_string.len().min(30)]
                ),
            });
        }

        // Suggest trying with normalized whitespace
        if context.attempt_count == 1 {
            let normalized = old_string
                .lines()
                .map(|l| l.trim())
                .collect::<Vec<_>>()
                .join("\n");

            if normalized != *old_string {
                let mut new_input = input.clone();
                if let Some(obj) = new_input.as_object_mut() {
                    obj.insert("old_string".to_string(), json!(normalized));
                }
                return Ok(RecoveryAction::Retry {
                    modified_input: new_input,
                    reason: "Trying with normalized whitespace".to_string(),
                });
            }
        }

        Ok(RecoveryAction::AskUser {
            question: "The text to replace was not found in the file. Please check the file content and provide the exact text to replace.".to_string(),
            context: format!("File: {}\nSearched for: {}...", path, &old_string[..old_string.len().min(50)]),
        })
    }
}

/// Recovery for rate limiting
pub struct RateLimitRecovery;

#[async_trait]
impl RecoveryStrategy for RateLimitRecovery {
    fn name(&self) -> &str {
        "rate_limit"
    }

    fn can_handle(&self, error: &ToolError) -> bool {
        matches!(error, ToolError::RateLimited { .. })
    }

    fn priority(&self) -> u8 {
        90
    }

    async fn recover(
        &self,
        _tool_name: &str,
        _input: &Value,
        error: &ToolError,
        _context: &RecoveryContext,
    ) -> Result<RecoveryAction> {
        let ToolError::RateLimited { retry_after_secs } = error else {
            return Err(Error::InvalidInput("Not a RateLimited error".to_string()));
        };

        let delay = retry_after_secs.unwrap_or(30) * 1000;

        Ok(RecoveryAction::WaitAndRetry {
            delay_ms: delay,
            reason: format!(
                "Rate limited. Waiting {} seconds before retry.",
                delay / 1000
            ),
        })
    }
}

/// Recovery for timeouts
pub struct TimeoutRecovery;

#[async_trait]
impl RecoveryStrategy for TimeoutRecovery {
    fn name(&self) -> &str {
        "timeout"
    }

    fn can_handle(&self, error: &ToolError) -> bool {
        matches!(error, ToolError::Timeout { .. })
    }

    fn priority(&self) -> u8 {
        60
    }

    async fn recover(
        &self,
        tool_name: &str,
        input: &Value,
        _error: &ToolError,
        context: &RecoveryContext,
    ) -> Result<RecoveryAction> {
        // For bash commands, suggest breaking into smaller parts
        if tool_name == "bash" {
            if let Some(command) = input.get("command").and_then(|v| v.as_str()) {
                if command.contains("&&") || command.contains(";") {
                    return Ok(RecoveryAction::AskUser {
                        question:
                            "Command timed out. Would you like to break it into smaller parts?"
                                .to_string(),
                        context: format!("Command: {}", command),
                    });
                }
            }
        }

        if context.attempt_count < 2 {
            // Retry once with same input
            Ok(RecoveryAction::Retry {
                modified_input: input.clone(),
                reason: "Timeout occurred, retrying...".to_string(),
            })
        } else {
            Ok(RecoveryAction::GiveUp {
                reason: "Operation timed out multiple times".to_string(),
                suggestions: vec![
                    "Try a simpler operation".to_string(),
                    "Check if the target resource is responsive".to_string(),
                ],
            })
        }
    }
}

/// Recovery for permission denied
pub struct PermissionDeniedRecovery;

#[async_trait]
impl RecoveryStrategy for PermissionDeniedRecovery {
    fn name(&self) -> &str {
        "permission_denied"
    }

    fn can_handle(&self, error: &ToolError) -> bool {
        matches!(error, ToolError::PermissionDenied { .. })
    }

    fn priority(&self) -> u8 {
        70
    }

    async fn recover(
        &self,
        _tool_name: &str,
        _input: &Value,
        error: &ToolError,
        _context: &RecoveryContext,
    ) -> Result<RecoveryAction> {
        let ToolError::PermissionDenied { tool, action } = error else {
            return Err(Error::InvalidInput(
                "Not a PermissionDenied error".to_string(),
            ));
        };

        Ok(RecoveryAction::AskUser {
            question: format!(
                "Permission denied for '{}'. Would you like to grant permission?",
                tool
            ),
            context: format!("Action: {}", action),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_parsing() {
        let error =
            ToolError::from_error_string("read", "No such file or directory: '/foo/bar.txt'");
        assert!(matches!(error, ToolError::FileNotFound { .. }));

        let error = ToolError::from_error_string("bash", "Permission denied");
        assert!(matches!(error, ToolError::PermissionDenied { .. }));

        let error = ToolError::from_error_string("edit", "Could not find 'foo' in file");
        assert!(matches!(error, ToolError::EditConflict { .. }));
    }

    #[test]
    fn test_error_category() {
        let error = ToolError::FileNotFound {
            path: "/foo".to_string(),
        };
        assert_eq!(error.category(), "file_not_found");

        let error = ToolError::RateLimited {
            retry_after_secs: Some(30),
        };
        assert_eq!(error.category(), "rate_limited");
    }

    #[tokio::test]
    async fn test_recovery_manager() {
        let recovery = ErrorRecovery::new();
        let mut context =
            RecoveryContext::new("/tmp", vec!["glob".to_string(), "read".to_string()]);

        let action = recovery
            .handle_error(
                "read",
                &json!({"file_path": "/nonexistent.txt"}),
                "No such file or directory: '/nonexistent.txt'",
                &mut context,
            )
            .await;

        // Should suggest using glob to find similar files
        assert!(matches!(action, RecoveryAction::UseFallback { tool, .. } if tool == "glob"));
    }

    #[tokio::test]
    async fn test_max_attempts() {
        let recovery = ErrorRecovery::new();
        let mut context = RecoveryContext::new("/tmp", vec![]);
        context.attempt_count = 10;
        context.max_attempts = 3;

        let action = recovery
            .handle_error("read", &json!({}), "Some error", &mut context)
            .await;

        assert!(matches!(action, RecoveryAction::GiveUp { .. }));
    }
}
