//! Context Optimizer
//!
//! Optimizes conversation context for efficient token usage while preserving
//! important information for the LLM.
//!
//! ## Features
//!
//! - **Token counting**: Accurate token estimation for context management
//! - **Message prioritization**: Keep important messages, compress less important ones
//! - **Tool result caching**: Cache and deduplicate tool outputs
//! - **Smart truncation**: Intelligent context window management
//!
//! ## Usage
//!
//! ```ignore
//! let optimizer = ContextOptimizer::new(100_000); // 100k token limit
//! let optimized = optimizer.optimize(&messages, &tool_results).await;
//! ```

#![allow(dead_code)]

use std::collections::HashMap;
use tracing::{debug, info};

/// Token estimation constants
const CHARS_PER_TOKEN: usize = 4;
const TOOL_RESULT_OVERHEAD: usize = 50; // JSON structure overhead per tool result
const MESSAGE_OVERHEAD: usize = 20; // Role, structure overhead per message

/// Context optimization configuration
#[derive(Debug, Clone)]
pub struct ContextOptimizerConfig {
    /// Maximum tokens for context
    pub max_tokens: usize,

    /// Reserve tokens for output
    pub reserved_output_tokens: usize,

    /// Minimum messages to keep (most recent)
    pub min_recent_messages: usize,

    /// Enable tool result caching
    pub enable_tool_cache: bool,

    /// Maximum tool result size before truncation
    pub max_tool_result_chars: usize,

    /// Enable message summarization
    pub enable_summarization: bool,
}

impl Default for ContextOptimizerConfig {
    fn default() -> Self {
        Self {
            max_tokens: 100_000,
            reserved_output_tokens: 4_000,
            min_recent_messages: 10,
            enable_tool_cache: true,
            max_tool_result_chars: 10_000,
            enable_summarization: true,
        }
    }
}

impl ContextOptimizerConfig {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            max_tokens,
            ..Default::default()
        }
    }

    pub fn available_tokens(&self) -> usize {
        self.max_tokens.saturating_sub(self.reserved_output_tokens)
    }
}

/// Message importance level for prioritization
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MessageImportance {
    /// System prompt - always keep
    Critical = 100,
    /// User's original question/request
    High = 80,
    /// Recent assistant responses
    Medium = 50,
    /// Old tool results
    Low = 30,
    /// Can be safely removed
    Droppable = 10,
}

/// Represents a message in the context
#[derive(Debug, Clone)]
pub struct ContextMessage {
    pub role: String,
    pub content: String,
    pub importance: MessageImportance,
    pub token_count: usize,
    pub can_summarize: bool,
    pub is_tool_result: bool,
    pub tool_call_id: Option<String>,
}

impl ContextMessage {
    pub fn new(role: impl Into<String>, content: impl Into<String>) -> Self {
        let content = content.into();
        let token_count = estimate_tokens(&content);
        Self {
            role: role.into(),
            content,
            importance: MessageImportance::Medium,
            token_count,
            can_summarize: true,
            is_tool_result: false,
            tool_call_id: None,
        }
    }

    pub fn with_importance(mut self, importance: MessageImportance) -> Self {
        self.importance = importance;
        self
    }

    pub fn as_tool_result(mut self, tool_call_id: impl Into<String>) -> Self {
        self.is_tool_result = true;
        self.tool_call_id = Some(tool_call_id.into());
        self.can_summarize = false; // Tool results need special handling
        self
    }

    pub fn critical(mut self) -> Self {
        self.importance = MessageImportance::Critical;
        self.can_summarize = false;
        self
    }
}

/// Cached tool result for deduplication
#[derive(Debug, Clone)]
struct CachedToolResult {
    tool_name: String,
    input_hash: u64,
    output: String,
    token_count: usize,
    hit_count: usize,
}

/// Context optimizer for managing conversation context
pub struct ContextOptimizer {
    config: ContextOptimizerConfig,
    tool_cache: HashMap<u64, CachedToolResult>,
    total_tokens_saved: usize,
    optimization_count: usize,
}

impl ContextOptimizer {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            config: ContextOptimizerConfig::new(max_tokens),
            tool_cache: HashMap::new(),
            total_tokens_saved: 0,
            optimization_count: 0,
        }
    }

    pub fn with_config(config: ContextOptimizerConfig) -> Self {
        Self {
            config,
            tool_cache: HashMap::new(),
            total_tokens_saved: 0,
            optimization_count: 0,
        }
    }

    /// Optimize a list of messages to fit within token budget
    pub fn optimize(&mut self, messages: Vec<ContextMessage>) -> OptimizationResult {
        let original_tokens: usize = messages.iter().map(|m| m.token_count).sum();
        let available = self.config.available_tokens();

        debug!(
            "Optimizing context: {} messages, {} tokens, {} available",
            messages.len(),
            original_tokens,
            available
        );

        // If already within budget, no optimization needed
        if original_tokens <= available {
            return OptimizationResult {
                messages,
                tokens_before: original_tokens,
                tokens_after: original_tokens,
                messages_removed: 0,
                messages_truncated: 0,
                cache_hits: 0,
            };
        }

        let mut result = OptimizationResult {
            messages: Vec::new(),
            tokens_before: original_tokens,
            tokens_after: 0,
            messages_removed: 0,
            messages_truncated: 0,
            cache_hits: 0,
        };

        // Step 1: Separate critical messages (system, recent user messages)
        let (critical, mut others): (Vec<_>, Vec<_>) = messages
            .into_iter()
            .partition(|m| m.importance == MessageImportance::Critical);

        let critical_tokens: usize = critical.iter().map(|m| m.token_count).sum();

        // Step 2: Sort by importance (descending)
        others.sort_by(|a, b| b.importance.cmp(&a.importance));

        // Step 3: Add messages until we hit the token limit
        let mut current_tokens = critical_tokens;
        let mut kept_messages: Vec<ContextMessage> = critical;

        for msg in others {
            if current_tokens + msg.token_count <= available {
                current_tokens += msg.token_count;
                kept_messages.push(msg);
            } else {
                // Try truncating if it's a large tool result
                if msg.is_tool_result && msg.content.len() > 1000 {
                    if let Some(truncated) =
                        self.truncate_tool_result(&msg, available - current_tokens)
                    {
                        current_tokens += truncated.token_count;
                        kept_messages.push(truncated);
                        result.messages_truncated += 1;
                        continue;
                    }
                }
                result.messages_removed += 1;
            }
        }

        // Step 4: Sort back to original order (by position if available, otherwise keep as-is)
        // For now, we keep the priority order as it preserves important context

        result.messages = kept_messages;
        result.tokens_after = current_tokens;
        self.total_tokens_saved += original_tokens.saturating_sub(current_tokens);
        self.optimization_count += 1;

        info!(
            "Context optimized: {} -> {} tokens, {} messages removed, {} truncated",
            result.tokens_before,
            result.tokens_after,
            result.messages_removed,
            result.messages_truncated
        );

        result
    }

    /// Truncate a tool result to fit within token budget
    fn truncate_tool_result(
        &self,
        msg: &ContextMessage,
        max_tokens: usize,
    ) -> Option<ContextMessage> {
        let max_chars = max_tokens * CHARS_PER_TOKEN;

        if max_chars < 100 {
            return None; // Not enough space for meaningful content
        }

        let truncated_content = if msg.content.len() > max_chars {
            let half = (max_chars - 50) / 2;
            format!(
                "{}...\n[TRUNCATED: {} chars removed]\n...{}",
                &msg.content[..half],
                msg.content.len() - max_chars,
                &msg.content[msg.content.len() - half..]
            )
        } else {
            msg.content.clone()
        };

        Some(ContextMessage {
            role: msg.role.clone(),
            content: truncated_content.clone(),
            importance: msg.importance,
            token_count: estimate_tokens(&truncated_content),
            can_summarize: false,
            is_tool_result: true,
            tool_call_id: msg.tool_call_id.clone(),
        })
    }

    /// Cache a tool result for potential reuse
    pub fn cache_tool_result(&mut self, tool_name: &str, input: &serde_json::Value, output: &str) {
        if !self.config.enable_tool_cache {
            return;
        }

        let hash = hash_tool_input(tool_name, input);
        let token_count = estimate_tokens(output);

        self.tool_cache.insert(
            hash,
            CachedToolResult {
                tool_name: tool_name.to_string(),
                input_hash: hash,
                output: output.to_string(),
                token_count,
                hit_count: 0,
            },
        );

        // Evict old entries if cache is too large
        if self.tool_cache.len() > 100 {
            self.evict_cache_entries();
        }
    }

    /// Try to get a cached tool result
    pub fn get_cached_result(
        &mut self,
        tool_name: &str,
        input: &serde_json::Value,
    ) -> Option<String> {
        if !self.config.enable_tool_cache {
            return None;
        }

        let hash = hash_tool_input(tool_name, input);

        if let Some(cached) = self.tool_cache.get_mut(&hash) {
            cached.hit_count += 1;
            debug!(
                "Cache hit for tool '{}' (hits: {})",
                tool_name, cached.hit_count
            );
            return Some(cached.output.clone());
        }

        None
    }

    /// Evict least-used cache entries
    fn evict_cache_entries(&mut self) {
        // Remove entries with lowest hit count
        let mut entries: Vec<_> = self.tool_cache.iter().collect();
        entries.sort_by_key(|(_, v)| v.hit_count);

        let to_remove: Vec<u64> = entries.iter().take(20).map(|(k, _)| **k).collect();

        for key in to_remove {
            self.tool_cache.remove(&key);
        }

        debug!(
            "Evicted {} cache entries, {} remaining",
            20,
            self.tool_cache.len()
        );
    }

    /// Get optimization statistics
    pub fn stats(&self) -> OptimizerStats {
        OptimizerStats {
            total_tokens_saved: self.total_tokens_saved,
            optimization_count: self.optimization_count,
            cache_size: self.tool_cache.len(),
            cache_hits: self.tool_cache.values().map(|v| v.hit_count).sum(),
        }
    }

    /// Clear all caches
    pub fn clear_cache(&mut self) {
        self.tool_cache.clear();
    }
}

/// Result of context optimization
#[derive(Debug, Clone)]
pub struct OptimizationResult {
    pub messages: Vec<ContextMessage>,
    pub tokens_before: usize,
    pub tokens_after: usize,
    pub messages_removed: usize,
    pub messages_truncated: usize,
    pub cache_hits: usize,
}

impl OptimizationResult {
    pub fn tokens_saved(&self) -> usize {
        self.tokens_before.saturating_sub(self.tokens_after)
    }

    pub fn compression_ratio(&self) -> f32 {
        if self.tokens_before == 0 {
            1.0
        } else {
            self.tokens_after as f32 / self.tokens_before as f32
        }
    }
}

/// Optimizer statistics
#[derive(Debug, Clone, Default)]
pub struct OptimizerStats {
    pub total_tokens_saved: usize,
    pub optimization_count: usize,
    pub cache_size: usize,
    pub cache_hits: usize,
}

/// Estimate token count from character count
pub fn estimate_tokens(text: &str) -> usize {
    (text.len() / CHARS_PER_TOKEN) + MESSAGE_OVERHEAD
}

/// Hash tool input for caching
fn hash_tool_input(tool_name: &str, input: &serde_json::Value) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    tool_name.hash(&mut hasher);
    input.to_string().hash(&mut hasher);
    hasher.finish()
}

/// Smart context compactor for long conversations
pub struct ContextCompactor {
    /// Maximum tokens for compacted context
    max_tokens: usize,

    /// Summary of older messages
    summary: Option<String>,

    /// Summarized token count
    summary_tokens: usize,
}

impl ContextCompactor {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            max_tokens,
            summary: None,
            summary_tokens: 0,
        }
    }

    /// Compact old messages into a summary
    ///
    /// Returns the summary and remaining messages that should be kept in full
    pub fn compact(
        &mut self,
        messages: Vec<ContextMessage>,
        keep_recent: usize,
    ) -> (Option<String>, Vec<ContextMessage>) {
        if messages.len() <= keep_recent {
            return (self.summary.clone(), messages);
        }

        let split_point = messages.len().saturating_sub(keep_recent);
        let (old, recent) = messages.split_at(split_point);

        // Generate summary of old messages
        let summary = self.summarize_messages(old);
        self.summary = Some(summary.clone());
        self.summary_tokens = estimate_tokens(&summary);

        (Some(summary), recent.to_vec())
    }

    /// Generate a summary of messages
    fn summarize_messages(&self, messages: &[ContextMessage]) -> String {
        let mut summary = String::from("Previous conversation summary:\n");

        // Count message types
        let user_count = messages.iter().filter(|m| m.role == "user").count();
        let assistant_count = messages.iter().filter(|m| m.role == "assistant").count();
        let tool_count = messages.iter().filter(|m| m.is_tool_result).count();

        summary.push_str(&format!(
            "- {} user messages, {} assistant responses, {} tool results\n",
            user_count, assistant_count, tool_count
        ));

        // Extract key topics from user messages
        let topics: Vec<_> = messages
            .iter()
            .filter(|m| m.role == "user")
            .filter_map(|m| extract_topic(&m.content))
            .take(5)
            .collect();

        if !topics.is_empty() {
            summary.push_str("- Key topics discussed: ");
            summary.push_str(&topics.join(", "));
            summary.push('\n');
        }

        // List tools used
        let tools_used: Vec<_> = messages
            .iter()
            .filter(|m| m.is_tool_result)
            .filter_map(|m| m.tool_call_id.as_ref())
            .map(|id| id.split('_').next().unwrap_or("unknown"))
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .take(10)
            .collect();

        if !tools_used.is_empty() {
            summary.push_str("- Tools used: ");
            summary.push_str(&tools_used.join(", "));
            summary.push('\n');
        }

        summary
    }
}

/// Extract a topic/keyword from a message
fn extract_topic(content: &str) -> Option<String> {
    // Simple heuristic: find quoted strings or capitalize words
    if let Some(start) = content.find('"') {
        if let Some(end) = content[start + 1..].find('"') {
            let quoted = &content[start + 1..start + 1 + end];
            if quoted.len() > 2 && quoted.len() < 50 {
                return Some(quoted.to_string());
            }
        }
    }

    // Find first capitalized word that looks like a name/keyword
    for word in content.split_whitespace().take(20) {
        if word.len() > 3
            && word.chars().next().is_some_and(|c| c.is_uppercase())
            && word.chars().skip(1).all(|c| c.is_alphanumeric())
        {
            return Some(word.to_string());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_tokens() {
        let text = "Hello, world!"; // 13 chars
        let tokens = estimate_tokens(text);
        assert!(tokens > 0);
        assert!(tokens < 100);
    }

    #[test]
    fn test_context_message() {
        let msg = ContextMessage::new("user", "Hello!").with_importance(MessageImportance::High);

        assert_eq!(msg.role, "user");
        assert_eq!(msg.importance, MessageImportance::High);
        assert!(msg.token_count > 0);
    }

    #[test]
    fn test_optimizer_no_change_needed() {
        let mut optimizer = ContextOptimizer::new(100_000);

        let messages = vec![
            ContextMessage::new("user", "Hello").critical(),
            ContextMessage::new("assistant", "Hi there!"),
        ];

        let result = optimizer.optimize(messages);

        assert_eq!(result.messages_removed, 0);
        assert_eq!(result.tokens_before, result.tokens_after);
    }

    #[test]
    fn test_optimizer_removes_low_priority() {
        // Use custom config with small limit but also small reserved tokens
        let config = ContextOptimizerConfig {
            max_tokens: 500,
            reserved_output_tokens: 100,
            min_recent_messages: 1,
            enable_tool_cache: true,
            max_tool_result_chars: 100,
            enable_summarization: false,
        };
        let mut optimizer = ContextOptimizer::with_config(config);

        let messages = vec![
            ContextMessage::new("system", "You are helpful").critical(),
            ContextMessage::new("user", "Hello").with_importance(MessageImportance::High),
            ContextMessage::new("assistant", "A very long response ".repeat(100))
                .with_importance(MessageImportance::Low),
        ];

        let result = optimizer.optimize(messages);

        // Should remove or truncate the long low-priority message
        assert!(result.messages_removed > 0 || result.messages_truncated > 0);
        // After optimization, tokens should be within available budget
        assert!(result.tokens_after <= optimizer.config.available_tokens());
    }

    #[test]
    fn test_tool_cache() {
        let mut optimizer = ContextOptimizer::new(100_000);

        let input = serde_json::json!({"path": "/test.txt"});

        // Cache miss
        assert!(optimizer.get_cached_result("read", &input).is_none());

        // Cache the result
        optimizer.cache_tool_result("read", &input, "file contents");

        // Cache hit
        let cached = optimizer.get_cached_result("read", &input);
        assert!(cached.is_some());
        assert_eq!(cached.unwrap(), "file contents");
    }

    #[test]
    fn test_context_compactor() {
        let mut compactor = ContextCompactor::new(50_000);

        let messages: Vec<ContextMessage> = (0..20)
            .map(|i| ContextMessage::new("user", format!("Message {}", i)))
            .collect();

        let (summary, recent) = compactor.compact(messages, 5);

        assert!(summary.is_some());
        assert_eq!(recent.len(), 5);
    }

    #[test]
    fn test_extract_topic() {
        assert!(extract_topic("Please fix \"the bug\" in the code").is_some());
        // "Hello" is capitalized and > 3 chars, so it matches
        assert!(extract_topic("Hello world").is_some());
        // All lowercase with no quotes -> None
        assert!(extract_topic("hello world").is_none());
        assert!(extract_topic("Check the FileName.rs").is_some());
    }
}
