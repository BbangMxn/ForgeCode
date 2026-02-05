//! # ForgeCode Cache System
//!
//! A resource-efficient caching system designed for LLM-powered coding agents.
//!
//! ## Design Principles
//!
//! 1. **Minimal Resource Usage** - Lazy initialization, bounded memory, aggressive eviction
//! 2. **Cost Efficiency** - Provider caching first, context reduction, observation masking
//! 3. **Simplicity** - No external dependencies, in-memory first, opt-in complexity
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                    ForgeCode Cache Architecture                          │
//! ├─────────────────────────────────────────────────────────────────────────┤
//! │                                                                          │
//! │  Layer 1: Context Management (Agent Level)                              │
//! │  ┌──────────────┐ ┌──────────────┐ ┌──────────────────────┐            │
//! │  │ Observation  │ │   Context    │ │    Conversation      │            │
//! │  │   Masker     │ │  Compactor   │ │    Summarizer        │            │
//! │  │  (52% save)  │ │ (reversible) │ │   (last resort)      │            │
//! │  └──────────────┘ └──────────────┘ └──────────────────────┘            │
//! │                                                                          │
//! │  Layer 2: Response Cache (Application Level)                            │
//! │  ┌──────────────┐ ┌──────────────┐                                      │
//! │  │    Tool      │ │     MCP      │                                      │
//! │  │   Cache      │ │    Cache     │                                      │
//! │  └──────────────┘ └──────────────┘                                      │
//! │                                                                          │
//! │  Layer 3: Provider Cache (API Level - Automatic)                        │
//! │  ┌─────────────────────────────────────────────────────────────────┐   │
//! │  │  Prompt Prefix Cache (Anthropic/OpenAI - 90% cost reduction)    │   │
//! │  └─────────────────────────────────────────────────────────────────┘   │
//! │                                                                          │
//! └─────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Quick Start
//!
//! ```rust,ignore
//! use forge_foundation::cache::{CacheManager, CacheConfig};
//!
//! // Default configuration
//! let mut cache = CacheManager::new();
//!
//! // Or minimal for resource-constrained environments
//! let mut cache = CacheManager::minimal();
//!
//! // Context management (before LLM calls)
//! cache.mask_observations(&mut messages);
//!
//! // Tool result caching
//! if let Some(result) = cache.get_tool_result("Read", &args) {
//!     return result.output.clone();
//! }
//! cache.cache_tool_result("Read", &args, output, true, vec![path]);
//!
//! // File change invalidation
//! cache.on_file_changed(Path::new("modified.rs"));
//! ```
//!
//! ## Modules
//!
//! - [`config`] - Cache configuration
//! - [`context`] - Context management (masking, compaction, summarization)
//! - [`response`] - Response caching (tools, MCP)
//! - [`manager`] - Unified cache manager
//! - [`util`] - Utilities (LRU cache, hashing)

pub mod config;
pub mod context;
pub mod manager;
pub mod response;
pub mod util;

// Re-exports for convenience
pub use config::{CacheConfig, CacheLimitsConfig, ContextCacheConfig, ResponseCacheConfig};

pub use context::{
    estimate_messages_tokens,
    estimate_tokens,
    CompactedContent,
    CompactorConfig,
    CompactorStats,
    ContentId,
    // Compactor
    ContextCompactor,
    // Summarizer
    ConversationSummarizer,
    MaskingStats,
    MessageRole,
    // Masker
    ObservationMasker,
    ObservationMaskerConfig,
    ObservationMessage,
    SimpleMessage,
    SummarizableMessage,
    SummarizationResult,
    SummarizerConfig,
};

pub use response::{
    CachedToolDefinition,
    CachedToolResult,
    // MCP Cache
    McpCache,
    McpCacheConfig,
    McpCacheStats,
    // Tool Cache
    ToolCache,
    ToolCacheConfig,
    ToolCacheKey,
    ToolCacheStats,
};

pub use manager::{CacheManager, CacheManagerStats};

pub use util::{
    compute_hash, hash_file_content, hash_file_content_fast, hash_json, CacheStats, CompositeKey,
    LruCache, LruCacheStats, TtlLruCache, TwoLevelCache, TwoLevelCacheBuilder,
};
