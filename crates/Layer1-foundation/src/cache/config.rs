//! Cache configuration

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Cache system configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Context Management settings
    pub context: ContextCacheConfig,

    /// Response Cache settings
    pub response: ResponseCacheConfig,

    /// Resource limits
    pub limits: CacheLimitsConfig,
}

/// Context management configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextCacheConfig {
    /// Number of recent observations to keep in full detail
    /// Older observations are replaced with placeholders
    #[serde(default = "default_observation_window")]
    pub observation_window: usize,

    /// Content size threshold (bytes) for compaction
    /// Content larger than this is replaced with a reference
    #[serde(default = "default_compact_threshold")]
    pub compact_threshold: usize,

    /// Token threshold for triggering summarization
    /// When context exceeds this, older messages are summarized
    #[serde(default = "default_summarize_threshold")]
    pub summarize_threshold: usize,

    /// Number of recent messages to preserve during summarization
    #[serde(default = "default_preserve_recent")]
    pub preserve_recent: usize,

    /// Model to use for summarization (should be cheap/fast)
    #[serde(default = "default_summary_model")]
    pub summary_model: String,
}

/// Response cache configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseCacheConfig {
    /// Maximum number of tool results to cache
    #[serde(default = "default_tool_cache_size")]
    pub tool_cache_size: usize,

    /// TTL for MCP tool definitions cache (seconds)
    #[serde(default = "default_mcp_cache_ttl_secs")]
    pub mcp_cache_ttl_secs: u64,

    /// Maximum number of LSP symbols to cache per file
    #[serde(default = "default_lsp_cache_per_file")]
    pub lsp_cache_per_file: usize,

    /// Enable file watcher for cache invalidation
    #[serde(default = "default_enable_file_watcher")]
    pub enable_file_watcher: bool,
}

/// Resource limits configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheLimitsConfig {
    /// Maximum memory usage for all caches (MB)
    #[serde(default = "default_max_memory_mb")]
    pub max_memory_mb: usize,

    /// Maximum number of compacted content entries to store
    #[serde(default = "default_max_compacted_entries")]
    pub max_compacted_entries: usize,

    /// Enable memory pressure monitoring
    #[serde(default = "default_enable_memory_monitor")]
    pub enable_memory_monitor: bool,
}

// Default value functions
fn default_observation_window() -> usize {
    10
}
fn default_compact_threshold() -> usize {
    4096
} // 4KB
fn default_summarize_threshold() -> usize {
    100_000
} // ~100K tokens
fn default_preserve_recent() -> usize {
    10
}
fn default_summary_model() -> String {
    "claude-3-haiku-20240307".to_string()
}
fn default_tool_cache_size() -> usize {
    100
}
fn default_mcp_cache_ttl_secs() -> u64 {
    1800
} // 30 minutes
fn default_lsp_cache_per_file() -> usize {
    500
}
fn default_enable_file_watcher() -> bool {
    true
}
fn default_max_memory_mb() -> usize {
    100
}
fn default_max_compacted_entries() -> usize {
    1000
}
fn default_enable_memory_monitor() -> bool {
    true
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            context: ContextCacheConfig::default(),
            response: ResponseCacheConfig::default(),
            limits: CacheLimitsConfig::default(),
        }
    }
}

impl Default for ContextCacheConfig {
    fn default() -> Self {
        Self {
            observation_window: default_observation_window(),
            compact_threshold: default_compact_threshold(),
            summarize_threshold: default_summarize_threshold(),
            preserve_recent: default_preserve_recent(),
            summary_model: default_summary_model(),
        }
    }
}

impl Default for ResponseCacheConfig {
    fn default() -> Self {
        Self {
            tool_cache_size: default_tool_cache_size(),
            mcp_cache_ttl_secs: default_mcp_cache_ttl_secs(),
            lsp_cache_per_file: default_lsp_cache_per_file(),
            enable_file_watcher: default_enable_file_watcher(),
        }
    }
}

impl Default for CacheLimitsConfig {
    fn default() -> Self {
        Self {
            max_memory_mb: default_max_memory_mb(),
            max_compacted_entries: default_max_compacted_entries(),
            enable_memory_monitor: default_enable_memory_monitor(),
        }
    }
}

impl ResponseCacheConfig {
    /// Get MCP cache TTL as Duration
    pub fn mcp_cache_ttl(&self) -> Duration {
        Duration::from_secs(self.mcp_cache_ttl_secs)
    }
}

impl CacheConfig {
    /// Create a minimal config for resource-constrained environments
    pub fn minimal() -> Self {
        Self {
            context: ContextCacheConfig {
                observation_window: 5,
                compact_threshold: 2048,
                summarize_threshold: 50_000,
                preserve_recent: 5,
                summary_model: default_summary_model(),
            },
            response: ResponseCacheConfig {
                tool_cache_size: 50,
                mcp_cache_ttl_secs: 900, // 15 minutes
                lsp_cache_per_file: 200,
                enable_file_watcher: false,
            },
            limits: CacheLimitsConfig {
                max_memory_mb: 50,
                max_compacted_entries: 500,
                enable_memory_monitor: true,
            },
        }
    }

    /// Create an aggressive caching config for performance
    pub fn performance() -> Self {
        Self {
            context: ContextCacheConfig {
                observation_window: 20,
                compact_threshold: 8192,
                summarize_threshold: 150_000,
                preserve_recent: 15,
                summary_model: default_summary_model(),
            },
            response: ResponseCacheConfig {
                tool_cache_size: 200,
                mcp_cache_ttl_secs: 3600, // 1 hour
                lsp_cache_per_file: 1000,
                enable_file_watcher: true,
            },
            limits: CacheLimitsConfig {
                max_memory_mb: 200,
                max_compacted_entries: 2000,
                enable_memory_monitor: true,
            },
        }
    }
}
