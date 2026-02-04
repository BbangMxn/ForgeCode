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
#[derive(Debug)]
pub struct LruCache<K, V> {
    /// Storage for cached items
    entries: HashMap<K, LruEntry<V>>,
    /// Maximum number of entries
    capacity: usize,
    /// Access counter for LRU tracking
    access_counter: u64,
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
        Self {
            entries: HashMap::with_capacity(capacity),
            capacity,
            access_counter: 0,
        }
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
        self.access_counter += 1;

        // Check if key already exists
        if let Some(entry) = self.entries.get_mut(&key) {
            let old_value = std::mem::replace(&mut entry.value, value);
            entry.last_access = self.access_counter;
            entry.size_bytes = size_bytes;
            return Some(old_value);
        }

        // Evict if at capacity
        if self.entries.len() >= self.capacity {
            self.evict_lru();
        }

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
        self.entries.remove(key).map(|e| e.value)
    }

    /// Remove all entries matching a predicate
    pub fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(&K, &V) -> bool,
    {
        self.entries.retain(|k, e| f(k, &e.value));
    }

    /// Clear all entries
    pub fn clear(&mut self) {
        self.entries.clear();
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
        self.capacity
    }

    /// Estimate total memory usage in bytes
    pub fn estimated_memory_bytes(&self) -> usize {
        self.entries.values().map(|e| e.size_bytes).sum()
    }

    /// Evict the least recently used entry
    fn evict_lru(&mut self) {
        if let Some(lru_key) = self.find_lru_key() {
            self.entries.remove(&lru_key);
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
        let total_bytes: usize = self.entries.values().map(|e| e.size_bytes).sum();
        let oldest = self.entries.values().map(|e| e.created_at).min();

        LruCacheStats {
            entries: self.entries.len(),
            capacity: self.capacity,
            total_bytes,
            oldest_entry: oldest,
        }
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct LruCacheStats {
    pub entries: usize,
    pub capacity: usize,
    pub total_bytes: usize,
    pub oldest_entry: Option<Instant>,
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
}
