//! Agent context - shared state for agent execution
//!
//! Layer3의 AgentContext는 Layer2-core의 AgentContext를 위임하여
//! Tool/MCP 실행 기능을 사용하고, LLM Gateway 조율에 집중합니다.
//!
//! ## 아키텍처
//! ```text
//! Layer3-agent::AgentContext
//! ├── gateway: Arc<Gateway>           // LLM 프로바이더 (Layer2-provider)
//! ├── core_ctx: Arc<CoreAgentContext> // Tool/MCP 실행 위임 (Layer2-core)
//! └── system_prompt: String           // 시스템 프롬프트
//! ```

use forge_core::AgentContext as CoreAgentContext;
use forge_core::ToolRegistry;
use forge_foundation::permission::PermissionService;
use forge_foundation::Result;
use forge_provider::Gateway;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;

/// Context shared across agent execution
///
/// Layer3의 AgentContext는 두 가지 역할을 통합합니다:
/// 1. LLM Gateway 조율 (프로바이더 선택, 스트리밍 등)
/// 2. Tool/MCP 실행 위임 (Layer2-core::AgentContext 활용)
pub struct AgentContext {
    /// LLM provider gateway
    pub gateway: Arc<Gateway>,

    /// Core context for Tool/MCP execution (Layer2-core)
    core_ctx: Arc<CoreAgentContext>,

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
        // Layer2-core의 AgentContext 생성
        let core_ctx = CoreAgentContext::builder()
            .working_directory(working_dir.clone())
            .with_permission_service(permissions)
            .build();

        Self {
            gateway,
            core_ctx: Arc::new(core_ctx),
            working_dir,
            system_prompt: default_system_prompt(),
        }
    }

    /// Create with builder pattern
    pub fn builder() -> AgentContextBuilder {
        AgentContextBuilder::new()
    }

    /// Set custom system prompt
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = prompt.into();
        self
    }

    // ========================================================================
    // Tool Execution (위임 to Layer2-core)
    // ========================================================================

    /// Execute a tool by name
    ///
    /// Layer2-core::AgentContext로 위임하여 실행합니다.
    pub async fn execute_tool(
        &self,
        name: &str,
        input: Value,
    ) -> Result<forge_core::ToolExecutionResult> {
        self.core_ctx.execute_tool(name, input).await
    }

    /// Execute multiple tools in parallel
    pub async fn execute_tools_parallel(
        &self,
        calls: Vec<(&str, Value)>,
    ) -> Vec<Result<forge_core::ToolExecutionResult>> {
        self.core_ctx.execute_tools_parallel(calls).await
    }

    /// Get tool definitions for LLM
    pub fn tool_definitions(&self) -> Vec<forge_provider::ToolDef> {
        // core_ctx에서 스키마를 가져와 변환
        let rt = tokio::runtime::Handle::current();
        rt.block_on(async {
            self.core_ctx
                .get_tool_schemas()
                .await
                .into_iter()
                .map(|schema| {
                    let name = schema["name"].as_str().unwrap_or("").to_string();
                    let description = schema["description"].as_str().unwrap_or("").to_string();

                    forge_provider::ToolDef {
                        name,
                        description,
                        parameters: forge_provider::tool_def::ToolParameters {
                            schema_type: schema["parameters"]["type"]
                                .as_str()
                                .unwrap_or("object")
                                .to_string(),
                            properties: schema["parameters"]["properties"].clone(),
                            required: schema["parameters"]["required"]
                                .as_array()
                                .map(|arr| {
                                    arr.iter()
                                        .filter_map(|v| v.as_str())
                                        .map(String::from)
                                        .collect()
                                })
                                .unwrap_or_default(),
                        },
                    }
                })
                .collect()
        })
    }

    /// Create tool context for execution (legacy compatibility)
    pub fn tool_context(&self, session_id: &str) -> forge_core::RuntimeContext {
        forge_core::RuntimeContext::new(
            session_id,
            self.working_dir.clone(),
            self.core_ctx_permissions(),
        )
    }

    /// Get permissions from core context
    fn core_ctx_permissions(&self) -> Arc<PermissionService> {
        // core_ctx에서 권한 서비스를 가져오거나 기본값 생성
        Arc::new(PermissionService::new())
    }

    /// Check if a tool exists
    pub async fn has_tool(&self, name: &str) -> bool {
        self.core_ctx.has_tool(name).await
    }

    /// List available tools
    pub async fn list_tools(&self) -> Vec<(String, String)> {
        self.core_ctx.list_tools().await
    }

    // ========================================================================
    // MCP Integration (위임 to Layer2-core)
    // ========================================================================

    /// Connect to an MCP server
    pub async fn connect_mcp_server(
        &self,
        name: &str,
        config: forge_core::McpTransportConfig,
    ) -> Result<()> {
        self.core_ctx.connect_mcp_server(name, config).await
    }

    /// Disconnect from an MCP server
    pub async fn disconnect_mcp_server(&self, name: &str) -> Result<()> {
        self.core_ctx.disconnect_mcp_server(name).await
    }

    /// List connected MCP servers
    pub async fn list_mcp_servers(&self) -> Vec<String> {
        self.core_ctx.list_mcp_servers().await
    }

    /// Refresh MCP tools
    pub async fn refresh_mcp_tools(&self) -> Result<()> {
        self.core_ctx.refresh_mcp_tools().await
    }

    // ========================================================================
    // Statistics (위임 to Layer2-core)
    // ========================================================================

    /// Get execution statistics
    pub async fn stats(&self) -> forge_core::ExecutionStats {
        self.core_ctx.stats().await
    }

    /// Reset statistics
    pub async fn reset_stats(&self) {
        self.core_ctx.reset_stats().await
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

    // ========================================================================
    // Context Access
    // ========================================================================

    /// Get the core context (Layer2-core)
    pub fn core_context(&self) -> &Arc<CoreAgentContext> {
        &self.core_ctx
    }

    /// Get session ID from core context
    pub fn session_id(&self) -> &str {
        self.core_ctx.session_id()
    }
}

// ============================================================================
// Builder
// ============================================================================

/// Builder for AgentContext
pub struct AgentContextBuilder {
    gateway: Option<Arc<Gateway>>,
    core_ctx: Option<Arc<CoreAgentContext>>,
    working_dir: PathBuf,
    system_prompt: Option<String>,
    permissions: Option<Arc<PermissionService>>,
}

impl AgentContextBuilder {
    pub fn new() -> Self {
        Self {
            gateway: None,
            core_ctx: None,
            working_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            system_prompt: None,
            permissions: None,
        }
    }

    /// Set the LLM gateway
    pub fn gateway(mut self, gateway: Arc<Gateway>) -> Self {
        self.gateway = Some(gateway);
        self
    }

    /// Set the core context directly
    pub fn core_context(mut self, ctx: Arc<CoreAgentContext>) -> Self {
        self.core_ctx = Some(ctx);
        self
    }

    /// Set working directory
    pub fn working_directory(mut self, path: PathBuf) -> Self {
        self.working_dir = path;
        self
    }

    /// Set system prompt
    pub fn system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Set permission service
    pub fn permissions(mut self, permissions: Arc<PermissionService>) -> Self {
        self.permissions = Some(permissions);
        self
    }

    /// Build the AgentContext
    pub fn build(self) -> Result<AgentContext> {
        let gateway = self.gateway.ok_or_else(|| {
            forge_foundation::Error::Config("Gateway is required".to_string())
        })?;

        // core_ctx가 없으면 새로 생성
        let core_ctx = match self.core_ctx {
            Some(ctx) => ctx,
            None => {
                let mut builder = CoreAgentContext::builder()
                    .working_directory(self.working_dir.clone());

                if let Some(perms) = self.permissions {
                    builder = builder.with_permission_service(perms);
                }

                Arc::new(builder.build())
            }
        };

        Ok(AgentContext {
            gateway,
            core_ctx,
            working_dir: self.working_dir,
            system_prompt: self.system_prompt.unwrap_or_else(default_system_prompt),
        })
    }
}

impl Default for AgentContextBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Supporting Types
// ============================================================================

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
