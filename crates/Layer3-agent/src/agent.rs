//! Core Agent Implementation
//!
//! Claude Code / OpenCode / Gemini CLI 스타일의 단순하고 효율적인 Agent Loop입니다.
//!
//! ## 핵심 원칙
//! - Single-threaded flat loop (복잡한 4단계 loop 대신)
//! - Hook system으로 확장성 확보
//! - 자동 컨텍스트 압축 (92% threshold)
//! - 실시간 steering (중단/재개/방향전환)
//!
//! ## 실행 흐름
//! ```text
//! User Input → hooks.before_agent()
//!     → Main Loop:
//!         1. Check context (>92%? → compress)
//!         2. Check steering (paused? stopped?)
//!         3. provider.stream(history, tools)
//!         4. Process stream events
//!         5. No tool calls? → break
//!         6. For each tool:
//!            - hooks.before_tool()
//!            - execute()
//!            - hooks.after_tool()
//!         7. Continue loop
//!     → hooks.after_agent()
//! → Return Response
//! ```

use crate::compressor::{CompressorConfig, ContextCompressor};
use crate::context::AgentContext;
use crate::history::MessageHistory;
use crate::hook::{AgentHook, HookManager, HookResult, ToolResult, TurnInfo};
use crate::parallel::ExecutionPlanner;
use crate::recovery::{ErrorRecovery, RecoveryAction, RecoveryContext};
use crate::steering::{AgentState, Steerable, SteeringChecker, SteeringHandle, SteeringQueue};
use forge_foundation::{Error, Result};
use forge_provider::{StreamEvent, ToolCall};
use futures::StreamExt;
use serde_json::Value;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

// ============================================================================
// Agent Events
// ============================================================================

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
        duration_ms: u64,
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

    /// Context compressed
    Compressed {
        tokens_before: usize,
        tokens_after: usize,
        tokens_saved: usize,
    },

    /// Turn started
    TurnStart { turn: u32 },

    /// Turn completed
    TurnComplete { turn: u32 },

    /// Agent paused
    Paused,

    /// Agent resumed
    Resumed,

    /// Agent stopped
    Stopped { reason: String },
}

// ============================================================================
// Agent Configuration
// ============================================================================

/// Agent 설정
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// 최대 반복 횟수
    pub max_iterations: usize,

    /// 컨텍스트 압축 설정
    pub compressor_config: CompressorConfig,

    /// 자동 압축 활성화
    pub auto_compress: bool,

    /// 스트리밍 활성화
    pub streaming: bool,

    /// 병렬 도구 실행 활성화
    /// read, glob, grep 등 독립적인 도구들을 동시에 실행
    pub parallel_tools: bool,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_iterations: 50,
            compressor_config: CompressorConfig::claude_code_style(),
            auto_compress: true,
            streaming: true,
            parallel_tools: true, // 기본 활성화
        }
    }
}

impl AgentConfig {
    /// 빠른 실행용 설정
    pub fn fast() -> Self {
        Self {
            max_iterations: 20,
            compressor_config: CompressorConfig::aggressive(),
            auto_compress: true,
            streaming: true,
            parallel_tools: true,
        }
    }

    /// 긴 세션용 설정
    pub fn long_session() -> Self {
        Self {
            max_iterations: 100,
            compressor_config: CompressorConfig::conservative(),
            auto_compress: true,
            streaming: true,
            parallel_tools: true,
        }
    }

    /// 순차 실행 (디버깅용)
    pub fn sequential() -> Self {
        Self {
            parallel_tools: false,
            ..Self::default()
        }
    }
}

// ============================================================================
// Agent
// ============================================================================

/// The core agent that handles conversation with LLM
///
/// Claude Code 스타일의 단순한 single-threaded loop를 사용합니다.
pub struct Agent {
    /// Shared context
    ctx: Arc<AgentContext>,

    /// Agent configuration
    config: AgentConfig,

    /// Error recovery system
    error_recovery: ErrorRecovery,

    /// Hook manager
    hooks: HookManager,

    /// Context compressor
    compressor: ContextCompressor,

    /// Steering queue
    steering_queue: SteeringQueue,

    /// Steering checker (stored for Steerable trait)
    steering_checker: SteeringChecker,
}

impl Agent {
    /// Create a new agent
    pub fn new(ctx: Arc<AgentContext>) -> Self {
        let config = AgentConfig::default();
        let steering_queue = SteeringQueue::new();
        let steering_checker = steering_queue.checker();
        Self {
            ctx,
            compressor: ContextCompressor::new(config.compressor_config.clone()),
            config,
            error_recovery: ErrorRecovery::new(),
            hooks: HookManager::new(),
            steering_queue,
            steering_checker,
        }
    }

    /// Create with custom configuration
    pub fn with_config(ctx: Arc<AgentContext>, config: AgentConfig) -> Self {
        let steering_queue = SteeringQueue::new();
        let steering_checker = steering_queue.checker();
        Self {
            ctx,
            compressor: ContextCompressor::new(config.compressor_config.clone()),
            config,
            error_recovery: ErrorRecovery::new(),
            hooks: HookManager::new(),
            steering_queue,
            steering_checker,
        }
    }

    /// Set maximum iterations for tool use loop
    pub fn with_max_iterations(mut self, max: usize) -> Self {
        self.config.max_iterations = max;
        self
    }

    /// Set custom error recovery
    pub fn with_error_recovery(mut self, recovery: ErrorRecovery) -> Self {
        self.error_recovery = recovery;
        self
    }

    /// Add a hook
    pub fn with_hook<H: AgentHook + 'static>(mut self, hook: H) -> Self {
        self.hooks.add(hook);
        self
    }

    /// Add a hook (Arc version)
    pub fn with_hook_arc(mut self, hook: Arc<dyn AgentHook>) -> Self {
        self.hooks.add_arc(hook);
        self
    }

    /// Get steering handle for external control
    pub fn steering_handle(&self) -> SteeringHandle {
        self.steering_queue.handle()
    }

    /// Get steering checker for internal use
    fn steering_checker(&self) -> &SteeringChecker {
        &self.steering_checker
    }

    /// Run the agent with a user message
    pub async fn run(
        &self,
        session_id: &str,
        history: &mut MessageHistory,
        user_message: &str,
        event_tx: mpsc::Sender<AgentEvent>,
    ) -> Result<String> {
        let steering = self.steering_checker();
        steering.set_state(AgentState::Running).await;

        // Add user message to history
        history.add_user(user_message);

        // Set system prompt if not set
        if history.system_prompt().is_none() {
            history.set_system_prompt(&self.ctx.system_prompt);
        }

        // Run before_agent hooks
        match self.hooks.run_before_agent(history).await? {
            HookResult::Stop { reason } => {
                let _ = event_tx
                    .send(AgentEvent::Stopped {
                        reason: reason.clone(),
                    })
                    .await;
                return Err(Error::Agent(format!("Stopped by hook: {}", reason)));
            }
            HookResult::Block { response } => {
                let _ = event_tx
                    .send(AgentEvent::Done {
                        full_response: response.clone(),
                    })
                    .await;
                return Ok(response);
            }
            _ => {}
        }

        let mut full_response = String::with_capacity(4096); // Pre-allocate typical response size
        let mut turn = 0u32;
        let mut total_input_tokens = 0u32;
        let mut total_output_tokens = 0u32;
        let mut tools_used = Vec::with_capacity(8); // Typical tool count

        loop {
            // Check max iterations
            if turn as usize >= self.config.max_iterations {
                warn!("Max iterations reached: {}", self.config.max_iterations);
                break;
            }
            turn += 1;
            steering.set_turn(turn);

            // Process steering commands
            steering.process_commands().await;

            // Check if stopped
            if steering.should_stop() {
                let reason = steering
                    .stop_reason()
                    .await
                    .unwrap_or_else(|| "Unknown".to_string());
                let _ = event_tx
                    .send(AgentEvent::Stopped {
                        reason: reason.clone(),
                    })
                    .await;
                return Err(Error::Agent(format!("Agent stopped: {}", reason)));
            }

            // Wait if paused
            if steering.is_paused() {
                let _ = event_tx.send(AgentEvent::Paused).await;
                steering.wait_if_paused().await;
                let _ = event_tx.send(AgentEvent::Resumed).await;
            }

            // Check and apply context compression if needed
            if self.config.auto_compress && self.compressor.needs_compression(history) {
                self.hooks.run_before_compress(history).await?;

                let result = self.compressor.compress(history)?;
                if result.compressed {
                    let _ = event_tx
                        .send(AgentEvent::Compressed {
                            tokens_before: result.tokens_before,
                            tokens_after: result.tokens_after,
                            tokens_saved: result.tokens_saved,
                        })
                        .await;
                    info!(
                        "Context compressed: {} -> {} tokens (saved {})",
                        result.tokens_before, result.tokens_after, result.tokens_saved
                    );

                    self.hooks
                        .run_after_compress(history, result.tokens_saved)
                        .await?;
                }
            }

            // Process injected instructions (from steering)
            let injected = steering.take_injected_instructions().await;
            for instruction in injected {
                history.add_user(format!("[System instruction]: {}", instruction));
            }

            // Process injected context
            let injected_ctx = steering.take_injected_context().await;
            for ctx in injected_ctx {
                history.add_user(format!("[Additional context]: {}", ctx));
            }

            // Run before_turn hook
            match self.hooks.run_before_turn(history, turn).await? {
                HookResult::Stop { reason } => {
                    let _ = event_tx.send(AgentEvent::Stopped { reason }).await;
                    break;
                }
                _ => {}
            }

            let _ = event_tx.send(AgentEvent::TurnStart { turn }).await;
            let _ = event_tx.send(AgentEvent::Thinking).await;
            steering.set_state(AgentState::WaitingForLlm).await;

            // Get tool definitions
            let tools = self.ctx.tool_definitions().await;

            // Get provider and create stream
            // Note: to_messages() currently clones, but provider API requires ownership
            // TODO: Consider modifying Provider trait to accept &[Message] for zero-copy
            let provider = self.ctx.gateway.get_default_provider_for_stream().await?;
            let system_prompt = history.system_prompt().map(String::from);
            let stream = provider.stream(history.to_messages(), tools, system_prompt);

            // Process stream
            let (response_text, tool_calls, usage) = self.process_stream(stream, &event_tx).await?;

            // Accumulate response text
            if !response_text.is_empty() {
                full_response.push_str(&response_text);
            }

            // Update token usage
            if let Some((input, output)) = usage {
                total_input_tokens += input;
                total_output_tokens += output;
                let _ = event_tx
                    .send(AgentEvent::Usage {
                        input_tokens: input,
                        output_tokens: output,
                    })
                    .await;
            }

            // If no tool calls, we're done
            if tool_calls.is_empty() {
                if !full_response.is_empty() {
                    history.add_assistant(&full_response);
                }

                // Run after_turn hook
                self.hooks
                    .run_after_turn(history, turn, &full_response)
                    .await?;
                let _ = event_tx.send(AgentEvent::TurnComplete { turn }).await;
                break;
            }

            // Add assistant message with tool calls
            history.add_assistant_with_tools(&response_text, tool_calls.clone());
            steering.set_state(AgentState::ExecutingTool).await;

            // Execute tool calls (parallel or sequential based on config)
            let tool_results = self
                .execute_tools(session_id, &tool_calls, history, &event_tx, &mut tools_used)
                .await?;

            // Add tool results to history
            for (tool_call_id, content, is_error) in tool_results {
                history.add_tool_result(&tool_call_id, &content, is_error);
            }

            // Run after_turn hook
            self.hooks
                .run_after_turn(history, turn, &response_text)
                .await?;
            let _ = event_tx.send(AgentEvent::TurnComplete { turn }).await;

            // Continue loop to get next LLM response
        }

        // Create turn info for after_agent hook
        let turn_info = TurnInfo {
            turn,
            input_tokens: total_input_tokens,
            output_tokens: total_output_tokens,
            tools_used,
        };

        // Run after_agent hooks
        self.hooks
            .run_after_agent(history, &full_response, &turn_info)
            .await?;

        steering.set_state(AgentState::Completed).await;

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
        let mut response_text = String::with_capacity(2048); // Pre-allocate for typical response
        let mut tool_calls = Vec::with_capacity(4); // Typical tool call count
        let mut usage = None;

        tokio::pin!(stream);

        while let Some(event) = stream.next().await {
            match event {
                StreamEvent::Text(text) => {
                    response_text.push_str(&text);
                    if self.config.streaming {
                        let _ = event_tx.send(AgentEvent::Text(text)).await;
                    }
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

    /// Execute multiple tool calls with optional parallelization
    ///
    /// Returns Vec<(tool_call_id, content, is_error)>
    async fn execute_tools(
        &self,
        session_id: &str,
        tool_calls: &[ToolCall],
        history: &mut MessageHistory,
        event_tx: &mpsc::Sender<AgentEvent>,
        tools_used: &mut Vec<String>,
    ) -> Result<Vec<(String, String, bool)>> {
        let steering = self.steering_checker();
        let mut results = Vec::with_capacity(tool_calls.len());

        if self.config.parallel_tools && tool_calls.len() > 1 {
            // Use ExecutionPlanner to create optimal execution plan
            let planner = ExecutionPlanner::new();
            let plan = planner.plan(tool_calls.to_vec());

            info!(
                "Parallel execution: {} tools in {} phases ({} parallelizable)",
                plan.tool_calls.len(),
                plan.phase_count(),
                plan.parallelizable_count()
            );

            for phase in &plan.phases {
                // Check steering before each phase
                steering.process_commands().await;
                if steering.should_stop() {
                    break;
                }

                if phase.parallel && phase.tool_indices.len() > 1 {
                    // Execute phase tools in parallel using tokio::spawn
                    let mut handles = Vec::with_capacity(phase.tool_indices.len());

                    for &idx in &phase.tool_indices {
                        let tool_call = &plan.tool_calls[idx];

                        // Run before_tool hook (still sequential for hooks)
                        match self.hooks.run_before_tool(tool_call, history).await? {
                            HookResult::Stop { reason } => {
                                let _ = event_tx.send(AgentEvent::Stopped { reason }).await;
                                return Err(Error::Agent("Stopped by hook".to_string()));
                            }
                            HookResult::Block { response } => {
                                results.push((tool_call.id.clone(), response, false));
                                continue;
                            }
                            _ => {}
                        }

                        // Track tools used
                        if !tools_used.contains(&tool_call.name) {
                            tools_used.push(tool_call.name.clone());
                        }

                        // Spawn parallel execution
                        let tc = tool_call.clone();
                        let sid = session_id.to_string();
                        let ctx = Arc::clone(&self.ctx);
                        let tx = event_tx.clone();

                        handles.push(tokio::spawn(async move {
                            let _tool_ctx = ctx.tool_context(&sid);
                            let start = Instant::now();

                            let _ = tx
                                .send(AgentEvent::ToolStart {
                                    tool_name: tc.name.clone(),
                                    tool_call_id: tc.id.clone(),
                                })
                                .await;

                            let result = ctx.execute_tool(&tc.name, tc.arguments.clone()).await;
                            let duration_ms = start.elapsed().as_millis() as u64;

                            let (content, is_error) = match result {
                                Ok(exec_result) if exec_result.success => {
                                    (exec_result.output, false)
                                }
                                Ok(exec_result) => {
                                    (exec_result.error.unwrap_or_else(|| exec_result.output), true)
                                }
                                Err(e) => (e.to_string(), true),
                            };

                            let _ = tx
                                .send(AgentEvent::ToolComplete {
                                    tool_name: tc.name.clone(),
                                    tool_call_id: tc.id.clone(),
                                    result: content.clone(),
                                    success: !is_error,
                                    duration_ms,
                                })
                                .await;

                            (tc.id.clone(), tc.name.clone(), content, is_error, duration_ms)
                        }));
                    }

                    // Collect parallel results
                    for handle in handles {
                        match handle.await {
                            Ok((id, _name, content, is_error, _duration_ms)) => {
                                // Note: We skip after_tool hook for parallel execution
                                // This is a trade-off for parallelization performance
                                results.push((id, content, is_error));
                            }
                            Err(e) => {
                                warn!("Parallel tool task failed: {}", e);
                            }
                        }
                    }
                } else {
                    // Sequential execution for this phase
                    for &idx in &phase.tool_indices {
                        let tool_call = &plan.tool_calls[idx];
                        let (id, content, is_error) = self
                            .execute_tool_sequential(session_id, tool_call, history, event_tx, tools_used)
                            .await?;
                        results.push((id, content, is_error));
                    }
                }
            }
        } else {
            // Sequential execution (original behavior)
            for tool_call in tool_calls {
                let (id, content, is_error) = self
                    .execute_tool_sequential(session_id, tool_call, history, event_tx, tools_used)
                    .await?;
                results.push((id, content, is_error));
            }
        }

        Ok(results)
    }

    /// Execute a single tool sequentially with hooks
    async fn execute_tool_sequential(
        &self,
        session_id: &str,
        tool_call: &ToolCall,
        history: &mut MessageHistory,
        event_tx: &mpsc::Sender<AgentEvent>,
        tools_used: &mut Vec<String>,
    ) -> Result<(String, String, bool)> {
        let steering = self.steering_checker();

        // Check steering before each tool
        steering.process_commands().await;
        if steering.should_stop() {
            return Err(Error::Agent("Stopped".to_string()));
        }

        // Run before_tool hook
        match self.hooks.run_before_tool(tool_call, history).await? {
            HookResult::Stop { reason } => {
                let _ = event_tx.send(AgentEvent::Stopped { reason }).await;
                return Err(Error::Agent("Stopped by hook".to_string()));
            }
            HookResult::Block { response } => {
                return Ok((tool_call.id.clone(), response, false));
            }
            _ => {}
        }

        let tool_start = Instant::now();
        let result = self.execute_tool(session_id, tool_call, event_tx).await;
        let duration_ms = tool_start.elapsed().as_millis() as u64;

        // Create ToolResult for hook
        let (content, is_error) = match &result {
            Ok(content) => (content.clone(), false),
            Err(e) => (e.to_string(), true),
        };

        let tool_result = ToolResult {
            tool_call_id: tool_call.id.clone(),
            tool_name: tool_call.name.clone(),
            output: content.clone(),
            success: !is_error,
            duration_ms,
        };

        // Run after_tool hook
        self.hooks
            .run_after_tool(tool_call, &tool_result, history)
            .await?;

        // Track tools used
        if !tools_used.contains(&tool_call.name) {
            tools_used.push(tool_call.name.clone());
        }

        Ok((tool_call.id.clone(), content, is_error))
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

        let start = Instant::now();

        // Create tool context
        let tool_ctx = self.ctx.tool_context(session_id);

        // Execute tool with recovery
        let result = self
            .execute_tool_with_recovery(
                session_id,
                &tool_call.name,
                &tool_call.id,
                tool_call.arguments.clone(),
                &tool_ctx,
                event_tx,
            )
            .await;

        let duration_ms = start.elapsed().as_millis() as u64;

        // Send completion event
        match &result {
            Ok(content) => {
                let _ = event_tx
                    .send(AgentEvent::ToolComplete {
                        tool_name: tool_call.name.clone(),
                        tool_call_id: tool_call.id.clone(),
                        result: content.clone(),
                        success: true,
                        duration_ms,
                    })
                    .await;
            }
            Err(e) => {
                let _ = event_tx
                    .send(AgentEvent::ToolComplete {
                        tool_name: tool_call.name.clone(),
                        tool_call_id: tool_call.id.clone(),
                        result: e.to_string(),
                        success: false,
                        duration_ms,
                    })
                    .await;
            }
        }

        result
    }

    /// Execute a tool with automatic error recovery
    async fn execute_tool_with_recovery(
        &self,
        _session_id: &str,
        tool_name: &str,
        _tool_call_id: &str,
        arguments: Value,
        _tool_ctx: &dyn forge_core::ToolContext,
        _event_tx: &mpsc::Sender<AgentEvent>,
    ) -> Result<String> {
        let mut recovery_ctx = RecoveryContext {
            cwd: self.ctx.working_dir.to_string_lossy().to_string(),
            tool_name: tool_name.to_string(),
            original_input: arguments.clone(),
            retry_count: 0,
            max_retries: 3,
            available_files: Vec::new(),
        };

        let mut current_tool = tool_name.to_string();
        let mut current_args = arguments;

        loop {
            // Execute tool via AgentContext (delegates to Layer2-core)
            let result = self
                .ctx
                .execute_tool(&current_tool, current_args.clone())
                .await;

            match result {
                Ok(exec_result) if exec_result.success => {
                    return Ok(exec_result.output);
                }
                Ok(exec_result) => {
                    // Tool executed but failed - attempt recovery
                    let error_msg = exec_result.error.unwrap_or_else(|| "Unknown error".to_string());
                    info!(
                        "Tool '{}' failed: {}. Attempting recovery...",
                        current_tool, error_msg
                    );

                    // Classify the error for recovery
                    let recoverable_error = match self.error_recovery.classify_error(&current_tool, &error_msg) {
                        Some(err) => err,
                        None => {
                            // Unrecognized error, give up
                            return Err(Error::Tool(error_msg));
                        }
                    };

                    let action = self
                        .error_recovery
                        .handle_error(&recoverable_error, &recovery_ctx)
                        .await;

                    match action {
                        RecoveryAction::Retry { modified_input, delay } => {
                            info!("Recovery: Retrying");
                            if let Some(input) = modified_input {
                                current_args = input;
                            }
                            if let Some(d) = delay {
                                tokio::time::sleep(d).await;
                            }
                            recovery_ctx.retry_count += 1;
                        }

                        RecoveryAction::UseFallback { tool, input } => {
                            info!("Recovery: Using fallback tool '{}'", tool);
                            current_tool = tool;
                            current_args = input;
                        }

                        RecoveryAction::Skip { reason } => {
                            info!("Recovery: Skipping - {}", reason);
                            return Err(Error::Tool(format!("Skipped: {}", reason)));
                        }

                        RecoveryAction::AskUser { question, suggestions } => {
                            let suggestions_str = suggestions.join(", ");
                            let error_with_question = format!(
                                "Tool failed: {}\n\nRecovery question: {}\nSuggestions: {}",
                                error_msg, question, suggestions_str
                            );
                            return Err(Error::Tool(error_with_question));
                        }

                        RecoveryAction::GiveUp { reason } => {
                            let error_with_reason = format!(
                                "Tool failed: {}\nReason: {}",
                                error_msg, reason
                            );
                            return Err(Error::Tool(error_with_reason));
                        }
                    }
                }
                Err(e) => {
                    // Tool execution error
                    return Err(Error::Tool(e.to_string()));
                }
            }
        }
    }
}

// Implement Steerable for Agent
impl Steerable for Agent {
    fn steering(&self) -> &SteeringChecker {
        &self.steering_checker
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_config_default() {
        let config = AgentConfig::default();
        assert_eq!(config.max_iterations, 50);
        assert!(config.auto_compress);
        assert!(config.streaming);
    }

    #[test]
    fn test_agent_config_fast() {
        let config = AgentConfig::fast();
        assert_eq!(config.max_iterations, 20);
    }

    #[test]
    fn test_agent_config_long_session() {
        let config = AgentConfig::long_session();
        assert_eq!(config.max_iterations, 100);
    }
}
