//! Groq provider implementation (OpenAI-compatible API)
//!
//! Groq provides fast inference for open-source models using their LPU architecture.
//! The API is compatible with OpenAI's chat completion format.

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

const DEFAULT_API_URL: &str = "https://api.groq.com/openai/v1/chat/completions";
const DEFAULT_TIMEOUT_SECS: u64 = 60; // Groq is fast

/// Groq provider (OpenAI-compatible)
pub struct GroqProvider {
    client: Client,
    api_key: String,
    model_info: ModelInfo,
    metadata: ProviderMetadata,
    max_tokens: u32,
    base_url: String,
}

impl GroqProvider {
    /// Create a new Groq provider
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
            id: "groq".to_string(),
            display_name: "Groq".to_string(),
            models: vec![
                Self::get_model_info("llama-3.3-70b-versatile"),
                Self::get_model_info("llama-3.1-8b-instant"),
                Self::get_model_info("llama-3.2-90b-vision-preview"),
                Self::get_model_info("mixtral-8x7b-32768"),
                Self::get_model_info("gemma2-9b-it"),
            ],
            default_model: "llama-3.3-70b-versatile".to_string(),
            config_keys: vec![ConfigKey {
                name: "api_key".to_string(),
                required: true,
                secret: true,
                env_var: Some("GROQ_API_KEY".to_string()),
                description: "Groq API key".to_string(),
            }],
            base_url: Some(DEFAULT_API_URL.to_string()),
        }
    }

    fn get_model_info(model_id: &str) -> ModelInfo {
        match model_id {
            "llama-3.3-70b-versatile" => ModelInfo {
                id: "llama-3.3-70b-versatile".to_string(),
                provider: "groq".to_string(),
                display_name: "Llama 3.3 70B Versatile".to_string(),
                context_window: 128000,
                max_output_tokens: 32768,
                supports_tools: true,
                supports_vision: false,
                supports_thinking: false,
                input_price_per_1m: 0.59,
                output_price_per_1m: 0.79,
            },
            "llama-3.1-8b-instant" => ModelInfo {
                id: "llama-3.1-8b-instant".to_string(),
                provider: "groq".to_string(),
                display_name: "Llama 3.1 8B Instant".to_string(),
                context_window: 128000,
                max_output_tokens: 8000,
                supports_tools: true,
                supports_vision: false,
                supports_thinking: false,
                input_price_per_1m: 0.05,
                output_price_per_1m: 0.08,
            },
            "llama-3.2-90b-vision-preview" => ModelInfo {
                id: "llama-3.2-90b-vision-preview".to_string(),
                provider: "groq".to_string(),
                display_name: "Llama 3.2 90B Vision".to_string(),
                context_window: 128000,
                max_output_tokens: 8000,
                supports_tools: true,
                supports_vision: true,
                supports_thinking: false,
                input_price_per_1m: 0.90,
                output_price_per_1m: 0.90,
            },
            "mixtral-8x7b-32768" => ModelInfo {
                id: "mixtral-8x7b-32768".to_string(),
                provider: "groq".to_string(),
                display_name: "Mixtral 8x7B".to_string(),
                context_window: 32768,
                max_output_tokens: 32768,
                supports_tools: true,
                supports_vision: false,
                supports_thinking: false,
                input_price_per_1m: 0.24,
                output_price_per_1m: 0.24,
            },
            "gemma2-9b-it" => ModelInfo {
                id: "gemma2-9b-it".to_string(),
                provider: "groq".to_string(),
                display_name: "Gemma 2 9B".to_string(),
                context_window: 8192,
                max_output_tokens: 8192,
                supports_tools: true,
                supports_vision: false,
                supports_thinking: false,
                input_price_per_1m: 0.20,
                output_price_per_1m: 0.20,
            },
            _ => ModelInfo::new(model_id, "groq"),
        }
    }

    fn build_request(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        system_prompt: Option<&str>,
        stream: bool,
    ) -> GroqRequest {
        let mut api_messages: Vec<GroqMessage> = vec![];

        // Add system prompt
        if let Some(system) = system_prompt {
            api_messages.push(GroqMessage {
                role: "system".to_string(),
                content: Some(system.to_string()),
                tool_calls: None,
                tool_call_id: None,
            });
        }

        // Convert messages
        for msg in messages {
            if msg.role == MessageRole::System {
                continue;
            }
            api_messages.push(msg.into());
        }

        let api_tools: Vec<GroqTool> = tools.iter().map(|t| t.into()).collect();

        GroqRequest {
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

    fn parse_error_response(status: reqwest::StatusCode, body: &str) -> ProviderError {
        // Try to parse as JSON error
        if let Ok(error_response) = serde_json::from_str::<GroqErrorResponse>(body) {
            let error = error_response.error;
            let message = error.message;

            return match error.code.as_deref() {
                Some("rate_limit_exceeded") => ProviderError::RateLimited {
                    retry_after_ms: None,
                },
                Some("context_length_exceeded") => ProviderError::ContextLengthExceeded(message),
                Some("invalid_api_key") => ProviderError::Authentication(message),
                Some("model_not_found") => ProviderError::ModelNotFound(message),
                _ => ProviderError::from_http_status(status.as_u16(), &message),
            };
        }

        ProviderError::from_http_status(status.as_u16(), body)
    }
}

#[async_trait]
impl Provider for GroqProvider {
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

            // Track accumulated tool calls
            let mut current_tool_calls: std::collections::HashMap<i32, PartialToolCall> =
                std::collections::HashMap::new();
            let mut total_usage = TokenUsage::default();

            // SSE stream parsing (same as OpenAI)
            let byte_stream = response.bytes_stream();
            let stream_reader = StreamReader::new(
                byte_stream.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)),
            );
            let mut reader = BufReader::new(stream_reader);
            let mut line_buffer = String::new();

            loop {
                line_buffer.clear();
                match reader.read_line(&mut line_buffer).await {
                    Ok(0) => break,
                    Ok(_) => {
                        let line = line_buffer.trim();

                        if line.is_empty() || line.starts_with(':') {
                            continue;
                        }

                        if let Some(data) = line.strip_prefix("data: ") {
                            if data == "[DONE]" {
                                // Emit remaining tool calls
                                for (_, partial) in current_tool_calls.drain() {
                                    if let Some(tool_call) = partial.into_tool_call() {
                                        yield StreamEvent::ToolCall(tool_call);
                                    }
                                }
                                yield StreamEvent::Usage(total_usage.clone());
                                yield StreamEvent::Done;
                                break;
                            }

                            match serde_json::from_str::<GroqStreamChunk>(data) {
                                Ok(chunk) => {
                                    for choice in chunk.choices {
                                        let delta = choice.delta;

                                        // Handle text
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
                                                for (_, partial) in current_tool_calls.drain() {
                                                    if let Some(tool_call) = partial.into_tool_call() {
                                                        yield StreamEvent::ToolCall(tool_call);
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    // Handle usage
                                    if let Some(usage) = chunk.usage {
                                        total_usage = TokenUsage {
                                            input_tokens: usage.prompt_tokens,
                                            output_tokens: usage.completion_tokens,
                                            cache_read_tokens: 0,
                                            cache_creation_tokens: 0,
                                        };
                                    }

                                    // Handle x_groq usage (Groq-specific)
                                    if let Some(x_groq) = chunk.x_groq {
                                        if let Some(usage) = x_groq.usage {
                                            total_usage = TokenUsage {
                                                input_tokens: usage.prompt_tokens,
                                                output_tokens: usage.completion_tokens,
                                                cache_read_tokens: 0,
                                                cache_creation_tokens: 0,
                                            };
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!("Failed to parse Groq chunk: {} - data: {}", e, data);
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

        let api_response: GroqResponse = response
            .json()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        let choice =
            api_response.choices.into_iter().next().ok_or_else(|| {
                ProviderError::InvalidResponse("No choices in response".to_string())
            })?;

        let content = choice.message.content.unwrap_or_default();
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

// Helper struct for building tool calls during streaming
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
// Groq API Types (OpenAI-compatible)
// ============================================================================

#[derive(Debug, Serialize)]
struct GroqRequest {
    model: String,
    messages: Vec<GroqMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<GroqTool>>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<StreamOptions>,
}

#[derive(Debug, Serialize)]
struct StreamOptions {
    include_usage: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct GroqMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<GroqToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GroqToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: GroqFunctionCall,
}

#[derive(Debug, Serialize, Deserialize)]
struct GroqFunctionCall {
    name: String,
    arguments: String,
}

impl GroqFunctionCall {
    fn arguments_parsed(&self) -> serde_json::Value {
        serde_json::from_str(&self.arguments).unwrap_or(serde_json::Value::Null)
    }
}

#[derive(Debug, Serialize)]
struct GroqTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: GroqFunction,
}

#[derive(Debug, Serialize)]
struct GroqFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

// Response types
#[derive(Debug, Deserialize)]
struct GroqResponse {
    choices: Vec<GroqChoice>,
    usage: GroqUsage,
}

#[derive(Debug, Deserialize)]
struct GroqChoice {
    message: GroqMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GroqUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
}

// Streaming types
#[derive(Debug, Deserialize)]
struct GroqStreamChunk {
    choices: Vec<GroqStreamChoice>,
    #[serde(default)]
    usage: Option<GroqUsage>,
    #[serde(default)]
    x_groq: Option<XGroq>,
}

#[derive(Debug, Deserialize)]
struct XGroq {
    usage: Option<GroqUsage>,
}

#[derive(Debug, Deserialize)]
struct GroqStreamChoice {
    delta: GroqDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GroqDelta {
    content: Option<String>,
    tool_calls: Option<Vec<GroqStreamToolCall>>,
}

#[derive(Debug, Deserialize)]
struct GroqStreamToolCall {
    index: i32,
    id: Option<String>,
    function: Option<GroqStreamFunction>,
}

#[derive(Debug, Deserialize)]
struct GroqStreamFunction {
    name: Option<String>,
    arguments: Option<String>,
}

// Error types
#[derive(Debug, Deserialize)]
struct GroqErrorResponse {
    error: GroqError,
}

#[derive(Debug, Deserialize)]
struct GroqError {
    message: String,
    code: Option<String>,
}

// ============================================================================
// Conversions
// ============================================================================

impl From<&Message> for GroqMessage {
    fn from(msg: &Message) -> Self {
        // Handle tool results
        if let Some(ref tool_result) = msg.tool_result {
            return GroqMessage {
                role: "tool".to_string(),
                content: Some(tool_result.content.clone()),
                tool_calls: None,
                tool_call_id: Some(tool_result.tool_call_id.clone()),
            };
        }

        let role = match msg.role {
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::System => "system",
            MessageRole::Tool => "tool",
        };

        let tool_calls = msg.tool_calls.as_ref().map(|tcs| {
            tcs.iter()
                .map(|tc| GroqToolCall {
                    id: tc.id.clone(),
                    call_type: "function".to_string(),
                    function: GroqFunctionCall {
                        name: tc.name.clone(),
                        arguments: tc.arguments.to_string(),
                    },
                })
                .collect()
        });

        let content = if msg.content.is_empty() {
            None
        } else {
            Some(msg.content.clone())
        };

        GroqMessage {
            role: role.to_string(),
            content,
            tool_calls,
            tool_call_id: None,
        }
    }
}

impl From<&ToolDef> for GroqTool {
    fn from(tool: &ToolDef) -> Self {
        GroqTool {
            tool_type: "function".to_string(),
            function: GroqFunction {
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
        let info = GroqProvider::get_model_info("llama-3.3-70b-versatile");
        assert_eq!(info.id, "llama-3.3-70b-versatile");
        assert_eq!(info.context_window, 128000);
        assert!(info.supports_tools);
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
