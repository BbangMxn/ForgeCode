//! OpenAI provider implementation with SSE streaming support

use crate::{
    error::ProviderError,
    r#trait::{
        ConfigKey, FinishReason, ModelInfo, Provider, ProviderMetadata, ProviderResponse,
        StreamEvent, TokenUsage,
    },
    Message, MessageRole, ToolCall, ToolDef,
};
use async_trait::async_trait;
use futures::{Stream, TryStreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_util::io::StreamReader;

const DEFAULT_API_URL: &str = "https://api.openai.com/v1/chat/completions";
const DEFAULT_TIMEOUT_SECS: u64 = 300;

/// OpenAI provider with SSE streaming support
pub struct OpenAiProvider {
    client: Client,
    api_key: String,
    model_info: ModelInfo,
    metadata: ProviderMetadata,
    max_tokens: u32,
    base_url: String,
}

impl OpenAiProvider {
    /// Create a new OpenAI provider
    pub fn new(api_key: impl Into<String>, model: impl Into<String>, max_tokens: u32) -> Self {
        let model_id = model.into();
        let model_info = Self::get_model_info(&model_id);

        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
                .build()
                .expect("Failed to create HTTP client"),
            api_key: api_key.into(),
            model_info,
            metadata: Self::create_metadata(),
            max_tokens,
            base_url: DEFAULT_API_URL.to_string(),
        }
    }

    /// Create with custom base URL (for OpenAI-compatible APIs like Azure, LocalAI, etc.)
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Set custom timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.client = Client::builder()
            .timeout(timeout)
            .build()
            .expect("Failed to create HTTP client");
        self
    }

    fn create_metadata() -> ProviderMetadata {
        ProviderMetadata {
            id: "openai".to_string(),
            display_name: "OpenAI".to_string(),
            models: vec![
                Self::get_model_info("gpt-4o"),
                Self::get_model_info("gpt-4o-mini"),
                Self::get_model_info("gpt-4-turbo"),
                Self::get_model_info("gpt-3.5-turbo"),
                Self::get_model_info("o1"),
                Self::get_model_info("o1-mini"),
            ],
            default_model: "gpt-4o".to_string(),
            config_keys: vec![
                ConfigKey {
                    name: "api_key".to_string(),
                    required: true,
                    secret: true,
                    env_var: Some("OPENAI_API_KEY".to_string()),
                    description: "OpenAI API key".to_string(),
                },
                ConfigKey {
                    name: "base_url".to_string(),
                    required: false,
                    secret: false,
                    env_var: Some("OPENAI_BASE_URL".to_string()),
                    description: "Custom API base URL".to_string(),
                },
            ],
            base_url: Some(DEFAULT_API_URL.to_string()),
        }
    }

    fn get_model_info(model_id: &str) -> ModelInfo {
        match model_id {
            "gpt-4o" => ModelInfo {
                id: "gpt-4o".to_string(),
                provider: "openai".to_string(),
                display_name: "GPT-4o".to_string(),
                context_window: 128000,
                max_output_tokens: 16384,
                supports_tools: true,
                supports_vision: true,
                supports_thinking: false,
                input_price_per_1m: 2.50,
                output_price_per_1m: 10.00,
            },
            "gpt-4o-mini" => ModelInfo {
                id: "gpt-4o-mini".to_string(),
                provider: "openai".to_string(),
                display_name: "GPT-4o Mini".to_string(),
                context_window: 128000,
                max_output_tokens: 16384,
                supports_tools: true,
                supports_vision: true,
                supports_thinking: false,
                input_price_per_1m: 0.15,
                output_price_per_1m: 0.60,
            },
            "gpt-4-turbo" | "gpt-4-turbo-preview" => ModelInfo {
                id: model_id.to_string(),
                provider: "openai".to_string(),
                display_name: "GPT-4 Turbo".to_string(),
                context_window: 128000,
                max_output_tokens: 4096,
                supports_tools: true,
                supports_vision: true,
                supports_thinking: false,
                input_price_per_1m: 10.00,
                output_price_per_1m: 30.00,
            },
            "gpt-3.5-turbo" => ModelInfo {
                id: "gpt-3.5-turbo".to_string(),
                provider: "openai".to_string(),
                display_name: "GPT-3.5 Turbo".to_string(),
                context_window: 16385,
                max_output_tokens: 4096,
                supports_tools: true,
                supports_vision: false,
                supports_thinking: false,
                input_price_per_1m: 0.50,
                output_price_per_1m: 1.50,
            },
            "o1" => ModelInfo {
                id: "o1".to_string(),
                provider: "openai".to_string(),
                display_name: "o1".to_string(),
                context_window: 200000,
                max_output_tokens: 100000,
                supports_tools: true,
                supports_vision: true,
                supports_thinking: true,
                input_price_per_1m: 15.00,
                output_price_per_1m: 60.00,
            },
            "o1-mini" => ModelInfo {
                id: "o1-mini".to_string(),
                provider: "openai".to_string(),
                display_name: "o1-mini".to_string(),
                context_window: 128000,
                max_output_tokens: 65536,
                supports_tools: true,
                supports_vision: true,
                supports_thinking: true,
                input_price_per_1m: 3.00,
                output_price_per_1m: 12.00,
            },
            _ => ModelInfo::new(model_id, "openai"),
        }
    }

    fn build_request(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        system_prompt: Option<&str>,
        stream: bool,
    ) -> OpenAiRequest {
        let mut api_messages: Vec<OpenAiMessage> = vec![];

        // Add system prompt
        if let Some(system) = system_prompt {
            api_messages.push(OpenAiMessage {
                role: "system".to_string(),
                content: Some(OpenAiContent::Text(system.to_string())),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            });
        }

        // Convert messages
        for msg in messages {
            if msg.role == MessageRole::System {
                continue; // Skip, handled above
            }
            api_messages.push(msg.into());
        }

        let api_tools: Vec<OpenAiTool> = tools.iter().map(|t| t.into()).collect();

        OpenAiRequest {
            model: self.model_info.id.clone(),
            messages: api_messages,
            max_tokens: Some(self.max_tokens),
            tools: if api_tools.is_empty() {
                None
            } else {
                Some(api_tools)
            },
            stream,
            stream_options: if stream {
                Some(StreamOptions {
                    include_usage: true,
                })
            } else {
                None
            },
        }
    }

    /// Parse error response from OpenAI API
    fn parse_error_response(status: reqwest::StatusCode, body: &str) -> ProviderError {
        // Try to parse as JSON error
        if let Ok(error_response) = serde_json::from_str::<OpenAiErrorResponse>(body) {
            let error = error_response.error;
            let message = error.message;

            return match error.code.as_deref() {
                Some("rate_limit_exceeded") => ProviderError::RateLimited {
                    retry_after_ms: None,
                },
                Some("context_length_exceeded") => ProviderError::ContextLengthExceeded(message),
                Some("invalid_api_key") => ProviderError::Authentication(message),
                Some("insufficient_quota") => ProviderError::QuotaExceeded(message),
                Some("model_not_found") => ProviderError::ModelNotFound(message),
                Some("content_policy_violation") => ProviderError::ContentFiltered(message),
                _ => ProviderError::from_http_status(status.as_u16(), &message),
            };
        }

        ProviderError::from_http_status(status.as_u16(), body)
    }
}

#[async_trait]
impl Provider for OpenAiProvider {
    fn metadata(&self) -> &ProviderMetadata {
        &self.metadata
    }

    fn model(&self) -> &ModelInfo {
        &self.model_info
    }

    fn stream(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolDef>,
        system_prompt: Option<String>,
    ) -> Pin<Box<dyn Stream<Item = StreamEvent> + Send + '_>> {
        let request = self.build_request(&messages, &tools, system_prompt.as_deref(), true);

        Box::pin(async_stream::stream! {
            let response = match self
                .client
                .post(&self.base_url)
                .header("Authorization", format!("Bearer {}", self.api_key))
                .header("Content-Type", "application/json")
                .header("Accept", "text/event-stream")
                .json(&request)
                .send()
                .await
            {
                Ok(resp) => resp,
                Err(e) => {
                    yield StreamEvent::Error(ProviderError::Network(e.to_string()));
                    return;
                }
            };

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                let error = Self::parse_error_response(status, &body);
                yield StreamEvent::Error(error);
                return;
            }

            // Track accumulated state for tool calls
            let mut current_tool_calls: std::collections::HashMap<i32, PartialToolCall> =
                std::collections::HashMap::new();
            let mut total_usage = TokenUsage::default();

            // Convert response body to async reader for SSE parsing
            let byte_stream = response.bytes_stream();
            let stream_reader = StreamReader::new(
                byte_stream.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)),
            );
            let mut reader = BufReader::new(stream_reader);
            let mut line_buffer = String::new();

            loop {
                line_buffer.clear();
                match reader.read_line(&mut line_buffer).await {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        let line = line_buffer.trim();

                        // Skip empty lines and comments
                        if line.is_empty() || line.starts_with(':') {
                            continue;
                        }

                        // Parse SSE data line
                        if let Some(data) = line.strip_prefix("data: ") {
                            // Check for stream end
                            if data == "[DONE]" {
                                // Emit any completed tool calls
                                for (_, partial) in current_tool_calls.drain() {
                                    if let Some(tool_call) = partial.into_tool_call() {
                                        yield StreamEvent::ToolCall(tool_call);
                                    }
                                }
                                yield StreamEvent::Usage(total_usage.clone());
                                yield StreamEvent::Done;
                                break;
                            }

                            // Parse chunk
                            match serde_json::from_str::<OpenAiStreamChunk>(data) {
                                Ok(chunk) => {
                                    // Process choices
                                    for choice in chunk.choices {
                                        let delta = choice.delta;

                                        // Handle text content
                                        if let Some(content) = delta.content {
                                            if !content.is_empty() {
                                                yield StreamEvent::Text(content);
                                            }
                                        }

                                        // Handle tool calls
                                        if let Some(tool_calls) = delta.tool_calls {
                                            for tc in tool_calls {
                                                let entry = current_tool_calls
                                                    .entry(tc.index)
                                                    .or_insert_with(|| PartialToolCall {
                                                        id: String::new(),
                                                        name: String::new(),
                                                        arguments: String::new(),
                                                    });

                                                if let Some(id) = tc.id {
                                                    entry.id = id;
                                                }
                                                if let Some(function) = tc.function {
                                                    if let Some(name) = function.name {
                                                        entry.name = name;
                                                    }
                                                    if let Some(args) = function.arguments {
                                                        entry.arguments.push_str(&args);
                                                    }
                                                }
                                            }
                                        }

                                        // Handle finish reason
                                        if let Some(reason) = choice.finish_reason {
                                            if reason == "tool_calls" {
                                                // Emit all accumulated tool calls
                                                for (_, partial) in current_tool_calls.drain() {
                                                    if let Some(tool_call) = partial.into_tool_call() {
                                                        yield StreamEvent::ToolCall(tool_call);
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    // Handle usage (typically in final chunk)
                                    if let Some(usage) = chunk.usage {
                                        total_usage = TokenUsage {
                                            input_tokens: usage.prompt_tokens,
                                            output_tokens: usage.completion_tokens,
                                            cache_read_tokens: 0,
                                            cache_creation_tokens: 0,
                                        };
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!("Failed to parse SSE chunk: {} - data: {}", e, data);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        yield StreamEvent::Error(ProviderError::StreamError(format!("Stream read error: {}", e)));
                        break;
                    }
                }
            }
        })
    }

    async fn complete(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolDef>,
        system_prompt: Option<String>,
    ) -> Result<ProviderResponse, ProviderError> {
        let request = self.build_request(&messages, &tools, system_prompt.as_deref(), false);

        let response = self
            .client
            .post(&self.base_url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(Self::parse_error_response(status, &body));
        }

        let api_response: OpenAiResponse = response
            .json()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        let choice =
            api_response.choices.into_iter().next().ok_or_else(|| {
                ProviderError::InvalidResponse("No choices in response".to_string())
            })?;

        let content = match choice.message.content {
            Some(OpenAiContent::Text(text)) => text,
            Some(OpenAiContent::Parts(parts)) => parts
                .into_iter()
                .filter_map(|p| {
                    if let OpenAiContentPart::Text { text } = p {
                        Some(text)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join(""),
            None => String::new(),
        };

        let tool_calls = choice
            .message
            .tool_calls
            .unwrap_or_default()
            .into_iter()
            .map(|tc| {
                let args = tc.function.arguments_parsed();
                ToolCall::new(tc.id, tc.function.name, args)
            })
            .collect();

        let finish_reason = match choice.finish_reason.as_deref() {
            Some("stop") => FinishReason::Stop,
            Some("length") => FinishReason::MaxTokens,
            Some("tool_calls") => FinishReason::ToolUse,
            Some("content_filter") => FinishReason::ContentFilter,
            _ => FinishReason::Other,
        };

        Ok(ProviderResponse {
            content,
            tool_calls,
            usage: TokenUsage {
                input_tokens: api_response.usage.prompt_tokens,
                output_tokens: api_response.usage.completion_tokens,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
            },
            finish_reason,
            model: self.model_info.id.clone(),
        })
    }

    fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }

    fn set_model(&mut self, model_id: &str) -> Result<(), ProviderError> {
        self.model_info = Self::get_model_info(model_id);
        Ok(())
    }
}

// Helper struct for accumulating tool calls during streaming
#[derive(Debug)]
struct PartialToolCall {
    id: String,
    name: String,
    arguments: String,
}

impl PartialToolCall {
    fn into_tool_call(self) -> Option<ToolCall> {
        if self.id.is_empty() || self.name.is_empty() {
            return None;
        }

        let arguments = serde_json::from_str(&self.arguments).unwrap_or(serde_json::Value::Null);
        Some(ToolCall::new(self.id, self.name, arguments))
    }
}

// ============================================================================
// OpenAI API Types
// ============================================================================

#[derive(Debug, Serialize)]
struct OpenAiRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenAiTool>>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<StreamOptions>,
}

#[derive(Debug, Serialize)]
struct StreamOptions {
    include_usage: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAiMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<OpenAiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAiToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum OpenAiContent {
    Text(String),
    Parts(Vec<OpenAiContentPart>),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
enum OpenAiContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: ImageUrl },
}

#[derive(Debug, Serialize, Deserialize)]
struct ImageUrl {
    url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAiToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: OpenAiFunctionCall,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAiFunctionCall {
    name: String,
    arguments: String,
}

impl OpenAiFunctionCall {
    fn arguments_parsed(&self) -> serde_json::Value {
        serde_json::from_str(&self.arguments).unwrap_or(serde_json::Value::Null)
    }
}

#[derive(Debug, Serialize)]
struct OpenAiTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: OpenAiFunction,
}

#[derive(Debug, Serialize)]
struct OpenAiFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

// Response types
#[derive(Debug, Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
    usage: OpenAiUsage,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
}

// Streaming types
#[derive(Debug, Deserialize)]
struct OpenAiStreamChunk {
    choices: Vec<OpenAiStreamChoice>,
    #[serde(default)]
    usage: Option<OpenAiUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamChoice {
    delta: OpenAiDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiDelta {
    content: Option<String>,
    tool_calls: Option<Vec<OpenAiStreamToolCall>>,
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamToolCall {
    index: i32,
    id: Option<String>,
    function: Option<OpenAiStreamFunction>,
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamFunction {
    name: Option<String>,
    arguments: Option<String>,
}

// Error types
#[derive(Debug, Deserialize)]
struct OpenAiErrorResponse {
    error: OpenAiError,
}

#[derive(Debug, Deserialize)]
struct OpenAiError {
    message: String,
    #[serde(rename = "type")]
    #[allow(dead_code)]
    error_type: Option<String>,
    code: Option<String>,
}

// ============================================================================
// Conversions
// ============================================================================

impl From<&Message> for OpenAiMessage {
    fn from(msg: &Message) -> Self {
        // Handle tool results
        if let Some(ref tool_result) = msg.tool_result {
            return OpenAiMessage {
                role: "tool".to_string(),
                content: Some(OpenAiContent::Text(tool_result.content.clone())),
                tool_calls: None,
                tool_call_id: Some(tool_result.tool_call_id.clone()),
                name: None,
            };
        }

        let role = match msg.role {
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::System => "system",
            MessageRole::Tool => "tool",
        };

        // Handle assistant messages with tool calls
        let tool_calls = msg.tool_calls.as_ref().map(|tcs| {
            tcs.iter()
                .map(|tc| OpenAiToolCall {
                    id: tc.id.clone(),
                    call_type: "function".to_string(),
                    function: OpenAiFunctionCall {
                        name: tc.name.clone(),
                        arguments: tc.arguments.to_string(),
                    },
                })
                .collect()
        });

        // Build content
        let content = if msg.content.is_empty() {
            None
        } else {
            Some(OpenAiContent::Text(msg.content.clone()))
        };

        OpenAiMessage {
            role: role.to_string(),
            content,
            tool_calls,
            tool_call_id: None,
            name: None,
        }
    }
}

impl From<&ToolDef> for OpenAiTool {
    fn from(tool: &ToolDef) -> Self {
        OpenAiTool {
            tool_type: "function".to_string(),
            function: OpenAiFunction {
                name: tool.name.clone(),
                description: tool.description.clone(),
                parameters: serde_json::json!({
                    "type": tool.parameters.schema_type,
                    "properties": tool.parameters.properties,
                    "required": tool.parameters.required
                }),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_info() {
        let info = OpenAiProvider::get_model_info("gpt-4o");
        assert_eq!(info.id, "gpt-4o");
        assert_eq!(info.context_window, 128000);
        assert!(info.supports_vision);
    }

    #[test]
    fn test_partial_tool_call() {
        let partial = PartialToolCall {
            id: "call_123".to_string(),
            name: "read_file".to_string(),
            arguments: r#"{"path": "/test.txt"}"#.to_string(),
        };

        let tool_call = partial.into_tool_call().unwrap();
        assert_eq!(tool_call.id, "call_123");
        assert_eq!(tool_call.name, "read_file");
    }
}
