//! Core agent implementation

use crate::context::AgentContext;
use crate::history::MessageHistory;
use forge_foundation::{Error, Result};
use forge_provider::{StreamEvent, ToolCall};
use futures::StreamExt;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// Events emitted by the agent during execution
#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// Agent is thinking/processing
    Thinking,

    /// Text response chunk from LLM
    Text(String),

    /// Tool execution started
    ToolStart {
        tool_name: String,
        tool_call_id: String,
    },

    /// Tool execution completed
    ToolComplete {
        tool_name: String,
        tool_call_id: String,
        result: String,
        success: bool,
    },

    /// Response completed
    Done { full_response: String },

    /// Error occurred
    Error(String),

    /// Token usage update
    Usage {
        input_tokens: u32,
        output_tokens: u32,
    },
}

/// The core agent that handles conversation with LLM
pub struct Agent {
    /// Shared context
    ctx: Arc<AgentContext>,

    /// Maximum iterations for tool use loop
    max_iterations: usize,
}

impl Agent {
    /// Create a new agent
    pub fn new(ctx: Arc<AgentContext>) -> Self {
        Self {
            ctx,
            max_iterations: 20,
        }
    }

    /// Set maximum iterations for tool use loop
    pub fn with_max_iterations(mut self, max: usize) -> Self {
        self.max_iterations = max;
        self
    }

    /// Run the agent with a user message
    pub async fn run(
        &self,
        session_id: &str,
        history: &mut MessageHistory,
        user_message: &str,
        event_tx: mpsc::Sender<AgentEvent>,
    ) -> Result<String> {
        // Add user message to history
        history.add_user(user_message);

        // Set system prompt if not set
        if history.system_prompt().is_none() {
            history.set_system_prompt(&self.ctx.system_prompt);
        }

        let mut full_response = String::new();
        let mut iterations = 0;

        loop {
            if iterations >= self.max_iterations {
                warn!("Max iterations reached");
                break;
            }
            iterations += 1;

            let _ = event_tx.send(AgentEvent::Thinking).await;

            // Get tool definitions
            let tools = self.ctx.tool_definitions();

            // Get provider and create stream
            let provider = self.ctx.gateway.get_default_provider_for_stream().await?;
            let stream = provider.stream(
                history.to_messages(),
                tools,
                history.system_prompt().map(|s| s.to_string()),
            );

            // Process stream
            let (response_text, tool_calls, usage) = self.process_stream(stream, &event_tx).await?;

            // Accumulate response text
            if !response_text.is_empty() {
                full_response.push_str(&response_text);
            }

            // Send usage update
            if let Some((input, output)) = usage {
                let _ = event_tx
                    .send(AgentEvent::Usage {
                        input_tokens: input,
                        output_tokens: output,
                    })
                    .await;
            }

            // If no tool calls, we're done
            if tool_calls.is_empty() {
                // Add assistant response to history
                if !full_response.is_empty() {
                    history.add_assistant(&full_response);
                }
                break;
            }

            // Add assistant message with tool calls
            history.add_assistant_with_tools(&response_text, tool_calls.clone());

            // Execute tool calls
            for tool_call in tool_calls {
                let result = self.execute_tool(session_id, &tool_call, &event_tx).await;

                // Add tool result to history
                let (content, is_error) = match result {
                    Ok(content) => (content, false),
                    Err(e) => (e.to_string(), true),
                };

                history.add_tool_result(&tool_call.id, &content, is_error);
            }

            // Continue loop to get next LLM response
        }

        // Send done event
        let _ = event_tx
            .send(AgentEvent::Done {
                full_response: full_response.clone(),
            })
            .await;

        Ok(full_response)
    }

    /// Process LLM stream and extract response
    async fn process_stream(
        &self,
        stream: std::pin::Pin<Box<dyn futures::Stream<Item = StreamEvent> + Send + '_>>,
        event_tx: &mpsc::Sender<AgentEvent>,
    ) -> Result<(String, Vec<ToolCall>, Option<(u32, u32)>)> {
        let mut response_text = String::new();
        let mut tool_calls = Vec::new();
        let mut usage = None;

        tokio::pin!(stream);

        while let Some(event) = stream.next().await {
            match event {
                StreamEvent::Text(text) => {
                    response_text.push_str(&text);
                    let _ = event_tx.send(AgentEvent::Text(text)).await;
                }
                StreamEvent::ToolCall(tc) => {
                    tool_calls.push(tc);
                }
                StreamEvent::Usage(u) => {
                    usage = Some((u.input_tokens, u.output_tokens));
                }
                StreamEvent::Error(e) => {
                    return Err(Error::Provider(e.to_string()));
                }
                StreamEvent::Done => {
                    break;
                }
                StreamEvent::Thinking(t) => {
                    debug!("Thinking: {}", t);
                }
                StreamEvent::ToolCallStart { .. } => {
                    // Partial tool call info, wait for complete ToolCall
                }
                StreamEvent::ToolCallDelta { .. } => {
                    // Partial tool call arguments, wait for complete ToolCall
                }
            }
        }

        Ok((response_text, tool_calls, usage))
    }

    /// Execute a single tool call
    async fn execute_tool(
        &self,
        session_id: &str,
        tool_call: &ToolCall,
        event_tx: &mpsc::Sender<AgentEvent>,
    ) -> Result<String> {
        info!("Executing tool: {}", tool_call.name);

        let _ = event_tx
            .send(AgentEvent::ToolStart {
                tool_name: tool_call.name.clone(),
                tool_call_id: tool_call.id.clone(),
            })
            .await;

        // Create tool context
        let tool_ctx = self.ctx.tool_context(session_id);

        // Execute tool
        let result = self
            .ctx
            .tools
            .execute(&tool_call.name, &tool_ctx, tool_call.arguments.clone())
            .await;

        let (content, success) = if result.success {
            (result.content.clone(), true)
        } else {
            (
                result.error.unwrap_or_else(|| "Unknown error".to_string()),
                false,
            )
        };

        let _ = event_tx
            .send(AgentEvent::ToolComplete {
                tool_name: tool_call.name.clone(),
                tool_call_id: tool_call.id.clone(),
                result: content.clone(),
                success,
            })
            .await;

        if success {
            Ok(content)
        } else {
            Err(Error::Tool(content))
        }
    }
}
