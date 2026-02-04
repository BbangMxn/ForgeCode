//! Context Compactor
//!
//! Compresses large content (file contents, tool results) into references.
//! This is a reversible operation - the original content can be retrieved.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use uuid::Uuid;

/// Unique identifier for compacted content
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContentId(Uuid);

impl ContentId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn as_str(&self) -> String {
        self.0.to_string()
    }
}

impl Default for ContentId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ContentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Configuration for context compaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactorConfig {
    /// Size threshold in bytes for compaction
    pub threshold_bytes: usize,
    /// Maximum number of entries to store
    pub max_entries: usize,
    /// Preview length for compacted content
    pub preview_length: usize,
}

impl Default for CompactorConfig {
    fn default() -> Self {
        Self {
            threshold_bytes: 4096, // 4KB
            max_entries: 1000,
            preview_length: 200,
        }
    }
}

/// Result of a compaction operation
#[derive(Debug, Clone)]
pub struct CompactedContent {
    /// Whether the content was compacted
    pub is_compacted: bool,
    /// The content to use (compacted reference or original)
    pub content: String,
    /// Key to restore original content (if compacted)
    pub restore_key: Option<ContentId>,
    /// Original content size in bytes
    pub original_size: usize,
}

/// Stored compacted entry
#[derive(Debug, Clone)]
struct StoredEntry {
    content: String,
    #[allow(dead_code)]
    content_type: ContentType,
    #[allow(dead_code)]
    created_at: std::time::Instant,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum ContentType {
    FileContent { path: String },
    ToolResult { tool_name: String },
    Generic,
}

/// Context Compactor for reducing context size
///
/// Compresses large content into references while preserving the ability
/// to restore the original content when needed.
///
/// This is particularly useful for:
/// - Large file contents from Read operations
/// - Verbose tool outputs (Grep results, Bash output)
/// - Any content that can be re-fetched if needed
///
/// # Example
///
/// ```rust,ignore
/// let mut compactor = ContextCompactor::new();
/// let result = compactor.compact_file_content(Path::new("large.rs"), &file_content);
/// if result.is_compacted {
///     // Use result.content (a reference like "[File: large.rs (50KB)]")
///     // Later, restore with: compactor.restore(&result.restore_key.unwrap())
/// }
/// ```
#[derive(Debug)]
pub struct ContextCompactor {
    config: CompactorConfig,
    storage: HashMap<ContentId, StoredEntry>,
    /// Order of insertion for LRU eviction
    insertion_order: Vec<ContentId>,
}

impl ContextCompactor {
    /// Create a new compactor with default settings
    pub fn new() -> Self {
        Self {
            config: CompactorConfig::default(),
            storage: HashMap::new(),
            insertion_order: Vec::new(),
        }
    }

    /// Create a compactor with a specific threshold
    pub fn with_threshold(threshold_bytes: usize) -> Self {
        Self {
            config: CompactorConfig {
                threshold_bytes,
                ..Default::default()
            },
            storage: HashMap::new(),
            insertion_order: Vec::new(),
        }
    }

    /// Create a compactor with custom configuration
    pub fn with_config(config: CompactorConfig) -> Self {
        Self {
            config,
            storage: HashMap::new(),
            insertion_order: Vec::new(),
        }
    }

    /// Compact file content
    pub fn compact_file_content(&mut self, path: &Path, content: &str) -> CompactedContent {
        let size = content.len();

        if size < self.config.threshold_bytes {
            return CompactedContent {
                is_compacted: false,
                content: content.to_string(),
                restore_key: None,
                original_size: size,
            };
        }

        let id = self.store(
            content,
            ContentType::FileContent {
                path: path.display().to_string(),
            },
        );

        let display_path = path.display();
        let size_display = format_size(size);

        CompactedContent {
            is_compacted: true,
            content: format!(
                "[File: {} ({}) - use Read tool to view full content]",
                display_path, size_display
            ),
            restore_key: Some(id),
            original_size: size,
        }
    }

    /// Compact tool result
    pub fn compact_tool_result(&mut self, tool_name: &str, result: &str) -> CompactedContent {
        let size = result.len();

        if size < self.config.threshold_bytes {
            return CompactedContent {
                is_compacted: false,
                content: result.to_string(),
                restore_key: None,
                original_size: size,
            };
        }

        let id = self.store(
            result,
            ContentType::ToolResult {
                tool_name: tool_name.to_string(),
            },
        );

        let preview_len = self.config.preview_length.min(size);
        let preview = &result[..preview_len];
        let size_display = format_size(size);

        CompactedContent {
            is_compacted: true,
            content: format!(
                "[{} output ({})]\n{}{}",
                tool_name,
                size_display,
                preview,
                if size > preview_len { "..." } else { "" }
            ),
            restore_key: Some(id),
            original_size: size,
        }
    }

    /// Compact generic content
    pub fn compact(&mut self, content: &str) -> CompactedContent {
        let size = content.len();

        if size < self.config.threshold_bytes {
            return CompactedContent {
                is_compacted: false,
                content: content.to_string(),
                restore_key: None,
                original_size: size,
            };
        }

        let id = self.store(content, ContentType::Generic);

        let preview_len = self.config.preview_length.min(size);
        let preview = &content[..preview_len];
        let size_display = format_size(size);

        CompactedContent {
            is_compacted: true,
            content: format!("[Content compacted ({})]\n{}...", size_display, preview),
            restore_key: Some(id),
            original_size: size,
        }
    }

    /// Try to compact content, returning the compacted version if applicable
    pub fn try_compact(&mut self, content: &str) -> Option<String> {
        if content.len() < self.config.threshold_bytes {
            None
        } else {
            Some(self.compact(content).content)
        }
    }

    /// Restore original content
    pub fn restore(&self, id: &ContentId) -> Option<&str> {
        self.storage.get(id).map(|e| e.content.as_str())
    }

    /// Check if an ID exists
    pub fn contains(&self, id: &ContentId) -> bool {
        self.storage.contains_key(id)
    }

    /// Remove a stored entry
    pub fn remove(&mut self, id: &ContentId) -> Option<String> {
        self.insertion_order.retain(|i| i != id);
        self.storage.remove(id).map(|e| e.content)
    }

    /// Clear all stored content
    pub fn clear(&mut self) {
        self.storage.clear();
        self.insertion_order.clear();
    }

    /// Get the number of stored entries
    pub fn len(&self) -> usize {
        self.storage.len()
    }

    /// Check if storage is empty
    pub fn is_empty(&self) -> bool {
        self.storage.is_empty()
    }

    /// Estimate memory usage in bytes
    pub fn memory_usage(&self) -> usize {
        self.storage.values().map(|e| e.content.len()).sum()
    }

    /// Get compaction statistics
    pub fn stats(&self) -> CompactorStats {
        let total_stored: usize = self.storage.values().map(|e| e.content.len()).sum();

        CompactorStats {
            entries: self.storage.len(),
            max_entries: self.config.max_entries,
            total_bytes_stored: total_stored,
            threshold_bytes: self.config.threshold_bytes,
        }
    }

    /// Store content and return its ID
    fn store(&mut self, content: &str, content_type: ContentType) -> ContentId {
        // Evict oldest if at capacity
        while self.storage.len() >= self.config.max_entries {
            if let Some(oldest_id) = self.insertion_order.first().copied() {
                self.storage.remove(&oldest_id);
                self.insertion_order.remove(0);
            } else {
                break;
            }
        }

        let id = ContentId::new();
        self.storage.insert(
            id,
            StoredEntry {
                content: content.to_string(),
                content_type,
                created_at: std::time::Instant::now(),
            },
        );
        self.insertion_order.push(id);

        id
    }
}

impl Default for ContextCompactor {
    fn default() -> Self {
        Self::new()
    }
}

/// Compactor statistics
#[derive(Debug, Clone)]
pub struct CompactorStats {
    pub entries: usize,
    pub max_entries: usize,
    pub total_bytes_stored: usize,
    pub threshold_bytes: usize,
}

/// Format byte size for display
fn format_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{} bytes", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_small_content_not_compacted() {
        let mut compactor = ContextCompactor::with_threshold(100);
        let content = "small content";

        let result = compactor.compact(content);

        assert!(!result.is_compacted);
        assert_eq!(result.content, content);
        assert!(result.restore_key.is_none());
    }

    #[test]
    fn test_large_content_compacted() {
        let mut compactor = ContextCompactor::with_threshold(100);
        let content = "x".repeat(200);

        let result = compactor.compact(&content);

        assert!(result.is_compacted);
        assert!(result.content.contains("compacted"));
        assert!(result.restore_key.is_some());

        // Verify restoration
        let restored = compactor.restore(&result.restore_key.unwrap());
        assert_eq!(restored, Some(content.as_str()));
    }

    #[test]
    fn test_file_content_compaction() {
        let mut compactor = ContextCompactor::with_threshold(50);
        let content = "fn main() { println!(\"Hello, World!\"); }".repeat(5);

        let result = compactor.compact_file_content(Path::new("main.rs"), &content);

        assert!(result.is_compacted);
        assert!(result.content.contains("main.rs"));
        assert!(result.content.contains("Read tool"));
    }

    #[test]
    fn test_tool_result_compaction() {
        let mut compactor = ContextCompactor::with_threshold(50);
        let content = "file1.rs:10: match found\nfile2.rs:20: match found\n".repeat(10);

        let result = compactor.compact_tool_result("Grep", &content);

        assert!(result.is_compacted);
        assert!(result.content.contains("Grep output"));
    }

    #[test]
    fn test_eviction_at_capacity() {
        let config = CompactorConfig {
            threshold_bytes: 10,
            max_entries: 3,
            preview_length: 50,
        };
        let mut compactor = ContextCompactor::with_config(config);

        // Add 4 entries (capacity is 3)
        let id1 = compactor.compact("x".repeat(20)).restore_key.unwrap();
        let _id2 = compactor.compact("y".repeat(20)).restore_key.unwrap();
        let _id3 = compactor.compact("z".repeat(20)).restore_key.unwrap();
        let _id4 = compactor.compact("w".repeat(20)).restore_key.unwrap();

        // First entry should be evicted
        assert!(compactor.restore(&id1).is_none());
        assert_eq!(compactor.len(), 3);
    }
}
