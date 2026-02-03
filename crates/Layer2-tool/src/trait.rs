//! Tool trait and related types

use async_trait::async_trait;
use forge_foundation::permission::PermissionService;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;

/// Definition of a tool for LLM function calling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    /// Tool name (unique identifier)
    pub name: String,

    /// Human-readable description
    pub description: String,

    /// JSON Schema for parameters
    pub parameters: ToolParameters,
}

/// Parameters schema for a tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolParameters {
    /// Type (usually "object")
    #[serde(rename = "type")]
    pub schema_type: String,

    /// Properties (parameter definitions)
    pub properties: Value,

    /// Required parameters
    #[serde(default)]
    pub required: Vec<String>,
}

impl ToolDef {
    /// Create a new tool definition builder
    pub fn builder(name: impl Into<String>, description: impl Into<String>) -> ToolDefBuilder {
        ToolDefBuilder::new(name, description)
    }
}

/// Builder for ToolDef
pub struct ToolDefBuilder {
    name: String,
    description: String,
    properties: serde_json::Map<String, Value>,
    required: Vec<String>,
}

impl ToolDefBuilder {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            properties: serde_json::Map::new(),
            required: vec![],
        }
    }

    /// Add a string parameter
    pub fn string_param(
        mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        required: bool,
    ) -> Self {
        let name = name.into();
        self.properties.insert(
            name.clone(),
            serde_json::json!({
                "type": "string",
                "description": description.into()
            }),
        );
        if required {
            self.required.push(name);
        }
        self
    }

    /// Add an integer parameter
    pub fn integer_param(
        mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        required: bool,
    ) -> Self {
        let name = name.into();
        self.properties.insert(
            name.clone(),
            serde_json::json!({
                "type": "integer",
                "description": description.into()
            }),
        );
        if required {
            self.required.push(name);
        }
        self
    }

    /// Add a boolean parameter
    pub fn boolean_param(
        mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        required: bool,
    ) -> Self {
        let name = name.into();
        self.properties.insert(
            name.clone(),
            serde_json::json!({
                "type": "boolean",
                "description": description.into()
            }),
        );
        if required {
            self.required.push(name);
        }
        self
    }

    /// Add an enum parameter
    pub fn enum_param(
        mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        values: Vec<&str>,
        required: bool,
    ) -> Self {
        let name = name.into();
        self.properties.insert(
            name.clone(),
            serde_json::json!({
                "type": "string",
                "description": description.into(),
                "enum": values
            }),
        );
        if required {
            self.required.push(name);
        }
        self
    }

    /// Build the ToolDef
    pub fn build(self) -> ToolDef {
        ToolDef {
            name: self.name,
            description: self.description,
            parameters: ToolParameters {
                schema_type: "object".to_string(),
                properties: Value::Object(self.properties),
                required: self.required,
            },
        }
    }
}

/// Context provided to tools during execution
pub struct ToolContext {
    /// Current session ID
    pub session_id: String,

    /// Working directory
    pub working_dir: PathBuf,

    /// Permission service for requesting permissions
    pub permissions: Arc<PermissionService>,

    /// Whether to auto-approve actions (dangerous!)
    pub auto_approve: bool,
}

impl ToolContext {
    pub fn new(
        session_id: impl Into<String>,
        working_dir: PathBuf,
        permissions: Arc<PermissionService>,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            working_dir,
            permissions,
            auto_approve: false,
        }
    }

    /// Create a context with auto-approve enabled
    pub fn with_auto_approve(mut self) -> Self {
        self.auto_approve = true;
        self
    }
}

/// Result of tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// Whether execution was successful
    pub success: bool,

    /// Result content (text output)
    pub content: String,

    /// Optional metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,

    /// Error message if failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ToolResult {
    /// Create a success result
    pub fn success(content: impl Into<String>) -> Self {
        Self {
            success: true,
            content: content.into(),
            metadata: None,
            error: None,
        }
    }

    /// Create a success result with metadata
    pub fn success_with_metadata(content: impl Into<String>, metadata: Value) -> Self {
        Self {
            success: true,
            content: content.into(),
            metadata: Some(metadata),
            error: None,
        }
    }

    /// Create an error result
    pub fn error(message: impl Into<String>) -> Self {
        let msg = message.into();
        Self {
            success: false,
            content: String::new(),
            metadata: None,
            error: Some(msg),
        }
    }

    /// Create a permission denied result
    pub fn permission_denied(description: impl Into<String>) -> Self {
        Self {
            success: false,
            content: String::new(),
            metadata: None,
            error: Some(format!("Permission denied: {}", description.into())),
        }
    }
}

/// Tool trait - implement this to create a new tool
#[async_trait]
pub trait Tool: Send + Sync {
    /// Get the tool definition
    fn definition(&self) -> ToolDef;

    /// Execute the tool with given parameters
    async fn execute(&self, ctx: &ToolContext, params: Value) -> ToolResult;

    /// Get the tool name (convenience method)
    fn name(&self) -> String {
        self.definition().name
    }
}
