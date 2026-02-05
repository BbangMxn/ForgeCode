//! Message history management
//!
//! Optimized for performance with:
//! - Token estimation caching (thread-safe with atomics)
//! - Efficient message access (no cloning)
//! - Arc-based history sharing

use forge_provider::{Message, MessageRole, ToolCall};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Message history for a session
///
/// Performance optimizations:
/// - Cached token estimation (invalidated on mutation)
/// - `messages()` returns slice instead of cloning
/// - Capacity hints for message vectors
/// - Thread-safe cache using atomics (Sync + Send)
#[derive(Debug, Default)]
pub struct MessageHistory {
    /// Messages in order
    messages: Vec<Message>,

    /// System prompt
    system_prompt: Option<String>,

    /// Cached token count (0 = invalid, needs recalculation)
    #[cfg(not(feature = "no-cache"))]
    cached_tokens: AtomicUsize,

    /// Token cache valid flag
    #[cfg(not(feature = "no-cache"))]
    cache_valid: AtomicBool,
}

// Manual Clone implementation since Atomics don't derive Clone
impl Clone for MessageHistory {
    fn clone(&self) -> Self {
        Self {
            messages: self.messages.clone(),
            system_prompt: self.system_prompt.clone(),
            #[cfg(not(feature = "no-cache"))]
            cached_tokens: AtomicUsize::new(self.cached_tokens.load(Ordering::Relaxed)),
            #[cfg(not(feature = "no-cache"))]
            cache_valid: AtomicBool::new(self.cache_valid.load(Ordering::Relaxed)),
        }
    }
}

impl MessageHistory {
    /// Create a new empty history
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with pre-allocated capacity for expected message count
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            messages: Vec::with_capacity(capacity),
            system_prompt: None,
            #[cfg(not(feature = "no-cache"))]
            cached_tokens: AtomicUsize::new(0),
            #[cfg(not(feature = "no-cache"))]
            cache_valid: AtomicBool::new(false),
        }
    }

    /// Create with a system prompt
    pub fn with_system_prompt(prompt: impl Into<String>) -> Self {
        Self {
            messages: Vec::with_capacity(32), // Typical conversation size
            system_prompt: Some(prompt.into()),
            #[cfg(not(feature = "no-cache"))]
            cached_tokens: AtomicUsize::new(0),
            #[cfg(not(feature = "no-cache"))]
            cache_valid: AtomicBool::new(false),
        }
    }

    /// Invalidate token cache (called on any mutation)
    #[inline]
    fn invalidate_cache(&self) {
        #[cfg(not(feature = "no-cache"))]
        self.cache_valid.store(false, Ordering::Release);
    }

    /// Set system prompt
    pub fn set_system_prompt(&mut self, prompt: impl Into<String>) {
        self.system_prompt = Some(prompt.into());
        self.invalidate_cache();
    }

    /// Get system prompt
    #[inline]
    pub fn system_prompt(&self) -> Option<&str> {
        self.system_prompt.as_deref()
    }

    /// Add a user message
    pub fn add_user(&mut self, content: impl Into<String>) {
        self.messages.push(Message::user(content));
        self.invalidate_cache();
    }

    /// Add an assistant message
    pub fn add_assistant(&mut self, content: impl Into<String>) {
        self.messages.push(Message::assistant(content));
        self.invalidate_cache();
    }

    /// Add an assistant message with tool calls
    pub fn add_assistant_with_tools(&mut self, content: impl Into<String>, tool_calls: Vec<ToolCall>) {
        self.messages
            .push(Message::assistant_with_tools(content, tool_calls));
        self.invalidate_cache();
    }

    /// Add a tool result
    pub fn add_tool_result(&mut self, tool_call_id: impl Into<String>, content: impl Into<String>, is_error: bool) {
        self.messages
            .push(Message::tool(tool_call_id, content, is_error));
        self.invalidate_cache();
    }

    /// Add a message directly
    pub fn add(&mut self, message: Message) {
        self.messages.push(message);
        self.invalidate_cache();
    }

    /// Get all messages as a slice (no allocation)
    #[inline]
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// Get messages as owned vec (only when ownership is needed)
    ///
    /// **Performance Note:** This clones all messages.
    /// Prefer `messages()` for read-only access.
    pub fn to_messages(&self) -> Vec<Message> {
        self.messages.clone()
    }

    /// Take ownership of messages, leaving empty history
    ///
    /// Use this when you need owned messages and will discard the history.
    pub fn take_messages(&mut self) -> Vec<Message> {
        self.invalidate_cache();
        std::mem::take(&mut self.messages)
    }

    /// Get message count
    #[inline]
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// Check if empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// Clear all messages
    pub fn clear(&mut self) {
        self.messages.clear();
        self.invalidate_cache();
    }

    /// Get the last message
    #[inline]
    pub fn last(&self) -> Option<&Message> {
        self.messages.last()
    }

    /// Get the last user message
    pub fn last_user(&self) -> Option<&Message> {
        self.messages
            .iter()
            .rev()
            .find(|m| m.role == MessageRole::User)
    }

    /// Get the last assistant message
    pub fn last_assistant(&self) -> Option<&Message> {
        self.messages
            .iter()
            .rev()
            .find(|m| m.role == MessageRole::Assistant)
    }

    /// Estimate token count with caching
    ///
    /// Uses a 4 chars/token approximation (accurate for English).
    /// The result is cached until the history is modified.
    pub fn estimate_tokens(&self) -> usize {
        #[cfg(not(feature = "no-cache"))]
        {
            if self.cache_valid.load(Ordering::Acquire) {
                return self.cached_tokens.load(Ordering::Relaxed);
            }
        }

        let tokens = self.calculate_tokens();

        #[cfg(not(feature = "no-cache"))]
        {
            self.cached_tokens.store(tokens, Ordering::Relaxed);
            self.cache_valid.store(true, Ordering::Release);
        }

        tokens
    }

    /// Calculate tokens without caching (internal)
    fn calculate_tokens(&self) -> usize {
        let mut tokens = 0;

        // System prompt
        if let Some(ref prompt) = self.system_prompt {
            tokens += prompt.len() / 4;
        }

        // Messages - use byte length directly (avoid allocations)
        for msg in &self.messages {
            tokens += msg.content.len() / 4;

            // Tool calls - use existing JSON string length
            if let Some(ref tool_calls) = msg.tool_calls {
                for tc in tool_calls {
                    tokens += tc.name.len() / 4;
                    // arguments is already a serde_json::Value
                    // Estimate based on typical JSON structure
                    tokens += estimate_json_tokens(&tc.arguments);
                }
            }

            // Tool results
            if let Some(ref result) = msg.tool_result {
                tokens += result.content.len() / 4;
            }
        }

        // Add overhead for message formatting (roles, separators)
        tokens += self.messages.len() * 4;

        tokens
    }

    /// Summarize history to reduce tokens
    pub fn summarize(&mut self, summary: impl Into<String>) {
        let summary = summary.into();

        // Keep system prompt, clear messages, add summary as context
        self.messages.clear();
        self.messages.push(Message::user(format!(
            "Previous conversation summary:\n{}\n\nPlease continue from here.",
            summary
        )));
        self.invalidate_cache();
    }

    /// Reserve capacity for additional messages
    pub fn reserve(&mut self, additional: usize) {
        self.messages.reserve(additional);
    }
}

/// Estimate tokens for a JSON value without serialization
#[inline]
fn estimate_json_tokens(value: &serde_json::Value) -> usize {
    match value {
        serde_json::Value::Null => 1,
        serde_json::Value::Bool(_) => 1,
        serde_json::Value::Number(n) => {
            // Numbers are typically 1-4 tokens
            let s = n.to_string();
            (s.len() / 4).max(1)
        }
        serde_json::Value::String(s) => s.len() / 4 + 1,
        serde_json::Value::Array(arr) => {
            let mut tokens = 2; // brackets
            for item in arr {
                tokens += estimate_json_tokens(item);
            }
            tokens
        }
        serde_json::Value::Object(obj) => {
            let mut tokens = 2; // braces
            for (key, val) in obj {
                tokens += key.len() / 4 + 1; // key
                tokens += estimate_json_tokens(val); // value
            }
            tokens
        }
    }
}

// ============================================================================
// History Manager
// ============================================================================

/// History manager for multiple sessions
///
/// Uses Arc<MessageHistory> to avoid cloning entire histories.
pub struct HistoryManager {
    histories: Arc<RwLock<HashMap<Arc<str>, Arc<RwLock<MessageHistory>>>>>,
}

impl HistoryManager {
    /// Create a new history manager
    pub fn new() -> Self {
        Self {
            histories: Arc::new(RwLock::new(HashMap::with_capacity(16))),
        }
    }

    /// Get or create history for a session
    ///
    /// Returns a clone of the history. For shared access, use `get_shared()`.
    pub async fn get_or_create(&self, session_id: &str) -> MessageHistory {
        // First try read lock
        {
            let histories = self.histories.read().await;
            if let Some(history) = histories.get(session_id) {
                return history.read().await.clone();
            }
        }

        // Need to create - acquire write lock
        let mut histories = self.histories.write().await;

        // Double-check after acquiring write lock
        if let Some(history) = histories.get(session_id) {
            return history.read().await.clone();
        }

        // Create new history
        let history = MessageHistory::with_capacity(32);
        let session_key: Arc<str> = session_id.into();
        histories.insert(session_key, Arc::new(RwLock::new(history.clone())));
        history
    }

    /// Get shared reference to history (avoids clone)
    pub async fn get_shared(&self, session_id: &str) -> Option<Arc<RwLock<MessageHistory>>> {
        let histories = self.histories.read().await;
        histories.get(session_id).cloned()
    }

    /// Update history for a session
    pub async fn update(&self, session_id: &str, history: MessageHistory) {
        let mut histories = self.histories.write().await;
        let session_key: Arc<str> = session_id.into();

        if let Some(existing) = histories.get(&session_key) {
            *existing.write().await = history;
        } else {
            histories.insert(session_key, Arc::new(RwLock::new(history)));
        }
    }

    /// Get history for a session
    pub async fn get(&self, session_id: &str) -> Option<MessageHistory> {
        let histories = self.histories.read().await;
        if let Some(history) = histories.get(session_id) {
            Some(history.read().await.clone())
        } else {
            None
        }
    }

    /// Delete history for a session
    pub async fn delete(&self, session_id: &str) {
        let mut histories = self.histories.write().await;
        histories.remove(session_id);
    }

    /// Clear all histories
    pub async fn clear(&self) {
        let mut histories = self.histories.write().await;
        histories.clear();
    }

    /// Get the number of active sessions
    pub async fn session_count(&self) -> usize {
        let histories = self.histories.read().await;
        histories.len()
    }
}

impl Default for HistoryManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_history_basic() {
        let mut history = MessageHistory::new();
        history.add_user("Hello");
        history.add_assistant("Hi there!");

        assert_eq!(history.len(), 2);
        assert!(!history.is_empty());
    }

    #[test]
    fn test_token_estimation_caching() {
        let mut history = MessageHistory::new();
        history.add_user("Test message for token estimation");

        let tokens1 = history.estimate_tokens();
        let tokens2 = history.estimate_tokens();

        // Should return same value (cached)
        assert_eq!(tokens1, tokens2);

        // Add message should invalidate cache
        history.add_assistant("Response");
        let tokens3 = history.estimate_tokens();

        // Token count should increase
        assert!(tokens3 > tokens1);
    }

    #[test]
    fn test_messages_no_clone() {
        let mut history = MessageHistory::new();
        history.add_user("Hello");

        // messages() should return slice without cloning
        let msgs = history.messages();
        assert_eq!(msgs.len(), 1);
    }

    #[test]
    fn test_with_capacity() {
        let history = MessageHistory::with_capacity(100);
        assert!(history.is_empty());
    }

    #[test]
    fn test_take_messages() {
        let mut history = MessageHistory::new();
        history.add_user("Hello");
        history.add_assistant("Hi");

        let messages = history.take_messages();
        assert_eq!(messages.len(), 2);
        assert!(history.is_empty());
    }

    #[test]
    fn test_json_token_estimation() {
        let simple = serde_json::json!({"key": "value"});
        let tokens = estimate_json_tokens(&simple);
        assert!(tokens > 0);

        let nested = serde_json::json!({
            "array": [1, 2, 3],
            "nested": {"deep": true}
        });
        let nested_tokens = estimate_json_tokens(&nested);
        assert!(nested_tokens > tokens);
    }

    #[tokio::test]
    async fn test_history_manager() {
        let manager = HistoryManager::new();

        let history = manager.get_or_create("session1").await;
        assert!(history.is_empty());

        let mut updated = history;
        updated.add_user("Hello");
        manager.update("session1", updated).await;

        let retrieved = manager.get("session1").await.unwrap();
        assert_eq!(retrieved.len(), 1);
    }
}
