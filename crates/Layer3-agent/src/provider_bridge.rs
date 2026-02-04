//! Provider Bridge
//!
//! Layer2의 AgentProvider와 Layer3의 Agent를 연결하는 브릿지입니다.
//!
//! ## 역할
//!
//! 1. **NativeAgentProvider 구현**: ForgeCode 자체 Agent를 AgentProvider로 래핑
//! 2. **이벤트 변환**: AgentEvent ↔ AgentStreamEvent 변환
//! 3. **세션 관리**: 세션 정보 저장 및 조회
//!
//! ## 사용 예
//!
//! ```ignore
//! use forge_agent::provider_bridge::ForgeNativeProvider;
//! use forge_provider::{AgentProvider, AgentQueryOptions};
//!
//! let provider = ForgeNativeProvider::new(agent_ctx);
//! let stream = provider.query("Hello", AgentQueryOptions::default()).await?;
//! ```

use crate::agent::{Agent, AgentConfig, AgentEvent};
use crate::context::AgentContext;
use crate::history::MessageHistory;
use crate::session::SessionManager;
use async_stream::stream;
use async_trait::async_trait;
use forge_provider::{
    AgentProvider, AgentProviderError, AgentProviderType, AgentQueryOptions, AgentStream,
    AgentStreamEvent, AgentTokenUsage, SessionInfo,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info};

// ============================================================================
// ForgeCode Native Provider
// ============================================================================

/// ForgeCode Native Agent Provider
///
/// Layer3의 Agent를 AgentProvider 인터페이스로 래핑합니다.
/// 이를 통해 다른 프로바이더(Claude SDK, Codex)와 동일한 방식으로 사용할 수 있습니다.
pub struct ForgeNativeProvider {
    /// Agent context
    ctx: Arc<AgentContext>,

    /// Agent configuration
    config: AgentConfig,

    /// Session manager
    sessions: Arc<RwLock<SessionManager>>,

    /// Active sessions data
    session_data: Arc<RwLock<HashMap<String, SessionData>>>,
}

/// Session data for tracking
struct SessionData {
    history: MessageHistory,
    created_at: chrono::DateTime<chrono::Utc>,
    token_usage: AgentTokenUsage,
    tools_used: Vec<String>,
}

impl ForgeNativeProvider {
    /// Create a new ForgeCode native provider
    pub fn new(ctx: Arc<AgentContext>) -> Self {
        Self {
            ctx,
            config: AgentConfig::default(),
            sessions: Arc::new(RwLock::new(SessionManager::new())),
            session_data: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create with custom configuration
    pub fn with_config(ctx: Arc<AgentContext>, config: AgentConfig) -> Self {
        Self {
            ctx,
            config,
            sessions: Arc::new(RwLock::new(SessionManager::new())),
            session_data: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get or create session data
    async fn get_or_create_session(&self, session_id: &str) -> SessionData {
        let mut data = self.session_data.write().await;
        if !data.contains_key(session_id) {
            data.insert(
                session_id.to_string(),
                SessionData {
                    history: MessageHistory::new(),
                    created_at: chrono::Utc::now(),
                    token_usage: AgentTokenUsage::default(),
                    tools_used: Vec::new(),
                },
            );
        }
        data.get(session_id).unwrap().clone()
    }

    /// Update session data
    async fn update_session(
        &self,
        session_id: &str,
        history: MessageHistory,
        tokens: AgentTokenUsage,
        tools: Vec<String>,
    ) {
        let mut data = self.session_data.write().await;
        if let Some(session) = data.get_mut(session_id) {
            session.history = history;
            session.token_usage.input_tokens += tokens.input_tokens;
            session.token_usage.output_tokens += tokens.output_tokens;
            session.token_usage.total_tokens =
                session.token_usage.input_tokens + session.token_usage.output_tokens;
            for tool in tools {
                if !session.tools_used.contains(&tool) {
                    session.tools_used.push(tool);
                }
            }
        }
    }

    /// Convert AgentEvent to AgentStreamEvent
    fn convert_event(event: AgentEvent, session_id: &str) -> Option<AgentStreamEvent> {
        match event {
            AgentEvent::Text(text) => Some(AgentStreamEvent::Text(text)),

            AgentEvent::Thinking => Some(AgentStreamEvent::Thinking("Processing...".to_string())),

            AgentEvent::ToolStart {
                tool_name,
                tool_call_id,
            } => Some(AgentStreamEvent::ToolCallStart {
                tool_use_id: tool_call_id,
                tool_name,
                arguments: serde_json::Value::Null,
            }),

            AgentEvent::ToolComplete {
                tool_name,
                tool_call_id,
                result,
                success,
                duration_ms,
            } => Some(AgentStreamEvent::ToolCallComplete {
                tool_use_id: tool_call_id,
                tool_name,
                result,
                success,
                duration_ms,
            }),

            AgentEvent::Usage {
                input_tokens,
                output_tokens,
            } => Some(AgentStreamEvent::Usage {
                input_tokens,
                output_tokens,
            }),

            AgentEvent::Done { full_response } => Some(AgentStreamEvent::Done {
                result: full_response,
                total_turns: 0,
            }),

            AgentEvent::Error(e) => Some(AgentStreamEvent::Error(e)),

            AgentEvent::TurnStart { turn } => {
                debug!("Turn {} started", turn);
                None
            }

            AgentEvent::TurnComplete { turn } => {
                debug!("Turn {} completed", turn);
                None
            }

            AgentEvent::Compressed {
                tokens_before,
                tokens_after,
                ..
            } => {
                info!(
                    "Context compressed: {} -> {} tokens",
                    tokens_before, tokens_after
                );
                None
            }

            AgentEvent::Paused => {
                debug!("Agent paused");
                None
            }

            AgentEvent::Resumed => {
                debug!("Agent resumed");
                None
            }

            AgentEvent::Stopped { reason } => Some(AgentStreamEvent::Error(format!(
                "Agent stopped: {}",
                reason
            ))),
        }
    }
}

// Clone implementation for SessionData
impl Clone for SessionData {
    fn clone(&self) -> Self {
        Self {
            history: self.history.clone(),
            created_at: self.created_at,
            token_usage: self.token_usage.clone(),
            tools_used: self.tools_used.clone(),
        }
    }
}

#[async_trait]
impl AgentProvider for ForgeNativeProvider {
    fn provider_type(&self) -> AgentProviderType {
        AgentProviderType::Native
    }

    fn name(&self) -> &str {
        "ForgeCode Native"
    }

    fn supported_tools(&self) -> Vec<String> {
        self.ctx
            .tools
            .list()
            .iter()
            .map(|(name, _)| name.to_string())
            .collect()
    }

    async fn is_available(&self) -> bool {
        // Check if we have any available LLM provider
        !self.ctx.list_providers().is_empty()
    }

    async fn query(
        &self,
        prompt: &str,
        options: AgentQueryOptions,
    ) -> Result<AgentStream, AgentProviderError> {
        // Generate session ID
        let session_id = options
            .resume_session
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        let session_id_clone = session_id.clone();

        // Get or create session
        let session_data = self.get_or_create_session(&session_id).await;
        let mut history = session_data.history;

        // Apply system prompt if provided
        if let Some(system_prompt) = &options.system_prompt {
            history.set_system_prompt(system_prompt);
        }

        // Create agent with configuration
        let mut config = self.config.clone();
        if let Some(max_turns) = options.max_turns {
            config.max_iterations = max_turns as usize;
        }

        let agent = Agent::with_config(self.ctx.clone(), config);

        // Create event channel
        let (event_tx, mut event_rx) = mpsc::channel::<AgentEvent>(100);

        // Clone for async move
        let prompt = prompt.to_string();
        let ctx = self.ctx.clone();
        let session_data_arc = self.session_data.clone();

        // Spawn agent execution
        let join_handle = tokio::spawn(async move {
            let result = agent
                .run(&session_id_clone, &mut history, &prompt, event_tx)
                .await;

            // Update session data
            let mut data = session_data_arc.write().await;
            if let Some(session) = data.get_mut(&session_id_clone) {
                session.history = history;
            }

            result
        });

        // Create stream from events
        let stream = stream! {
            // Emit session start
            yield AgentStreamEvent::SessionStart {
                session_id: session_id.clone(),
                provider: AgentProviderType::Native,
            };

            let mut total_turns = 0u32;
            let mut final_response = String::new();

            // Stream events
            while let Some(event) = event_rx.recv().await {
                // Track turns
                if let AgentEvent::TurnComplete { turn } = &event {
                    total_turns = *turn;
                }

                // Track final response
                if let AgentEvent::Done { full_response } = &event {
                    final_response = full_response.clone();
                }

                // Convert and yield event
                if let Some(stream_event) = Self::convert_event(event, &session_id) {
                    yield stream_event;
                }
            }

            // Wait for agent to complete
            match join_handle.await {
                Ok(Ok(_)) => {
                    // Success - already sent Done event
                }
                Ok(Err(e)) => {
                    yield AgentStreamEvent::Error(e.to_string());
                }
                Err(e) => {
                    yield AgentStreamEvent::Error(format!("Agent task failed: {}", e));
                }
            }
        };

        Ok(Box::pin(stream))
    }

    async fn get_session_info(&self, session_id: &str) -> Result<SessionInfo, AgentProviderError> {
        let data = self.session_data.read().await;
        if let Some(session) = data.get(session_id) {
            Ok(SessionInfo {
                id: session_id.to_string(),
                provider: AgentProviderType::Native,
                created_at: session.created_at,
                message_count: session.history.len(),
                token_usage: session.token_usage.clone(),
                tools_used: session.tools_used.clone(),
            })
        } else {
            Err(AgentProviderError::SessionNotFound(session_id.to_string()))
        }
    }

    async fn list_models(&self) -> Vec<String> {
        self.ctx.list_models().await
    }

    fn current_model(&self) -> &str {
        // Return a static string since we can't make this async
        "default"
    }
}

// ============================================================================
// Provider Registry Builder
// ============================================================================

use forge_provider::AgentProviderRegistry;

/// Build a provider registry with ForgeCode native provider
pub fn build_provider_registry(ctx: Arc<AgentContext>) -> AgentProviderRegistry {
    let mut registry = AgentProviderRegistry::new();

    // Register native provider
    let native = ForgeNativeProvider::new(ctx);
    registry.register("native", Box::new(native));

    // TODO: Register Claude SDK provider if available
    // if let Some(claude) = ClaudeAgentSdkProvider::from_env() {
    //     registry.register("claude-sdk", Box::new(claude));
    // }

    // TODO: Register Codex provider if available
    // if let Some(codex) = CodexProvider::from_env() {
    //     registry.register("codex", Box::new(codex));
    // }

    registry
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_conversion_text() {
        let event = AgentEvent::Text("Hello".to_string());
        let converted = ForgeNativeProvider::convert_event(event, "test-session");
        assert!(matches!(converted, Some(AgentStreamEvent::Text(_))));
    }

    #[test]
    fn test_event_conversion_tool_start() {
        let event = AgentEvent::ToolStart {
            tool_name: "Read".to_string(),
            tool_call_id: "tc_123".to_string(),
        };
        let converted = ForgeNativeProvider::convert_event(event, "test-session");
        assert!(matches!(
            converted,
            Some(AgentStreamEvent::ToolCallStart { .. })
        ));
    }

    #[test]
    fn test_event_conversion_done() {
        let event = AgentEvent::Done {
            full_response: "Done!".to_string(),
        };
        let converted = ForgeNativeProvider::convert_event(event, "test-session");
        assert!(matches!(converted, Some(AgentStreamEvent::Done { .. })));
    }
}
