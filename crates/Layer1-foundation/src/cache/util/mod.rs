//! Cache utilities
//!
//! Provides caching primitives for ForgeCode:
//! - `LruCache`: Simple LRU cache
//! - `TtlLruCache`: LRU cache with TTL support
//! - `TwoLevelCache`: Two-level cache (L1 hot + L2 warm)

mod hash;
mod lru;
mod two_level;

pub use hash::{compute_hash, hash_file_content, hash_file_content_fast, hash_json, CompositeKey};
pub use lru::{LruCache, LruCacheConfig, LruCacheStats, TtlLruCache};
pub use two_level::{ArcTwoLevelCache, CacheStats, TwoLevelCache, TwoLevelCacheBuilder};
