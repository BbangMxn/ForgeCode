//! # forge-provider
//!
//! LLM provider abstraction layer for ForgeCode.
//! Supports multiple providers with a unified interface.
//!
//! ## Features
//! - SSE streaming for real-time responses
//! - Automatic retry with exponential backoff
//! - Multiple provider support (Anthropic, OpenAI, Ollama)
//! - Tool/function calling support
//!
//! ## Error 변환
//!
//! `ProviderError`는 `forge_foundation::Error`로 자동 변환됩니다:
//! ```ignore
//! use forge_provider::ProviderError;
//! use forge_foundation::Error;
//!
//! let provider_err = ProviderError::RateLimited { retry_after_ms: Some(1000) };
//! let foundation_err: Error = provider_err.into();
//! ```

pub mod agent_provider;
pub mod error;
pub mod gateway;
pub mod message;
pub mod providers;
pub mod retry;
pub mod tool_def;
pub mod r#trait;

// Core traits and types
pub use gateway::Gateway;
pub use message::{Message, MessageRole, ToolCall, ToolResult};
pub use r#trait::{
    FinishReason, ModelInfo, Provider, ProviderMetadata, ProviderResponse, StreamEvent, TokenCount,
    TokenUsage,
};
pub use tool_def::ToolDef;

// Error and retry
pub use error::ProviderError;
pub use retry::RetryConfig;

// Provider implementations
pub use providers::anthropic::AnthropicProvider;
pub use providers::gemini::GeminiProvider;
pub use providers::groq::GroqProvider;
pub use providers::ollama::OllamaProvider;
pub use providers::openai::OpenAiProvider;

// Agent provider abstraction (for Claude Agent SDK, Codex, etc.)
pub use agent_provider::{
    map_tool_name, normalize_tool_name, AgentProvider, AgentProviderError, AgentProviderRegistry,
    AgentProviderType, AgentQueryOptions, AgentStream, AgentStreamEvent, ClaudeAgentSdkProvider,
    CodexProvider, McpServerConfig, NativeAgentProvider, NativeProviderConfig, PermissionMode,
    SessionInfo, SubagentDefinition, TokenUsage as AgentTokenUsage,
};
