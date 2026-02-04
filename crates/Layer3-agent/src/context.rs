//! Agent context - shared state for agent execution

use forge_core::ToolRegistry;
use forge_foundation::permission::PermissionService;
use forge_foundation::Result;
use forge_provider::Gateway;
use std::path::PathBuf;
use std::sync::Arc;

/// Context shared across agent execution
pub struct AgentContext {
    /// LLM provider gateway
    pub gateway: Arc<Gateway>,

    /// Tool registry
    pub tools: Arc<ToolRegistry>,

    /// Permission service
    pub permissions: Arc<PermissionService>,

    /// Working directory
    pub working_dir: PathBuf,

    /// System prompt template
    pub system_prompt: String,
}

impl AgentContext {
    /// Create a new agent context
    pub fn new(
        gateway: Arc<Gateway>,
        tools: Arc<ToolRegistry>,
        permissions: Arc<PermissionService>,
        working_dir: PathBuf,
    ) -> Self {
        Self {
            gateway,
            tools,
            permissions,
            working_dir,
            system_prompt: default_system_prompt(),
        }
    }

    /// Set custom system prompt
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = prompt.into();
        self
    }

    /// Get tool definitions for LLM
    pub fn tool_definitions(&self) -> Vec<forge_provider::ToolDef> {
        self.tools
            .definitions()
            .into_iter()
            .map(|t| forge_provider::ToolDef {
                name: t.name,
                description: t.description,
                parameters: forge_provider::tool_def::ToolParameters {
                    schema_type: t.parameters.schema_type,
                    properties: t.parameters.properties,
                    required: t.parameters.required,
                },
            })
            .collect()
    }

    /// Create tool context for execution
    pub fn tool_context(&self, session_id: &str) -> forge_core::RuntimeContext {
        forge_core::RuntimeContext::new(
            session_id,
            self.working_dir.clone(),
            self.permissions.clone(),
        )
    }

    // ========================================================================
    // Runtime LLM Management
    // ========================================================================

    /// Switch to a different provider at runtime
    pub async fn switch_provider(&self, name: &str) -> Result<()> {
        self.gateway.set_default(name).await
    }

    /// Get current provider name
    pub async fn current_provider(&self) -> String {
        self.gateway.default_provider_name().await
    }

    /// Get current model info
    pub async fn current_model(&self) -> Option<String> {
        if let Ok(provider) = self.gateway.default_provider().await {
            Some(provider.model().id.clone())
        } else {
            None
        }
    }

    /// List available providers
    pub fn list_providers(&self) -> Vec<&str> {
        self.gateway.list_providers()
    }

    /// List available models for current provider
    pub async fn list_models(&self) -> Vec<String> {
        if let Ok(provider) = self.gateway.default_provider().await {
            provider
                .list_models()
                .iter()
                .map(|m| m.id.clone())
                .collect()
        } else {
            vec![]
        }
    }

    /// Get provider info (name, model, available status)
    pub fn provider_info(&self) -> Vec<ProviderInfo> {
        self.gateway
            .list_provider_info()
            .into_iter()
            .map(|(name, meta)| ProviderInfo {
                name: name.to_string(),
                display_name: meta.display_name.clone(),
                default_model: meta.default_model.clone(),
                available: self.gateway.is_provider_available(name),
            })
            .collect()
    }
}

/// Information about a provider
#[derive(Debug, Clone)]
pub struct ProviderInfo {
    pub name: String,
    pub display_name: String,
    pub default_model: String,
    pub available: bool,
}

/// Default system prompt for the agent
fn default_system_prompt() -> String {
    r#"You are ForgeCode, an AI coding assistant running in the terminal.

You help users with software engineering tasks including:
- Writing and editing code
- Debugging and fixing bugs
- Explaining code
- Running commands and tests
- Searching and navigating codebases

Guidelines:
- Be concise and direct in your responses
- Use tools to gather information before making changes
- Always read files before editing them
- Make minimal, focused changes
- Explain what you're doing and why
- Ask for clarification if requirements are unclear

You have access to various tools to help accomplish tasks. Use them effectively."#
        .to_string()
}
