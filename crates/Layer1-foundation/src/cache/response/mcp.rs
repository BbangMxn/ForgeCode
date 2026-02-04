//! MCP Tool Definition Cache
//!
//! Caches MCP server tool definitions to avoid repeated server queries.
//! Tool definitions rarely change, so a long TTL is appropriate.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Configuration for MCP caching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpCacheConfig {
    /// Time-to-live for cached tool definitions
    pub ttl_secs: u64,
    /// Maximum number of servers to cache
    pub max_servers: usize,
}

impl Default for McpCacheConfig {
    fn default() -> Self {
        Self {
            ttl_secs: 1800, // 30 minutes
            max_servers: 50,
        }
    }
}

impl McpCacheConfig {
    pub fn ttl(&self) -> Duration {
        Duration::from_secs(self.ttl_secs)
    }
}

/// A cached tool definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedToolDefinition {
    /// Tool name
    pub name: String,
    /// Tool description
    pub description: String,
    /// Input schema (JSON Schema)
    pub input_schema: serde_json::Value,
}

/// Cached entry for a server
#[derive(Debug, Clone)]
struct ServerCacheEntry {
    tools: Vec<CachedToolDefinition>,
    cached_at: Instant,
}

/// MCP Tool Definition Cache
///
/// Caches tool definitions from MCP servers to avoid repeated queries.
/// Tool definitions typically don't change during a session, so
/// caching provides significant performance benefits.
///
/// # Invalidation
///
/// Cache is invalidated when:
/// - TTL expires (default 30 minutes)
/// - Server restarts/reconnects
/// - Manual invalidation call
///
/// # Example
///
/// ```rust,ignore
/// let mut cache = McpCache::new();
///
/// // Try to get cached tools
/// if let Some(tools) = cache.get_tools("github-server") {
///     return tools.to_vec();
/// }
///
/// // Query server and cache
/// let tools = server.list_tools().await?;
/// cache.set_tools("github-server", tools.clone());
/// ```
#[derive(Debug)]
pub struct McpCache {
    config: McpCacheConfig,
    servers: HashMap<String, ServerCacheEntry>,
    /// Statistics
    hits: u64,
    misses: u64,
}

impl McpCache {
    /// Create a new MCP cache with default settings
    pub fn new() -> Self {
        Self::with_config(McpCacheConfig::default())
    }

    /// Create an MCP cache with custom TTL
    pub fn with_ttl(ttl: Duration) -> Self {
        Self::with_config(McpCacheConfig {
            ttl_secs: ttl.as_secs(),
            ..Default::default()
        })
    }

    /// Create an MCP cache with custom configuration
    pub fn with_config(config: McpCacheConfig) -> Self {
        Self {
            config,
            servers: HashMap::new(),
            hits: 0,
            misses: 0,
        }
    }

    /// Get cached tool definitions for a server
    pub fn get_tools(&mut self, server_id: &str) -> Option<&[CachedToolDefinition]> {
        let entry = self.servers.get(server_id)?;

        // Check TTL
        if entry.cached_at.elapsed() > self.config.ttl() {
            self.misses += 1;
            return None;
        }

        self.hits += 1;
        Some(&entry.tools)
    }

    /// Check if tools are cached for a server (without updating stats)
    pub fn has_tools(&self, server_id: &str) -> bool {
        if let Some(entry) = self.servers.get(server_id) {
            entry.cached_at.elapsed() <= self.config.ttl()
        } else {
            false
        }
    }

    /// Cache tool definitions for a server
    pub fn set_tools(&mut self, server_id: &str, tools: Vec<CachedToolDefinition>) {
        // Enforce max servers limit (simple LRU-ish eviction)
        if self.servers.len() >= self.config.max_servers && !self.servers.contains_key(server_id) {
            // Remove oldest entry
            if let Some(oldest_key) = self.find_oldest_server() {
                self.servers.remove(&oldest_key);
            }
        }

        self.servers.insert(
            server_id.to_string(),
            ServerCacheEntry {
                tools,
                cached_at: Instant::now(),
            },
        );
    }

    /// Invalidate cache for a specific server
    pub fn invalidate(&mut self, server_id: &str) {
        self.servers.remove(server_id);
    }

    /// Invalidate all cached entries
    pub fn invalidate_all(&mut self) {
        self.servers.clear();
    }

    /// Remove expired entries
    pub fn cleanup_expired(&mut self) {
        let ttl = self.config.ttl();
        self.servers
            .retain(|_, entry| entry.cached_at.elapsed() <= ttl);
    }

    /// Get cache statistics
    pub fn stats(&self) -> McpCacheStats {
        let total = self.hits + self.misses;
        let hit_rate = if total > 0 {
            self.hits as f64 / total as f64
        } else {
            0.0
        };

        McpCacheStats {
            servers_cached: self.servers.len(),
            total_tools: self.servers.values().map(|e| e.tools.len()).sum(),
            hits: self.hits,
            misses: self.misses,
            hit_rate,
        }
    }

    /// Get list of cached server IDs
    pub fn cached_servers(&self) -> Vec<&str> {
        self.servers.keys().map(|s| s.as_str()).collect()
    }

    /// Find the oldest cached server (for eviction)
    fn find_oldest_server(&self) -> Option<String> {
        self.servers
            .iter()
            .min_by_key(|(_, entry)| entry.cached_at)
            .map(|(k, _)| k.clone())
    }
}

impl Default for McpCache {
    fn default() -> Self {
        Self::new()
    }
}

/// MCP cache statistics
#[derive(Debug, Clone)]
pub struct McpCacheStats {
    pub servers_cached: usize,
    pub total_tools: usize,
    pub hits: u64,
    pub misses: u64,
    pub hit_rate: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_test_tools() -> Vec<CachedToolDefinition> {
        vec![
            CachedToolDefinition {
                name: "search".to_string(),
                description: "Search files".to_string(),
                input_schema: json!({"type": "object"}),
            },
            CachedToolDefinition {
                name: "create".to_string(),
                description: "Create item".to_string(),
                input_schema: json!({"type": "object"}),
            },
        ]
    }

    #[test]
    fn test_cache_miss_then_hit() {
        let mut cache = McpCache::new();

        // Miss
        assert!(cache.get_tools("server1").is_none());

        // Set
        cache.set_tools("server1", create_test_tools());

        // Hit
        let tools = cache.get_tools("server1");
        assert!(tools.is_some());
        assert_eq!(tools.unwrap().len(), 2);
    }

    #[test]
    fn test_ttl_expiration() {
        let mut cache = McpCache::with_ttl(Duration::from_millis(1));

        cache.set_tools("server1", create_test_tools());

        // Wait for TTL to expire
        std::thread::sleep(Duration::from_millis(10));

        assert!(cache.get_tools("server1").is_none());
    }

    #[test]
    fn test_invalidation() {
        let mut cache = McpCache::new();

        cache.set_tools("server1", create_test_tools());
        assert!(cache.has_tools("server1"));

        cache.invalidate("server1");
        assert!(!cache.has_tools("server1"));
    }

    #[test]
    fn test_max_servers_eviction() {
        let config = McpCacheConfig {
            ttl_secs: 3600,
            max_servers: 2,
        };
        let mut cache = McpCache::with_config(config);

        cache.set_tools("server1", create_test_tools());
        cache.set_tools("server2", create_test_tools());
        cache.set_tools("server3", create_test_tools()); // Should evict server1

        assert_eq!(cache.servers.len(), 2);
        // server1 should be evicted (oldest)
        assert!(!cache.has_tools("server1"));
    }
}
