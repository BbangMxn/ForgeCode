//! Observation Masker
//!
//! The most efficient context management technique.
//! Replaces old tool results with placeholders to reduce context size.
//!
//! Based on JetBrains research showing 52% cost reduction with equal or better performance.

use serde::{Deserialize, Serialize};

/// Configuration for observation masking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservationMaskerConfig {
    /// Number of recent observations to keep in full detail
    pub window_size: usize,
    /// Placeholder template for masked observations
    pub placeholder: String,
    /// Whether to include size hint in placeholder
    pub include_size_hint: bool,
}

impl Default for ObservationMaskerConfig {
    fn default() -> Self {
        Self {
            window_size: 10,
            placeholder: "[Previous output truncated]".to_string(),
            include_size_hint: true,
        }
    }
}

/// Observation Masker for reducing context size
///
/// This is the most effective and simplest context management technique.
/// It replaces older tool results (observations) with placeholders while
/// keeping recent ones intact.
///
/// # Example
///
/// ```rust,ignore
/// let masker = ObservationMasker::new(10);
/// let mut messages = vec![...];  // conversation with tool results
/// masker.mask_observations(&mut messages);
/// // Now older tool results are replaced with placeholders
/// ```
#[derive(Debug, Clone)]
pub struct ObservationMasker {
    config: ObservationMaskerConfig,
}

impl ObservationMasker {
    /// Create a new masker with default settings
    pub fn new() -> Self {
        Self {
            config: ObservationMaskerConfig::default(),
        }
    }

    /// Create a masker with a specific window size
    pub fn with_window(window_size: usize) -> Self {
        Self {
            config: ObservationMaskerConfig {
                window_size,
                ..Default::default()
            },
        }
    }

    /// Create a masker with custom configuration
    pub fn with_config(config: ObservationMaskerConfig) -> Self {
        Self { config }
    }

    /// Get the current window size
    pub fn window_size(&self) -> usize {
        self.config.window_size
    }

    /// Mask observations in a list of generic messages
    ///
    /// This works with any message type that can identify tool results
    /// and has mutable content.
    pub fn mask<M>(&self, messages: &mut [M])
    where
        M: ObservationMessage,
    {
        // Find all observation indices
        let observation_indices: Vec<usize> = messages
            .iter()
            .enumerate()
            .filter(|(_, m)| m.is_observation())
            .map(|(i, _)| i)
            .collect();

        // Skip if we have fewer observations than window size
        if observation_indices.len() <= self.config.window_size {
            return;
        }

        // Mask all but the most recent `window_size` observations
        let mask_count = observation_indices.len() - self.config.window_size;
        for &idx in observation_indices.iter().take(mask_count) {
            let original_len = messages[idx].content_len();
            let placeholder = if self.config.include_size_hint && original_len > 0 {
                format!("{} ({} chars)", self.config.placeholder, original_len)
            } else {
                self.config.placeholder.clone()
            };
            messages[idx].set_content(placeholder);
        }
    }

    /// Calculate how much content would be masked
    pub fn estimate_savings<M>(&self, messages: &[M]) -> MaskingStats
    where
        M: ObservationMessage,
    {
        let observations: Vec<(usize, usize)> = messages
            .iter()
            .enumerate()
            .filter(|(_, m)| m.is_observation())
            .map(|(i, m)| (i, m.content_len()))
            .collect();

        let total_observations = observations.len();
        let total_chars: usize = observations.iter().map(|(_, len)| len).sum();

        if observations.len() <= self.config.window_size {
            return MaskingStats {
                total_observations,
                masked_observations: 0,
                original_chars: total_chars,
                masked_chars: 0,
                savings_percent: 0.0,
            };
        }

        let mask_count = observations.len() - self.config.window_size;
        let masked_chars: usize = observations
            .iter()
            .take(mask_count)
            .map(|(_, len)| len)
            .sum();

        let savings_percent = if total_chars > 0 {
            (masked_chars as f64 / total_chars as f64) * 100.0
        } else {
            0.0
        };

        MaskingStats {
            total_observations,
            masked_observations: mask_count,
            original_chars: total_chars,
            masked_chars,
            savings_percent,
        }
    }
}

impl Default for ObservationMasker {
    fn default() -> Self {
        Self::new()
    }
}

/// Trait for messages that can be masked
///
/// Implement this for your message type to enable observation masking.
pub trait ObservationMessage {
    /// Check if this message is a tool result (observation)
    fn is_observation(&self) -> bool;

    /// Get the content length in characters
    fn content_len(&self) -> usize;

    /// Replace the content with a new value
    fn set_content(&mut self, content: String);
}

/// Statistics about masking operation
#[derive(Debug, Clone)]
pub struct MaskingStats {
    /// Total number of observations in the conversation
    pub total_observations: usize,
    /// Number of observations that would be/were masked
    pub masked_observations: usize,
    /// Total characters in all observations
    pub original_chars: usize,
    /// Characters that would be/were removed
    pub masked_chars: usize,
    /// Percentage of content saved
    pub savings_percent: f64,
}

/// A simple message implementation for testing and basic use
#[derive(Debug, Clone)]
pub struct SimpleMessage {
    pub role: MessageRole,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
    System,
    ToolResult,
}

impl ObservationMessage for SimpleMessage {
    fn is_observation(&self) -> bool {
        self.role == MessageRole::ToolResult
    }

    fn content_len(&self) -> usize {
        self.content.len()
    }

    fn set_content(&mut self, content: String) {
        self.content = content;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_messages() -> Vec<SimpleMessage> {
        vec![
            SimpleMessage {
                role: MessageRole::User,
                content: "Read file.txt".into(),
            },
            SimpleMessage {
                role: MessageRole::ToolResult,
                content: "a".repeat(1000),
            },
            SimpleMessage {
                role: MessageRole::Assistant,
                content: "Here's the content".into(),
            },
            SimpleMessage {
                role: MessageRole::User,
                content: "Read file2.txt".into(),
            },
            SimpleMessage {
                role: MessageRole::ToolResult,
                content: "b".repeat(2000),
            },
            SimpleMessage {
                role: MessageRole::Assistant,
                content: "Here's more content".into(),
            },
            SimpleMessage {
                role: MessageRole::User,
                content: "Read file3.txt".into(),
            },
            SimpleMessage {
                role: MessageRole::ToolResult,
                content: "c".repeat(3000),
            },
        ]
    }

    #[test]
    fn test_no_masking_within_window() {
        let masker = ObservationMasker::with_window(3);
        let mut messages = create_messages();

        masker.mask(&mut messages);

        // All 3 tool results should be preserved (window = 3)
        assert!(messages[1].content.starts_with("a"));
        assert!(messages[4].content.starts_with("b"));
        assert!(messages[7].content.starts_with("c"));
    }

    #[test]
    fn test_masking_beyond_window() {
        let masker = ObservationMasker::with_window(2);
        let mut messages = create_messages();

        masker.mask(&mut messages);

        // First tool result should be masked
        assert!(messages[1].content.contains("truncated"));
        // Last 2 should be preserved
        assert!(messages[4].content.starts_with("b"));
        assert!(messages[7].content.starts_with("c"));
    }

    #[test]
    fn test_estimate_savings() {
        let masker = ObservationMasker::with_window(1);
        let messages = create_messages();

        let stats = masker.estimate_savings(&messages);

        assert_eq!(stats.total_observations, 3);
        assert_eq!(stats.masked_observations, 2);
        assert_eq!(stats.original_chars, 6000);
        assert_eq!(stats.masked_chars, 3000); // 1000 + 2000
    }
}
