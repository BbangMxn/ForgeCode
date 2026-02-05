//! Provider trait and common types
//!
//! ## 타입 의존성
//!
//! - `TokenUsage`: Layer1-foundation에서 re-export (표준 타입)
//! - `StreamEvent`: 이 레이어 고유 정의 (ProviderError 포함)
//! - `Message`, `ToolCall`: Layer1-foundation에서 re-export

use crate::error::ProviderError;
use crate::{Message, ToolCall, ToolDef};
use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};
use std::pin::Pin;

// Re-export TokenUsage from Layer1-foundation (표준 타입)
pub use forge_foundation::TokenUsage;

/// Events emitted during streaming
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// Text content delta
    Text(String),

    /// Thinking/reasoning content (for models that support it)
    Thinking(String),

    /// Tool call started (partial)
    ToolCallStart {
        index: usize,
        id: String,
        name: String,
    },

    /// Tool call argument delta
    ToolCallDelta {
        index: usize,
        arguments_delta: String,
    },

    /// Tool call completed
    ToolCall(ToolCall),

    /// Token usage update
    Usage(TokenUsage),

    /// Stream completed
    Done,

    /// Error occurred
    Error(ProviderError),
}

/// Model information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Model ID (e.g., "claude-sonnet-4-20250514")
    pub id: String,

    /// Provider name (e.g., "anthropic")
    pub provider: String,

    /// Display name
    pub display_name: String,

    /// Context window size (tokens)
    pub context_window: u32,

    /// Max output tokens
    pub max_output_tokens: u32,

    /// Whether the model supports tool use
    pub supports_tools: bool,

    /// Whether the model supports vision/images
    pub supports_vision: bool,

    /// Whether the model supports extended thinking
    pub supports_thinking: bool,

    /// Input price per 1M tokens (USD)
    pub input_price_per_1m: f64,

    /// Output price per 1M tokens (USD)
    pub output_price_per_1m: f64,
}

impl ModelInfo {
    /// Create a basic model info
    pub fn new(id: impl Into<String>, provider: impl Into<String>) -> Self {
        let id = id.into();
        Self {
            display_name: id.clone(),
            id,
            provider: provider.into(),
            context_window: 128000,
            max_output_tokens: 8192,
            supports_tools: true,
            supports_vision: false,
            supports_thinking: false,
            input_price_per_1m: 0.0,
            output_price_per_1m: 0.0,
        }
    }
}

/// Provider configuration keys
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigKey {
    /// Key name (e.g., "api_key")
    pub name: String,

    /// Whether this key is required
    pub required: bool,

    /// Whether this is a secret (should be masked in UI)
    pub secret: bool,

    /// Environment variable to check
    pub env_var: Option<String>,

    /// Description for users
    pub description: String,
}

/// Provider metadata
#[derive(Debug, Clone)]
pub struct ProviderMetadata {
    /// Provider ID (e.g., "anthropic")
    pub id: String,

    /// Display name (e.g., "Anthropic")
    pub display_name: String,

    /// Available models
    pub models: Vec<ModelInfo>,

    /// Default model ID
    pub default_model: String,

    /// Configuration keys needed
    pub config_keys: Vec<ConfigKey>,

    /// Base URL (for OpenAI-compatible providers)
    pub base_url: Option<String>,
}

/// Token counting result
#[derive(Debug, Clone, Default)]
pub struct TokenCount {
    /// Total token count
    pub total: u32,

    /// Tokens from messages
    pub messages: u32,

    /// Tokens from system prompt
    pub system: u32,

    /// Tokens from tool definitions
    pub tools: u32,

    /// Whether this is an estimate (not exact count)
    pub is_estimate: bool,
}

impl TokenCount {
    /// Create a new token count
    pub fn new(total: u32) -> Self {
        Self {
            total,
            messages: total,
            system: 0,
            tools: 0,
            is_estimate: false,
        }
    }

    /// Create an estimated token count
    pub fn estimate(total: u32) -> Self {
        Self {
            total,
            messages: total,
            system: 0,
            tools: 0,
            is_estimate: true,
        }
    }

    /// Check if request fits within context window
    pub fn fits_context(&self, context_window: u32, reserve_output: u32) -> bool {
        self.total + reserve_output <= context_window
    }
}

/// LLM Provider trait
///
/// Implement this trait to add support for a new LLM provider.
#[async_trait]
pub trait Provider: Send + Sync {
    /// Get provider metadata
    fn metadata(&self) -> &ProviderMetadata;

    /// Get current model information
    fn model(&self) -> &ModelInfo;

    /// Send messages and get a streaming response
    fn stream(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolDef>,
        system_prompt: Option<String>,
    ) -> Pin<Box<dyn Stream<Item = StreamEvent> + Send + '_>>;

    /// Send messages and get a complete response (non-streaming)
    async fn complete(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolDef>,
        system_prompt: Option<String>,
    ) -> Result<ProviderResponse, ProviderError>;

    /// Check if the provider is available (e.g., API key is set)
    fn is_available(&self) -> bool;

    /// Change the current model
    fn set_model(&mut self, model_id: &str) -> Result<(), ProviderError>;

    /// List available models
    fn list_models(&self) -> &[ModelInfo] {
        &self.metadata().models
    }

    /// Count tokens for the given input
    ///
    /// This is used to check if a request will fit within the context window
    /// before sending it to the provider. The default implementation uses
    /// a character-based estimate (4 chars per token).
    ///
    /// Providers with native tokenizers should override this method for
    /// accurate counting.
    fn count_tokens(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        system_prompt: Option<&str>,
    ) -> TokenCount {
        // Default implementation: estimate based on character count
        // Rough approximation: ~4 characters per token for English text
        let mut total_chars = 0usize;
        let mut message_chars = 0usize;
        let mut tool_chars = 0usize;
        let mut system_chars = 0usize;

        // Count message content
        for msg in messages {
            message_chars += msg.content.len();
            // Tool results tend to be more token-dense
            if let Some(ref result) = msg.tool_result {
                message_chars += result.content.len();
            }
        }

        // Count tool definitions (JSON tends to be more token-dense)
        for tool in tools {
            tool_chars += tool.name.len();
            tool_chars += tool.description.len();
            // JSON schema is typically ~3 chars per token
            if let Ok(params_json) = serde_json::to_string(&tool.parameters) {
                tool_chars += params_json.len();
            }
        }

        // Count system prompt
        if let Some(system) = system_prompt {
            system_chars = system.len();
        }

        total_chars = message_chars + tool_chars + system_chars;

        // Convert to tokens (rough estimate)
        let chars_per_token = 4;
        TokenCount {
            total: (total_chars / chars_per_token) as u32,
            messages: (message_chars / chars_per_token) as u32,
            system: (system_chars / chars_per_token) as u32,
            tools: (tool_chars / chars_per_token) as u32,
            is_estimate: true,
        }
    }

    /// Check if the input fits within the model's context window
    ///
    /// Returns the token count and whether it fits, reserving space for output.
    fn check_context_fit(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        system_prompt: Option<&str>,
        reserve_output_tokens: Option<u32>,
    ) -> (TokenCount, bool) {
        let count = self.count_tokens(messages, tools, system_prompt);
        let model = self.model();
        let reserve = reserve_output_tokens.unwrap_or(model.max_output_tokens);
        let fits = count.fits_context(model.context_window, reserve);
        (count, fits)
    }
}

/// Complete response from provider (for non-streaming)
#[derive(Debug, Clone)]
pub struct ProviderResponse {
    /// Text content
    pub content: String,

    /// Tool calls (if any)
    pub tool_calls: Vec<ToolCall>,

    /// Token usage
    pub usage: TokenUsage,

    /// Finish reason
    pub finish_reason: FinishReason,

    /// Model used (may differ from requested if fallback occurred)
    pub model: String,
}

/// Reason for completion finishing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FinishReason {
    /// Completed naturally
    Stop,

    /// Hit max tokens limit
    MaxTokens,

    /// Tool use requested
    ToolUse,

    /// Content filtered
    ContentFilter,

    /// Unknown/other
    Other,
}

impl Default for FinishReason {
    fn default() -> Self {
        Self::Other
    }
}
