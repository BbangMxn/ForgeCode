//! Unified Cache Manager
//!
//! Integrates all caching components into a single, easy-to-use interface.

use serde_json::Value;
use std::path::Path;

use super::config::CacheConfig;
use super::context::{
    CompactedContent, CompactorConfig, CompactorStats, ContextCompactor, ConversationSummarizer,
    MaskingStats, ObservationMasker, ObservationMaskerConfig, ObservationMessage,
    SummarizableMessage, SummarizerConfig,
};
use super::response::{
    CachedToolDefinition, CachedToolResult, McpCache, McpCacheConfig, McpCacheStats, ToolCache,
    ToolCacheConfig, ToolCacheStats,
};

/// Unified Cache Manager
///
/// Provides a single point of access to all caching functionality in ForgeCode.
///
/// # Architecture
///
/// ```text
/// ┌─────────────────────────────────────────────────────────┐
/// │                    CacheManager                          │
/// ├─────────────────────────────────────────────────────────┤
/// │  Context Management (Layer 1)                           │
/// │  ├── ObservationMasker   (most efficient)               │
/// │  ├── ContextCompactor    (reversible)                   │
/// │  └── ConversationSummarizer (last resort)               │
/// ├─────────────────────────────────────────────────────────┤
/// │  Response Cache (Layer 2)                               │
/// │  ├── ToolCache           (Read, Glob, Grep)             │
/// │  └── McpCache            (tool definitions)             │
/// └─────────────────────────────────────────────────────────┘
/// ```
///
/// # Usage
///
/// ```rust,ignore
/// // Create with default config
/// let mut cache = CacheManager::new();
///
/// // Or with custom config
/// let config = CacheConfig::minimal();
/// let mut cache = CacheManager::with_config(config);
///
/// // Context management
/// cache.mask_observations(&mut messages);
/// let compacted = cache.compact_file_content(path, &content);
///
/// // Response caching
/// if let Some(result) = cache.get_tool_result("Read", &args) {
///     return result;
/// }
/// cache.cache_tool_result("Read", &args, output, true, vec![path]);
/// ```
#[derive(Debug)]
pub struct CacheManager {
    config: CacheConfig,

    // Context Management
    observation_masker: ObservationMasker,
    context_compactor: ContextCompactor,
    conversation_summarizer: ConversationSummarizer,

    // Response Cache
    tool_cache: ToolCache,
    mcp_cache: McpCache,
}

impl CacheManager {
    /// Create a new cache manager with default configuration
    pub fn new() -> Self {
        Self::with_config(CacheConfig::default())
    }

    /// Create a cache manager with custom configuration
    pub fn with_config(config: CacheConfig) -> Self {
        // Build context management components
        let observation_masker = ObservationMasker::with_config(ObservationMaskerConfig {
            window_size: config.context.observation_window,
            placeholder: "[Previous output truncated]".to_string(),
            include_size_hint: true,
        });

        let context_compactor = ContextCompactor::with_config(CompactorConfig {
            threshold_bytes: config.context.compact_threshold,
            max_entries: config.limits.max_compacted_entries,
            preview_length: 200,
        });

        let conversation_summarizer = ConversationSummarizer::with_config(SummarizerConfig {
            threshold_tokens: config.context.summarize_threshold,
            preserve_recent: config.context.preserve_recent,
            summary_model: config.context.summary_model.clone(),
            max_summary_tokens: 2000,
        });

        // Build response cache components
        let tool_cache = ToolCache::with_config(ToolCacheConfig {
            max_entries: config.response.tool_cache_size,
            cacheable_tools: vec!["Read".to_string(), "Glob".to_string(), "Grep".to_string()],
            enable_file_invalidation: config.response.enable_file_watcher,
        });

        let mcp_cache = McpCache::with_config(McpCacheConfig {
            ttl_secs: config.response.mcp_cache_ttl_secs,
            max_servers: 50,
        });

        Self {
            config,
            observation_masker,
            context_compactor,
            conversation_summarizer,
            tool_cache,
            mcp_cache,
        }
    }

    /// Create a minimal cache manager (resource-constrained environments)
    pub fn minimal() -> Self {
        Self::with_config(CacheConfig::minimal())
    }

    /// Create a performance-optimized cache manager
    pub fn performance() -> Self {
        Self::with_config(CacheConfig::performance())
    }

    // =========================================================================
    // Context Management
    // =========================================================================

    /// Mask old observations in a message list
    ///
    /// This is the most efficient context reduction technique.
    pub fn mask_observations<M: ObservationMessage>(&self, messages: &mut [M]) {
        self.observation_masker.mask(messages);
    }

    /// Estimate savings from observation masking
    pub fn estimate_masking_savings<M: ObservationMessage>(&self, messages: &[M]) -> MaskingStats {
        self.observation_masker.estimate_savings(messages)
    }

    /// Compact file content if it exceeds threshold
    pub fn compact_file_content(&mut self, path: &Path, content: &str) -> CompactedContent {
        self.context_compactor.compact_file_content(path, content)
    }

    /// Compact tool result if it exceeds threshold
    pub fn compact_tool_result(&mut self, tool_name: &str, result: &str) -> CompactedContent {
        self.context_compactor
            .compact_tool_result(tool_name, result)
    }

    /// Try to compact content (returns None if below threshold)
    pub fn try_compact(&mut self, content: &str) -> Option<String> {
        self.context_compactor.try_compact(content)
    }

    /// Restore compacted content
    pub fn restore_content(&self, id: &super::context::ContentId) -> Option<&str> {
        self.context_compactor.restore(id)
    }

    /// Check if summarization is needed
    pub fn needs_summarization(&self, estimated_tokens: usize) -> bool {
        self.conversation_summarizer
            .needs_summarization(estimated_tokens)
    }

    /// Get the summarization prompt
    pub fn build_summary_prompt(&self, messages: &[SummarizableMessage]) -> String {
        self.conversation_summarizer.build_summary_prompt(messages)
    }

    /// Format a summary as a message
    pub fn format_summary_message(&self, summary: &str, count: usize) -> String {
        self.conversation_summarizer
            .format_summary_message(summary, count)
    }

    /// Get the number of messages to preserve during summarization
    pub fn preserve_recent_count(&self) -> usize {
        self.conversation_summarizer.preserve_count()
    }

    // =========================================================================
    // Tool Result Caching
    // =========================================================================

    /// Get a cached tool result
    pub fn get_tool_result(&mut self, tool_name: &str, args: &Value) -> Option<&CachedToolResult> {
        self.tool_cache.get(tool_name, args)
    }

    /// Cache a tool result
    pub fn cache_tool_result(
        &mut self,
        tool_name: &str,
        args: &Value,
        output: String,
        success: bool,
        involved_files: Vec<std::path::PathBuf>,
    ) {
        self.tool_cache
            .insert(tool_name, args, output, success, involved_files);
    }

    /// Check if a tool is cacheable
    pub fn is_tool_cacheable(&self, tool_name: &str) -> bool {
        self.tool_cache.is_cacheable(tool_name)
    }

    // =========================================================================
    // MCP Caching
    // =========================================================================

    /// Get cached MCP tool definitions
    pub fn get_mcp_tools(&mut self, server_id: &str) -> Option<&[CachedToolDefinition]> {
        self.mcp_cache.get_tools(server_id)
    }

    /// Cache MCP tool definitions
    pub fn cache_mcp_tools(&mut self, server_id: &str, tools: Vec<CachedToolDefinition>) {
        self.mcp_cache.set_tools(server_id, tools);
    }

    /// Check if MCP tools are cached
    pub fn has_mcp_tools(&self, server_id: &str) -> bool {
        self.mcp_cache.has_tools(server_id)
    }

    // =========================================================================
    // Invalidation
    // =========================================================================

    /// Invalidate caches related to a file change
    pub fn on_file_changed(&mut self, path: &Path) {
        self.tool_cache.invalidate_for_file(path);
    }

    /// Invalidate MCP cache for a server
    pub fn on_mcp_server_changed(&mut self, server_id: &str) {
        self.mcp_cache.invalidate(server_id);
    }

    /// Invalidate all caches
    pub fn invalidate_all(&mut self) {
        self.tool_cache.invalidate_all();
        self.mcp_cache.invalidate_all();
        self.context_compactor.clear();
    }

    // =========================================================================
    // Statistics & Monitoring
    // =========================================================================

    /// Get overall cache statistics
    pub fn stats(&self) -> CacheManagerStats {
        CacheManagerStats {
            tool_cache: self.tool_cache.stats(),
            mcp_cache: self.mcp_cache.stats(),
            compactor: self.context_compactor.stats(),
        }
    }

    /// Estimate total memory usage
    pub fn memory_usage(&self) -> usize {
        self.tool_cache.stats().memory_bytes + self.context_compactor.memory_usage()
        // MCP cache is relatively small, skip for now
    }

    /// Check memory pressure and evict if necessary
    pub fn check_memory_pressure(&mut self) {
        let max_bytes = self.config.limits.max_memory_mb * 1024 * 1024;
        let current = self.memory_usage();

        if current > max_bytes {
            // Evict from tool cache first (most likely to be large)
            self.tool_cache.invalidate_all();
        }
    }

    /// Cleanup expired entries
    pub fn cleanup(&mut self) {
        self.mcp_cache.cleanup_expired();
    }

    /// Get the current configuration
    pub fn config(&self) -> &CacheConfig {
        &self.config
    }
}

impl Default for CacheManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Combined cache statistics
#[derive(Debug, Clone)]
pub struct CacheManagerStats {
    pub tool_cache: ToolCacheStats,
    pub mcp_cache: McpCacheStats,
    pub compactor: CompactorStats,
}

impl CacheManagerStats {
    /// Get overall cache hit rate
    pub fn overall_hit_rate(&self) -> f64 {
        let total_hits = self.tool_cache.hits + self.mcp_cache.hits;
        let total_misses = self.tool_cache.misses + self.mcp_cache.misses;
        let total = total_hits + total_misses;

        if total > 0 {
            total_hits as f64 / total as f64
        } else {
            0.0
        }
    }

    /// Get total memory usage estimate
    pub fn total_memory_bytes(&self) -> usize {
        self.tool_cache.memory_bytes + self.compactor.total_bytes_stored
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_cache_manager_creation() {
        let cache = CacheManager::new();
        assert!(cache.is_tool_cacheable("Read"));
        assert!(!cache.is_tool_cacheable("Bash"));
    }

    #[test]
    fn test_tool_caching() {
        let mut cache = CacheManager::new();
        let args = json!({"path": "/test.rs"});

        // Miss
        assert!(cache.get_tool_result("Read", &args).is_none());

        // Insert
        cache.cache_tool_result("Read", &args, "content".to_string(), true, vec![]);

        // Hit
        let result = cache.get_tool_result("Read", &args);
        assert!(result.is_some());
        assert_eq!(result.unwrap().output, "content");
    }

    #[test]
    fn test_file_invalidation() {
        let mut cache = CacheManager::new();
        let path = std::path::PathBuf::from("/test.rs");
        let args = json!({"path": "/test.rs"});

        cache.cache_tool_result(
            "Read",
            &args,
            "content".to_string(),
            true,
            vec![path.clone()],
        );

        cache.on_file_changed(&path);

        assert!(cache.get_tool_result("Read", &args).is_none());
    }

    #[test]
    fn test_context_compaction() {
        let mut cache = CacheManager::with_config(CacheConfig {
            context: super::super::config::ContextCacheConfig {
                compact_threshold: 10,
                ..Default::default()
            },
            ..Default::default()
        });

        let content = "x".repeat(100);
        let result = cache.compact_file_content(Path::new("test.rs"), &content);

        assert!(result.is_compacted);
        assert!(result.restore_key.is_some());

        // Verify restoration
        let restored = cache.restore_content(&result.restore_key.unwrap());
        assert_eq!(restored, Some(content.as_str()));
    }

    #[test]
    fn test_stats() {
        let cache = CacheManager::new();
        let stats = cache.stats();

        assert_eq!(stats.tool_cache.entries, 0);
        assert_eq!(stats.mcp_cache.servers_cached, 0);
    }
}
