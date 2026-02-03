//! Tool Registry - manages available tools

use crate::{Tool, ToolContext, ToolDef, ToolResult};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// Registry of available tools
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
    disabled: Vec<String>,
}

impl ToolRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            disabled: vec![],
        }
    }

    /// Create a registry with default builtin tools
    pub fn with_builtins() -> Self {
        let mut registry = Self::new();

        // Register builtin tools
        registry.register(Arc::new(crate::builtin::bash::BashTool::new()));
        registry.register(Arc::new(crate::builtin::read::ReadTool::new()));
        registry.register(Arc::new(crate::builtin::write::WriteTool::new()));
        registry.register(Arc::new(crate::builtin::edit::EditTool::new()));
        registry.register(Arc::new(crate::builtin::grep::GrepTool::new()));
        registry.register(Arc::new(crate::builtin::glob::GlobTool::new()));
        registry.register(Arc::new(crate::builtin::forgecmd_tool::ForgeCmdTool::new()));

        registry
    }

    /// Register a tool
    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        let name = tool.name();
        self.tools.insert(name, tool);
    }

    /// Unregister a tool
    pub fn unregister(&mut self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.remove(name)
    }

    /// Disable a tool (keeps it registered but excludes from definitions)
    pub fn disable(&mut self, name: &str) {
        if !self.disabled.contains(&name.to_string()) {
            self.disabled.push(name.to_string());
        }
    }

    /// Enable a previously disabled tool
    pub fn enable(&mut self, name: &str) {
        self.disabled.retain(|n| n != name);
    }

    /// Get a tool by name
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    /// Check if a tool exists
    pub fn contains(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    /// Get all tool definitions (for sending to LLM)
    pub fn definitions(&self) -> Vec<ToolDef> {
        self.tools
            .iter()
            .filter(|(name, _)| !self.disabled.contains(name))
            .map(|(_, tool)| tool.definition())
            .collect()
    }

    /// Get all tool names
    pub fn names(&self) -> Vec<&str> {
        self.tools.keys().map(|s| s.as_str()).collect()
    }

    /// Execute a tool by name
    pub async fn execute(&self, name: &str, ctx: &ToolContext, params: Value) -> ToolResult {
        match self.get(name) {
            Some(tool) => {
                if self.disabled.contains(&name.to_string()) {
                    ToolResult::error(format!("Tool '{}' is disabled", name))
                } else {
                    tool.execute(ctx, params).await
                }
            }
            None => ToolResult::error(format!("Tool '{}' not found", name)),
        }
    }

    /// Get the number of registered tools
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Check if the registry is empty
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::with_builtins()
    }
}
