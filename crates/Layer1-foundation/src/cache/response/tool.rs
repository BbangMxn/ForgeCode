//! Tool Result Cache
//!
//! Caches results from pure/idempotent tools like Read, Glob, Grep.
//! Tools with side effects (Bash, Write, Edit) are NOT cached.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::cache::util::{hash_json, LruCache};

/// Configuration for tool caching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCacheConfig {
    /// Maximum number of entries
    pub max_entries: usize,
    /// List of cacheable tool names
    pub cacheable_tools: Vec<String>,
    /// Enable file-based invalidation
    pub enable_file_invalidation: bool,
}

impl Default for ToolCacheConfig {
    fn default() -> Self {
        Self {
            max_entries: 100,
            cacheable_tools: vec!["Read".to_string(), "Glob".to_string(), "Grep".to_string()],
            enable_file_invalidation: true,
        }
    }
}

/// Cache key for tool results
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ToolCacheKey {
    tool_name: String,
    args_hash: u64,
}

impl ToolCacheKey {
    pub fn new(tool_name: &str, args: &Value) -> Self {
        Self {
            tool_name: tool_name.to_string(),
            args_hash: hash_json(args),
        }
    }
}

/// Cached tool result entry
#[derive(Debug, Clone)]
pub struct CachedToolResult {
    /// The cached result
    pub output: String,
    /// Whether the tool execution was successful
    pub success: bool,
    /// When the result was cached
    pub cached_at: Instant,
    /// Files involved (for invalidation)
    pub involved_files: Vec<PathBuf>,
    /// Size of the result in bytes
    pub size_bytes: usize,
}

/// Tool Result Cache
///
/// Caches results from pure/idempotent tools to avoid redundant executions.
///
/// # Cacheable Tools
///
/// Only tools that are guaranteed to produce the same output for the same
/// input (given no file changes) are cached:
///
/// - **Read**: File content (invalidated on file change)
/// - **Glob**: File patterns (invalidated on file create/delete)
/// - **Grep**: Search results (invalidated on file change)
///
/// # Non-cacheable Tools
///
/// Tools with side effects are NEVER cached:
/// - Bash (can modify system state)
/// - Write/Edit (modifies files)
/// - WebFetch (external state)
///
/// # Example
///
/// ```rust,ignore
/// let mut cache = ToolCache::new(100);
///
/// // Try to get cached result
/// if let Some(result) = cache.get("Read", &args) {
///     return result.clone();
/// }
///
/// // Execute tool and cache result
/// let result = execute_tool("Read", &args).await?;
/// cache.insert("Read", &args, result.clone());
/// ```
#[derive(Debug)]
pub struct ToolCache {
    config: ToolCacheConfig,
    cache: LruCache<ToolCacheKey, CachedToolResult>,
    cacheable_tools: HashSet<String>,
    /// Track which files are involved in cached entries
    file_to_keys: std::collections::HashMap<PathBuf, Vec<ToolCacheKey>>,
    /// Statistics
    hits: u64,
    misses: u64,
}

impl ToolCache {
    /// Create a new tool cache with default settings
    pub fn new(max_entries: usize) -> Self {
        let config = ToolCacheConfig {
            max_entries,
            ..Default::default()
        };
        Self::with_config(config)
    }

    /// Create a tool cache with custom configuration
    pub fn with_config(config: ToolCacheConfig) -> Self {
        let cacheable_tools: HashSet<String> = config.cacheable_tools.iter().cloned().collect();

        Self {
            cache: LruCache::new(config.max_entries),
            cacheable_tools,
            file_to_keys: std::collections::HashMap::new(),
            hits: 0,
            misses: 0,
            config,
        }
    }

    /// Check if a tool is cacheable
    pub fn is_cacheable(&self, tool_name: &str) -> bool {
        self.cacheable_tools.contains(tool_name)
    }

    /// Get a cached tool result
    pub fn get(&mut self, tool_name: &str, args: &Value) -> Option<&CachedToolResult> {
        if !self.is_cacheable(tool_name) {
            return None;
        }

        let key = ToolCacheKey::new(tool_name, args);

        if let Some(result) = self.cache.get(&key) {
            self.hits += 1;
            Some(result)
        } else {
            self.misses += 1;
            None
        }
    }

    /// Insert a tool result into the cache
    pub fn insert(
        &mut self,
        tool_name: &str,
        args: &Value,
        output: String,
        success: bool,
        involved_files: Vec<PathBuf>,
    ) {
        if !self.is_cacheable(tool_name) {
            return;
        }

        let key = ToolCacheKey::new(tool_name, args);
        let size_bytes = output.len();

        // Track file associations for invalidation
        if self.config.enable_file_invalidation {
            for file in &involved_files {
                self.file_to_keys
                    .entry(file.clone())
                    .or_default()
                    .push(key.clone());
            }
        }

        let result = CachedToolResult {
            output,
            success,
            cached_at: Instant::now(),
            involved_files,
            size_bytes,
        };

        self.cache.insert_with_size(key, result, size_bytes);
    }

    /// Invalidate cache entries related to a file
    pub fn invalidate_for_file(&mut self, path: &Path) {
        if !self.config.enable_file_invalidation {
            return;
        }

        // Get all keys associated with this file
        if let Some(keys) = self.file_to_keys.remove(path) {
            for key in keys {
                self.cache.remove(&key);
            }
        }

        // Also check for directory-based invalidation (for Glob)
        let path_str = path.to_string_lossy();
        let keys_to_remove: Vec<ToolCacheKey> = self
            .file_to_keys
            .iter()
            .filter(|(p, _)| {
                let p_str = p.to_string_lossy();
                path_str.starts_with(p_str.as_ref()) || p_str.starts_with(path_str.as_ref())
            })
            .flat_map(|(_, keys)| keys.clone())
            .collect();

        for key in keys_to_remove {
            self.cache.remove(&key);
        }
    }

    /// Invalidate all cache entries
    pub fn invalidate_all(&mut self) {
        self.cache.clear();
        self.file_to_keys.clear();
    }

    /// Get cache statistics
    pub fn stats(&self) -> ToolCacheStats {
        let total_requests = self.hits + self.misses;
        let hit_rate = if total_requests > 0 {
            self.hits as f64 / total_requests as f64
        } else {
            0.0
        };

        ToolCacheStats {
            entries: self.cache.len(),
            capacity: self.config.max_entries,
            hits: self.hits,
            misses: self.misses,
            hit_rate,
            memory_bytes: self.cache.estimated_memory_bytes(),
        }
    }

    /// Get the number of cached entries
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
}

/// Tool cache statistics
#[derive(Debug, Clone)]
pub struct ToolCacheStats {
    pub entries: usize,
    pub capacity: usize,
    pub hits: u64,
    pub misses: u64,
    pub hit_rate: f64,
    pub memory_bytes: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_cacheable_tools() {
        let cache = ToolCache::new(10);

        assert!(cache.is_cacheable("Read"));
        assert!(cache.is_cacheable("Glob"));
        assert!(cache.is_cacheable("Grep"));
        assert!(!cache.is_cacheable("Bash"));
        assert!(!cache.is_cacheable("Write"));
    }

    #[test]
    fn test_cache_hit_miss() {
        let mut cache = ToolCache::new(10);
        let args = json!({"path": "/test/file.rs"});

        // Miss
        assert!(cache.get("Read", &args).is_none());

        // Insert
        cache.insert("Read", &args, "content".to_string(), true, vec![]);

        // Hit
        let result = cache.get("Read", &args);
        assert!(result.is_some());
        assert_eq!(result.unwrap().output, "content");
    }

    #[test]
    fn test_file_invalidation() {
        let mut cache = ToolCache::new(10);
        let path = PathBuf::from("/test/file.rs");
        let args = json!({"path": "/test/file.rs"});

        cache.insert(
            "Read",
            &args,
            "content".to_string(),
            true,
            vec![path.clone()],
        );
        assert!(cache.get("Read", &args).is_some());

        // Invalidate
        cache.invalidate_for_file(&path);
        assert!(cache.get("Read", &args).is_none());
    }

    #[test]
    fn test_non_cacheable_not_stored() {
        let mut cache = ToolCache::new(10);
        let args = json!({"command": "ls"});

        cache.insert("Bash", &args, "output".to_string(), true, vec![]);
        assert!(cache.get("Bash", &args).is_none());
    }

    #[test]
    fn test_stats() {
        let mut cache = ToolCache::new(10);
        let args = json!({"path": "/test"});

        cache.get("Read", &args); // miss
        cache.insert("Read", &args, "x".to_string(), true, vec![]);
        cache.get("Read", &args); // hit
        cache.get("Read", &args); // hit

        let stats = cache.stats();
        assert_eq!(stats.hits, 2);
        assert_eq!(stats.misses, 1);
        assert!((stats.hit_rate - 0.666).abs() < 0.01);
    }
}
