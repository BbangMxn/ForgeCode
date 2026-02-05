//! Zero-Copy String Utilities
//!
//! Provides efficient string handling patterns for ForgeCode:
//! - Static string constants for commonly used values
//! - `CowStr` type alias for zero-copy string operations
//! - Interned string pool for frequently used strings
//!
//! # Performance Benefits
//!
//! - Eliminates allocations for static strings (provider names, tool names, etc.)
//! - Copy-on-write semantics for strings that may or may not need modification
//! - String interning for repeated lookups
//!
//! # Usage
//!
//! ```ignore
//! use forge_foundation::strings::{CowStr, PROVIDER_ANTHROPIC, SCHEMA_TYPE_OBJECT};
//!
//! // Zero-copy static string
//! let provider: CowStr = PROVIDER_ANTHROPIC.into();
//!
//! // Owned string when needed
//! let dynamic: CowStr = format!("tool_{}", id).into();
//! ```

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

/// Copy-on-write string type
///
/// Use this instead of `String` when:
/// - The value is often a static/constant string
/// - The value is passed through without modification
/// - You want to avoid unnecessary allocations
pub type CowStr<'a> = Cow<'a, str>;

/// Owned version of CowStr for struct fields
pub type CowString = Cow<'static, str>;

// ============================================================================
// Provider Constants
// ============================================================================

/// Anthropic provider ID
pub const PROVIDER_ANTHROPIC: &str = "anthropic";
/// OpenAI provider ID
pub const PROVIDER_OPENAI: &str = "openai";
/// Ollama provider ID
pub const PROVIDER_OLLAMA: &str = "ollama";
/// Gemini provider ID
pub const PROVIDER_GEMINI: &str = "gemini";
/// Groq provider ID
pub const PROVIDER_GROQ: &str = "groq";

/// Display name for Anthropic
pub const DISPLAY_ANTHROPIC: &str = "Anthropic";
/// Display name for OpenAI
pub const DISPLAY_OPENAI: &str = "OpenAI";
/// Display name for Ollama
pub const DISPLAY_OLLAMA: &str = "Ollama";
/// Display name for Gemini
pub const DISPLAY_GEMINI: &str = "Google Gemini";
/// Display name for Groq
pub const DISPLAY_GROQ: &str = "Groq";

// ============================================================================
// Schema Constants
// ============================================================================

/// JSON Schema type for object
pub const SCHEMA_TYPE_OBJECT: &str = "object";
/// JSON Schema type for string
pub const SCHEMA_TYPE_STRING: &str = "string";
/// JSON Schema type for integer
pub const SCHEMA_TYPE_INTEGER: &str = "integer";
/// JSON Schema type for number
pub const SCHEMA_TYPE_NUMBER: &str = "number";
/// JSON Schema type for boolean
pub const SCHEMA_TYPE_BOOLEAN: &str = "boolean";
/// JSON Schema type for array
pub const SCHEMA_TYPE_ARRAY: &str = "array";

// ============================================================================
// Tool Name Constants
// ============================================================================

/// Read tool name
pub const TOOL_READ: &str = "read";
/// Write tool name
pub const TOOL_WRITE: &str = "write";
/// Edit tool name
pub const TOOL_EDIT: &str = "edit";
/// Bash tool name
pub const TOOL_BASH: &str = "bash";
/// Glob tool name
pub const TOOL_GLOB: &str = "glob";
/// Grep tool name
pub const TOOL_GREP: &str = "grep";

// ============================================================================
// Message Role Constants
// ============================================================================

/// System role
pub const ROLE_SYSTEM: &str = "system";
/// User role
pub const ROLE_USER: &str = "user";
/// Assistant role
pub const ROLE_ASSISTANT: &str = "assistant";
/// Tool role
pub const ROLE_TOOL: &str = "tool";

// ============================================================================
// Common Environment Variables
// ============================================================================

/// PATH environment variable
pub const ENV_PATH: &str = "PATH";
/// HOME environment variable
pub const ENV_HOME: &str = "HOME";
/// USER environment variable
pub const ENV_USER: &str = "USER";
/// SHELL environment variable
pub const ENV_SHELL: &str = "SHELL";
/// TERM environment variable
pub const ENV_TERM: &str = "TERM";
/// PWD environment variable
pub const ENV_PWD: &str = "PWD";

// ============================================================================
// String Interning
// ============================================================================

/// Global string interner for frequently used strings
static STRING_INTERNER: OnceLock<RwLock<StringInterner>> = OnceLock::new();

/// Get the global string interner
pub fn interner() -> &'static RwLock<StringInterner> {
    STRING_INTERNER.get_or_init(|| RwLock::new(StringInterner::new()))
}

/// Intern a string, returning a static reference if already interned
///
/// This is useful for strings that are used repeatedly as HashMap keys
/// or compared frequently.
///
/// # Example
/// ```ignore
/// use forge_foundation::strings::intern;
///
/// let s1 = intern("frequently_used");
/// let s2 = intern("frequently_used");
/// assert!(std::ptr::eq(s1.as_ptr(), s2.as_ptr())); // Same memory
/// ```
pub fn intern(s: &str) -> CowString {
    // Check if it's a known static string first (fast path)
    if let Some(static_str) = try_static(s) {
        return Cow::Borrowed(static_str);
    }

    // Otherwise use the interner
    let interner = interner();

    // Try read lock first (fast path for already interned)
    {
        let reader = interner.read().unwrap();
        if let Some(interned) = reader.get(s) {
            return Cow::Borrowed(interned);
        }
    }

    // Need to intern - acquire write lock
    let mut writer = interner.write().unwrap();
    // Double-check after acquiring write lock
    if let Some(interned) = writer.get(s) {
        return Cow::Borrowed(interned);
    }

    // Actually intern the string
    Cow::Borrowed(writer.intern(s))
}

/// Try to return a static string reference for known constants
#[inline]
fn try_static(s: &str) -> Option<&'static str> {
    match s {
        // Providers
        "anthropic" => Some(PROVIDER_ANTHROPIC),
        "openai" => Some(PROVIDER_OPENAI),
        "ollama" => Some(PROVIDER_OLLAMA),
        "gemini" => Some(PROVIDER_GEMINI),
        "groq" => Some(PROVIDER_GROQ),
        // Display names
        "Anthropic" => Some(DISPLAY_ANTHROPIC),
        "OpenAI" => Some(DISPLAY_OPENAI),
        "Ollama" => Some(DISPLAY_OLLAMA),
        "Google Gemini" => Some(DISPLAY_GEMINI),
        "Groq" => Some(DISPLAY_GROQ),
        // Schema types
        "object" => Some(SCHEMA_TYPE_OBJECT),
        "string" => Some(SCHEMA_TYPE_STRING),
        "integer" => Some(SCHEMA_TYPE_INTEGER),
        "number" => Some(SCHEMA_TYPE_NUMBER),
        "boolean" => Some(SCHEMA_TYPE_BOOLEAN),
        "array" => Some(SCHEMA_TYPE_ARRAY),
        // Tools
        "read" => Some(TOOL_READ),
        "write" => Some(TOOL_WRITE),
        "edit" => Some(TOOL_EDIT),
        "bash" => Some(TOOL_BASH),
        "glob" => Some(TOOL_GLOB),
        "grep" => Some(TOOL_GREP),
        // Roles
        "system" => Some(ROLE_SYSTEM),
        "user" => Some(ROLE_USER),
        "assistant" => Some(ROLE_ASSISTANT),
        "tool" => Some(ROLE_TOOL),
        // Env vars
        "PATH" => Some(ENV_PATH),
        "HOME" => Some(ENV_HOME),
        "USER" => Some(ENV_USER),
        "SHELL" => Some(ENV_SHELL),
        "TERM" => Some(ENV_TERM),
        "PWD" => Some(ENV_PWD),
        _ => None,
    }
}

/// String interner for storing unique string instances
///
/// Uses a simple approach: Box<str> stored in a Vec, with HashMap for lookup.
/// The Vec owns the strings and they live as long as the interner.
pub struct StringInterner {
    strings: Vec<Box<str>>,
    lookup: HashMap<&'static str, usize>,
}

impl StringInterner {
    /// Create a new string interner
    pub fn new() -> Self {
        Self {
            strings: Vec::with_capacity(256),
            lookup: HashMap::with_capacity(256),
        }
    }

    /// Get an interned string if it exists
    pub fn get(&self, s: &str) -> Option<&'static str> {
        self.lookup.get(s).map(|&idx| {
            // SAFETY: The string is stored in self.strings and lives as long as self
            unsafe { &*(&*self.strings[idx] as *const str) }
        })
    }

    /// Intern a string, returning a static reference
    pub fn intern(&mut self, s: &str) -> &'static str {
        let boxed: Box<str> = s.into();
        let ptr = &*boxed as *const str;
        let idx = self.strings.len();
        self.strings.push(boxed);

        // SAFETY: The string is owned by self.strings and won't be moved/dropped
        // as long as self exists
        let static_ref: &'static str = unsafe { &*ptr };
        self.lookup.insert(static_ref, idx);
        static_ref
    }

    /// Get the number of interned strings
    pub fn len(&self) -> usize {
        self.strings.len()
    }

    /// Check if the interner is empty
    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }

    /// Estimate memory usage in bytes
    pub fn memory_usage(&self) -> usize {
        let string_bytes: usize = self.strings.iter().map(|s| s.len()).sum();
        let overhead = self.strings.capacity() * std::mem::size_of::<Box<str>>()
            + self.lookup.capacity()
                * (std::mem::size_of::<&str>() + std::mem::size_of::<usize>());
        string_bytes + overhead
    }
}

impl Default for StringInterner {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Conversion Traits
// ============================================================================

/// Trait for types that can be converted to CowString efficiently
pub trait IntoCowString {
    fn into_cow_string(self) -> CowString;
}

impl IntoCowString for &'static str {
    #[inline]
    fn into_cow_string(self) -> CowString {
        Cow::Borrowed(self)
    }
}

impl IntoCowString for String {
    #[inline]
    fn into_cow_string(self) -> CowString {
        Cow::Owned(self)
    }
}

impl IntoCowString for CowString {
    #[inline]
    fn into_cow_string(self) -> CowString {
        self
    }
}

/// Convert a non-static &str to CowString, checking for known static strings
///
/// Use this function when you have a runtime &str that might match a known constant.
#[inline]
pub fn str_to_cow(s: &str) -> CowString {
    // Check if it's a known static string
    if let Some(static_str) = try_static(s) {
        Cow::Borrowed(static_str)
    } else {
        Cow::Owned(s.to_string())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_static_constants() {
        assert_eq!(PROVIDER_ANTHROPIC, "anthropic");
        assert_eq!(SCHEMA_TYPE_OBJECT, "object");
        assert_eq!(TOOL_READ, "read");
        assert_eq!(ROLE_SYSTEM, "system");
    }

    #[test]
    fn test_try_static() {
        assert_eq!(try_static("anthropic"), Some(PROVIDER_ANTHROPIC));
        assert_eq!(try_static("object"), Some(SCHEMA_TYPE_OBJECT));
        assert_eq!(try_static("unknown"), None);
    }

    #[test]
    fn test_cow_string_static() {
        let s: CowString = PROVIDER_ANTHROPIC.into_cow_string();
        assert!(matches!(s, Cow::Borrowed(_)));
    }

    #[test]
    fn test_cow_string_owned() {
        let s: CowString = String::from("dynamic").into_cow_string();
        assert!(matches!(s, Cow::Owned(_)));
    }

    #[test]
    fn test_intern_static() {
        let s = intern("anthropic");
        assert!(matches!(s, Cow::Borrowed(_)));
        // Should contain the same content
        assert_eq!(s.as_ref(), PROVIDER_ANTHROPIC);
    }

    #[test]
    fn test_intern_dynamic() {
        let s1 = intern("dynamic_string_123");
        let s2 = intern("dynamic_string_123");

        // Both should be borrowed (interned)
        assert!(matches!(s1, Cow::Borrowed(_)));
        assert!(matches!(s2, Cow::Borrowed(_)));

        // Should point to same memory
        assert!(std::ptr::eq(s1.as_ref().as_ptr(), s2.as_ref().as_ptr()));
    }

    #[test]
    fn test_interner_memory() {
        let mut interner = StringInterner::new();
        interner.intern("test1");
        interner.intern("test2");
        assert_eq!(interner.len(), 2);
        assert!(interner.memory_usage() > 0);
    }

    #[test]
    fn test_str_to_cow_with_static_check() {
        // Known static string should be borrowed
        let s: CowString = str_to_cow("anthropic");
        assert!(matches!(s, Cow::Borrowed(_)));

        // Unknown string should be owned
        let s: CowString = str_to_cow("unknown_string");
        assert!(matches!(s, Cow::Owned(_)));
    }
}
