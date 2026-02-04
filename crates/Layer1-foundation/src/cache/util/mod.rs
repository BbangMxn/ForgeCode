//! Cache utilities

mod hash;
mod lru;

pub use hash::{compute_hash, hash_file_content, hash_file_content_fast, hash_json, CompositeKey};
pub use lru::{LruCache, LruCacheStats, TtlLruCache};
