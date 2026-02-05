//! Two-Level Cache Implementation
//!
//! Implements a CPU cache-inspired hierarchy:
//! - L1: Small, fast hot cache (no TTL, pure LRU)
//! - L2: Larger warm cache with TTL support
//!
//! Access pattern:
//! 1. Check L1 first (fastest)
//! 2. On L1 miss, check L2
//! 3. On L2 hit, promote to L1
//! 4. On L2 miss, fetch from source and insert into both
//!
//! This pattern optimizes for:
//! - Hot data: Frequently accessed items stay in L1
//! - Warm data: Recently accessed items stay in L2
//! - Memory efficiency: L1 is small, L2 handles overflow

use super::lru::{LruCache, TtlLruCache};
use std::hash::Hash;
use std::time::Duration;

/// Two-level cache with L1 (hot) and L2 (warm) tiers
///
/// # Type Parameters
/// - `K`: Key type (must be Clone for promotion between levels)
/// - `V`: Value type (must be Clone for promotion between levels)
///
/// # Example
/// ```ignore
/// use forge_foundation::cache::util::TwoLevelCache;
/// use std::time::Duration;
///
/// let mut cache = TwoLevelCache::new(32, 256, Duration::from_secs(300));
/// cache.insert("key".to_string(), "value".to_string());
///
/// // First access from L1 or L2
/// if let Some(value) = cache.get(&"key".to_string()) {
///     println!("Got: {}", value);
/// }
/// ```
#[derive(Debug)]
pub struct TwoLevelCache<K, V> {
    /// L1: Hot cache - small, fast, no TTL
    l1: LruCache<K, V>,
    /// L2: Warm cache - larger, with TTL
    l2: TtlLruCache<K, V>,
    /// Statistics
    stats: CacheStats,
}

/// Cache statistics for monitoring
#[derive(Debug, Default, Clone)]
pub struct CacheStats {
    /// L1 cache hits
    pub l1_hits: u64,
    /// L2 cache hits (promoted to L1)
    pub l2_hits: u64,
    /// Total misses
    pub misses: u64,
    /// Number of L1 -> L2 demotions
    pub demotions: u64,
    /// Number of L2 -> L1 promotions
    pub promotions: u64,
}

impl CacheStats {
    /// Calculate overall hit rate
    #[inline]
    pub fn hit_rate(&self) -> f64 {
        let total = self.l1_hits + self.l2_hits + self.misses;
        if total == 0 {
            return 0.0;
        }
        (self.l1_hits + self.l2_hits) as f64 / total as f64
    }

    /// Calculate L1 hit rate (of all hits)
    #[inline]
    pub fn l1_hit_rate(&self) -> f64 {
        let total_hits = self.l1_hits + self.l2_hits;
        if total_hits == 0 {
            return 0.0;
        }
        self.l1_hits as f64 / total_hits as f64
    }
}

impl<K, V> TwoLevelCache<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    /// Create a new two-level cache
    ///
    /// # Arguments
    /// - `l1_capacity`: L1 hot cache capacity (recommend: 16-64)
    /// - `l2_capacity`: L2 warm cache capacity (recommend: 4-8x L1)
    /// - `l2_ttl`: Time-to-live for L2 entries
    pub fn new(l1_capacity: usize, l2_capacity: usize, l2_ttl: Duration) -> Self {
        Self {
            l1: LruCache::new(l1_capacity),
            l2: TtlLruCache::new(l2_capacity, l2_ttl),
            stats: CacheStats::default(),
        }
    }

    /// Create with typical defaults for code caching
    ///
    /// L1: 32 entries, L2: 256 entries, 5 minute TTL
    pub fn default_for_code() -> Self {
        Self::new(32, 256, Duration::from_secs(300))
    }

    /// Create with typical defaults for token estimation caching
    ///
    /// L1: 64 entries, L2: 512 entries, 10 minute TTL
    pub fn default_for_tokens() -> Self {
        Self::new(64, 512, Duration::from_secs(600))
    }

    /// Get a value from the cache
    ///
    /// Checks L1 first, then L2. On L2 hit, promotes to L1.
    #[inline]
    pub fn get(&mut self, key: &K) -> Option<&V> {
        // Fast path: check L1 first
        if self.l1.contains(key) {
            self.stats.l1_hits += 1;
            return self.l1.get(key);
        }

        // Slow path: check L2
        if let Some(value) = self.l2.get(key) {
            self.stats.l2_hits += 1;
            self.stats.promotions += 1;

            // Promote to L1 (clone required)
            let value_clone = value.clone();
            self.l1.insert(key.clone(), value_clone);

            // Return reference from L1
            return self.l1.get(key);
        }

        self.stats.misses += 1;
        None
    }

    /// Get a value without promotion (peek)
    ///
    /// Useful when you want to check existence without affecting cache state
    pub fn peek(&mut self, key: &K) -> Option<&V> {
        if let Some(v) = self.l1.get(key) {
            return Some(v);
        }
        self.l2.get(key)
    }

    /// Insert a value into both cache levels
    ///
    /// Inserts into L1 (hot) and L2 (warm) for durability
    #[inline]
    pub fn insert(&mut self, key: K, value: V) {
        // Insert into L1
        self.l1.insert(key.clone(), value.clone());
        // Insert into L2 as backup
        self.l2.insert(key, value);
    }

    /// Insert only into L2 (for less frequently accessed data)
    pub fn insert_cold(&mut self, key: K, value: V) {
        self.l2.insert(key, value);
    }

    /// Remove a key from both cache levels
    pub fn remove(&mut self, key: &K) -> Option<V> {
        let l1_result = self.l1.remove(key);
        let l2_result = self.l2.remove(key);
        l1_result.or(l2_result)
    }

    /// Clear both cache levels
    pub fn clear(&mut self) {
        self.l1.clear();
        self.l2.clear();
        self.stats = CacheStats::default();
    }

    /// Get cache statistics
    pub fn stats(&self) -> &CacheStats {
        &self.stats
    }

    /// Reset statistics
    pub fn reset_stats(&mut self) {
        self.stats = CacheStats::default();
    }

    /// Get L1 cache size
    pub fn l1_len(&self) -> usize {
        self.l1.len()
    }

    /// Get L2 cache size
    pub fn l2_len(&self) -> usize {
        self.l2.len()
    }

    /// Total entries across both levels (may have duplicates)
    pub fn total_entries(&self) -> usize {
        self.l1.len() + self.l2.len()
    }

    /// Cleanup expired L2 entries
    pub fn cleanup_expired(&mut self) {
        self.l2.cleanup_expired();
    }

    /// Demote cold entries from L1 to L2
    ///
    /// This is an advanced operation for manual cache management.
    /// Normally, LRU eviction handles this automatically.
    pub fn demote_oldest(&mut self, count: usize) {
        // This would require exposing more internals from LruCache
        // For now, let LRU handle it naturally
        let _ = count;
    }
}

/// Builder for TwoLevelCache with fluent API
pub struct TwoLevelCacheBuilder<K, V> {
    l1_capacity: usize,
    l2_capacity: usize,
    l2_ttl: Duration,
    _marker: std::marker::PhantomData<(K, V)>,
}

impl<K, V> Default for TwoLevelCacheBuilder<K, V> {
    fn default() -> Self {
        Self {
            l1_capacity: 32,
            l2_capacity: 256,
            l2_ttl: Duration::from_secs(300),
            _marker: std::marker::PhantomData,
        }
    }
}

impl<K, V> TwoLevelCacheBuilder<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    /// Create a new builder with defaults
    pub fn new() -> Self {
        Self::default()
    }

    /// Set L1 (hot cache) capacity
    pub fn l1_capacity(mut self, capacity: usize) -> Self {
        self.l1_capacity = capacity;
        self
    }

    /// Set L2 (warm cache) capacity
    pub fn l2_capacity(mut self, capacity: usize) -> Self {
        self.l2_capacity = capacity;
        self
    }

    /// Set L2 TTL
    pub fn l2_ttl(mut self, ttl: Duration) -> Self {
        self.l2_ttl = ttl;
        self
    }

    /// Build the cache
    pub fn build(self) -> TwoLevelCache<K, V> {
        TwoLevelCache::new(self.l1_capacity, self.l2_capacity, self.l2_ttl)
    }
}

// ============================================================================
// Arc-based Two-Level Cache (Zero-Copy Promotion)
// ============================================================================

use std::sync::Arc;

/// Two-level cache with Arc-based values for zero-copy promotion
///
/// This variant wraps values in Arc internally, so L2 -> L1 promotion
/// only copies the Arc pointer (cheap) instead of cloning the value.
///
/// Use this when:
/// - Values are large (> 64 bytes)
/// - Promotion happens frequently
/// - Values don't need to be mutated after insertion
///
/// # Type Parameters
/// - `K`: Key type (must be Clone for key duplication)
/// - `V`: Value type (no Clone required - wrapped in Arc)
#[derive(Debug)]
pub struct ArcTwoLevelCache<K, V> {
    /// L1: Hot cache with Arc<V>
    l1: LruCache<K, Arc<V>>,
    /// L2: Warm cache with Arc<V>
    l2: TtlLruCache<K, Arc<V>>,
    /// Statistics
    stats: CacheStats,
}

impl<K, V> ArcTwoLevelCache<K, V>
where
    K: Eq + Hash + Clone,
{
    /// Create a new Arc-based two-level cache
    pub fn new(l1_capacity: usize, l2_capacity: usize, l2_ttl: Duration) -> Self {
        Self {
            l1: LruCache::new(l1_capacity),
            l2: TtlLruCache::new(l2_capacity, l2_ttl),
            stats: CacheStats::default(),
        }
    }

    /// Create with typical defaults
    pub fn default_config() -> Self {
        Self::new(32, 256, Duration::from_secs(300))
    }

    /// Get a value from the cache (returns Arc reference)
    ///
    /// Zero-copy promotion: L2 -> L1 only copies Arc pointer
    #[inline]
    pub fn get(&mut self, key: &K) -> Option<Arc<V>> {
        // Fast path: check L1 first
        if let Some(value) = self.l1.get(key) {
            self.stats.l1_hits += 1;
            return Some(Arc::clone(value));
        }

        // Slow path: check L2
        if let Some(value) = self.l2.get(key) {
            self.stats.l2_hits += 1;
            self.stats.promotions += 1;

            // Zero-copy promotion: just clone the Arc (pointer copy)
            let arc_clone = Arc::clone(value);
            self.l1.insert(key.clone(), arc_clone.clone());

            return Some(arc_clone);
        }

        self.stats.misses += 1;
        None
    }

    /// Get a reference to the cached value
    pub fn get_ref(&mut self, key: &K) -> Option<&V> {
        self.get(key).map(|arc| {
            // SAFETY: Arc is valid as long as cache entry exists
            // We return &V which borrows from self
            unsafe { &*(Arc::as_ptr(&arc)) }
        })
    }

    /// Insert a value (wrapped in Arc internally)
    #[inline]
    pub fn insert(&mut self, key: K, value: V) {
        let arc = Arc::new(value);
        self.l1.insert(key.clone(), Arc::clone(&arc));
        self.l2.insert(key, arc);
    }

    /// Insert a pre-wrapped Arc value
    #[inline]
    pub fn insert_arc(&mut self, key: K, value: Arc<V>) {
        self.l1.insert(key.clone(), Arc::clone(&value));
        self.l2.insert(key, value);
    }

    /// Insert only into L2 (cold path)
    pub fn insert_cold(&mut self, key: K, value: V) {
        self.l2.insert(key, Arc::new(value));
    }

    /// Remove a key from both levels
    pub fn remove(&mut self, key: &K) -> Option<Arc<V>> {
        let l1_result = self.l1.remove(key);
        let l2_result = self.l2.remove(key);
        l1_result.or(l2_result)
    }

    /// Clear both cache levels
    pub fn clear(&mut self) {
        self.l1.clear();
        self.l2.clear();
        self.stats = CacheStats::default();
    }

    /// Get cache statistics
    pub fn stats(&self) -> &CacheStats {
        &self.stats
    }

    /// Get L1 cache size
    pub fn l1_len(&self) -> usize {
        self.l1.len()
    }

    /// Get L2 cache size
    pub fn l2_len(&self) -> usize {
        self.l2.len()
    }

    /// Cleanup expired L2 entries
    pub fn cleanup_expired(&mut self) {
        self.l2.cleanup_expired();
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_insert_get() {
        let mut cache = TwoLevelCache::new(2, 4, Duration::from_secs(60));

        cache.insert("a".to_string(), 1);
        cache.insert("b".to_string(), 2);

        assert_eq!(cache.get(&"a".to_string()), Some(&1));
        assert_eq!(cache.get(&"b".to_string()), Some(&2));
        assert_eq!(cache.get(&"c".to_string()), None);
    }

    #[test]
    fn test_l1_hit() {
        let mut cache = TwoLevelCache::new(2, 4, Duration::from_secs(60));

        cache.insert("a".to_string(), 1);

        // First get - L1 hit
        assert_eq!(cache.get(&"a".to_string()), Some(&1));
        assert_eq!(cache.stats().l1_hits, 1);
        assert_eq!(cache.stats().l2_hits, 0);

        // Second get - still L1 hit
        assert_eq!(cache.get(&"a".to_string()), Some(&1));
        assert_eq!(cache.stats().l1_hits, 2);
    }

    #[test]
    fn test_l2_promotion() {
        let mut cache = TwoLevelCache::new(1, 4, Duration::from_secs(60));

        // Insert a and b, but L1 only holds 1
        cache.insert("a".to_string(), 1);
        cache.insert("b".to_string(), 2); // This evicts "a" from L1

        // "a" should be in L2 only now
        // Getting "a" should promote it back to L1
        assert_eq!(cache.get(&"a".to_string()), Some(&1));

        // Should be L2 hit + promotion
        assert_eq!(cache.stats().l2_hits, 1);
        assert_eq!(cache.stats().promotions, 1);
    }

    #[test]
    fn test_miss() {
        let mut cache: TwoLevelCache<String, i32> =
            TwoLevelCache::new(2, 4, Duration::from_secs(60));

        assert_eq!(cache.get(&"nonexistent".to_string()), None);
        assert_eq!(cache.stats().misses, 1);
    }

    #[test]
    fn test_insert_cold() {
        let mut cache = TwoLevelCache::new(2, 4, Duration::from_secs(60));

        cache.insert_cold("cold".to_string(), 100);

        // Should be L2 hit, then promoted to L1
        assert_eq!(cache.get(&"cold".to_string()), Some(&100));
        assert_eq!(cache.stats().l2_hits, 1);
    }

    #[test]
    fn test_remove() {
        let mut cache = TwoLevelCache::new(2, 4, Duration::from_secs(60));

        cache.insert("a".to_string(), 1);
        assert_eq!(cache.remove(&"a".to_string()), Some(1));
        assert_eq!(cache.get(&"a".to_string()), None);
    }

    #[test]
    fn test_clear() {
        let mut cache = TwoLevelCache::new(2, 4, Duration::from_secs(60));

        cache.insert("a".to_string(), 1);
        cache.insert("b".to_string(), 2);

        cache.clear();

        assert_eq!(cache.l1_len(), 0);
        assert_eq!(cache.l2_len(), 0);
        assert_eq!(cache.stats().l1_hits, 0);
    }

    #[test]
    fn test_hit_rate() {
        let mut cache = TwoLevelCache::new(2, 4, Duration::from_secs(60));

        cache.insert("a".to_string(), 1);

        // 2 hits
        cache.get(&"a".to_string());
        cache.get(&"a".to_string());

        // 1 miss
        cache.get(&"nonexistent".to_string());

        let hit_rate = cache.stats().hit_rate();
        assert!((hit_rate - 0.666).abs() < 0.01); // 2/3 = ~0.666
    }

    #[test]
    fn test_builder() {
        let cache: TwoLevelCache<String, i32> = TwoLevelCacheBuilder::new()
            .l1_capacity(16)
            .l2_capacity(128)
            .l2_ttl(Duration::from_secs(120))
            .build();

        assert_eq!(cache.l1.capacity(), 16);
    }

    #[test]
    fn test_default_for_code() {
        let cache: TwoLevelCache<String, String> = TwoLevelCache::default_for_code();
        assert_eq!(cache.l1.capacity(), 32);
    }

    #[test]
    fn test_default_for_tokens() {
        let cache: TwoLevelCache<String, usize> = TwoLevelCache::default_for_tokens();
        assert_eq!(cache.l1.capacity(), 64);
    }

    // ========== ArcTwoLevelCache Tests ==========

    #[test]
    fn test_arc_basic_insert_get() {
        let mut cache = ArcTwoLevelCache::new(2, 4, Duration::from_secs(60));

        cache.insert("a".to_string(), "value_a".to_string());
        cache.insert("b".to_string(), "value_b".to_string());

        assert_eq!(cache.get(&"a".to_string()).as_deref(), Some(&"value_a".to_string()));
        assert_eq!(cache.get(&"b".to_string()).as_deref(), Some(&"value_b".to_string()));
        assert!(cache.get(&"c".to_string()).is_none());
    }

    #[test]
    fn test_arc_zero_copy_promotion() {
        let mut cache = ArcTwoLevelCache::new(1, 4, Duration::from_secs(60));

        // Insert a large value
        let large_value = "x".repeat(1000);
        cache.insert("a".to_string(), large_value.clone());
        cache.insert("b".to_string(), "b_value".to_string()); // Evicts "a" from L1

        // Get "a" - should promote from L2 to L1 (Arc pointer copy only)
        let arc1 = cache.get(&"a".to_string()).unwrap();
        assert_eq!(*arc1, large_value);

        // Stats should show L2 hit
        assert_eq!(cache.stats().l2_hits, 1);
        assert_eq!(cache.stats().promotions, 1);

        // Getting again should be L1 hit
        let arc2 = cache.get(&"a".to_string()).unwrap();
        assert_eq!(cache.stats().l1_hits, 1);

        // Both Arcs should point to same memory (zero-copy)
        assert!(Arc::ptr_eq(&arc1, &arc2));
    }

    #[test]
    fn test_arc_insert_pre_wrapped() {
        let mut cache = ArcTwoLevelCache::new(2, 4, Duration::from_secs(60));

        let shared = Arc::new("shared_value".to_string());
        cache.insert_arc("key".to_string(), Arc::clone(&shared));

        let retrieved = cache.get(&"key".to_string()).unwrap();
        assert!(Arc::ptr_eq(&shared, &retrieved));
    }
}
