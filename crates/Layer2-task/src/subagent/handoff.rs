//! Handoff System - Session-to-session context transfer
//!
//! Based on Amp's Handoff concept, this module provides clean session transitions
//! to avoid the "summaries of summaries" problem that degrades performance.
//!
//! ## Key Concepts
//! - Handoff Summary: Structured data passed to new session
//! - Handoff Trigger: Automatic detection of when handoff is needed
//! - Handoff Execution: Clean transition to new session
//!
//! ## Usage
//! ```ignore
//! let handoff_mgr = HandoffManager::new();
//!
//! // Check if handoff is recommended
//! if handoff_mgr.should_handoff(&context) {
//!     let summary = handoff_mgr.create_handoff(&context);
//!     let new_session = handoff_mgr.execute_handoff(summary)?;
//! }
//! ```

use super::context::{
    MessageRole, PreRotLevel, SubAgentContext,
    SummaryDecision, SummaryFact,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Handoff data package - everything needed to start a new session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandoffPackage {
    /// Unique handoff ID
    pub id: Uuid,

    /// Source session ID
    pub source_session_id: String,

    /// When the handoff was created
    pub created_at: DateTime<Utc>,

    /// Reason for handoff
    pub reason: HandoffReason,

    /// Current task/goal
    pub current_task: Option<String>,

    /// Task progress (0-100)
    pub progress_percent: Option<u8>,

    /// Key decisions made
    pub decisions: Vec<SummaryDecision>,

    /// Important facts/discoveries
    pub facts: Vec<SummaryFact>,

    /// Files being worked on
    pub active_files: Vec<String>,

    /// Recent modifications
    pub recent_changes: Vec<FileChange>,

    /// Open questions/blockers
    pub open_questions: Vec<String>,

    /// Structured context summary
    pub context_summary: String,

    /// Key code snippets to preserve
    pub code_snippets: Vec<CodeSnippet>,

    /// Environment context (working directory, etc.)
    pub environment: EnvironmentContext,

    /// Token count of this package
    pub token_count: usize,

    /// Quality metrics from source session
    pub source_quality: QualityMetrics,
}

impl HandoffPackage {
    /// Create a new handoff package
    pub fn new(source_session_id: impl Into<String>, reason: HandoffReason) -> Self {
        Self {
            id: Uuid::new_v4(),
            source_session_id: source_session_id.into(),
            created_at: Utc::now(),
            reason,
            current_task: None,
            progress_percent: None,
            decisions: Vec::new(),
            facts: Vec::new(),
            active_files: Vec::new(),
            recent_changes: Vec::new(),
            open_questions: Vec::new(),
            context_summary: String::new(),
            code_snippets: Vec::new(),
            environment: EnvironmentContext::default(),
            token_count: 0,
            source_quality: QualityMetrics::default(),
        }
    }

    /// Set the current task
    pub fn with_task(mut self, task: impl Into<String>) -> Self {
        self.current_task = Some(task.into());
        self
    }

    /// Add a decision
    pub fn add_decision(&mut self, decision: SummaryDecision) {
        self.decisions.push(decision);
    }

    /// Add a fact
    pub fn add_fact(&mut self, fact: SummaryFact) {
        self.facts.push(fact);
    }

    /// Add an active file
    pub fn add_active_file(&mut self, path: impl Into<String>) {
        let path = path.into();
        if !self.active_files.contains(&path) {
            self.active_files.push(path);
        }
    }

    /// Add a file change
    pub fn add_change(&mut self, change: FileChange) {
        self.recent_changes.push(change);
    }

    /// Add a code snippet
    pub fn add_snippet(&mut self, snippet: CodeSnippet) {
        self.code_snippets.push(snippet);
    }

    /// Convert to injection prompt for new session
    pub fn to_injection_prompt(&self) -> String {
        let mut prompt = String::new();

        prompt.push_str("# Handoff from Previous Session\n\n");
        prompt.push_str(&format!(
            "This is a continuation of session `{}` (handoff reason: {:?}).\n\n",
            &self.source_session_id[..8.min(self.source_session_id.len())],
            self.reason
        ));

        // Current task
        if let Some(task) = &self.current_task {
            prompt.push_str(&format!("## Current Task\n{}\n\n", task));
            if let Some(progress) = self.progress_percent {
                prompt.push_str(&format!("Progress: {}%\n\n", progress));
            }
        }

        // Key decisions
        if !self.decisions.is_empty() {
            prompt.push_str("## Key Decisions Made\n");
            for d in &self.decisions {
                prompt.push_str(&format!("- **{}**: {}\n", d.topic, d.decision));
                if let Some(reason) = &d.reason {
                    prompt.push_str(&format!("  - Reason: {}\n", reason));
                }
            }
            prompt.push('\n');
        }

        // Important facts
        if !self.facts.is_empty() {
            prompt.push_str("## Important Facts\n");
            for f in &self.facts {
                prompt.push_str(&format!("- [{}] {}\n", f.category, f.content));
            }
            prompt.push('\n');
        }

        // Active files
        if !self.active_files.is_empty() {
            prompt.push_str("## Files Being Worked On\n");
            for file in &self.active_files {
                prompt.push_str(&format!("- `{}`\n", file));
            }
            prompt.push('\n');
        }

        // Recent changes
        if !self.recent_changes.is_empty() {
            prompt.push_str("## Recent Changes\n");
            for change in &self.recent_changes {
                prompt.push_str(&format!(
                    "- `{}`: {} ({})\n",
                    change.path,
                    change.description,
                    match change.change_type {
                        ChangeType::Created => "created",
                        ChangeType::Modified => "modified",
                        ChangeType::Deleted => "deleted",
                    }
                ));
            }
            prompt.push('\n');
        }

        // Code snippets
        if !self.code_snippets.is_empty() {
            prompt.push_str("## Key Code Snippets\n");
            for snippet in &self.code_snippets {
                prompt.push_str(&format!("### {} ({})\n", snippet.description, snippet.file));
                prompt.push_str(&format!(
                    "```{}\n{}\n```\n\n",
                    snippet.language, snippet.code
                ));
            }
        }

        // Open questions
        if !self.open_questions.is_empty() {
            prompt.push_str("## Open Questions/Blockers\n");
            for q in &self.open_questions {
                prompt.push_str(&format!("- {}\n", q));
            }
            prompt.push('\n');
        }

        // Context summary
        if !self.context_summary.is_empty() {
            prompt.push_str("## Additional Context\n");
            prompt.push_str(&self.context_summary);
            prompt.push('\n');
        }

        prompt
    }

    /// Estimate token count
    pub fn estimate_tokens(&mut self) {
        let content = self.to_injection_prompt();
        self.token_count = content.len() / 4; // rough estimate
    }
}

/// Reason for handoff
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandoffReason {
    /// Pre-rot threshold reached
    PreRotThreshold,
    /// Context window full
    ContextFull,
    /// User requested
    UserRequested,
    /// Task completed
    TaskCompleted,
    /// Error recovery
    ErrorRecovery,
    /// Scheduled (time-based)
    Scheduled,
    /// Quality degradation detected
    QualityDegraded,
}

/// File change record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    /// File path
    pub path: String,
    /// Change type
    pub change_type: ChangeType,
    /// Brief description
    pub description: String,
    /// Lines affected (approximate)
    pub lines_affected: Option<usize>,
}

impl FileChange {
    pub fn new(
        path: impl Into<String>,
        change_type: ChangeType,
        description: impl Into<String>,
    ) -> Self {
        Self {
            path: path.into(),
            change_type,
            description: description.into(),
            lines_affected: None,
        }
    }
}

/// Change type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeType {
    Created,
    Modified,
    Deleted,
}

/// Code snippet for handoff
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSnippet {
    /// Source file
    pub file: String,
    /// Language
    pub language: String,
    /// The code
    pub code: String,
    /// Description/purpose
    pub description: String,
    /// Line range (start, end)
    pub line_range: Option<(usize, usize)>,
}

impl CodeSnippet {
    pub fn new(
        file: impl Into<String>,
        language: impl Into<String>,
        code: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            file: file.into(),
            language: language.into(),
            code: code.into(),
            description: description.into(),
            line_range: None,
        }
    }

    pub fn with_line_range(mut self, start: usize, end: usize) -> Self {
        self.line_range = Some((start, end));
        self
    }
}

/// Environment context
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EnvironmentContext {
    /// Working directory
    pub working_directory: Option<String>,
    /// Git branch
    pub git_branch: Option<String>,
    /// Git status summary
    pub git_status: Option<String>,
    /// Active tools/MCP servers
    pub active_tools: Vec<String>,
    /// Environment variables (relevant ones)
    pub env_vars: HashMap<String, String>,
}

/// Quality metrics from source session
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QualityMetrics {
    /// Context usage percentage at handoff
    pub context_usage_percent: f32,
    /// Estimated quality score
    pub quality_score: f32,
    /// Number of compressions performed
    pub compression_count: usize,
    /// Total messages processed
    pub total_messages: usize,
    /// Successful tool calls
    pub successful_tool_calls: usize,
    /// Failed tool calls
    pub failed_tool_calls: usize,
}

/// Handoff trigger configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandoffTriggerConfig {
    /// Enable automatic handoff
    pub auto_handoff: bool,
    /// Pre-rot level to trigger handoff
    pub trigger_level: PreRotLevel,
    /// Minimum messages before considering handoff
    pub min_messages: usize,
    /// Maximum time before forced handoff (seconds)
    pub max_session_time_secs: Option<u64>,
    /// Trigger on quality drop below threshold
    pub quality_threshold: f32,
}

impl Default for HandoffTriggerConfig {
    fn default() -> Self {
        Self {
            auto_handoff: true,
            trigger_level: PreRotLevel::Critical,
            min_messages: 10,
            max_session_time_secs: None,
            quality_threshold: 0.6,
        }
    }
}

/// Handoff manager
pub struct HandoffManager {
    /// Configuration
    config: HandoffTriggerConfig,
    /// Handoff history
    history: Vec<HandoffRecord>,
}

/// Handoff record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandoffRecord {
    /// Handoff ID
    pub id: Uuid,
    /// Source session
    pub source_session: String,
    /// Target session (if known)
    pub target_session: Option<String>,
    /// Reason
    pub reason: HandoffReason,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
    /// Package token count
    pub token_count: usize,
}

impl HandoffManager {
    /// Create a new handoff manager
    pub fn new() -> Self {
        Self {
            config: HandoffTriggerConfig::default(),
            history: Vec::new(),
        }
    }

    /// Create with custom config
    pub fn with_config(config: HandoffTriggerConfig) -> Self {
        Self {
            config,
            history: Vec::new(),
        }
    }

    /// Check if handoff should be triggered
    pub fn should_handoff(&self, context: &SubAgentContext) -> bool {
        if !self.config.auto_handoff {
            return false;
        }

        // Check minimum messages
        if context.message_count() < self.config.min_messages {
            return false;
        }

        // Check pre-rot status
        let pre_rot = context.pre_rot_status();
        let trigger = match self.config.trigger_level {
            PreRotLevel::Warning => matches!(
                pre_rot.level,
                PreRotLevel::Warning
                    | PreRotLevel::Critical
                    | PreRotLevel::Degraded
                    | PreRotLevel::Full
            ),
            PreRotLevel::Critical => matches!(
                pre_rot.level,
                PreRotLevel::Critical | PreRotLevel::Degraded | PreRotLevel::Full
            ),
            PreRotLevel::Degraded => {
                matches!(pre_rot.level, PreRotLevel::Degraded | PreRotLevel::Full)
            }
            PreRotLevel::Full => matches!(pre_rot.level, PreRotLevel::Full),
            PreRotLevel::Healthy => false,
        };

        if trigger {
            return true;
        }

        // Check quality threshold
        if pre_rot.estimated_quality < self.config.quality_threshold {
            return true;
        }

        false
    }

    /// Get handoff recommendation
    pub fn get_recommendation(&self, context: &SubAgentContext) -> HandoffRecommendation {
        let pre_rot = context.pre_rot_status();

        if !self.config.auto_handoff {
            return HandoffRecommendation {
                should_handoff: false,
                urgency: HandoffUrgency::None,
                reason: None,
                estimated_quality_loss: 0.0,
            };
        }

        let (should_handoff, urgency, reason) = if pre_rot.level == PreRotLevel::Full {
            (
                true,
                HandoffUrgency::Immediate,
                Some("Context is full".to_string()),
            )
        } else if pre_rot.level == PreRotLevel::Degraded {
            (
                true,
                HandoffUrgency::High,
                Some("Quality degradation detected".to_string()),
            )
        } else if pre_rot.level == PreRotLevel::Critical {
            (
                true,
                HandoffUrgency::Medium,
                Some("Approaching context limit".to_string()),
            )
        } else if pre_rot.level == PreRotLevel::Warning {
            (
                false,
                HandoffUrgency::Low,
                Some("Consider preparing for handoff".to_string()),
            )
        } else {
            (false, HandoffUrgency::None, None)
        };

        // Estimate quality loss if no handoff
        let quality_loss = if should_handoff {
            (1.0 - pre_rot.estimated_quality) * 0.5
        } else {
            0.0
        };

        HandoffRecommendation {
            should_handoff,
            urgency,
            reason,
            estimated_quality_loss: quality_loss,
        }
    }

    /// Create a handoff package from context
    pub fn create_handoff(
        &self,
        context: &SubAgentContext,
        session_id: &str,
        reason: HandoffReason,
    ) -> HandoffPackage {
        let mut package = HandoffPackage::new(session_id, reason);

        // Extract from structured summary
        if let Some(summary) = &context.structured_summary {
            package.current_task = summary.current_task.clone();
            package.progress_percent = summary.progress_percent;

            for decision in &summary.decisions {
                package.add_decision(decision.clone());
            }

            for fact in &summary.facts {
                package.add_fact(fact.clone());
            }

            for file in &summary.files_touched {
                package.add_active_file(&file.path);
            }
        }

        // Extract from discoveries
        for discovery in &context.discoveries {
            package.add_fact(SummaryFact::new(&discovery.category, &discovery.content));
        }

        // Build context summary from recent messages
        let recent_messages: Vec<_> = context.messages.iter().rev().take(5).rev().collect();
        let mut summary = String::new();
        for msg in recent_messages {
            let role = match msg.role {
                MessageRole::User => "User",
                MessageRole::Assistant => "Assistant",
                MessageRole::System => continue,
                MessageRole::Tool => "Tool",
            };
            summary.push_str(&format!("{}: {}\n", role, truncate(&msg.content, 200)));
        }
        package.context_summary = summary;

        // Quality metrics
        let pre_rot = context.pre_rot_status();
        let window_status = context.window_status();
        package.source_quality = QualityMetrics {
            context_usage_percent: window_status.usage_percent,
            quality_score: pre_rot.estimated_quality,
            compression_count: context.compression_stats.total_compressions,
            total_messages: context.message_count(),
            successful_tool_calls: context.tool_results.iter().filter(|r| r.success).count(),
            failed_tool_calls: context.tool_results.iter().filter(|r| !r.success).count(),
        };

        package.estimate_tokens();
        package
    }

    /// Record a handoff
    pub fn record_handoff(&mut self, package: &HandoffPackage, target_session: Option<String>) {
        self.history.push(HandoffRecord {
            id: package.id,
            source_session: package.source_session_id.clone(),
            target_session,
            reason: package.reason,
            timestamp: Utc::now(),
            token_count: package.token_count,
        });
    }

    /// Get handoff history
    pub fn history(&self) -> &[HandoffRecord] {
        &self.history
    }

    /// Get stats
    pub fn stats(&self) -> HandoffStats {
        let total = self.history.len();
        let by_reason: HashMap<String, usize> =
            self.history.iter().fold(HashMap::new(), |mut acc, r| {
                *acc.entry(format!("{:?}", r.reason)).or_default() += 1;
                acc
            });
        let avg_tokens = if total > 0 {
            self.history.iter().map(|r| r.token_count).sum::<usize>() / total
        } else {
            0
        };

        HandoffStats {
            total_handoffs: total,
            by_reason,
            average_package_tokens: avg_tokens,
        }
    }
}

impl Default for HandoffManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Handoff recommendation
#[derive(Debug, Clone)]
pub struct HandoffRecommendation {
    /// Should handoff now
    pub should_handoff: bool,
    /// Urgency level
    pub urgency: HandoffUrgency,
    /// Reason
    pub reason: Option<String>,
    /// Estimated quality loss if no handoff
    pub estimated_quality_loss: f32,
}

/// Handoff urgency
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandoffUrgency {
    None,
    Low,
    Medium,
    High,
    Immediate,
}

/// Handoff statistics
#[derive(Debug, Clone)]
pub struct HandoffStats {
    /// Total handoffs performed
    pub total_handoffs: usize,
    /// Handoffs by reason
    pub by_reason: HashMap<String, usize>,
    /// Average package token count
    pub average_package_tokens: usize,
}

/// Truncate string
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handoff_package() {
        let mut package = HandoffPackage::new("session-123", HandoffReason::PreRotThreshold)
            .with_task("Implement feature X");

        package.add_active_file("src/main.rs");
        package.add_change(FileChange::new(
            "src/lib.rs",
            ChangeType::Modified,
            "Added new function",
        ));

        let prompt = package.to_injection_prompt();
        assert!(prompt.contains("feature X"));
        assert!(prompt.contains("src/main.rs"));
    }

    #[test]
    fn test_handoff_manager() {
        let manager = HandoffManager::new();
        let context = SubAgentContext::new();

        // New context shouldn't trigger handoff
        assert!(!manager.should_handoff(&context));
    }

    #[test]
    fn test_recommendation() {
        let manager = HandoffManager::new();
        let context = SubAgentContext::new();

        let rec = manager.get_recommendation(&context);
        assert!(!rec.should_handoff);
        assert_eq!(rec.urgency, HandoffUrgency::None);
    }
}
