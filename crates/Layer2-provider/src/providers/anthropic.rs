//! Anthropic (Claude) provider implementation with SSE streaming

use crate::{
    error::ProviderError,
    r#trait::{
        ConfigKey, FinishReason, ModelInfo, Provider, ProviderMetadata, ProviderResponse,
        StreamEvent, TokenUsage,
    },
    retry::{with_retry, RetryConfig},
    Message, MessageRole, ToolCall, ToolDef,
};
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use tracing::{debug, error};

const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Anthropic Claude provider
pub struct AnthropicProvider {
    client: Client,
    api_key: String,
    metadata: ProviderMetadata,
    current_model: ModelInfo,
    max_tokens: u32,
    retry_config: RetryConfig,
}

impl AnthropicProvider {
    /// Create a new Anthropic provider
    pub fn new(api_key: impl Into<String>, model: impl Into<String>, max_tokens: u32) -> Self {
        let api_key = api_key.into();
        let model_id = model.into();

        let models = Self::available_models();
        let current_model = models
            .iter()
            .find(|m| m.id == model_id)
            .cloned()
            .unwrap_or_else(|| ModelInfo::new(&model_id, "anthropic"));

        Self {
            client: Client::new(),
            api_key,
            metadata: ProviderMetadata {
                id: "anthropic".to_string(),
                display_name: "Anthropic".to_string(),
                models: models.clone(),
                default_model: "claude-sonnet-4-20250514".to_string(),
                config_keys: vec![ConfigKey {
                    name: "api_key".to_string(),
                    required: true,
                    secret: true,
                    env_var: Some("ANTHROPIC_API_KEY".to_string()),
                    description: "Anthropic API key".to_string(),
                }],
                base_url: Some(ANTHROPIC_API_URL.to_string()),
            },
            current_model,
            max_tokens,
            retry_config: RetryConfig::default(),
        }
    }

    /// Get list of available Anthropic models
    fn available_models() -> Vec<ModelInfo> {
        vec![
            ModelInfo {
                id: "claude-opus-4-20250514".to_string(),
                provider: "anthropic".to_string(),
                display_name: "Claude Opus 4".to_string(),
                context_window: 200000,
                max_output_tokens: 32000,
                supports_tools: true,
                supports_vision: true,
                supports_thinking: true,
                input_price_per_1m: 15.0,
                output_price_per_1m: 75.0,
            },
            ModelInfo {
                id: "claude-sonnet-4-20250514".to_string(),
                provider: "anthropic".to_string(),
                display_name: "Claude Sonnet 4".to_string(),
                context_window: 200000,
                max_output_tokens: 16000,
                supports_tools: true,
                supports_vision: true,
                supports_thinking: true,
                input_price_per_1m: 3.0,
                output_price_per_1m: 15.0,
            },
            ModelInfo {
                id: "claude-3-5-haiku-20241022".to_string(),
                provider: "anthropic".to_string(),
                display_name: "Claude 3.5 Haiku".to_string(),
                context_window: 200000,
                max_output_tokens: 8192,
                supports_tools: true,
                supports_vision: true,
                supports_thinking: false,
                input_price_per_1m: 0.8,
                output_price_per_1m: 4.0,
            },
        ]
    }

    /// Build request for Anthropic API
    fn build_request(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        system_prompt: Option<&str>,
        stream: bool,
    ) -> AnthropicRequest {
        let api_messages: Vec<AnthropicMessage> = messages
            .iter()
            .filter(|m| m.role != MessageRole::System)
            .map(|m| m.into())
            .collect();

        let api_tools: Vec<AnthropicTool> = tools.iter().map(|t| t.into()).collect();

        AnthropicRequest {
            model: self.current_model.id.clone(),
            max_tokens: self.max_tokens,
            system: system_prompt.map(|s| s.to_string()),
            messages: api_messages,
            tools: if api_tools.is_empty() {
                None
            } else {
                Some(api_tools)
            },
            stream,
        }
    }

    /// Make HTTP request to Anthropic API
    async fn make_request(
        &self,
        request: &AnthropicRequest,
    ) -> Result<reqwest::Response, ProviderError> {
        let response = self
            .client
            .post(ANTHROPIC_API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(request)
            .send()
            .await
            .map_err(|e| ProviderError::RequestFailed(e.to_string()))?;

        let status = response.status().as_u16();

        if status != 200 {
            let body = response.text().await.unwrap_or_default();
            return Err(ProviderError::from_http_status(status, &body));
        }

        Ok(response)
    }

    /// Parse SSE event line
    fn parse_sse_line(line: &str) -> Option<AnthropicStreamEvent> {
        if !line.starts_with("data: ") {
            return None;
        }

        let data = &line[6..];
        if data == "[DONE]" {
            return None;
        }

        serde_json::from_str(data).ok()
    }
}

#[async_trait]
impl Provider for AnthropicProvider {
    fn metadata(&self) -> &ProviderMetadata {
        &self.metadata
    }

    fn model(&self) -> &ModelInfo {
        &self.current_model
    }

    fn stream(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolDef>,
        system_prompt: Option<String>,
    ) -> Pin<Box<dyn Stream<Item = StreamEvent> + Send + '_>> {
        let request = self.build_request(&messages, &tools, system_prompt.as_deref(), true);

        Box::pin(async_stream::stream! {
            // Make request
            let response = match self.make_request(&request).await {
                Ok(r) => r,
                Err(e) => {
                    yield StreamEvent::Error(e);
                    return;
                }
            };

            // Process SSE stream
            let mut byte_stream = response.bytes_stream();
            let mut buffer = String::new();
            let mut current_text = String::new();
            let mut tool_calls: Vec<PartialToolCall> = Vec::new();
            let mut usage = TokenUsage::default();

            while let Some(chunk_result) = byte_stream.next().await {
                let chunk = match chunk_result {
                    Ok(c) => c,
                    Err(e) => {
                        yield StreamEvent::Error(ProviderError::StreamError(e.to_string()));
                        return;
                    }
                };

                buffer.push_str(&String::from_utf8_lossy(&chunk));

                // Process complete lines
                while let Some(newline_pos) = buffer.find('\n') {
                    let line = buffer[..newline_pos].trim().to_string();
                    buffer = buffer[newline_pos + 1..].to_string();

                    if line.is_empty() {
                        continue;
                    }

                    if let Some(event) = Self::parse_sse_line(&line) {
                        match event {
                            AnthropicStreamEvent::ContentBlockStart { index, content_block } => {
                                match content_block {
                                    ContentBlock::Text { .. } => {
                                        // Text block started
                                    }
                                    ContentBlock::ToolUse { id, name, .. } => {
                                        // Tool call started
                                        while tool_calls.len() <= index {
                                            tool_calls.push(PartialToolCall::default());
                                        }
                                        tool_calls[index] = PartialToolCall {
                                            id,
                                            name: name.clone(),
                                            arguments: String::new(),
                                        };
                                        yield StreamEvent::ToolCallStart {
                                            index,
                                            id: tool_calls[index].id.clone(),
                                            name,
                                        };
                                    }
                                    _ => {}
                                }
                            }
                            AnthropicStreamEvent::ContentBlockDelta { index, delta } => {
                                match delta {
                                    ContentDelta::TextDelta { text } => {
                                        current_text.push_str(&text);
                                        yield StreamEvent::Text(text);
                                    }
                                    ContentDelta::ThinkingDelta { thinking } => {
                                        yield StreamEvent::Thinking(thinking);
                                    }
                                    ContentDelta::InputJsonDelta { partial_json } => {
                                        if index < tool_calls.len() {
                                            tool_calls[index].arguments.push_str(&partial_json);
                                            yield StreamEvent::ToolCallDelta {
                                                index,
                                                arguments_delta: partial_json,
                                            };
                                        }
                                    }
                                }
                            }
                            AnthropicStreamEvent::ContentBlockStop { index } => {
                                // If this was a tool call, emit the complete tool call
                                if index < tool_calls.len() {
                                    let tc = &tool_calls[index];
                                    let arguments = serde_json::from_str(&tc.arguments)
                                        .unwrap_or(serde_json::Value::Null);
                                    yield StreamEvent::ToolCall(ToolCall::new(
                                        &tc.id,
                                        &tc.name,
                                        arguments,
                                    ));
                                }
                            }
                            AnthropicStreamEvent::MessageDelta { usage: msg_usage, .. } => {
                                if let Some(u) = msg_usage {
                                    usage.output_tokens = u.output_tokens;
                                }
                            }
                            AnthropicStreamEvent::MessageStart { message } => {
                                if let Some(u) = message.usage {
                                    usage.input_tokens = u.input_tokens;
                                    usage.cache_read_tokens = u.cache_read_input_tokens.unwrap_or(0);
                                    usage.cache_creation_tokens = u.cache_creation_input_tokens.unwrap_or(0);
                                }
                            }
                            AnthropicStreamEvent::MessageStop => {
                                yield StreamEvent::Usage(usage.clone());
                                yield StreamEvent::Done;
                                return;
                            }
                            _ => {}
                        }
                    }
                }
            }

            // Stream ended without MessageStop
            yield StreamEvent::Usage(usage);
            yield StreamEvent::Done;
        })
    }

    async fn complete(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolDef>,
        system_prompt: Option<String>,
    ) -> Result<ProviderResponse, ProviderError> {
        let request = self.build_request(&messages, &tools, system_prompt.as_deref(), false);

        // Execute with retry
        let response = with_retry(&self.retry_config, "anthropic_complete", || async {
            self.make_request(&request).await
        })
        .await?;

        let api_response: AnthropicResponse = response
            .json()
            .await
            .map_err(|e| ProviderError::ParseError(e.to_string()))?;

        // Extract content and tool calls
        let mut content = String::new();
        let mut tool_calls = Vec::new();

        for block in api_response.content {
            match block {
                ContentBlock::Text { text } => {
                    content.push_str(&text);
                }
                ContentBlock::ToolUse { id, name, input } => {
                    tool_calls.push(ToolCall::new(id, name, input));
                }
                _ => {}
            }
        }

        let finish_reason = match api_response.stop_reason.as_deref() {
            Some("end_turn") => FinishReason::Stop,
            Some("max_tokens") => FinishReason::MaxTokens,
            Some("tool_use") => FinishReason::ToolUse,
            _ => FinishReason::Other,
        };

        Ok(ProviderResponse {
            content,
            tool_calls,
            usage: TokenUsage {
                input_tokens: api_response.usage.input_tokens,
                output_tokens: api_response.usage.output_tokens,
                cache_read_tokens: api_response.usage.cache_read_input_tokens.unwrap_or(0),
                cache_creation_tokens: api_response.usage.cache_creation_input_tokens.unwrap_or(0),
            },
            finish_reason,
            model: api_response.model,
        })
    }

    fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }

    fn set_model(&mut self, model_id: &str) -> Result<(), ProviderError> {
        let model = self
            .metadata
            .models
            .iter()
            .find(|m| m.id == model_id)
            .cloned()
            .ok_or_else(|| ProviderError::ModelNotAvailable(model_id.to_string()))?;

        self.current_model = model;
        Ok(())
    }
}

// Helper struct for building tool calls during streaming
#[derive(Default)]
struct PartialToolCall {
    id: String,
    name: String,
    arguments: String,
}

// ============================================================================
// Anthropic API Types
// ============================================================================

#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<AnthropicTool>>,
    stream: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct AnthropicMessage {
    role: String,
    content: AnthropicContent,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum AnthropicContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "thinking")]
    Thinking { thinking: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
enum ContentDelta {
    #[serde(rename = "text_delta")]
    TextDelta { text: String },
    #[serde(rename = "thinking_delta")]
    ThinkingDelta { thinking: String },
    #[serde(rename = "input_json_delta")]
    InputJsonDelta { partial_json: String },
}

#[derive(Debug, Serialize)]
struct AnthropicTool {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    content: Vec<ContentBlock>,
    model: String,
    stop_reason: Option<String>,
    usage: AnthropicUsage,
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    input_tokens: u32,
    output_tokens: u32,
    cache_read_input_tokens: Option<u32>,
    cache_creation_input_tokens: Option<u32>,
}

// SSE Event types
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum AnthropicStreamEvent {
    #[serde(rename = "message_start")]
    MessageStart { message: MessageStartData },
    #[serde(rename = "content_block_start")]
    ContentBlockStart {
        index: usize,
        content_block: ContentBlock,
    },
    #[serde(rename = "content_block_delta")]
    ContentBlockDelta { index: usize, delta: ContentDelta },
    #[serde(rename = "content_block_stop")]
    ContentBlockStop { index: usize },
    #[serde(rename = "message_delta")]
    MessageDelta {
        delta: Option<MessageDeltaData>,
        usage: Option<MessageDeltaUsage>,
    },
    #[serde(rename = "message_stop")]
    MessageStop,
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "error")]
    Error { error: ErrorData },
}

#[derive(Debug, Deserialize)]
struct MessageStartData {
    usage: Option<AnthropicUsage>,
}

#[derive(Debug, Deserialize)]
struct MessageDeltaData {
    stop_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MessageDeltaUsage {
    output_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct ErrorData {
    message: String,
}

// ============================================================================
// Conversions
// ============================================================================

impl From<&Message> for AnthropicMessage {
    fn from(msg: &Message) -> Self {
        let role = match msg.role {
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::Tool => "user",
            MessageRole::System => "user",
        };

        // Handle tool results
        if let Some(ref tool_result) = msg.tool_result {
            return AnthropicMessage {
                role: "user".to_string(),
                content: AnthropicContent::Blocks(vec![ContentBlock::ToolResult {
                    tool_use_id: tool_result.tool_call_id.clone(),
                    content: tool_result.content.clone(),
                    is_error: if tool_result.is_error {
                        Some(true)
                    } else {
                        None
                    },
                }]),
            };
        }

        // Handle assistant messages with tool calls
        if let Some(ref tool_calls) = msg.tool_calls {
            let mut blocks: Vec<ContentBlock> = vec![];

            if !msg.content.is_empty() {
                blocks.push(ContentBlock::Text {
                    text: msg.content.clone(),
                });
            }

            for tc in tool_calls {
                blocks.push(ContentBlock::ToolUse {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                    input: tc.arguments.clone(),
                });
            }

            return AnthropicMessage {
                role: role.to_string(),
                content: AnthropicContent::Blocks(blocks),
            };
        }

        // Simple text message
        AnthropicMessage {
            role: role.to_string(),
            content: AnthropicContent::Text(msg.content.clone()),
        }
    }
}

impl From<&ToolDef> for AnthropicTool {
    fn from(tool: &ToolDef) -> Self {
        AnthropicTool {
            name: tool.name.clone(),
            description: tool.description.clone(),
            input_schema: serde_json::json!({
                "type": tool.parameters.schema_type,
                "properties": tool.parameters.properties,
                "required": tool.parameters.required
            }),
        }
    }
}
