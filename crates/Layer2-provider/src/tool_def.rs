//! Tool definitions for LLM function calling

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Definition of a tool that can be called by the LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    /// Tool name (should be unique)
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
    /// Create a new tool definition
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters: ToolParameters {
                schema_type: "object".to_string(),
                properties: serde_json::json!({}),
                required: vec![],
            },
        }
    }

    /// Add a string parameter
    pub fn with_string_param(
        mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        required: bool,
    ) -> Self {
        let name = name.into();

        if let Value::Object(ref mut props) = self.parameters.properties {
            props.insert(
                name.clone(),
                serde_json::json!({
                    "type": "string",
                    "description": description.into()
                }),
            );
        }

        if required {
            self.parameters.required.push(name);
        }

        self
    }

    /// Add an integer parameter
    pub fn with_integer_param(
        mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        required: bool,
    ) -> Self {
        let name = name.into();

        if let Value::Object(ref mut props) = self.parameters.properties {
            props.insert(
                name.clone(),
                serde_json::json!({
                    "type": "integer",
                    "description": description.into()
                }),
            );
        }

        if required {
            self.parameters.required.push(name);
        }

        self
    }

    /// Add a boolean parameter
    pub fn with_boolean_param(
        mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        required: bool,
    ) -> Self {
        let name = name.into();

        if let Value::Object(ref mut props) = self.parameters.properties {
            props.insert(
                name.clone(),
                serde_json::json!({
                    "type": "boolean",
                    "description": description.into()
                }),
            );
        }

        if required {
            self.parameters.required.push(name);
        }

        self
    }

    /// Add an enum parameter
    pub fn with_enum_param(
        mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        values: Vec<&str>,
        required: bool,
    ) -> Self {
        let name = name.into();

        if let Value::Object(ref mut props) = self.parameters.properties {
            props.insert(
                name.clone(),
                serde_json::json!({
                    "type": "string",
                    "description": description.into(),
                    "enum": values
                }),
            );
        }

        if required {
            self.parameters.required.push(name);
        }

        self
    }

    /// Add a custom parameter with full schema
    pub fn with_param(mut self, name: impl Into<String>, schema: Value, required: bool) -> Self {
        let name = name.into();

        if let Value::Object(ref mut props) = self.parameters.properties {
            props.insert(name.clone(), schema);
        }

        if required {
            self.parameters.required.push(name);
        }

        self
    }
}
