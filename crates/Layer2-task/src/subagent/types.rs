//! Sub-agent type definitions

use crate::subagent::{SubAgentConfig, SubAgentContext};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// Unique identifier for a sub-agent
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SubAgentId(pub Uuid);

impl SubAgentId {
    /// Generate a new random SubAgentId
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create from string (for resuming)
    pub fn from_string(s: &str) -> Option<Self> {
        Uuid::parse_str(s).ok().map(Self)
    }
}

impl Default for SubAgentId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SubAgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.0.to_string()[..8])
    }
}

/// Sub-agent type - determines capabilities and default configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SubAgentType {
    /// Read-only exploration, optimized for codebase navigation
    /// Allowed: Read, Grep, Glob
    /// Disallowed: Write, Edit, Bash
    Explore,

    /// Architecture planning, no modifications allowed
    /// Allowed: Read, Grep, Glob
    /// Disallowed: Write, Edit, Bash
    Plan,

    /// General purpose with full tool access
    /// Allowed: All tools
    General,

    /// Command execution specialist
    /// Allowed: Bash, Read
    /// Disallowed: Write, Edit (should use Bash for file ops)
    Bash,

    /// Custom agent with user-defined configuration
    Custom(String),
}

impl SubAgentType {
    /// Get default allowed tools for this agent type
    pub fn default_allowed_tools(&self) -> Vec<String> {
        match self {
            Self::Explore => vec![
                "read".into(),
                "grep".into(),
                "glob".into(),
            ],
            Self::Plan => vec![
                "read".into(),
                "grep".into(),
                "glob".into(),
            ],
            Self::General => vec![
                "bash".into(),
                "read".into(),
                "write".into(),
                "edit".into(),
                "grep".into(),
                "glob".into(),
                "forgecmd".into(),
            ],
            Self::Bash => vec![
                "bash".into(),
                "read".into(),
                "forgecmd".into(),
            ],
            Self::Custom(_) => vec![], // Must be explicitly configured
        }
    }

    /// Get default disallowed tools for this agent type
    pub fn default_disallowed_tools(&self) -> Vec<String> {
        match self {
            Self::Explore => vec![
                "write".into(),
                "edit".into(),
                "bash".into(),
                "forgecmd".into(),
            ],
            Self::Plan => vec![
                "write".into(),
                "edit".into(),
                "bash".into(),
                "forgecmd".into(),
            ],
            Self::General => vec![], // No restrictions
            Self::Bash => vec![
                "write".into(),
                "edit".into(),
            ],
            Self::Custom(_) => vec![], // Must be explicitly configured
        }
    }

    /// Get a human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            Self::Explore => "Read-only exploration for codebase navigation",
            Self::Plan => "Architecture planning without modifications",
            Self::General => "General purpose with full tool access",
            Self::Bash => "Command execution specialist",
            Self::Custom(_) => "Custom user-defined agent",
        }
    }

    /// Get the display name
    pub fn display_name(&self) -> String {
        match self {
            Self::Explore => "Explore".into(),
            Self::Plan => "Plan".into(),
            Self::General => "General".into(),
            Self::Bash => "Bash".into(),
            Self::Custom(name) => name.clone(),
        }
    }
}

impl Default for SubAgentType {
    fn default() -> Self {
        Self::General
    }
}

/// Sub-agent execution state
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubAgentState {
    /// Agent created but not started
    Created,

    /// Agent is running
    Running {
        /// Current turn number
        turn: u32,
        /// Maximum turns allowed
        max_turns: u32,
    },

    /// Agent completed successfully
    Completed {
        /// Summary of results
        summary: String,
        /// Number of turns used
        turns_used: u32,
    },

    /// Agent failed with error
    Failed {
        /// Error message
        error: String,
        /// Turn where failure occurred
        at_turn: u32,
    },

    /// Agent was cancelled
    Cancelled {
        /// Reason for cancellation
        reason: Option<String>,
    },

    /// Agent is paused (can be resumed)
    Paused {
        /// Turn where paused
        at_turn: u32,
        /// Reason for pause
        reason: Option<String>,
    },
}

impl SubAgentState {
    /// Check if agent is in a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed { .. } | Self::Failed { .. } | Self::Cancelled { .. }
        )
    }

    /// Check if agent is currently running
    pub fn is_running(&self) -> bool {
        matches!(self, Self::Running { .. })
    }

    /// Check if agent can be resumed
    pub fn is_resumable(&self) -> bool {
        matches!(
            self,
            Self::Paused { .. } | Self::Completed { .. }
        )
    }

    /// Get display symbol
    pub fn symbol(&self) -> &'static str {
        match self {
            Self::Created => "◯",
            Self::Running { .. } => "⟳",
            Self::Completed { .. } => "✓",
            Self::Failed { .. } => "✗",
            Self::Cancelled { .. } => "⊘",
            Self::Paused { .. } => "⏸",
        }
    }

    /// Get display name
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Created => "Created",
            Self::Running { .. } => "Running",
            Self::Completed { .. } => "Completed",
            Self::Failed { .. } => "Failed",
            Self::Cancelled { .. } => "Cancelled",
            Self::Paused { .. } => "Paused",
        }
    }
}

impl Default for SubAgentState {
    fn default() -> Self {
        Self::Created
    }
}

/// A sub-agent instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgent {
    /// Unique identifier
    pub id: SubAgentId,

    /// Configuration
    pub config: SubAgentConfig,

    /// Current state
    pub state: SubAgentState,

    /// Isolated context
    pub context: SubAgentContext,

    /// Parent session ID
    pub parent_session_id: String,

    /// Short description of what this agent is doing
    pub description: String,

    /// Initial prompt
    pub prompt: String,

    /// Output file path (for background agents)
    pub output_file: Option<PathBuf>,

    /// When the agent was created
    pub created_at: DateTime<Utc>,

    /// When the agent started running
    pub started_at: Option<DateTime<Utc>>,

    /// When the agent completed
    pub completed_at: Option<DateTime<Utc>>,
}

impl SubAgent {
    /// Create a new sub-agent
    pub fn new(
        parent_session_id: impl Into<String>,
        config: SubAgentConfig,
        prompt: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            id: SubAgentId::new(),
            config,
            state: SubAgentState::Created,
            context: SubAgentContext::new(),
            parent_session_id: parent_session_id.into(),
            description: description.into(),
            prompt: prompt.into(),
            output_file: None,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
        }
    }

    /// Start the agent
    pub fn start(&mut self, max_turns: u32) {
        self.state = SubAgentState::Running { turn: 0, max_turns };
        self.started_at = Some(Utc::now());
    }

    /// Increment turn counter
    pub fn next_turn(&mut self) -> bool {
        if let SubAgentState::Running { turn, max_turns } = &mut self.state {
            *turn += 1;
            *turn <= *max_turns
        } else {
            false
        }
    }

    /// Complete the agent successfully
    pub fn complete(&mut self, summary: impl Into<String>) {
        let turns_used = match &self.state {
            SubAgentState::Running { turn, .. } => *turn,
            _ => 0,
        };
        self.state = SubAgentState::Completed {
            summary: summary.into(),
            turns_used,
        };
        self.completed_at = Some(Utc::now());
    }

    /// Fail the agent
    pub fn fail(&mut self, error: impl Into<String>) {
        let at_turn = match &self.state {
            SubAgentState::Running { turn, .. } => *turn,
            _ => 0,
        };
        self.state = SubAgentState::Failed {
            error: error.into(),
            at_turn,
        };
        self.completed_at = Some(Utc::now());
    }

    /// Cancel the agent
    pub fn cancel(&mut self, reason: Option<String>) {
        self.state = SubAgentState::Cancelled { reason };
        self.completed_at = Some(Utc::now());
    }

    /// Pause the agent
    pub fn pause(&mut self, reason: Option<String>) {
        let at_turn = match &self.state {
            SubAgentState::Running { turn, .. } => *turn,
            _ => 0,
        };
        self.state = SubAgentState::Paused { at_turn, reason };
    }

    /// Get duration since started
    pub fn duration(&self) -> Option<std::time::Duration> {
        let start = self.started_at?;
        let end = self.completed_at.unwrap_or_else(Utc::now);
        Some((end - start).to_std().unwrap_or_default())
    }

    /// Check if this agent is running in background
    pub fn is_background(&self) -> bool {
        self.config.run_in_background
    }

    /// Set the output file path
    pub fn set_output_file(&mut self, path: PathBuf) {
        self.output_file = Some(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subagent::config::SubAgentConfig;

    #[test]
    fn test_subagent_type_tools() {
        let explore = SubAgentType::Explore;
        assert!(explore.default_allowed_tools().contains(&"read".into()));
        assert!(explore.default_disallowed_tools().contains(&"write".into()));

        let general = SubAgentType::General;
        assert!(general.default_allowed_tools().contains(&"write".into()));
        assert!(general.default_disallowed_tools().is_empty());
    }

    #[test]
    fn test_subagent_lifecycle() {
        let config = SubAgentConfig::for_type(SubAgentType::Explore);
        let mut agent = SubAgent::new("session-1", config, "Find API endpoints", "API analysis");

        assert!(matches!(agent.state, SubAgentState::Created));

        agent.start(10);
        assert!(matches!(agent.state, SubAgentState::Running { .. }));

        agent.next_turn();
        if let SubAgentState::Running { turn, .. } = agent.state {
            assert_eq!(turn, 1);
        }

        agent.complete("Found 5 API endpoints");
        assert!(matches!(agent.state, SubAgentState::Completed { .. }));
    }

    #[test]
    fn test_subagent_id() {
        let id = SubAgentId::new();
        let id_str = id.0.to_string();
        let parsed = SubAgentId::from_string(&id_str).unwrap();
        assert_eq!(id, parsed);
    }
}
