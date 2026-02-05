//! Lightweight LRU Cache implementation
//!
//! Minimal dependencies, optimized for ForgeCode's use case.

use std::collections::HashMap;
use std::hash::Hash;
use std::time::Instant;

/// A simple LRU (Least Recently Used) cache
///
/// This implementation prioritizes simplicity and low memory overhead
/// over maximum performance. For most ForgeCode use cases, this is sufficient.
/// Configuration for LRU cache with memory limits
#[derive(Debug, Clone)]
pub struct LruCacheConfig {
    /// Maximum number of entries
    pub max_entries: usize,
    /// Maximum memory in bytes (0 = unlimited)
    pub max_bytes: usize,
    /// Maximum size per entry in bytes (0 = unlimited)
    pub max_entry_bytes: usize,
}

impl Default for LruCacheConfig {
    fn default() -> Self {
        Self {
            max_entries: 100,
            max_bytes: 0,
            max_entry_bytes: 0,
        }
    }
}

impl LruCacheConfig {
    /// Create config with entry limit only
    pub fn with_entries(max_entries: usize) -> Self {
        Self {
            max_entries,
            ..Default::default()
        }
    }

    /// Create config with memory limit
    pub fn with_memory(max_entries: usize, max_bytes: usize) -> Self {
        Self {
            max_entries,
            max_bytes,
            max_entry_bytes: 0,
        }
    }

    /// Create config with all limits
    pub fn with_limits(max_entries: usize, max_bytes: usize, max_entry_bytes: usize) -> Self {
        Self {
            max_entries,
            max_bytes,
            max_entry_bytes,
        }
    }
}

#[derive(Debug)]
pub struct LruCache<K, V> {
    /// Storage for cached items
    entries: HashMap<K, LruEntry<V>>,
    /// Configuration
    config: LruCacheConfig,
    /// Access counter for LRU tracking
    access_counter: u64,
    /// Current total memory usage
    current_bytes: usize,
}

#[derive(Debug)]
struct LruEntry<V> {
    value: V,
    last_access: u64,
    created_at: Instant,
    size_bytes: usize,
}

impl<K: Eq + Hash + Clone, V> LruCache<K, V> {
    /// Create a new LRU cache with the given capacity
    pub fn new(capacity: usize) -> Self {
        Self::with_config(LruCacheConfig::with_entries(capacity))
    }

    /// Create a new LRU cache with configuration
    pub fn with_config(config: LruCacheConfig) -> Self {
        Self {
            entries: HashMap::with_capacity(config.max_entries),
            config,
            access_counter: 0,
            current_bytes: 0,
        }
    }

    /// Create a new LRU cache with memory limit
    pub fn with_memory_limit(max_entries: usize, max_bytes: usize) -> Self {
        Self::with_config(LruCacheConfig::with_memory(max_entries, max_bytes))
    }

    /// Get a reference to a cached value
    ///
    /// Updates the access time for LRU tracking.
    pub fn get(&mut self, key: &K) -> Option<&V> {
        self.access_counter += 1;
        if let Some(entry) = self.entries.get_mut(key) {
            entry.last_access = self.access_counter;
            Some(&entry.value)
        } else {
            None
        }
    }

    /// Get a mutable reference to a cached value
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.access_counter += 1;
        if let Some(entry) = self.entries.get_mut(key) {
            entry.last_access = self.access_counter;
            Some(&mut entry.value)
        } else {
            None
        }
    }

    /// Check if a key exists without updating access time
    pub fn contains(&self, key: &K) -> bool {
        self.entries.contains_key(key)
    }

    /// Insert a value into the cache
    ///
    /// If the cache is at capacity, the least recently used item is evicted.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.insert_with_size(key, value, 0)
    }

    /// Insert a value with a known size (for memory tracking)
    pub fn insert_with_size(&mut self, key: K, value: V, size_bytes: usize) -> Option<V> {
        // Check max entry size limit
        if self.config.max_entry_bytes > 0 && size_bytes > self.config.max_entry_bytes {
            return None; // Reject oversized entry
        }

        self.access_counter += 1;

        // Check if key already exists
        if let Some(entry) = self.entries.get_mut(&key) {
            let old_size = entry.size_bytes;
            let old_value = std::mem::replace(&mut entry.value, value);
            entry.last_access = self.access_counter;
            entry.size_bytes = size_bytes;
            // Update byte tracking
            self.current_bytes = self.current_bytes.saturating_sub(old_size) + size_bytes;
            return Some(old_value);
        }

        // Evict if at entry capacity
        while self.entries.len() >= self.config.max_entries {
            self.evict_lru();
        }

        // Evict if at memory capacity
        if self.config.max_bytes > 0 {
            while self.current_bytes + size_bytes > self.config.max_bytes && !self.entries.is_empty()
            {
                self.evict_lru();
            }
        }

        self.current_bytes += size_bytes;
        self.entries.insert(
            key,
            LruEntry {
                value,
                last_access: self.access_counter,
                created_at: Instant::now(),
                size_bytes,
            },
        );

        None
    }

    /// Remove a specific key from the cache
    pub fn remove(&mut self, key: &K) -> Option<V> {
        self.entries.remove(key).map(|e| {
            self.current_bytes = self.current_bytes.saturating_sub(e.size_bytes);
            e.value
        })
    }

    /// Remove all entries matching a predicate
    pub fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(&K, &V) -> bool,
    {
        let mut removed_bytes = 0usize;
        self.entries.retain(|k, e| {
            let keep = f(k, &e.value);
            if !keep {
                removed_bytes += e.size_bytes;
            }
            keep
        });
        self.current_bytes = self.current_bytes.saturating_sub(removed_bytes);
    }

    /// Clear all entries
    pub fn clear(&mut self) {
        self.entries.clear();
        self.current_bytes = 0;
    }

    /// Get the number of entries
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the cache is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get the capacity
    pub fn capacity(&self) -> usize {
        self.config.max_entries
    }

    /// Get the maximum memory limit in bytes (0 = unlimited)
    pub fn max_bytes(&self) -> usize {
        self.config.max_bytes
    }

    /// Get total memory usage in bytes
    pub fn current_bytes(&self) -> usize {
        self.current_bytes
    }

    /// Estimate total memory usage in bytes (alias for backwards compatibility)
    pub fn estimated_memory_bytes(&self) -> usize {
        self.current_bytes
    }

    /// Check if cache is at memory limit
    pub fn is_at_memory_limit(&self) -> bool {
        self.config.max_bytes > 0 && self.current_bytes >= self.config.max_bytes
    }

    /// Evict the least recently used entry
    fn evict_lru(&mut self) {
        if let Some(lru_key) = self.find_lru_key() {
            if let Some(entry) = self.entries.remove(&lru_key) {
                self.current_bytes = self.current_bytes.saturating_sub(entry.size_bytes);
            }
        }
    }

    /// Find the key with the oldest access time
    fn find_lru_key(&self) -> Option<K> {
        self.entries
            .iter()
            .min_by_key(|(_, e)| e.last_access)
            .map(|(k, _)| k.clone())
    }

    /// Get cache statistics
    pub fn stats(&self) -> LruCacheStats {
        let oldest = self.entries.values().map(|e| e.created_at).min();
        let memory_utilization = if self.config.max_bytes > 0 {
            self.current_bytes as f64 / self.config.max_bytes as f64
        } else {
            0.0
        };

        LruCacheStats {
            entries: self.entries.len(),
            capacity: self.config.max_entries,
            total_bytes: self.current_bytes,
            max_bytes: self.config.max_bytes,
            oldest_entry: oldest,
            memory_utilization,
        }
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct LruCacheStats {
    pub entries: usize,
    pub capacity: usize,
    pub total_bytes: usize,
    pub max_bytes: usize,
    pub oldest_entry: Option<Instant>,
    /// Memory utilization (0.0 - 1.0, or 0 if unlimited)
    pub memory_utilization: f64,
}

/// LRU Cache with TTL (Time-To-Live) support
#[derive(Debug)]
pub struct TtlLruCache<K, V> {
    inner: LruCache<K, TtlEntry<V>>,
    default_ttl: std::time::Duration,
}

#[derive(Debug)]
struct TtlEntry<V> {
    value: V,
    expires_at: Instant,
}

impl<K: Eq + Hash + Clone, V> TtlLruCache<K, V> {
    /// Create a new TTL-enabled LRU cache
    pub fn new(capacity: usize, default_ttl: std::time::Duration) -> Self {
        Self {
            inner: LruCache::new(capacity),
            default_ttl,
        }
    }

    /// Get a value if it exists and hasn't expired
    pub fn get(&mut self, key: &K) -> Option<&V> {
        // First check if entry exists and is valid
        let now = Instant::now();
        if let Some(entry) = self.inner.get(key) {
            if entry.expires_at > now {
                // Re-borrow to return reference
                return self.inner.get(key).map(|e| &e.value);
            } else {
                // Entry expired, remove it
                self.inner.remove(key);
            }
        }
        None
    }

    /// Insert a value with the default TTL
    pub fn insert(&mut self, key: K, value: V) {
        self.insert_with_ttl(key, value, self.default_ttl);
    }

    /// Insert a value with a custom TTL
    pub fn insert_with_ttl(&mut self, key: K, value: V, ttl: std::time::Duration) {
        let entry = TtlEntry {
            value,
            expires_at: Instant::now() + ttl,
        };
        self.inner.insert(key, entry);
    }

    /// Remove expired entries
    pub fn cleanup_expired(&mut self) {
        let now = Instant::now();
        self.inner.retain(|_, e| e.expires_at > now);
    }

    /// Remove a specific key
    pub fn remove(&mut self, key: &K) -> Option<V> {
        self.inner.remove(key).map(|e| e.value)
    }

    /// Clear all entries
    pub fn clear(&mut self) {
        self.inner.clear();
    }

    /// Get the number of entries (including potentially expired ones)
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lru_basic() {
        let mut cache = LruCache::new(3);

        cache.insert("a", 1);
        cache.insert("b", 2);
        cache.insert("c", 3);

        assert_eq!(cache.get(&"a"), Some(&1));
        assert_eq!(cache.get(&"b"), Some(&2));
        assert_eq!(cache.get(&"c"), Some(&3));
        assert_eq!(cache.len(), 3);
    }

    #[test]
    fn test_lru_eviction() {
        let mut cache = LruCache::new(2);

        cache.insert("a", 1);
        cache.insert("b", 2);

        // Access "a" to make it more recent
        cache.get(&"a");

        // Insert "c", should evict "b" (least recently used)
        cache.insert("c", 3);

        assert_eq!(cache.get(&"a"), Some(&1));
        assert_eq!(cache.get(&"b"), None); // evicted
        assert_eq!(cache.get(&"c"), Some(&3));
    }

    #[test]
    fn test_lru_update() {
        let mut cache = LruCache::new(2);

        cache.insert("a", 1);
        let old = cache.insert("a", 10);

        assert_eq!(old, Some(1));
        assert_eq!(cache.get(&"a"), Some(&10));
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_memory_limit_eviction() {
        // Cache with max 100 bytes
        let mut cache: LruCache<&str, String> =
            LruCache::with_config(LruCacheConfig::with_memory(10, 100));

        // Insert 50 bytes
        cache.insert_with_size("a", "value_a".to_string(), 50);
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.current_bytes(), 50);

        // Insert another 40 bytes (total: 90)
        cache.insert_with_size("b", "value_b".to_string(), 40);
        assert_eq!(cache.len(), 2);
        assert_eq!(cache.current_bytes(), 90);

        // Insert 60 bytes - should evict "a" (oldest, 50 bytes) to make room
        // After eviction: 40 + 60 = 100 bytes
        cache.insert_with_size("c", "value_c".to_string(), 60);
        assert_eq!(cache.len(), 2); // "a" evicted
        assert_eq!(cache.current_bytes(), 100); // 40 + 60
        assert!(cache.get(&"a").is_none());
        assert!(cache.get(&"b").is_some());
        assert!(cache.get(&"c").is_some());
    }

    #[test]
    fn test_max_entry_size_rejection() {
        // Cache with 50 bytes max per entry
        let mut cache: LruCache<&str, String> =
            LruCache::with_config(LruCacheConfig::with_limits(10, 1000, 50));

        // Small entry should be accepted
        let result = cache.insert_with_size("small", "x".to_string(), 10);
        assert!(result.is_none()); // No old value
        assert_eq!(cache.len(), 1);

        // Large entry should be rejected
        let result = cache.insert_with_size("large", "y".to_string(), 100);
        assert!(result.is_none()); // Rejected, no old value
        assert_eq!(cache.len(), 1); // Still only 1 entry
        assert!(cache.get(&"large").is_none());
    }

    #[test]
    fn test_memory_tracking_accuracy() {
        let mut cache: LruCache<i32, String> =
            LruCache::with_config(LruCacheConfig::with_memory(100, 0)); // No byte limit, just tracking

        cache.insert_with_size(1, "a".to_string(), 10);
        cache.insert_with_size(2, "b".to_string(), 20);
        cache.insert_with_size(3, "c".to_string(), 30);
        assert_eq!(cache.current_bytes(), 60);

        // Remove entry
        cache.remove(&2);
        assert_eq!(cache.current_bytes(), 40);

        // Update entry with different size
        cache.insert_with_size(1, "aa".to_string(), 15);
        assert_eq!(cache.current_bytes(), 45); // 40 - 10 + 15

        // Clear
        cache.clear();
        assert_eq!(cache.current_bytes(), 0);
    }

    #[test]
    fn test_stats_with_memory() {
        let mut cache: LruCache<&str, String> =
            LruCache::with_config(LruCacheConfig::with_memory(10, 100));

        cache.insert_with_size("a", "test".to_string(), 25);
        cache.insert_with_size("b", "test2".to_string(), 25);

        let stats = cache.stats();
        assert_eq!(stats.entries, 2);
        assert_eq!(stats.total_bytes, 50);
        assert_eq!(stats.max_bytes, 100);
        assert!((stats.memory_utilization - 0.5).abs() < 0.01);
    }
}
