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
    FinishReason, ModelInfo, Provider, ProviderMetadata, ProviderResponse, StreamEvent, TokenUsage,
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
