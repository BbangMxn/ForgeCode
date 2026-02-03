//! Provider trait and common types

use crate::error::ProviderError;
use crate::{Message, ToolCall, ToolDef};
use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};
use std::pin::Pin;

/// Token usage information
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Input tokens (prompt)
    pub input_tokens: u32,

    /// Output tokens (completion)
    pub output_tokens: u32,

    /// Cached input tokens (if applicable)
    pub cache_read_tokens: u32,

    /// Tokens used to create cache
    pub cache_creation_tokens: u32,
}

impl TokenUsage {
    /// Total tokens used
    pub fn total(&self) -> u32 {
        self.input_tokens + self.output_tokens
    }

    /// Add another usage to this one
    pub fn add(&mut self, other: &TokenUsage) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.cache_read_tokens += other.cache_read_tokens;
        self.cache_creation_tokens += other.cache_creation_tokens;
    }

    /// Estimate cost in USD (rough approximation)
    pub fn estimate_cost(&self, input_price_per_1m: f64, output_price_per_1m: f64) -> f64 {
        let input_cost = (self.input_tokens as f64 / 1_000_000.0) * input_price_per_1m;
        let output_cost = (self.output_tokens as f64 / 1_000_000.0) * output_price_per_1m;
        input_cost + output_cost
    }
}

impl std::ops::Add for TokenUsage {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            input_tokens: self.input_tokens + other.input_tokens,
            output_tokens: self.output_tokens + other.output_tokens,
            cache_read_tokens: self.cache_read_tokens + other.cache_read_tokens,
            cache_creation_tokens: self.cache_creation_tokens + other.cache_creation_tokens,
        }
    }
}

impl std::ops::AddAssign for TokenUsage {
    fn add_assign(&mut self, other: Self) {
        self.add(&other);
    }
}

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
