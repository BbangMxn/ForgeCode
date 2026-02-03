//! Google Gemini provider implementation with SSE streaming support

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

const DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta";
const DEFAULT_TIMEOUT_SECS: u64 = 300;

/// Google Gemini provider
pub struct GeminiProvider {
    client: Client,
    api_key: String,
    model_info: ModelInfo,
    metadata: ProviderMetadata,
    max_tokens: u32,
    base_url: String,
}

impl GeminiProvider {
    /// Create a new Gemini provider
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
            base_url: DEFAULT_BASE_URL.to_string(),
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
            id: "gemini".to_string(),
            display_name: "Google Gemini".to_string(),
            models: vec![
                Self::get_model_info("gemini-2.0-flash"),
                Self::get_model_info("gemini-2.0-flash-lite"),
                Self::get_model_info("gemini-1.5-pro"),
                Self::get_model_info("gemini-1.5-flash"),
            ],
            default_model: "gemini-2.0-flash".to_string(),
            config_keys: vec![ConfigKey {
                name: "api_key".to_string(),
                required: true,
                secret: true,
                env_var: Some("GEMINI_API_KEY".to_string()),
                description: "Google AI API key".to_string(),
            }],
            base_url: Some(DEFAULT_BASE_URL.to_string()),
        }
    }

    fn get_model_info(model_id: &str) -> ModelInfo {
        match model_id {
            "gemini-2.0-flash" | "gemini-2.0-flash-exp" => ModelInfo {
                id: "gemini-2.0-flash".to_string(),
                provider: "gemini".to_string(),
                display_name: "Gemini 2.0 Flash".to_string(),
                context_window: 1048576,
                max_output_tokens: 8192,
                supports_tools: true,
                supports_vision: true,
                supports_thinking: true,
                input_price_per_1m: 0.075,
                output_price_per_1m: 0.30,
            },
            "gemini-2.0-flash-lite" => ModelInfo {
                id: "gemini-2.0-flash-lite".to_string(),
                provider: "gemini".to_string(),
                display_name: "Gemini 2.0 Flash Lite".to_string(),
                context_window: 1048576,
                max_output_tokens: 8192,
                supports_tools: true,
                supports_vision: true,
                supports_thinking: false,
                input_price_per_1m: 0.0375,
                output_price_per_1m: 0.15,
            },
            "gemini-1.5-pro" | "gemini-1.5-pro-latest" => ModelInfo {
                id: "gemini-1.5-pro".to_string(),
                provider: "gemini".to_string(),
                display_name: "Gemini 1.5 Pro".to_string(),
                context_window: 2097152,
                max_output_tokens: 8192,
                supports_tools: true,
                supports_vision: true,
                supports_thinking: false,
                input_price_per_1m: 1.25,
                output_price_per_1m: 5.00,
            },
            "gemini-1.5-flash" | "gemini-1.5-flash-latest" => ModelInfo {
                id: "gemini-1.5-flash".to_string(),
                provider: "gemini".to_string(),
                display_name: "Gemini 1.5 Flash".to_string(),
                context_window: 1048576,
                max_output_tokens: 8192,
                supports_tools: true,
                supports_vision: true,
                supports_thinking: false,
                input_price_per_1m: 0.075,
                output_price_per_1m: 0.30,
            },
            _ => ModelInfo::new(model_id, "gemini"),
        }
    }

    fn generate_url(&self, stream: bool) -> String {
        let action = if stream {
            "streamGenerateContent"
        } else {
            "generateContent"
        };
        format!(
            "{}/models/{}:{}?key={}",
            self.base_url, self.model_info.id, action, self.api_key
        )
    }

    fn build_request(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        system_prompt: Option<&str>,
    ) -> GeminiRequest {
        let mut contents: Vec<GeminiContent> = vec![];

        // Convert messages
        for msg in messages {
            if msg.role == MessageRole::System {
                continue; // Handled separately
            }
            contents.push(msg.into());
        }

        // Build tools
        let gemini_tools = if tools.is_empty() {
            None
        } else {
            Some(vec![GeminiTool {
                function_declarations: tools.iter().map(|t| t.into()).collect(),
            }])
        };

        // System instruction
        let system_instruction = system_prompt.map(|s| GeminiSystemInstruction {
            parts: vec![GeminiPart::Text {
                text: s.to_string(),
            }],
        });

        GeminiRequest {
            contents,
            tools: gemini_tools,
            system_instruction,
            generation_config: Some(GeminiGenerationConfig {
                max_output_tokens: Some(self.max_tokens),
                temperature: None,
                top_p: None,
                top_k: None,
            }),
        }
    }

    fn parse_error_response(status: reqwest::StatusCode, body: &str) -> ProviderError {
        // Try to parse as JSON error
        if let Ok(error_response) = serde_json::from_str::<GeminiErrorResponse>(body) {
            let error = error_response.error;
            let message = error.message;

            return match error.status.as_deref() {
                Some("RESOURCE_EXHAUSTED") => ProviderError::RateLimited {
                    retry_after_ms: None,
                },
                Some("INVALID_ARGUMENT") => {
                    if message.contains("context") || message.contains("token") {
                        ProviderError::ContextLengthExceeded(message)
                    } else {
                        ProviderError::InvalidRequest(message)
                    }
                }
                Some("PERMISSION_DENIED") | Some("UNAUTHENTICATED") => {
                    ProviderError::Authentication(message)
                }
                Some("NOT_FOUND") => ProviderError::ModelNotFound(message),
                _ => ProviderError::from_http_status(status.as_u16(), &message),
            };
        }

        ProviderError::from_http_status(status.as_u16(), body)
    }
}

#[async_trait]
impl Provider for GeminiProvider {
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
        let request = self.build_request(&messages, &tools, system_prompt.as_deref());
        let url = self.generate_url(true);

        Box::pin(async_stream::stream! {
            let response = match self
                .client
                .post(&url)
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

            let mut total_usage = TokenUsage::default();
            let mut accumulated_tool_calls: Vec<ToolCall> = vec![];

            // Gemini streams JSON array elements
            // Response format: [{...}, {...}, ...]
            let byte_stream = response.bytes_stream();
            let stream_reader = StreamReader::new(
                byte_stream.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)),
            );
            let mut reader = BufReader::new(stream_reader);
            let mut buffer = String::new();

            loop {
                buffer.clear();
                match reader.read_line(&mut buffer).await {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        let line = buffer.trim();
                        if line.is_empty() || line == "[" || line == "]" || line == "," {
                            continue;
                        }

                        // Remove trailing comma if present
                        let json_str = line.trim_end_matches(',');

                        match serde_json::from_str::<GeminiStreamChunk>(json_str) {
                            Ok(chunk) => {
                                // Process candidates
                                for candidate in chunk.candidates.unwrap_or_default() {
                                    if let Some(content) = candidate.content {
                                        for part in content.parts {
                                            match part {
                                                GeminiPart::Text { text } => {
                                                    if !text.is_empty() {
                                                        yield StreamEvent::Text(text);
                                                    }
                                                }
                                                GeminiPart::FunctionCall { function_call } => {
                                                    let tool_call = ToolCall::new(
                                                        format!("call_{}", accumulated_tool_calls.len()),
                                                        function_call.name,
                                                        function_call.args,
                                                    );
                                                    accumulated_tool_calls.push(tool_call.clone());
                                                    yield StreamEvent::ToolCall(tool_call);
                                                }
                                                _ => {}
                                            }
                                        }
                                    }
                                }

                                // Update usage
                                if let Some(usage) = chunk.usage_metadata {
                                    total_usage = TokenUsage {
                                        input_tokens: usage.prompt_token_count.unwrap_or(0),
                                        output_tokens: usage.candidates_token_count.unwrap_or(0),
                                        cache_read_tokens: usage.cached_content_token_count.unwrap_or(0),
                                        cache_creation_tokens: 0,
                                    };
                                }
                            }
                            Err(e) => {
                                tracing::warn!("Failed to parse Gemini chunk: {} - line: {}", e, json_str);
                            }
                        }
                    }
                    Err(e) => {
                        yield StreamEvent::Error(ProviderError::StreamError(format!("Stream read error: {}", e)));
                        break;
                    }
                }
            }

            yield StreamEvent::Usage(total_usage);
            yield StreamEvent::Done;
        })
    }

    async fn complete(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolDef>,
        system_prompt: Option<String>,
    ) -> Result<ProviderResponse, ProviderError> {
        let request = self.build_request(&messages, &tools, system_prompt.as_deref());
        let url = self.generate_url(false);

        let response = self
            .client
            .post(&url)
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

        let api_response: GeminiResponse = response
            .json()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        let candidate = api_response.candidates.into_iter().next().ok_or_else(|| {
            ProviderError::InvalidResponse("No candidates in response".to_string())
        })?;

        let mut content = String::new();
        let mut tool_calls = Vec::new();

        if let Some(content_block) = candidate.content {
            for part in content_block.parts {
                match part {
                    GeminiPart::Text { text } => {
                        content.push_str(&text);
                    }
                    GeminiPart::FunctionCall { function_call } => {
                        tool_calls.push(ToolCall::new(
                            format!("call_{}", tool_calls.len()),
                            function_call.name,
                            function_call.args,
                        ));
                    }
                    _ => {}
                }
            }
        }

        let finish_reason = match candidate.finish_reason.as_deref() {
            Some("STOP") => FinishReason::Stop,
            Some("MAX_TOKENS") => FinishReason::MaxTokens,
            Some("SAFETY") => FinishReason::ContentFilter,
            Some("TOOL_CALL") | Some("FUNCTION_CALL") => FinishReason::ToolUse,
            _ => {
                if !tool_calls.is_empty() {
                    FinishReason::ToolUse
                } else {
                    FinishReason::Stop
                }
            }
        };

        let usage = api_response.usage_metadata.unwrap_or_default();

        Ok(ProviderResponse {
            content,
            tool_calls,
            usage: TokenUsage {
                input_tokens: usage.prompt_token_count.unwrap_or(0),
                output_tokens: usage.candidates_token_count.unwrap_or(0),
                cache_read_tokens: usage.cached_content_token_count.unwrap_or(0),
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

// ============================================================================
// Gemini API Types
// ============================================================================

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<GeminiTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiSystemInstruction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GeminiGenerationConfig>,
}

#[derive(Debug, Serialize)]
struct GeminiSystemInstruction {
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiGenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_k: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiContent {
    role: String,
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum GeminiPart {
    Text {
        text: String,
    },
    FunctionCall {
        #[serde(rename = "functionCall")]
        function_call: GeminiFunctionCall,
    },
    FunctionResponse {
        #[serde(rename = "functionResponse")]
        function_response: GeminiFunctionResponse,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiFunctionCall {
    name: String,
    args: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiFunctionResponse {
    name: String,
    response: serde_json::Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiTool {
    function_declarations: Vec<GeminiFunctionDeclaration>,
}

#[derive(Debug, Serialize)]
struct GeminiFunctionDeclaration {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

// Response types
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiResponse {
    candidates: Vec<GeminiCandidate>,
    #[serde(default)]
    usage_metadata: Option<GeminiUsageMetadata>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiStreamChunk {
    #[serde(default)]
    candidates: Option<Vec<GeminiCandidate>>,
    #[serde(default)]
    usage_metadata: Option<GeminiUsageMetadata>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiCandidate {
    content: Option<GeminiContent>,
    finish_reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiUsageMetadata {
    prompt_token_count: Option<u32>,
    candidates_token_count: Option<u32>,
    cached_content_token_count: Option<u32>,
}

// Error types
#[derive(Debug, Deserialize)]
struct GeminiErrorResponse {
    error: GeminiError,
}

#[derive(Debug, Deserialize)]
struct GeminiError {
    message: String,
    status: Option<String>,
}

// ============================================================================
// Conversions
// ============================================================================

impl From<&Message> for GeminiContent {
    fn from(msg: &Message) -> Self {
        // Handle tool results
        if let Some(ref tool_result) = msg.tool_result {
            return GeminiContent {
                role: "function".to_string(),
                parts: vec![GeminiPart::FunctionResponse {
                    function_response: GeminiFunctionResponse {
                        name: tool_result.tool_call_id.clone(),
                        response: serde_json::json!({
                            "result": tool_result.content,
                            "is_error": tool_result.is_error
                        }),
                    },
                }],
            };
        }

        let role = match msg.role {
            MessageRole::User => "user",
            MessageRole::Assistant => "model",
            MessageRole::System => "user",
            MessageRole::Tool => "function",
        };

        let mut parts: Vec<GeminiPart> = vec![];

        // Add text content
        if !msg.content.is_empty() {
            parts.push(GeminiPart::Text {
                text: msg.content.clone(),
            });
        }

        // Add tool calls as function calls
        if let Some(ref tool_calls) = msg.tool_calls {
            for tc in tool_calls {
                parts.push(GeminiPart::FunctionCall {
                    function_call: GeminiFunctionCall {
                        name: tc.name.clone(),
                        args: tc.arguments.clone(),
                    },
                });
            }
        }

        if parts.is_empty() {
            parts.push(GeminiPart::Text {
                text: String::new(),
            });
        }

        GeminiContent {
            role: role.to_string(),
            parts,
        }
    }
}

impl From<&ToolDef> for GeminiFunctionDeclaration {
    fn from(tool: &ToolDef) -> Self {
        GeminiFunctionDeclaration {
            name: tool.name.clone(),
            description: tool.description.clone(),
            parameters: serde_json::json!({
                "type": tool.parameters.schema_type,
                "properties": tool.parameters.properties,
                "required": tool.parameters.required
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_info() {
        let info = GeminiProvider::get_model_info("gemini-2.0-flash");
        assert_eq!(info.id, "gemini-2.0-flash");
        assert_eq!(info.context_window, 1048576);
        assert!(info.supports_vision);
    }

    #[test]
    fn test_generate_url() {
        let provider = GeminiProvider::new("test-key", "gemini-2.0-flash", 8192);
        let url = provider.generate_url(false);
        assert!(url.contains("generateContent"));
        assert!(url.contains("gemini-2.0-flash"));
    }
}
