//! Response Caching
//!
//! Caches for various types of responses to avoid redundant operations.
//!
//! ## Components
//!
//! - **ToolCache**: Caches results from pure tools (Read, Glob, Grep)
//! - **McpCache**: Caches MCP server tool definitions

mod mcp;
mod tool;

pub use tool::{CachedToolResult, ToolCache, ToolCacheConfig, ToolCacheKey, ToolCacheStats};

pub use mcp::{CachedToolDefinition, McpCache, McpCacheConfig, McpCacheStats};
