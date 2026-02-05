//! Error recovery mechanisms for agent execution
//!
//! Provides intelligent error handling and recovery strategies:
//! - File not found → search for similar files
//! - Permission denied → request permission
//! - Timeout → retry with extended timeout
//! - Rate limit → wait and retry

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;
use tracing::{debug, info};

/// Error types that can be recovered from
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RecoverableError {
    /// File or path not found
    FileNotFound { path: String },
    /// Permission denied
    PermissionDenied { resource: String },
    /// Timeout occurred
    Timeout { operation: String },
    /// Rate limit exceeded
    RateLimited { retry_after: Option<Duration> },
    /// Network error
    NetworkError { message: String },
    /// Invalid input
    InvalidInput { message: String },
    /// Tool execution failed
    ToolFailed { tool: String, message: String },
    /// Parse error
    ParseError { message: String },
}

impl RecoverableError {
    /// Create from tool error message
    pub fn from_error_message(tool: &str, message: &str) -> Option<Self> {
        let msg = message.to_lowercase();

        if msg.contains("not found") || msg.contains("no such file") {
            // Extract path from message
            let path = Self::extract_path(message).unwrap_or_default();
            return Some(Self::FileNotFound { path });
        }

        if msg.contains("permission denied") || msg.contains("access denied") {
            return Some(Self::PermissionDenied {
                resource: Self::extract_path(message).unwrap_or_default(),
            });
        }

        if msg.contains("timeout") || msg.contains("timed out") {
            return Some(Self::Timeout {
                operation: tool.to_string(),
            });
        }

        if msg.contains("rate limit") || msg.contains("too many requests") {
            return Some(Self::RateLimited { retry_after: None });
        }

        if msg.contains("connection") || msg.contains("network") {
            return Some(Self::NetworkError {
                message: message.to_string(),
            });
        }

        None
    }

    fn extract_path(message: &str) -> Option<String> {
        // Simple heuristic: look for quoted strings or paths
        if let Some(start) = message.find('\'') {
            if let Some(end) = message[start + 1..].find('\'') {
                return Some(message[start + 1..start + 1 + end].to_string());
            }
        }
        if let Some(start) = message.find('"') {
            if let Some(end) = message[start + 1..].find('"') {
                return Some(message[start + 1..start + 1 + end].to_string());
            }
        }
        // Look for path-like strings
        for word in message.split_whitespace() {
            if word.contains('/') || word.contains('\\') {
                return Some(word.trim_matches(|c| c == '\'' || c == '"' || c == ':').to_string());
            }
        }
        None
    }
}

/// Action to take for recovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecoveryAction {
    /// Retry the same operation
    Retry {
        /// Modified input parameters
        modified_input: Option<Value>,
        /// Delay before retry
        delay: Option<Duration>,
    },
    /// Use a fallback tool
    UseFallback {
        /// Fallback tool name
        tool: String,
        /// Input for fallback tool
        input: Value,
    },
    /// Ask user for help
    AskUser {
        /// Question to ask
        question: String,
        /// Suggestions
        suggestions: Vec<String>,
    },
    /// Skip this operation
    Skip {
        /// Reason for skipping
        reason: String,
    },
    /// Give up
    GiveUp {
        /// Reason for giving up
        reason: String,
    },
}

/// Recovery strategy trait
#[async_trait]
pub trait RecoveryStrategy: Send + Sync {
    /// Get strategy name
    fn name(&self) -> &str;

    /// Check if this strategy can handle the error
    fn can_handle(&self, error: &RecoverableError) -> bool;

    /// Attempt recovery
    async fn recover(
        &self,
        error: &RecoverableError,
        context: &RecoveryContext,
    ) -> RecoveryAction;
}

/// Context for recovery operations
#[derive(Debug, Clone)]
pub struct RecoveryContext {
    /// Current working directory
    pub cwd: String,
    /// Original tool call
    pub tool_name: String,
    /// Original input
    pub original_input: Value,
    /// Retry count so far
    pub retry_count: u32,
    /// Max retries allowed
    pub max_retries: u32,
    /// Available files (for suggestions)
    pub available_files: Vec<String>,
}

impl Default for RecoveryContext {
    fn default() -> Self {
        Self {
            cwd: ".".to_string(),
            tool_name: String::new(),
            original_input: Value::Null,
            retry_count: 0,
            max_retries: 3,
            available_files: Vec::new(),
        }
    }
}

/// Error recovery manager
pub struct ErrorRecovery {
    strategies: Vec<Box<dyn RecoveryStrategy>>,
    max_retries: u32,
}

impl ErrorRecovery {
    /// Create with default strategies
    pub fn new() -> Self {
        let mut recovery = Self {
            strategies: Vec::new(),
            max_retries: 3,
        };

        // Register default strategies
        recovery.add_strategy(Box::new(FileNotFoundRecovery::new()));
        recovery.add_strategy(Box::new(TimeoutRecovery::new()));
        recovery.add_strategy(Box::new(RateLimitRecovery::new()));
        recovery.add_strategy(Box::new(NetworkErrorRecovery::new()));

        recovery
    }

    /// Add a recovery strategy
    pub fn add_strategy(&mut self, strategy: Box<dyn RecoveryStrategy>) {
        self.strategies.push(strategy);
    }

    /// Set max retries
    pub fn with_max_retries(mut self, max: u32) -> Self {
        self.max_retries = max;
        self
    }

    /// Handle an error
    pub async fn handle_error(
        &self,
        error: &RecoverableError,
        context: &RecoveryContext,
    ) -> RecoveryAction {
        // Check retry limit
        if context.retry_count >= self.max_retries {
            return RecoveryAction::GiveUp {
                reason: format!("Max retries ({}) exceeded", self.max_retries),
            };
        }

        // Try each strategy
        for strategy in &self.strategies {
            if strategy.can_handle(error) {
                debug!("Using recovery strategy: {}", strategy.name());
                return strategy.recover(error, context).await;
            }
        }

        // No strategy found
        RecoveryAction::GiveUp {
            reason: "No recovery strategy available".to_string(),
        }
    }

    /// Create RecoverableError from tool error
    pub fn classify_error(&self, tool: &str, message: &str) -> Option<RecoverableError> {
        RecoverableError::from_error_message(tool, message)
    }
}

impl Default for ErrorRecovery {
    fn default() -> Self {
        Self::new()
    }
}

// ============== Built-in Recovery Strategies ==============

/// Recovery strategy for file not found errors
pub struct FileNotFoundRecovery {
    /// Maximum Levenshtein distance for suggestions
    max_distance: usize,
}

impl FileNotFoundRecovery {
    pub fn new() -> Self {
        Self { max_distance: 3 }
    }

    fn find_similar_files(&self, target: &str, available: &[String]) -> Vec<String> {
        let target_name = std::path::Path::new(target)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(target);

        let mut matches: Vec<(String, usize)> = available
            .iter()
            .filter_map(|file| {
                let file_name = std::path::Path::new(file)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(file);

                let distance = Self::levenshtein(target_name, file_name);
                if distance <= self.max_distance {
                    Some((file.clone(), distance))
                } else {
                    None
                }
            })
            .collect();

        matches.sort_by_key(|(_, d)| *d);
        matches.into_iter().take(5).map(|(f, _)| f).collect()
    }

    fn levenshtein(a: &str, b: &str) -> usize {
        let a_len = a.chars().count();
        let b_len = b.chars().count();

        if a_len == 0 {
            return b_len;
        }
        if b_len == 0 {
            return a_len;
        }

        let mut matrix = vec![vec![0usize; b_len + 1]; a_len + 1];

        for i in 0..=a_len {
            matrix[i][0] = i;
        }
        for j in 0..=b_len {
            matrix[0][j] = j;
        }

        for (i, ca) in a.chars().enumerate() {
            for (j, cb) in b.chars().enumerate() {
                let cost = if ca == cb { 0 } else { 1 };
                matrix[i + 1][j + 1] = (matrix[i][j + 1] + 1)
                    .min(matrix[i + 1][j] + 1)
                    .min(matrix[i][j] + cost);
            }
        }

        matrix[a_len][b_len]
    }
}

#[async_trait]
impl RecoveryStrategy for FileNotFoundRecovery {
    fn name(&self) -> &str {
        "file_not_found"
    }

    fn can_handle(&self, error: &RecoverableError) -> bool {
        matches!(error, RecoverableError::FileNotFound { .. })
    }

    async fn recover(
        &self,
        error: &RecoverableError,
        context: &RecoveryContext,
    ) -> RecoveryAction {
        if let RecoverableError::FileNotFound { path } = error {
            // Find similar files
            let suggestions = self.find_similar_files(path, &context.available_files);

            if suggestions.len() == 1 {
                // Auto-correct if only one match
                let new_path = &suggestions[0];
                info!("Auto-correcting path: {} -> {}", path, new_path);

                let mut new_input = context.original_input.clone();
                if let Some(obj) = new_input.as_object_mut() {
                    // Update path field
                    if obj.contains_key("path") {
                        obj.insert("path".to_string(), json!(new_path));
                    } else if obj.contains_key("file_path") {
                        obj.insert("file_path".to_string(), json!(new_path));
                    }
                }

                return RecoveryAction::Retry {
                    modified_input: Some(new_input),
                    delay: None,
                };
            } else if !suggestions.is_empty() {
                // Ask user to choose
                return RecoveryAction::AskUser {
                    question: format!(
                        "File '{}' not found. Did you mean one of these?",
                        path
                    ),
                    suggestions,
                };
            }

            // Try glob search
            return RecoveryAction::UseFallback {
                tool: "glob".to_string(),
                input: json!({
                    "pattern": format!("**/*{}*", 
                        std::path::Path::new(path)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("*"))
                }),
            };
        }

        RecoveryAction::GiveUp {
            reason: "Not a file not found error".to_string(),
        }
    }
}

/// Recovery strategy for timeout errors
pub struct TimeoutRecovery {
    timeout_multiplier: f64,
}

impl TimeoutRecovery {
    pub fn new() -> Self {
        Self {
            timeout_multiplier: 2.0,
        }
    }
}

#[async_trait]
impl RecoveryStrategy for TimeoutRecovery {
    fn name(&self) -> &str {
        "timeout"
    }

    fn can_handle(&self, error: &RecoverableError) -> bool {
        matches!(error, RecoverableError::Timeout { .. })
    }

    async fn recover(
        &self,
        _error: &RecoverableError,
        context: &RecoveryContext,
    ) -> RecoveryAction {
        if context.retry_count < 2 {
            // Retry with longer timeout
            let mut new_input = context.original_input.clone();
            if let Some(obj) = new_input.as_object_mut() {
                if let Some(timeout) = obj.get("timeout").and_then(|v| v.as_u64()) {
                    let new_timeout = (timeout as f64 * self.timeout_multiplier) as u64;
                    obj.insert("timeout".to_string(), json!(new_timeout));
                }
            }

            RecoveryAction::Retry {
                modified_input: Some(new_input),
                delay: Some(Duration::from_secs(1)),
            }
        } else {
            RecoveryAction::GiveUp {
                reason: "Operation timed out after multiple retries".to_string(),
            }
        }
    }
}

/// Recovery strategy for rate limit errors
pub struct RateLimitRecovery;

impl RateLimitRecovery {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl RecoveryStrategy for RateLimitRecovery {
    fn name(&self) -> &str {
        "rate_limit"
    }

    fn can_handle(&self, error: &RecoverableError) -> bool {
        matches!(error, RecoverableError::RateLimited { .. })
    }

    async fn recover(
        &self,
        error: &RecoverableError,
        context: &RecoveryContext,
    ) -> RecoveryAction {
        let delay = if let RecoverableError::RateLimited { retry_after } = error {
            retry_after.unwrap_or(Duration::from_secs(5))
        } else {
            Duration::from_secs(5)
        };

        // Exponential backoff
        let actual_delay = Duration::from_secs_f64(
            delay.as_secs_f64() * (1.5f64.powi(context.retry_count as i32)),
        );

        info!("Rate limited, waiting {:?} before retry", actual_delay);

        RecoveryAction::Retry {
            modified_input: None,
            delay: Some(actual_delay),
        }
    }
}

/// Recovery strategy for network errors
pub struct NetworkErrorRecovery;

impl NetworkErrorRecovery {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl RecoveryStrategy for NetworkErrorRecovery {
    fn name(&self) -> &str {
        "network_error"
    }

    fn can_handle(&self, error: &RecoverableError) -> bool {
        matches!(error, RecoverableError::NetworkError { .. })
    }

    async fn recover(
        &self,
        _error: &RecoverableError,
        context: &RecoveryContext,
    ) -> RecoveryAction {
        if context.retry_count < 3 {
            // Simple retry with backoff
            let delay = Duration::from_secs((context.retry_count + 1) as u64 * 2);

            RecoveryAction::Retry {
                modified_input: None,
                delay: Some(delay),
            }
        } else {
            RecoveryAction::GiveUp {
                reason: "Network error persists after retries".to_string(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_classification() {
        let error = RecoverableError::from_error_message(
            "read",
            "Error: File not found: '/path/to/file.txt'",
        );
        assert!(matches!(error, Some(RecoverableError::FileNotFound { .. })));

        let error = RecoverableError::from_error_message(
            "bash",
            "Permission denied: /etc/passwd",
        );
        assert!(matches!(error, Some(RecoverableError::PermissionDenied { .. })));
    }

    #[test]
    fn test_levenshtein() {
        assert_eq!(FileNotFoundRecovery::levenshtein("test", "test"), 0);
        assert_eq!(FileNotFoundRecovery::levenshtein("test", "tests"), 1);
        assert_eq!(FileNotFoundRecovery::levenshtein("test", "best"), 1);
    }

    #[tokio::test]
    async fn test_file_not_found_recovery() {
        let recovery = FileNotFoundRecovery::new();
        // Only provide one similar file to ensure auto-correction (Retry)
        let context = RecoveryContext {
            available_files: vec![
                "src/main.rs".to_string(),
                "src/utils.rs".to_string(), // very different, won't match
            ],
            original_input: json!({"path": "src/mian.rs"}),
            ..Default::default()
        };

        let error = RecoverableError::FileNotFound {
            path: "src/mian.rs".to_string(),
        };

        let action = recovery.recover(&error, &context).await;
        // Should auto-correct to main.rs (only one suggestion)
        assert!(matches!(action, RecoveryAction::Retry { .. }));
    }

    #[tokio::test]
    async fn test_file_not_found_multiple_suggestions() {
        let recovery = FileNotFoundRecovery::new();
        // Provide multiple similar files to trigger AskUser
        let context = RecoveryContext {
            available_files: vec![
                "src/main.rs".to_string(),
                "src/mein.rs".to_string(), // also similar to "mian.rs"
            ],
            original_input: json!({"path": "src/mian.rs"}),
            ..Default::default()
        };

        let error = RecoverableError::FileNotFound {
            path: "src/mian.rs".to_string(),
        };

        let action = recovery.recover(&error, &context).await;
        // Should ask user when multiple suggestions
        assert!(matches!(action, RecoveryAction::AskUser { .. }));
    }
}
