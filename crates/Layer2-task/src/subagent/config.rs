//! Sub-agent configuration
//!
//! ## Token Budget Inheritance
//!
//! Sub-agents can inherit token budget constraints from their parent:
//!
//! ```ignore
//! // Parent has 200k context, sub-agent gets proportional allocation
//! let config = SubAgentConfig::for_type(SubAgentType::Explore)
//!     .with_token_budget(TokenBudgetConfig::from_parent(parent_budget, 0.3)); // 30% of parent
//! ```

use crate::subagent::SubAgentType;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Model selection for sub-agent
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelSelection {
    /// Use the same model as parent
    Inherit,

    /// Claude Haiku - fast, cost-effective
    Haiku,

    /// Claude Sonnet - balanced
    Sonnet,

    /// Claude Opus - most capable
    Opus,
}

impl Default for ModelSelection {
    fn default() -> Self {
        Self::Inherit
    }
}

impl ModelSelection {
    /// Get the model identifier string
    pub fn model_id(&self, parent_model: &str) -> String {
        match self {
            Self::Inherit => parent_model.to_string(),
            Self::Haiku => "claude-3-5-haiku-20241022".to_string(),
            Self::Sonnet => "claude-sonnet-4-20250514".to_string(),
            Self::Opus => "claude-opus-4-20250514".to_string(),
        }
    }
}

/// Permission mode for sub-agent
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PermissionMode {
    /// Inherit permissions from parent session
    Inherit,

    /// Request new permissions
    Prompt,

    /// Auto-approve all (dangerous!)
    AutoApprove,

    /// Deny all operations requiring permission
    Strict,
}

impl Default for PermissionMode {
    fn default() -> Self {
        Self::Inherit
    }
}

// ============================================================================
// Token Budget Configuration
// ============================================================================

/// Token budget configuration for sub-agent
///
/// Controls how much of the context window the sub-agent can use.
/// Can be configured absolutely or relative to parent's budget.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenBudgetConfig {
    /// Use default for the model (inherit model's context window)
    Default,

    /// Fixed token budget
    Fixed {
        /// Maximum context window tokens
        max_tokens: usize,
        /// Reserved for response
        reserved_for_response: usize,
    },

    /// Proportional to parent's remaining budget
    ProportionalToParent {
        /// Fraction of parent's available tokens (0.0 - 1.0)
        fraction: f32,
        /// Minimum tokens to allocate
        min_tokens: usize,
        /// Maximum tokens to allocate
        max_tokens: usize,
    },

    /// Share parent's budget (deduct from parent when used)
    SharedWithParent {
        /// Maximum tokens this agent can use
        max_tokens: usize,
    },
}

impl Default for TokenBudgetConfig {
    fn default() -> Self {
        Self::Default
    }
}

impl TokenBudgetConfig {
    /// Create a fixed budget
    pub fn fixed(max_tokens: usize) -> Self {
        Self::Fixed {
            max_tokens,
            reserved_for_response: 4_000,
        }
    }

    /// Create proportional budget from parent
    pub fn proportional(fraction: f32) -> Self {
        Self::ProportionalToParent {
            fraction: fraction.clamp(0.1, 0.9),
            min_tokens: 8_000,
            max_tokens: 200_000,
        }
    }

    /// Create shared budget with parent
    pub fn shared(max_tokens: usize) -> Self {
        Self::SharedWithParent { max_tokens }
    }

    /// Calculate effective token budget given parent's state
    ///
    /// # Arguments
    /// * `parent_max_tokens` - Parent's maximum context window
    /// * `parent_used_tokens` - Tokens already used by parent
    /// * `model_default_tokens` - Default for the model being used
    pub fn calculate_effective_budget(
        &self,
        parent_max_tokens: usize,
        parent_used_tokens: usize,
        model_default_tokens: usize,
    ) -> EffectiveTokenBudget {
        let parent_available = parent_max_tokens.saturating_sub(parent_used_tokens);

        match self {
            Self::Default => EffectiveTokenBudget {
                max_tokens: model_default_tokens,
                reserved_for_response: 4_000,
                source: TokenBudgetSource::ModelDefault,
                parent_allocation: None,
            },

            Self::Fixed {
                max_tokens,
                reserved_for_response,
            } => EffectiveTokenBudget {
                max_tokens: *max_tokens,
                reserved_for_response: *reserved_for_response,
                source: TokenBudgetSource::Fixed,
                parent_allocation: None,
            },

            Self::ProportionalToParent {
                fraction,
                min_tokens,
                max_tokens,
            } => {
                let proportional = (parent_available as f32 * fraction) as usize;
                let effective = proportional.clamp(*min_tokens, *max_tokens);

                EffectiveTokenBudget {
                    max_tokens: effective,
                    reserved_for_response: (effective / 20).max(2_000), // 5% or min 2k
                    source: TokenBudgetSource::ProportionalToParent {
                        parent_available,
                        fraction: *fraction,
                    },
                    parent_allocation: Some(effective),
                }
            }

            Self::SharedWithParent { max_tokens } => {
                // Can use up to max_tokens but limited by parent's available
                let effective = (*max_tokens).min(parent_available);

                EffectiveTokenBudget {
                    max_tokens: effective,
                    reserved_for_response: (effective / 20).max(2_000),
                    source: TokenBudgetSource::SharedWithParent,
                    parent_allocation: Some(effective),
                }
            }
        }
    }

    /// Presets for different agent types
    pub fn for_explore_agent() -> Self {
        // Explore agents typically need moderate context
        Self::ProportionalToParent {
            fraction: 0.25, // 25% of parent
            min_tokens: 16_000,
            max_tokens: 64_000,
        }
    }

    pub fn for_plan_agent() -> Self {
        // Plan agents may need more context for complex planning
        Self::ProportionalToParent {
            fraction: 0.35, // 35% of parent
            min_tokens: 32_000,
            max_tokens: 100_000,
        }
    }

    pub fn for_bash_agent() -> Self {
        // Bash agents need less context
        Self::Fixed {
            max_tokens: 16_000,
            reserved_for_response: 2_000,
        }
    }

    pub fn for_general_agent() -> Self {
        // General agents get substantial allocation
        Self::ProportionalToParent {
            fraction: 0.5, // 50% of parent
            min_tokens: 32_000,
            max_tokens: 128_000,
        }
    }
}

/// Calculated effective token budget
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectiveTokenBudget {
    /// Maximum tokens for this sub-agent
    pub max_tokens: usize,
    /// Reserved for response
    pub reserved_for_response: usize,
    /// How this budget was determined
    pub source: TokenBudgetSource,
    /// Tokens allocated from parent (if applicable)
    pub parent_allocation: Option<usize>,
}

impl EffectiveTokenBudget {
    /// Available tokens for input (excluding response reservation)
    pub fn available_for_input(&self) -> usize {
        self.max_tokens.saturating_sub(self.reserved_for_response)
    }

    /// Check if usage is within budget
    pub fn is_within_budget(&self, used_tokens: usize) -> bool {
        used_tokens <= self.available_for_input()
    }

    /// Calculate remaining tokens
    pub fn remaining(&self, used_tokens: usize) -> usize {
        self.available_for_input().saturating_sub(used_tokens)
    }

    /// Get usage percentage
    pub fn usage_percent(&self, used_tokens: usize) -> f32 {
        if self.available_for_input() == 0 {
            100.0
        } else {
            (used_tokens as f32 / self.available_for_input() as f32) * 100.0
        }
    }
}

/// Source of the token budget
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenBudgetSource {
    /// Using model's default context window
    ModelDefault,
    /// Fixed configuration
    Fixed,
    /// Proportional to parent
    ProportionalToParent {
        parent_available: usize,
        fraction: f32,
    },
    /// Shared with parent
    SharedWithParent,
}

/// Configuration for a sub-agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgentConfig {
    /// Agent type
    pub agent_type: SubAgentType,

    /// System prompt override (if any)
    pub system_prompt: Option<String>,

    /// Allowed tools (empty = use type defaults)
    pub allowed_tools: Vec<String>,

    /// Disallowed tools (takes precedence over allowed)
    pub disallowed_tools: Vec<String>,

    /// Model to use
    pub model: ModelSelection,

    /// Permission mode
    pub permission_mode: PermissionMode,

    /// Run in background
    pub run_in_background: bool,

    /// Maximum turns (API round-trips)
    pub max_turns: u32,

    /// Timeout for entire agent execution
    pub timeout: Duration,

    /// Whether to share discoveries with parent context
    pub share_discoveries: bool,

    /// Whether agent has access to parent context
    pub inherit_context: bool,

    /// Token budget configuration
    pub token_budget: TokenBudgetConfig,
}

impl Default for SubAgentConfig {
    fn default() -> Self {
        Self {
            agent_type: SubAgentType::General,
            system_prompt: None,
            allowed_tools: vec![],
            disallowed_tools: vec![],
            model: ModelSelection::Inherit,
            permission_mode: PermissionMode::Inherit,
            run_in_background: false,
            max_turns: 50,
            timeout: Duration::from_secs(600), // 10 minutes
            share_discoveries: true,
            inherit_context: false,
            token_budget: TokenBudgetConfig::Default,
        }
    }
}

impl SubAgentConfig {
    /// Create a new configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Create configuration for a specific agent type
    pub fn for_type(agent_type: SubAgentType) -> Self {
        let (allowed_tools, disallowed_tools) = (
            agent_type.default_allowed_tools(),
            agent_type.default_disallowed_tools(),
        );

        // Adjust defaults based on type
        let (model, max_turns, token_budget) = match &agent_type {
            SubAgentType::Explore => (
                ModelSelection::Haiku,
                30,
                TokenBudgetConfig::for_explore_agent(),
            ),
            SubAgentType::Plan => (
                ModelSelection::Sonnet,
                20,
                TokenBudgetConfig::for_plan_agent(),
            ),
            SubAgentType::General => (
                ModelSelection::Inherit,
                50,
                TokenBudgetConfig::for_general_agent(),
            ),
            SubAgentType::Bash => (
                ModelSelection::Haiku,
                20,
                TokenBudgetConfig::for_bash_agent(),
            ),
            SubAgentType::Custom(_) => (ModelSelection::Inherit, 50, TokenBudgetConfig::Default),
        };

        Self {
            agent_type,
            allowed_tools,
            disallowed_tools,
            model,
            max_turns,
            token_budget,
            ..Default::default()
        }
    }

    /// Builder: set system prompt
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Builder: add allowed tool
    pub fn allow_tool(mut self, tool: impl Into<String>) -> Self {
        self.allowed_tools.push(tool.into());
        self
    }

    /// Builder: add disallowed tool
    pub fn disallow_tool(mut self, tool: impl Into<String>) -> Self {
        self.disallowed_tools.push(tool.into());
        self
    }

    /// Builder: set model
    pub fn with_model(mut self, model: ModelSelection) -> Self {
        self.model = model;
        self
    }

    /// Builder: set permission mode
    pub fn with_permission_mode(mut self, mode: PermissionMode) -> Self {
        self.permission_mode = mode;
        self
    }

    /// Builder: enable background execution
    pub fn run_in_background(mut self) -> Self {
        self.run_in_background = true;
        self
    }

    /// Builder: set max turns
    pub fn with_max_turns(mut self, turns: u32) -> Self {
        self.max_turns = turns;
        self
    }

    /// Builder: set timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Builder: enable context inheritance
    pub fn inherit_context(mut self) -> Self {
        self.inherit_context = true;
        self
    }

    /// Builder: set token budget configuration
    pub fn with_token_budget(mut self, budget: TokenBudgetConfig) -> Self {
        self.token_budget = budget;
        self
    }

    /// Builder: set fixed token budget
    pub fn with_fixed_tokens(mut self, max_tokens: usize) -> Self {
        self.token_budget = TokenBudgetConfig::fixed(max_tokens);
        self
    }

    /// Builder: set proportional token budget from parent
    pub fn with_proportional_tokens(mut self, fraction: f32) -> Self {
        self.token_budget = TokenBudgetConfig::proportional(fraction);
        self
    }

    /// Calculate effective token budget given parent's state
    pub fn calculate_token_budget(
        &self,
        parent_max_tokens: usize,
        parent_used_tokens: usize,
    ) -> EffectiveTokenBudget {
        // Get model default based on selection
        let model_default = match &self.model {
            ModelSelection::Haiku => 200_000,
            ModelSelection::Sonnet => 200_000,
            ModelSelection::Opus => 200_000,
            ModelSelection::Inherit => parent_max_tokens,
        };

        self.token_budget.calculate_effective_budget(
            parent_max_tokens,
            parent_used_tokens,
            model_default,
        )
    }

    /// Check if a tool is allowed for this agent
    pub fn is_tool_allowed(&self, tool_name: &str) -> bool {
        // Disallowed takes precedence
        if self.disallowed_tools.contains(&tool_name.to_string()) {
            return false;
        }

        // If allowed list is empty, use type defaults
        if self.allowed_tools.is_empty() {
            let defaults = self.agent_type.default_allowed_tools();
            if defaults.is_empty() {
                // Custom type with no config - deny all
                return false;
            }
            return defaults.contains(&tool_name.to_string());
        }

        // Check allowed list
        self.allowed_tools.contains(&tool_name.to_string())
    }

    /// Get the effective system prompt
    pub fn effective_system_prompt(&self) -> String {
        if let Some(ref prompt) = self.system_prompt {
            return prompt.clone();
        }

        // Generate default prompt based on type
        match &self.agent_type {
            SubAgentType::Explore => {
                "You are an exploration agent. Your task is to navigate and understand \
                 the codebase. You can only read files and search for patterns. \
                 You cannot modify any files. Focus on gathering information efficiently."
                    .to_string()
            }
            SubAgentType::Plan => {
                "You are a planning agent. Your task is to design implementation strategies \
                 and architectural decisions. You can only read files to understand the codebase. \
                 You cannot modify any files. Provide detailed, actionable plans."
                    .to_string()
            }
            SubAgentType::General => "You are a general-purpose agent with full tool access. \
                 Complete the assigned task efficiently and thoroughly."
                .to_string(),
            SubAgentType::Bash => {
                "You are a command execution specialist. Your task is to run commands \
                 and scripts. Focus on executing commands correctly and handling their output."
                    .to_string()
            }
            SubAgentType::Custom(name) => {
                format!(
                    "You are a custom agent: {}. Complete the assigned task.",
                    name
                )
            }
        }
    }
}

/// Predefined configurations for common use cases
impl SubAgentConfig {
    /// Quick codebase exploration
    pub fn quick_explore() -> Self {
        Self::for_type(SubAgentType::Explore)
            .with_model(ModelSelection::Haiku)
            .with_max_turns(15)
    }

    /// Thorough codebase exploration
    pub fn thorough_explore() -> Self {
        Self::for_type(SubAgentType::Explore)
            .with_model(ModelSelection::Sonnet)
            .with_max_turns(50)
    }

    /// Implementation planning
    pub fn implementation_plan() -> Self {
        Self::for_type(SubAgentType::Plan)
            .with_model(ModelSelection::Sonnet)
            .with_max_turns(30)
    }

    /// Background build/test runner
    pub fn background_runner() -> Self {
        Self::for_type(SubAgentType::Bash)
            .run_in_background()
            .with_timeout(Duration::from_secs(1800)) // 30 minutes
            .with_max_turns(100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_allowed() {
        let config = SubAgentConfig::for_type(SubAgentType::Explore);
        assert!(config.is_tool_allowed("read"));
        assert!(config.is_tool_allowed("grep"));
        assert!(!config.is_tool_allowed("write"));
        assert!(!config.is_tool_allowed("bash"));
    }

    #[test]
    fn test_disallow_overrides_allow() {
        let config = SubAgentConfig::for_type(SubAgentType::General).disallow_tool("bash");

        assert!(config.is_tool_allowed("read"));
        assert!(!config.is_tool_allowed("bash"));
    }

    #[test]
    fn test_model_selection() {
        let haiku = ModelSelection::Haiku;
        assert!(haiku.model_id("parent").contains("haiku"));

        let inherit = ModelSelection::Inherit;
        assert_eq!(inherit.model_id("claude-sonnet"), "claude-sonnet");
    }

    #[test]
    fn test_builder_pattern() {
        let config = SubAgentConfig::new()
            .with_model(ModelSelection::Opus)
            .with_max_turns(100)
            .run_in_background()
            .allow_tool("custom_tool");

        assert_eq!(config.model, ModelSelection::Opus);
        assert_eq!(config.max_turns, 100);
        assert!(config.run_in_background);
        assert!(config.allowed_tools.contains(&"custom_tool".into()));
    }

    #[test]
    fn test_token_budget_fixed() {
        let budget = TokenBudgetConfig::fixed(50_000);
        let effective = budget.calculate_effective_budget(200_000, 100_000, 128_000);

        assert_eq!(effective.max_tokens, 50_000);
        assert_eq!(effective.reserved_for_response, 4_000);
        assert!(matches!(effective.source, TokenBudgetSource::Fixed));
    }

    #[test]
    fn test_token_budget_proportional() {
        let budget = TokenBudgetConfig::proportional(0.25); // 25% of parent
        let effective = budget.calculate_effective_budget(200_000, 40_000, 128_000);

        // Parent has 160k available, 25% = 40k
        assert_eq!(effective.max_tokens, 40_000);
        assert!(matches!(
            effective.source,
            TokenBudgetSource::ProportionalToParent { .. }
        ));
    }

    #[test]
    fn test_token_budget_shared() {
        let budget = TokenBudgetConfig::shared(50_000);

        // Parent has enough
        let effective = budget.calculate_effective_budget(200_000, 100_000, 128_000);
        assert_eq!(effective.max_tokens, 50_000);

        // Parent doesn't have enough
        let effective = budget.calculate_effective_budget(200_000, 180_000, 128_000);
        assert_eq!(effective.max_tokens, 20_000); // Limited by parent's available
    }

    #[test]
    fn test_effective_budget_methods() {
        let budget = EffectiveTokenBudget {
            max_tokens: 50_000,
            reserved_for_response: 4_000,
            source: TokenBudgetSource::Fixed,
            parent_allocation: None,
        };

        assert_eq!(budget.available_for_input(), 46_000);
        assert!(budget.is_within_budget(40_000));
        assert!(!budget.is_within_budget(50_000));
        assert_eq!(budget.remaining(20_000), 26_000);
        assert!((budget.usage_percent(23_000) - 50.0).abs() < 0.1);
    }

    #[test]
    fn test_subagent_config_token_budget() {
        let config = SubAgentConfig::for_type(SubAgentType::Explore);

        // Explore agent should have proportional budget
        let effective = config.calculate_token_budget(200_000, 50_000);

        // Parent has 150k available, explore gets 25%
        assert!(effective.max_tokens >= 16_000);
        assert!(effective.max_tokens <= 64_000);
    }

    #[test]
    fn test_subagent_config_builder_with_tokens() {
        let config = SubAgentConfig::new()
            .with_fixed_tokens(32_000)
            .with_max_turns(20);

        let effective = config.calculate_token_budget(200_000, 0);
        assert_eq!(effective.max_tokens, 32_000);
    }
}
