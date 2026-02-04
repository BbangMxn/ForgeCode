//! Hashing utilities for cache keys

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Compute a hash for any hashable value
pub fn compute_hash<T: Hash>(value: &T) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

/// Compute a hash for a JSON value
///
/// Normalizes the JSON to ensure consistent hashing regardless of key order.
pub fn hash_json(value: &serde_json::Value) -> u64 {
    let mut hasher = DefaultHasher::new();
    hash_json_value(value, &mut hasher);
    hasher.finish()
}

fn hash_json_value(value: &serde_json::Value, hasher: &mut DefaultHasher) {
    use serde_json::Value;

    match value {
        Value::Null => {
            hasher.write_u8(0);
        }
        Value::Bool(b) => {
            hasher.write_u8(1);
            b.hash(hasher);
        }
        Value::Number(n) => {
            hasher.write_u8(2);
            // Convert to string for consistent hashing
            n.to_string().hash(hasher);
        }
        Value::String(s) => {
            hasher.write_u8(3);
            s.hash(hasher);
        }
        Value::Array(arr) => {
            hasher.write_u8(4);
            hasher.write_usize(arr.len());
            for item in arr {
                hash_json_value(item, hasher);
            }
        }
        Value::Object(obj) => {
            hasher.write_u8(5);
            hasher.write_usize(obj.len());
            // Sort keys for consistent hashing
            let mut keys: Vec<_> = obj.keys().collect();
            keys.sort();
            for key in keys {
                key.hash(hasher);
                if let Some(v) = obj.get(key) {
                    hash_json_value(v, hasher);
                }
            }
        }
    }
}

/// Compute a fast hash for file content
///
/// Uses a combination of length and sampled bytes for speed.
pub fn hash_file_content_fast(content: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();

    // Hash length
    hasher.write_usize(content.len());

    if content.len() <= 1024 {
        // Small files: hash everything
        hasher.write(content);
    } else {
        // Large files: sample beginning, middle, and end
        hasher.write(&content[..512]);

        let mid = content.len() / 2;
        hasher.write(&content[mid..mid + 256]);

        hasher.write(&content[content.len() - 256..]);
    }

    hasher.finish()
}

/// Compute a full hash for file content
pub fn hash_file_content(content: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    hasher.write(content);
    hasher.finish()
}

/// A cache key combining multiple components
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CompositeKey {
    components: Vec<u64>,
}

impl CompositeKey {
    pub fn new() -> Self {
        Self {
            components: Vec::new(),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            components: Vec::with_capacity(capacity),
        }
    }

    pub fn push<T: Hash>(&mut self, value: &T) {
        self.components.push(compute_hash(value));
    }

    pub fn push_str(&mut self, s: &str) {
        self.components.push(compute_hash(&s));
    }

    pub fn push_json(&mut self, value: &serde_json::Value) {
        self.components.push(hash_json(value));
    }

    /// Build a single hash from all components
    pub fn finalize(&self) -> u64 {
        compute_hash(&self.components)
    }
}

impl Default for CompositeKey {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_json_hash_order_independent() {
        let obj1 = json!({"a": 1, "b": 2});
        let obj2 = json!({"b": 2, "a": 1});

        assert_eq!(hash_json(&obj1), hash_json(&obj2));
    }

    #[test]
    fn test_json_hash_different_values() {
        let obj1 = json!({"a": 1});
        let obj2 = json!({"a": 2});

        assert_ne!(hash_json(&obj1), hash_json(&obj2));
    }

    #[test]
    fn test_composite_key() {
        let mut key1 = CompositeKey::new();
        key1.push_str("tool");
        key1.push_json(&json!({"path": "/test"}));

        let mut key2 = CompositeKey::new();
        key2.push_str("tool");
        key2.push_json(&json!({"path": "/test"}));

        assert_eq!(key1.finalize(), key2.finalize());
    }
}
