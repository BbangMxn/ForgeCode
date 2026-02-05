//! Ollama (local) provider implementation with SSE streaming support

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

const DEFAULT_TIMEOUT_SECS: u64 = 600; // Longer timeout for local models

/// Ollama provider for local models
///
/// Supports automatic model info detection via /api/show endpoint.
/// Use `with_auto_config()` async constructor to auto-detect model capabilities,
/// or `with_capabilities()` to manually configure.
pub struct OllamaProvider {
    client: Client,
    base_url: String,
    model_info: ModelInfo,
    metadata: ProviderMetadata,
}

impl OllamaProvider {
    /// Create a new Ollama provider with default settings
    ///
    /// For auto-detection of model capabilities, use `with_auto_config()` instead.
    pub fn new(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        let model_id = model.into();
        let base_url = base_url.into();

        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
                .build()
                .expect("Failed to create HTTP client"),
            base_url: base_url.clone(),
            model_info: ModelInfo::new(&model_id, "ollama"),
            metadata: Self::create_metadata(&base_url),
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

    /// Set model capabilities manually
    pub fn with_capabilities(
        mut self,
        context_window: u32,
        max_output_tokens: u32,
        supports_vision: bool,
    ) -> Self {
        self.model_info.context_window = context_window;
        self.model_info.max_output_tokens = max_output_tokens;
        self.model_info.supports_vision = supports_vision;
        self
    }

    /// Internal method to fetch model info from /api/show
    async fn fetch_model_info_internal(&self, model: &str) -> Result<OllamaModelDetails, ProviderError> {
        let url = format!("{}/api/show", self.base_url);

        #[derive(Serialize)]
        struct ShowRequest<'a> {
            name: &'a str,
        }

        let response = self
            .client
            .post(&url)
            .json(&ShowRequest { name: model })
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        if !response.status().is_success() {
            return Err(ProviderError::ServerError(format!(
                "Failed to fetch model info: {}",
                response.status()
            )));
        }

        response
            .json()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))
    }

    fn create_metadata(base_url: &str) -> ProviderMetadata {
        ProviderMetadata {
            id: "ollama".to_string(),
            display_name: "Ollama".to_string(),
            models: vec![], // Will be populated dynamically
            default_model: "llama3.2".to_string(),
            config_keys: vec![ConfigKey {
                name: "base_url".to_string(),
                required: true,
                secret: false,
                env_var: Some("OLLAMA_HOST".to_string()),
                description: "Ollama API base URL".to_string(),
            }],
            base_url: Some(base_url.to_string()),
        }
    }

    fn chat_url(&self) -> String {
        format!("{}/api/chat", self.base_url)
    }

    fn build_request(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        system_prompt: Option<&str>,
        stream: bool,
    ) -> OllamaRequest {
        let mut api_messages: Vec<OllamaMessage> = vec![];

        // Add system prompt
        if let Some(system) = system_prompt {
            api_messages.push(OllamaMessage {
                role: "system".to_string(),
                content: system.to_string(),
                tool_calls: None,
            });
        }

        // Convert messages
        for msg in messages {
            if msg.role == MessageRole::System {
                continue;
            }
            api_messages.push(msg.into());
        }

        let api_tools: Vec<OllamaTool> = tools.iter().map(|t| t.into()).collect();

        OllamaRequest {
            model: self.model_info.id.clone(),
            messages: api_messages,
            tools: if api_tools.is_empty() {
                None
            } else {
                Some(api_tools)
            },
            stream,
        }
    }

    /// Check if Ollama server is reachable
    pub async fn ping(&self) -> bool {
        let url = format!("{}/api/tags", self.base_url);
        self.client
            .get(&url)
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    /// List available models
    pub async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        let url = format!("{}/api/tags", self.base_url);
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        if !response.status().is_success() {
            return Err(ProviderError::ServerError(
                "Failed to list models".to_string(),
            ));
        }

        #[derive(Deserialize)]
        struct TagsResponse {
            models: Vec<ModelEntry>,
        }

        #[derive(Deserialize)]
        struct ModelEntry {
            name: String,
        }

        let tags: TagsResponse = response
            .json()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        Ok(tags.models.into_iter().map(|m| m.name).collect())
    }

    /// Fetch model information from Ollama API
    /// This calls /api/show to get actual model parameters
    pub async fn fetch_model_info(&self) -> Result<OllamaModelDetails, ProviderError> {
        self.fetch_model_info_internal(&self.model_info.id).await
    }

    /// Apply model details to ModelInfo
    fn apply_model_details(info: &mut ModelInfo, details: &OllamaModelDetails) {
        // Extract context window from model parameters
        if let Some(ref params) = details.model_info {
            // Try to get context length from various parameter names
            // Ollama uses different keys depending on model
            if let Some(ctx) = params
                .get("context_length")
                .or_else(|| params.get("num_ctx"))
                .or_else(|| params.get("context_window"))
                .or_else(|| params.get("llama.context_length"))
            {
                if let Some(ctx_value) = ctx.as_u64() {
                    info.context_window = ctx_value as u32;
                }
            }
        }

        // Check for vision support from model details
        if let Some(ref template) = details.template {
            // Models with vision support often have specific templates
            if template.contains("vision") || template.contains("image") {
                info.supports_vision = true;
            }
        }

        // Check model family from details
        if let Some(ref family) = details.details.as_ref().and_then(|d| d.family.as_ref()) {
            let family_lower = family.to_lowercase();
            // llava, moondream, bakllava 등은 vision 지원
            if family_lower.contains("llava")
                || family_lower.contains("moondream")
                || family_lower.contains("bakllava")
            {
                info.supports_vision = true;
            }
        }

        // Check projector in details (vision models have this)
        if let Some(ref projector) = details.projector_info {
            if !projector.is_empty() {
                info.supports_vision = true;
            }
        }
    }

    /// Create provider with auto-detected model info from Ollama API
    ///
    /// This async constructor fetches model details via /api/show endpoint
    /// and automatically configures context_window and vision support.
    ///
    /// # Example
    /// ```ignore
    /// let provider = OllamaProvider::with_auto_config(
    ///     "http://localhost:11434",
    ///     "llama3.2"
    /// ).await?;
    ///
    /// // Model info is now auto-configured
    /// println!("Context window: {}", provider.model().context_window);
    /// ```
    pub async fn with_auto_config(
        base_url: impl Into<String>,
        model: impl Into<String>,
    ) -> Result<Self, ProviderError> {
        let mut provider = Self::new(base_url, model);

        // Try to fetch model info
        match provider.fetch_model_info().await {
            Ok(details) => {
                Self::apply_model_details(&mut provider.model_info, &details);
                tracing::info!(
                    model = %provider.model_info.id,
                    context_window = provider.model_info.context_window,
                    supports_vision = provider.model_info.supports_vision,
                    "Auto-configured Ollama model from /api/show"
                );
            }
            Err(e) => {
                tracing::warn!(
                    model = %provider.model_info.id,
                    error = %e,
                    "Could not auto-configure model, using defaults"
                );
            }
        }

        Ok(provider)
    }
}

// ============================================================================
// Ollama Model Details (from /api/show)
// ============================================================================

/// Model details returned by /api/show endpoint
#[derive(Debug, Deserialize)]
pub struct OllamaModelDetails {
    /// Model file path
    #[serde(default)]
    pub modelfile: String,

    /// Parameters string
    #[serde(default)]
    pub parameters: String,

    /// Template for prompt formatting
    #[serde(default)]
    pub template: Option<String>,

    /// Model details (family, parameter size, etc.)
    #[serde(default)]
    pub details: Option<OllamaModelInfo>,

    /// Model info with numeric parameters
    #[serde(default)]
    pub model_info: Option<serde_json::Map<String, serde_json::Value>>,

    /// Projector info (for vision models)
    #[serde(default)]
    pub projector_info: Option<serde_json::Map<String, serde_json::Value>>,
}

/// Detailed model information
#[derive(Debug, Deserialize)]
pub struct OllamaModelInfo {
    /// Model format (e.g., "gguf")
    pub format: Option<String>,

    /// Model family (e.g., "llama", "qwen")
    pub family: Option<String>,

    /// Parameter size (e.g., "8B", "70B")
    pub parameter_size: Option<String>,

    /// Quantization level (e.g., "Q4_K_M")
    pub quantization_level: Option<String>,
}

#[async_trait]
impl Provider for OllamaProvider {
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
                .post(&self.chat_url())
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
                let body = response.text().await.unwrap_or_default();
                yield StreamEvent::Error(ProviderError::ServerError(format!("Ollama API error: {}", body)));
                return;
            }

            let mut total_input_tokens = 0u32;
            let mut total_output_tokens = 0u32;
            let mut accumulated_tool_calls: Vec<OllamaToolCall> = vec![];

            // Ollama streams JSON objects separated by newlines (NDJSON)
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
                        if line.is_empty() {
                            continue;
                        }

                        // Parse NDJSON chunk
                        match serde_json::from_str::<OllamaStreamChunk>(line) {
                            Ok(chunk) => {
                                // Handle text content
                                if !chunk.message.content.is_empty() {
                                    yield StreamEvent::Text(chunk.message.content);
                                }

                                // Accumulate tool calls
                                if let Some(tool_calls) = chunk.message.tool_calls {
                                    accumulated_tool_calls.extend(tool_calls);
                                }

                                // Update token counts
                                if let Some(count) = chunk.prompt_eval_count {
                                    total_input_tokens = count;
                                }
                                if let Some(count) = chunk.eval_count {
                                    total_output_tokens = count;
                                }

                                // Check if done
                                if chunk.done {
                                    // Emit accumulated tool calls
                                    for (i, tc) in accumulated_tool_calls.into_iter().enumerate() {
                                        yield StreamEvent::ToolCall(ToolCall::new(
                                            format!("call_{}", i),
                                            tc.function.name,
                                            tc.function.arguments,
                                        ));
                                    }

                                    yield StreamEvent::Usage(TokenUsage {
                                        input_tokens: total_input_tokens,
                                        output_tokens: total_output_tokens,
                                        cache_read_tokens: 0,
                                        cache_creation_tokens: 0,
                                    });
                                    yield StreamEvent::Done;
                                    break;
                                }
                            }
                            Err(e) => {
                                tracing::warn!("Failed to parse Ollama chunk: {} - line: {}", e, line);
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
            .post(&self.chat_url())
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(match status.as_u16() {
                404 => ProviderError::ModelNotFound(format!(
                    "Model '{}' not found. Run 'ollama pull {}' first.",
                    self.model_info.id, self.model_info.id
                )),
                _ => ProviderError::ServerError(format!("Ollama error: {}", body)),
            });
        }

        let api_response: OllamaResponse = response
            .json()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        let content = api_response.message.content.clone();
        let has_tool_calls = api_response.message.tool_calls.is_some();
        let tool_calls = api_response
            .message
            .tool_calls
            .unwrap_or_default()
            .into_iter()
            .enumerate()
            .map(|(i, tc)| {
                ToolCall::new(
                    format!("call_{}", i),
                    tc.function.name,
                    tc.function.arguments,
                )
            })
            .collect();

        let finish_reason = if api_response.done {
            if has_tool_calls {
                FinishReason::ToolUse
            } else {
                FinishReason::Stop
            }
        } else {
            FinishReason::Other
        };

        Ok(ProviderResponse {
            content,
            tool_calls,
            usage: TokenUsage {
                input_tokens: api_response.prompt_eval_count.unwrap_or(0),
                output_tokens: api_response.eval_count.unwrap_or(0),
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
            },
            finish_reason,
            model: self.model_info.id.clone(),
        })
    }

    fn is_available(&self) -> bool {
        // For sync check, we assume it's available
        // Use ping() for async verification
        true
    }

    fn set_model(&mut self, model_id: &str) -> Result<(), ProviderError> {
        self.model_info = ModelInfo::new(model_id, "ollama");
        Ok(())
    }
}

// ============================================================================
// Ollama API Types
// ============================================================================

#[derive(Debug, Serialize)]
struct OllamaRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OllamaTool>>,
    stream: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct OllamaMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OllamaToolCall>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct OllamaToolCall {
    function: OllamaFunctionCall,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct OllamaFunctionCall {
    name: String,
    arguments: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct OllamaTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: OllamaFunction,
}

#[derive(Debug, Serialize)]
struct OllamaFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct OllamaResponse {
    message: OllamaMessage,
    done: bool,
    prompt_eval_count: Option<u32>,
    eval_count: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct OllamaStreamChunk {
    message: OllamaMessage,
    done: bool,
    #[serde(default)]
    prompt_eval_count: Option<u32>,
    #[serde(default)]
    eval_count: Option<u32>,
}

// ============================================================================
// Conversions
// ============================================================================

impl From<&Message> for OllamaMessage {
    fn from(msg: &Message) -> Self {
        // Handle tool results
        if let Some(ref tool_result) = msg.tool_result {
            return OllamaMessage {
                role: "tool".to_string(),
                content: tool_result.content.clone(),
                tool_calls: None,
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
                .map(|tc| OllamaToolCall {
                    function: OllamaFunctionCall {
                        name: tc.name.clone(),
                        arguments: tc.arguments.clone(),
                    },
                })
                .collect()
        });

        OllamaMessage {
            role: role.to_string(),
            content: msg.content.clone(),
            tool_calls,
        }
    }
}

impl From<&ToolDef> for OllamaTool {
    fn from(tool: &ToolDef) -> Self {
        OllamaTool {
            tool_type: "function".to_string(),
            function: OllamaFunction {
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
    fn test_chat_url() {
        let provider = OllamaProvider::new("http://localhost:11434", "llama2");
        assert_eq!(provider.chat_url(), "http://localhost:11434/api/chat");
    }

    #[test]
    fn test_model_info() {
        let provider = OllamaProvider::new("http://localhost:11434", "codellama:7b")
            .with_capabilities(16384, 8192, false);

        assert_eq!(provider.model().id, "codellama:7b");
        assert_eq!(provider.model().context_window, 16384);
    }
}
