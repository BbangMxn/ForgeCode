//! Context Management
//!
//! Techniques for reducing LLM context size while preserving essential information.
//!
//! ## Priority Order (most to least efficient)
//!
//! 1. **Observation Masking** - Replace old tool results with placeholders
//!    - 52% cost reduction (JetBrains research)
//!    - Zero additional cost
//!    - No information loss for recent context
//!
//! 2. **Context Compaction** - Replace large content with references
//!    - Reversible operation
//!    - Good for file contents and verbose outputs
//!
//! 3. **Summarization** - LLM-based conversation summary
//!    - Last resort (has additional LLM cost)
//!    - Lossy operation
//!    - Only when other techniques aren't sufficient

mod compactor;
mod masker;
mod summarizer;

pub use masker::{
    MaskingStats, MessageRole, ObservationMasker, ObservationMaskerConfig, ObservationMessage,
    SimpleMessage,
};

pub use compactor::{
    CompactedContent, CompactorConfig, CompactorStats, ContentId, ContextCompactor,
};

pub use summarizer::{
    estimate_messages_tokens, estimate_tokens, ConversationSummarizer, SummarizableMessage,
    SummarizationResult, SummarizerConfig,
};
