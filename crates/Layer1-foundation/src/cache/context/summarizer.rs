//! Conversation Summarizer
//!
//! Uses LLM to summarize older conversation turns when context exceeds threshold.
//! This is the "last resort" - only used when other techniques aren't sufficient.
//!
//! Note: This technique has additional cost (LLM call for summarization).

use serde::{Deserialize, Serialize};

/// Configuration for conversation summarization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummarizerConfig {
    /// Token threshold for triggering summarization
    pub threshold_tokens: usize,
    /// Number of recent messages to preserve (not summarize)
    pub preserve_recent: usize,
    /// Model to use for summarization (should be cheap/fast)
    pub summary_model: String,
    /// Maximum tokens for the summary output
    pub max_summary_tokens: usize,
}

impl Default for SummarizerConfig {
    fn default() -> Self {
        Self {
            threshold_tokens: 100_000,
            preserve_recent: 10,
            summary_model: "claude-3-haiku-20240307".to_string(),
            max_summary_tokens: 2000,
        }
    }
}

/// Result of a summarization operation
#[derive(Debug, Clone)]
pub struct SummarizationResult {
    /// The summary message to insert
    pub summary: String,
    /// Number of messages that were summarized
    pub messages_summarized: usize,
    /// Estimated tokens saved
    pub tokens_saved: usize,
    /// Cost of summarization (estimated)
    pub summarization_cost: f64,
}

/// Conversation Summarizer
///
/// Summarizes older parts of the conversation when context grows too large.
/// This is a "lossy" operation - some information is lost in the summary.
///
/// # Usage Priority
///
/// This should only be used when:
/// 1. Observation masking is already applied
/// 2. Context compaction is already applied
/// 3. Context still exceeds the threshold
///
/// # Example
///
/// ```rust,ignore
/// let summarizer = ConversationSummarizer::new();
/// let token_count = estimate_tokens(&messages);
///
/// if summarizer.needs_summarization(token_count) {
///     let result = summarizer.summarize(&messages, &provider).await?;
///     // Replace old messages with summary
/// }
/// ```
#[derive(Debug, Clone)]
pub struct ConversationSummarizer {
    config: SummarizerConfig,
}

impl ConversationSummarizer {
    /// Create a new summarizer with default settings
    pub fn new() -> Self {
        Self {
            config: SummarizerConfig::default(),
        }
    }

    /// Create a summarizer with a specific token threshold
    pub fn with_threshold(threshold_tokens: usize) -> Self {
        Self {
            config: SummarizerConfig {
                threshold_tokens,
                ..Default::default()
            },
        }
    }

    /// Create a summarizer with custom configuration
    pub fn with_config(config: SummarizerConfig) -> Self {
        Self { config }
    }

    /// Check if summarization is needed based on token count
    pub fn needs_summarization(&self, estimated_tokens: usize) -> bool {
        estimated_tokens > self.config.threshold_tokens
    }

    /// Get the number of messages to preserve
    pub fn preserve_count(&self) -> usize {
        self.config.preserve_recent
    }

    /// Get the summary model name
    pub fn summary_model(&self) -> &str {
        &self.config.summary_model
    }

    /// Build the summarization prompt
    ///
    /// This creates a prompt that instructs the LLM to summarize
    /// the conversation while preserving key information.
    pub fn build_summary_prompt(&self, messages_to_summarize: &[SummarizableMessage]) -> String {
        let mut content = String::new();

        for msg in messages_to_summarize {
            content.push_str(&format!("[{}]: {}\n\n", msg.role, msg.content));
        }

        format!(
            r#"Summarize the following conversation history concisely.
Focus on:
1. Key decisions and conclusions reached
2. Important facts and data discovered
3. Actions taken and their results
4. Current state and pending tasks

Be concise but preserve critical information.
Output only the summary, no preamble.

---
CONVERSATION TO SUMMARIZE:
{}
---

SUMMARY:"#,
            content
        )
    }

    /// Format the summary as a system message
    pub fn format_summary_message(&self, summary: &str, messages_count: usize) -> String {
        format!(
            "[Conversation Summary - {} previous messages]\n\n{}\n\n[End of Summary]",
            messages_count, summary
        )
    }

    /// Estimate the cost of summarization
    ///
    /// Based on Claude 3 Haiku pricing (approximate)
    pub fn estimate_cost(&self, input_tokens: usize, output_tokens: usize) -> f64 {
        // Haiku pricing: $0.25/M input, $1.25/M output (approximate)
        let input_cost = (input_tokens as f64 / 1_000_000.0) * 0.25;
        let output_cost = (output_tokens as f64 / 1_000_000.0) * 1.25;
        input_cost + output_cost
    }

    /// Calculate how many messages should be summarized
    pub fn messages_to_summarize(&self, total_messages: usize) -> usize {
        if total_messages <= self.config.preserve_recent {
            0
        } else {
            total_messages - self.config.preserve_recent
        }
    }
}

impl Default for ConversationSummarizer {
    fn default() -> Self {
        Self::new()
    }
}

/// A message that can be summarized
#[derive(Debug, Clone)]
pub struct SummarizableMessage {
    pub role: String,
    pub content: String,
}

/// Rough token estimation
///
/// This is a simple heuristic - actual token count depends on the tokenizer.
/// Rule of thumb: ~4 characters per token for English text.
pub fn estimate_tokens(text: &str) -> usize {
    // Simple heuristic: ~4 chars per token
    text.len() / 4
}

/// Estimate tokens for a list of messages
pub fn estimate_messages_tokens(messages: &[SummarizableMessage]) -> usize {
    messages
        .iter()
        .map(|m| estimate_tokens(&m.content) + 10)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_needs_summarization() {
        let summarizer = ConversationSummarizer::with_threshold(1000);

        assert!(!summarizer.needs_summarization(500));
        assert!(!summarizer.needs_summarization(1000));
        assert!(summarizer.needs_summarization(1001));
    }

    #[test]
    fn test_messages_to_summarize() {
        let summarizer = ConversationSummarizer::new(); // preserve_recent = 10

        assert_eq!(summarizer.messages_to_summarize(5), 0);
        assert_eq!(summarizer.messages_to_summarize(10), 0);
        assert_eq!(summarizer.messages_to_summarize(15), 5);
        assert_eq!(summarizer.messages_to_summarize(100), 90);
    }

    #[test]
    fn test_build_summary_prompt() {
        let summarizer = ConversationSummarizer::new();
        let messages = vec![
            SummarizableMessage {
                role: "user".to_string(),
                content: "Hello".to_string(),
            },
            SummarizableMessage {
                role: "assistant".to_string(),
                content: "Hi there!".to_string(),
            },
        ];

        let prompt = summarizer.build_summary_prompt(&messages);

        assert!(prompt.contains("[user]: Hello"));
        assert!(prompt.contains("[assistant]: Hi there!"));
        assert!(prompt.contains("SUMMARY:"));
    }

    #[test]
    fn test_estimate_tokens() {
        // ~4 chars per token
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("test"), 1);
        assert_eq!(estimate_tokens("hello world!"), 3); // 12 chars / 4
    }
}
