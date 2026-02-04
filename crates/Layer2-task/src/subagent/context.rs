//! Sub-agent context management
//!
//! Each sub-agent maintains its own isolated context including:
//! - Message history with token tracking
//! - Tool results
//! - Discovered knowledge
//! - Context window management (truncation, summarization)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Default context window size (tokens)
const DEFAULT_CONTEXT_WINDOW: usize = 128_000;

/// Reserved tokens for response
const RESERVED_FOR_RESPONSE: usize = 4_000;

/// Minimum tokens to keep after truncation
const MIN_PRESERVED_TOKENS: usize = 8_000;

/// Unique identifier for a discovery
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DiscoveryId(pub Uuid);

impl DiscoveryId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for DiscoveryId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for DiscoveryId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.0.to_string()[..8])
    }
}

/// A discovered piece of knowledge from exploration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Discovery {
    /// Unique identifier
    pub id: DiscoveryId,

    /// Category of discovery (e.g., "file_structure", "api_endpoint", "pattern")
    pub category: String,

    /// The actual content/knowledge
    pub content: String,

    /// Source file or location (if applicable)
    pub source: Option<String>,

    /// Additional metadata
    pub metadata: HashMap<String, String>,

    /// When this was discovered
    pub created_at: DateTime<Utc>,

    /// Confidence score (0.0 - 1.0)
    pub confidence: f32,
}

impl Discovery {
    /// Create a new discovery
    pub fn new(category: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            id: DiscoveryId::new(),
            category: category.into(),
            content: content.into(),
            source: None,
            metadata: HashMap::new(),
            created_at: Utc::now(),
            confidence: 1.0,
        }
    }

    /// Add source
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Set confidence
    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    /// Estimate token count
    pub fn token_count(&self) -> usize {
        estimate_tokens(&self.content) + estimate_tokens(&self.category)
    }
}

/// Message in sub-agent context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextMessage {
    /// Message role
    pub role: MessageRole,

    /// Message content
    pub content: String,

    /// When the message was created
    pub created_at: DateTime<Utc>,

    /// Token count (estimated)
    pub token_count: usize,

    /// Whether this message is summarized
    pub is_summarized: bool,
}

/// Message role
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

impl ContextMessage {
    /// Create a system message
    pub fn system(content: impl Into<String>) -> Self {
        let content = content.into();
        let tokens = estimate_tokens(&content);
        Self {
            role: MessageRole::System,
            content,
            created_at: Utc::now(),
            token_count: tokens,
            is_summarized: false,
        }
    }

    /// Create a user message
    pub fn user(content: impl Into<String>) -> Self {
        let content = content.into();
        let tokens = estimate_tokens(&content);
        Self {
            role: MessageRole::User,
            content,
            created_at: Utc::now(),
            token_count: tokens,
            is_summarized: false,
        }
    }

    /// Create an assistant message
    pub fn assistant(content: impl Into<String>) -> Self {
        let content = content.into();
        let tokens = estimate_tokens(&content);
        Self {
            role: MessageRole::Assistant,
            content,
            created_at: Utc::now(),
            token_count: tokens,
            is_summarized: false,
        }
    }

    /// Create a tool result message
    pub fn tool(content: impl Into<String>) -> Self {
        let content = content.into();
        let tokens = estimate_tokens(&content);
        Self {
            role: MessageRole::Tool,
            content,
            created_at: Utc::now(),
            token_count: tokens,
            is_summarized: false,
        }
    }

    /// Mark as summarized
    pub fn summarized(mut self) -> Self {
        self.is_summarized = true;
        self
    }
}

/// Tool execution result stored in context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextToolResult {
    /// Tool name
    pub tool_name: String,

    /// Tool call ID
    pub call_id: String,

    /// Input parameters
    pub input: serde_json::Value,

    /// Output
    pub output: String,

    /// Whether it succeeded
    pub success: bool,

    /// Execution duration in ms
    pub duration_ms: u64,

    /// When executed
    pub executed_at: DateTime<Utc>,

    /// Token count
    pub token_count: usize,
}

impl ContextToolResult {
    /// Estimate token count
    pub fn new(
        tool_name: impl Into<String>,
        call_id: impl Into<String>,
        input: serde_json::Value,
        output: impl Into<String>,
        success: bool,
        duration_ms: u64,
    ) -> Self {
        let output = output.into();
        let input_str = serde_json::to_string(&input).unwrap_or_default();
        let tokens = estimate_tokens(&output) + estimate_tokens(&input_str);

        Self {
            tool_name: tool_name.into(),
            call_id: call_id.into(),
            input,
            output,
            success,
            duration_ms,
            executed_at: Utc::now(),
            token_count: tokens,
        }
    }
}

/// Context window configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextWindowConfig {
    /// Maximum context window size (tokens)
    pub max_tokens: usize,

    /// Reserved tokens for response
    pub reserved_for_response: usize,

    /// Minimum tokens to preserve after truncation
    pub min_preserved: usize,

    /// Whether to auto-summarize old messages
    pub auto_summarize: bool,

    /// Threshold to trigger summarization (percentage of max_tokens)
    pub summarize_threshold: f32,

    /// Pre-rot threshold configuration
    pub pre_rot: PreRotConfig,
}

/// Pre-Rot Threshold Configuration
///
/// Based on Amp's research, context quality degrades before reaching the limit.
/// This system proactively manages context to maintain quality.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreRotConfig {
    /// Enable pre-rot detection
    pub enabled: bool,

    /// Warning threshold (percentage) - recommend handoff
    /// Default: 25% (based on Amp's research)
    pub warning_threshold: f32,

    /// Critical threshold (percentage) - force action
    /// Default: 50%
    pub critical_threshold: f32,

    /// Degradation threshold (percentage) - quality noticeably affected
    /// Default: 75%
    pub degradation_threshold: f32,

    /// Action to take when warning threshold reached
    pub warning_action: PreRotAction,

    /// Action to take when critical threshold reached
    pub critical_action: PreRotAction,

    /// Track message coherence (experimental)
    pub track_coherence: bool,

    /// Coherence drop threshold (0.0-1.0)
    pub coherence_threshold: f32,
}

/// Pre-Rot action to take
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreRotAction {
    /// No action, just log
    None,
    /// Log a warning
    Warn,
    /// Suggest handoff to new session
    SuggestHandoff,
    /// Trigger compression
    Compress,
    /// Force handoff
    ForceHandoff,
    /// Notify callback
    Callback,
}

impl Default for PreRotConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            warning_threshold: 0.25,     // 25% - based on Amp's research
            critical_threshold: 0.50,    // 50%
            degradation_threshold: 0.75, // 75%
            warning_action: PreRotAction::Warn,
            critical_action: PreRotAction::SuggestHandoff,
            track_coherence: false, // Experimental
            coherence_threshold: 0.7,
        }
    }
}

impl PreRotConfig {
    /// Create a conservative config (earlier warnings)
    pub fn conservative() -> Self {
        Self {
            warning_threshold: 0.20,
            critical_threshold: 0.40,
            degradation_threshold: 0.60,
            ..Default::default()
        }
    }

    /// Create an aggressive config (later warnings, more context usage)
    pub fn aggressive() -> Self {
        Self {
            warning_threshold: 0.40,
            critical_threshold: 0.65,
            degradation_threshold: 0.85,
            ..Default::default()
        }
    }

    /// Disable pre-rot detection
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Default::default()
        }
    }
}

/// Pre-Rot status for the context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreRotStatus {
    /// Current usage percentage
    pub usage_percent: f32,

    /// Current pre-rot level
    pub level: PreRotLevel,

    /// Estimated quality score (0.0-1.0)
    pub estimated_quality: f32,

    /// Recommended action
    pub recommended_action: PreRotAction,

    /// Messages since last significant interaction
    pub messages_since_significant: usize,

    /// Whether handoff is recommended
    pub handoff_recommended: bool,

    /// Reason for handoff recommendation (if any)
    pub handoff_reason: Option<String>,
}

/// Pre-Rot severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreRotLevel {
    /// Everything is fine
    Healthy,
    /// Warning threshold reached
    Warning,
    /// Critical threshold reached
    Critical,
    /// Degradation threshold reached
    Degraded,
    /// Context is at capacity
    Full,
}

impl PreRotLevel {
    /// Get a human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            Self::Healthy => "Context is healthy",
            Self::Warning => "Consider preparing for handoff",
            Self::Critical => "Handoff recommended soon",
            Self::Degraded => "Quality may be affected",
            Self::Full => "Context at capacity",
        }
    }

    /// Get an icon/symbol
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Healthy => "🟢",
            Self::Warning => "🟡",
            Self::Critical => "🟠",
            Self::Degraded => "🔴",
            Self::Full => "⛔",
        }
    }
}

impl Default for ContextWindowConfig {
    fn default() -> Self {
        Self {
            max_tokens: DEFAULT_CONTEXT_WINDOW,
            reserved_for_response: RESERVED_FOR_RESPONSE,
            min_preserved: MIN_PRESERVED_TOKENS,
            auto_summarize: true,
            summarize_threshold: 0.8, // 80% triggers summarization
            pre_rot: PreRotConfig::default(),
        }
    }
}

impl ContextWindowConfig {
    /// Create config for a specific model
    pub fn for_model(model: &str) -> Self {
        let max_tokens = match model {
            m if m.contains("gpt-4") => 128_000,
            m if m.contains("gpt-3.5") => 16_000,
            m if m.contains("claude-3") => 200_000,
            m if m.contains("claude-2") => 100_000,
            m if m.contains("gemini") => 1_000_000,
            m if m.contains("haiku") => 200_000,
            m if m.contains("sonnet") => 200_000,
            m if m.contains("opus") => 200_000,
            _ => DEFAULT_CONTEXT_WINDOW,
        };

        Self {
            max_tokens,
            ..Default::default()
        }
    }

    /// Available tokens for content (excluding reserved)
    pub fn available_tokens(&self) -> usize {
        self.max_tokens.saturating_sub(self.reserved_for_response)
    }

    /// Summarization trigger threshold
    pub fn summarization_threshold(&self) -> usize {
        (self.available_tokens() as f32 * self.summarize_threshold) as usize
    }

    /// Pre-rot warning threshold in tokens
    pub fn pre_rot_warning_tokens(&self) -> usize {
        (self.available_tokens() as f32 * self.pre_rot.warning_threshold) as usize
    }

    /// Pre-rot critical threshold in tokens
    pub fn pre_rot_critical_tokens(&self) -> usize {
        (self.available_tokens() as f32 * self.pre_rot.critical_threshold) as usize
    }

    /// Pre-rot degradation threshold in tokens
    pub fn pre_rot_degradation_tokens(&self) -> usize {
        (self.available_tokens() as f32 * self.pre_rot.degradation_threshold) as usize
    }
}

/// Context window status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextWindowStatus {
    /// Total tokens used
    pub total_tokens: usize,

    /// Available tokens
    pub available_tokens: usize,

    /// Usage percentage
    pub usage_percent: f32,

    /// Whether context needs truncation
    pub needs_truncation: bool,

    /// Whether summarization is recommended
    pub needs_summarization: bool,

    /// Recoverable compression available
    pub has_recoverable_backup: bool,

    /// Tokens in backup storage
    pub backup_tokens: usize,
}

/// Compression checkpoint - stores original messages for recovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionCheckpoint {
    /// Checkpoint ID
    pub id: Uuid,

    /// When the checkpoint was created
    pub created_at: DateTime<Utc>,

    /// Original messages that were compressed
    pub original_messages: Vec<ContextMessage>,

    /// Original tool results
    pub original_tool_results: Vec<ContextToolResult>,

    /// Total tokens in this checkpoint
    pub token_count: usize,

    /// Summary that replaced these messages
    pub summary: String,

    /// Whether this checkpoint has been partially restored
    pub partial_restore: bool,
}

impl CompressionCheckpoint {
    /// Create a new checkpoint
    pub fn new(
        messages: Vec<ContextMessage>,
        tool_results: Vec<ContextToolResult>,
        summary: String,
    ) -> Self {
        let token_count = messages.iter().map(|m| m.token_count).sum::<usize>()
            + tool_results.iter().map(|r| r.token_count).sum::<usize>();

        Self {
            id: Uuid::new_v4(),
            created_at: Utc::now(),
            original_messages: messages,
            original_tool_results: tool_results,
            token_count,
            summary,
            partial_restore: false,
        }
    }

    /// Get message count
    pub fn message_count(&self) -> usize {
        self.original_messages.len()
    }

    /// Get tool result count
    pub fn tool_result_count(&self) -> usize {
        self.original_tool_results.len()
    }
}

/// Recoverable compression configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoverableCompressionConfig {
    /// Enable recoverable compression
    pub enabled: bool,

    /// Maximum checkpoints to keep
    pub max_checkpoints: usize,

    /// Maximum tokens to store in backup
    pub max_backup_tokens: usize,

    /// Auto-restore when context usage drops below threshold
    pub auto_restore_threshold: f32,

    /// Priority restoration - restore most recent first
    pub restore_recent_first: bool,
}

impl Default for RecoverableCompressionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_checkpoints: 5,
            max_backup_tokens: 100_000,
            auto_restore_threshold: 0.5, // Restore when below 50% usage
            restore_recent_first: true,
        }
    }
}

/// Compression statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompressionStats {
    /// Total compressions performed
    pub total_compressions: usize,

    /// Total messages compressed
    pub total_messages_compressed: usize,

    /// Total tokens compressed
    pub total_tokens_compressed: usize,

    /// Total restorations performed
    pub total_restorations: usize,

    /// Total messages restored
    pub total_messages_restored: usize,

    /// Total tokens restored
    pub total_tokens_restored: usize,
}

/// Structured summary for context compression
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredSummary {
    /// Summary ID
    pub id: Uuid,

    /// When created
    pub created_at: DateTime<Utc>,

    /// Task/goal being worked on
    pub current_task: Option<String>,

    /// Key decisions made
    pub decisions: Vec<SummaryDecision>,

    /// Important facts discovered
    pub facts: Vec<SummaryFact>,

    /// Files modified or read
    pub files_touched: Vec<SummaryFileRef>,

    /// Tools used and outcomes
    pub tool_usage: Vec<SummaryToolUsage>,

    /// Open questions/blockers
    pub open_questions: Vec<String>,

    /// Progress percentage (estimated)
    pub progress_percent: Option<u8>,

    /// Estimated tokens for this summary
    pub token_count: usize,
}

impl Default for StructuredSummary {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            created_at: Utc::now(),
            current_task: None,
            decisions: Vec::new(),
            facts: Vec::new(),
            files_touched: Vec::new(),
            tool_usage: Vec::new(),
            open_questions: Vec::new(),
            progress_percent: None,
            token_count: 0,
        }
    }
}

impl StructuredSummary {
    /// Create a new empty summary
    pub fn new() -> Self {
        Self::default()
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

    /// Add a file reference
    pub fn add_file(&mut self, file: SummaryFileRef) {
        // Avoid duplicates
        if !self.files_touched.iter().any(|f| f.path == file.path) {
            self.files_touched.push(file);
        }
    }

    /// Add tool usage
    pub fn add_tool_usage(&mut self, usage: SummaryToolUsage) {
        self.tool_usage.push(usage);
    }

    /// Convert to markdown format
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();

        md.push_str("## Context Summary\n\n");

        // Current task
        if let Some(task) = &self.current_task {
            md.push_str(&format!("**Task**: {}\n\n", task));
        }

        // Progress
        if let Some(progress) = self.progress_percent {
            md.push_str(&format!("**Progress**: {}%\n\n", progress));
        }

        // Key decisions
        if !self.decisions.is_empty() {
            md.push_str("### Decisions Made\n");
            for decision in &self.decisions {
                md.push_str(&format!(
                    "- **{}**: {}\n",
                    decision.topic, decision.decision
                ));
                if let Some(reason) = &decision.reason {
                    md.push_str(&format!("  - Reason: {}\n", reason));
                }
            }
            md.push('\n');
        }

        // Important facts
        if !self.facts.is_empty() {
            md.push_str("### Key Facts\n");
            for fact in &self.facts {
                md.push_str(&format!("- [{}] {}\n", fact.category, fact.content));
            }
            md.push('\n');
        }

        // Files touched
        if !self.files_touched.is_empty() {
            md.push_str("### Files\n");
            for file in &self.files_touched {
                let action = match file.action {
                    FileAction::Read => "read",
                    FileAction::Modified => "modified",
                    FileAction::Created => "created",
                    FileAction::Deleted => "deleted",
                };
                md.push_str(&format!("- `{}` ({})\n", file.path, action));
            }
            md.push('\n');
        }

        // Tool usage summary
        if !self.tool_usage.is_empty() {
            md.push_str("### Tool Usage\n");
            // Group by tool
            let mut by_tool: std::collections::HashMap<String, Vec<&SummaryToolUsage>> =
                std::collections::HashMap::new();
            for usage in &self.tool_usage {
                by_tool
                    .entry(usage.tool_name.clone())
                    .or_default()
                    .push(usage);
            }
            for (tool, usages) in by_tool {
                let success_count = usages.iter().filter(|u| u.success).count();
                md.push_str(&format!(
                    "- `{}`: {} calls ({} successful)\n",
                    tool,
                    usages.len(),
                    success_count
                ));
            }
            md.push('\n');
        }

        // Open questions
        if !self.open_questions.is_empty() {
            md.push_str("### Open Questions\n");
            for question in &self.open_questions {
                md.push_str(&format!("- {}\n", question));
            }
            md.push('\n');
        }

        md
    }

    /// Convert to compact format (fewer tokens)
    pub fn to_compact(&self) -> String {
        let mut parts = Vec::new();

        if let Some(task) = &self.current_task {
            parts.push(format!("Task: {}", task));
        }

        if !self.decisions.is_empty() {
            let decisions: Vec<_> = self
                .decisions
                .iter()
                .map(|d| format!("{}: {}", d.topic, d.decision))
                .collect();
            parts.push(format!("Decisions: {}", decisions.join("; ")));
        }

        if !self.facts.is_empty() {
            let facts: Vec<_> = self.facts.iter().map(|f| f.content.as_str()).collect();
            parts.push(format!("Facts: {}", facts.join("; ")));
        }

        if !self.files_touched.is_empty() {
            let files: Vec<_> = self.files_touched.iter().map(|f| f.path.as_str()).collect();
            parts.push(format!("Files: {}", files.join(", ")));
        }

        if !self.tool_usage.is_empty() {
            parts.push(format!("Tools: {} calls", self.tool_usage.len()));
        }

        parts.join(" | ")
    }

    /// Estimate token count
    pub fn estimate_tokens(&mut self) {
        let content = self.to_markdown();
        self.token_count = estimate_tokens(&content);
    }
}

/// A decision recorded in the summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryDecision {
    /// Topic of the decision
    pub topic: String,
    /// The decision made
    pub decision: String,
    /// Reason for the decision
    pub reason: Option<String>,
    /// Confidence level (0.0-1.0)
    pub confidence: f32,
}

impl SummaryDecision {
    pub fn new(topic: impl Into<String>, decision: impl Into<String>) -> Self {
        Self {
            topic: topic.into(),
            decision: decision.into(),
            reason: None,
            confidence: 1.0,
        }
    }

    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }
}

/// A fact recorded in the summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryFact {
    /// Category of the fact
    pub category: String,
    /// The fact content
    pub content: String,
    /// Source of the fact (file, tool, etc.)
    pub source: Option<String>,
    /// Importance level (1-5)
    pub importance: u8,
}

impl SummaryFact {
    pub fn new(category: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            category: category.into(),
            content: content.into(),
            source: None,
            importance: 3,
        }
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    pub fn with_importance(mut self, importance: u8) -> Self {
        self.importance = importance.clamp(1, 5);
        self
    }
}

/// A file reference in the summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryFileRef {
    /// File path
    pub path: String,
    /// Action performed
    pub action: FileAction,
    /// Brief note about what was done
    pub note: Option<String>,
}

impl SummaryFileRef {
    pub fn new(path: impl Into<String>, action: FileAction) -> Self {
        Self {
            path: path.into(),
            action,
            note: None,
        }
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }
}

/// File action type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileAction {
    Read,
    Modified,
    Created,
    Deleted,
}

/// Tool usage record in the summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryToolUsage {
    /// Tool name
    pub tool_name: String,
    /// Brief description of what was done
    pub description: String,
    /// Whether it succeeded
    pub success: bool,
    /// Key output (truncated)
    pub key_output: Option<String>,
}

impl SummaryToolUsage {
    pub fn new(
        tool_name: impl Into<String>,
        description: impl Into<String>,
        success: bool,
    ) -> Self {
        Self {
            tool_name: tool_name.into(),
            description: description.into(),
            success,
            key_output: None,
        }
    }

    pub fn with_output(mut self, output: impl Into<String>) -> Self {
        let output = output.into();
        self.key_output = Some(truncate_for_summary(&output, 100));
        self
    }
}

/// Sub-agent context - isolated conversation and knowledge
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgentContext {
    /// Message history
    pub messages: Vec<ContextMessage>,

    /// Tool execution results
    pub tool_results: Vec<ContextToolResult>,

    /// Discovered knowledge
    pub discoveries: Vec<Discovery>,

    /// Injected context from parent (if any)
    pub injected_context: Option<String>,

    /// Total token count
    pub total_tokens: usize,

    /// Context window configuration
    pub config: ContextWindowConfig,

    /// Summary of truncated messages (if any)
    pub truncated_summary: Option<String>,

    /// Number of messages truncated
    pub truncated_count: usize,

    /// Compression checkpoints for recovery
    #[serde(default)]
    pub compression_checkpoints: Vec<CompressionCheckpoint>,

    /// Recoverable compression configuration
    #[serde(default)]
    pub compression_config: RecoverableCompressionConfig,

    /// Compression statistics
    #[serde(default)]
    pub compression_stats: CompressionStats,

    /// Structured summary (current session)
    #[serde(default)]
    pub structured_summary: Option<StructuredSummary>,

    /// Historical structured summaries (from compressions)
    #[serde(default)]
    pub summary_history: Vec<StructuredSummary>,
}

impl Default for SubAgentContext {
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            tool_results: Vec::new(),
            discoveries: Vec::new(),
            injected_context: None,
            total_tokens: 0,
            config: ContextWindowConfig::default(),
            truncated_summary: None,
            truncated_count: 0,
            compression_checkpoints: Vec::new(),
            compression_config: RecoverableCompressionConfig::default(),
            compression_stats: CompressionStats::default(),
            structured_summary: None,
            summary_history: Vec::new(),
        }
    }
}

impl SubAgentContext {
    /// Create a new empty context
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with specific config
    pub fn with_config(config: ContextWindowConfig) -> Self {
        Self {
            config,
            ..Default::default()
        }
    }

    /// Create for a specific model
    pub fn for_model(model: &str) -> Self {
        Self {
            config: ContextWindowConfig::for_model(model),
            ..Default::default()
        }
    }

    /// Get context window status
    pub fn window_status(&self) -> ContextWindowStatus {
        let available = self.config.available_tokens();
        let usage_percent = self.total_tokens as f32 / available as f32 * 100.0;
        let backup_tokens: usize = self
            .compression_checkpoints
            .iter()
            .map(|c| c.token_count)
            .sum();

        ContextWindowStatus {
            total_tokens: self.total_tokens,
            available_tokens: available.saturating_sub(self.total_tokens),
            usage_percent,
            needs_truncation: self.total_tokens > available,
            needs_summarization: self.total_tokens > self.config.summarization_threshold(),
            has_recoverable_backup: !self.compression_checkpoints.is_empty(),
            backup_tokens,
        }
    }

    /// Check if context needs management
    pub fn needs_management(&self) -> bool {
        let status = self.window_status();
        status.needs_truncation || status.needs_summarization
    }

    /// Get pre-rot status
    pub fn pre_rot_status(&self) -> PreRotStatus {
        if !self.config.pre_rot.enabled {
            return PreRotStatus {
                usage_percent: self.window_status().usage_percent,
                level: PreRotLevel::Healthy,
                estimated_quality: 1.0,
                recommended_action: PreRotAction::None,
                messages_since_significant: 0,
                handoff_recommended: false,
                handoff_reason: None,
            };
        }

        let status = self.window_status();
        let usage_ratio = status.usage_percent / 100.0;

        // Determine level
        let level = if status.needs_truncation {
            PreRotLevel::Full
        } else if usage_ratio >= self.config.pre_rot.degradation_threshold {
            PreRotLevel::Degraded
        } else if usage_ratio >= self.config.pre_rot.critical_threshold {
            PreRotLevel::Critical
        } else if usage_ratio >= self.config.pre_rot.warning_threshold {
            PreRotLevel::Warning
        } else {
            PreRotLevel::Healthy
        };

        // Estimate quality based on usage
        // Quality decreases non-linearly as usage increases
        let estimated_quality = self.estimate_quality(usage_ratio);

        // Determine recommended action
        let recommended_action = match level {
            PreRotLevel::Healthy => PreRotAction::None,
            PreRotLevel::Warning => self.config.pre_rot.warning_action,
            PreRotLevel::Critical | PreRotLevel::Degraded => self.config.pre_rot.critical_action,
            PreRotLevel::Full => PreRotAction::ForceHandoff,
        };

        // Count messages since last "significant" interaction
        let messages_since_significant = self.count_messages_since_significant();

        // Determine if handoff is recommended
        let (handoff_recommended, handoff_reason) =
            self.should_recommend_handoff(level, usage_ratio);

        PreRotStatus {
            usage_percent: status.usage_percent,
            level,
            estimated_quality,
            recommended_action,
            messages_since_significant,
            handoff_recommended,
            handoff_reason,
        }
    }

    /// Estimate quality score based on context usage
    fn estimate_quality(&self, usage_ratio: f32) -> f32 {
        // Based on Amp's research: quality starts degrading around 25%
        // Using a sigmoid-like curve for quality estimation
        if usage_ratio < 0.25 {
            1.0
        } else if usage_ratio < 0.50 {
            // Linear decay from 1.0 to 0.85
            1.0 - (usage_ratio - 0.25) * 0.6
        } else if usage_ratio < 0.75 {
            // Faster decay from 0.85 to 0.6
            0.85 - (usage_ratio - 0.50) * 1.0
        } else {
            // Rapid decay from 0.6 to 0.3
            0.6 - (usage_ratio - 0.75) * 1.2
        }
        .max(0.1) // Never go below 0.1
    }

    /// Count messages since last significant interaction
    fn count_messages_since_significant(&self) -> usize {
        // "Significant" = longer messages or tool results
        let significant_threshold = 500; // tokens

        let mut count = 0;
        for msg in self.messages.iter().rev() {
            if msg.token_count >= significant_threshold {
                break;
            }
            count += 1;
        }
        count
    }

    /// Determine if handoff should be recommended
    fn should_recommend_handoff(
        &self,
        level: PreRotLevel,
        usage_ratio: f32,
    ) -> (bool, Option<String>) {
        match level {
            PreRotLevel::Healthy => (false, None),
            PreRotLevel::Warning => {
                if usage_ratio > 0.30 {
                    (
                        false,
                        Some("Consider preparing handoff summary".to_string()),
                    )
                } else {
                    (false, None)
                }
            }
            PreRotLevel::Critical => (
                true,
                Some(format!(
                    "Context at {:.0}% capacity, handoff recommended to maintain quality",
                    usage_ratio * 100.0
                )),
            ),
            PreRotLevel::Degraded => (
                true,
                Some(format!(
                    "Context at {:.0}% capacity, quality likely degraded",
                    usage_ratio * 100.0
                )),
            ),
            PreRotLevel::Full => (true, Some("Context full, handoff required".to_string())),
        }
    }

    /// Check if pre-rot warning threshold reached
    pub fn is_pre_rot_warning(&self) -> bool {
        if !self.config.pre_rot.enabled {
            return false;
        }
        self.total_tokens >= self.config.pre_rot_warning_tokens()
    }

    /// Check if pre-rot critical threshold reached
    pub fn is_pre_rot_critical(&self) -> bool {
        if !self.config.pre_rot.enabled {
            return false;
        }
        self.total_tokens >= self.config.pre_rot_critical_tokens()
    }

    /// Get a handoff summary for transitioning to new session
    pub fn generate_handoff_summary(&self) -> String {
        let mut summary = String::new();

        summary.push_str("# Session Handoff Summary\n\n");

        // Context status
        let status = self.window_status();
        let pre_rot = self.pre_rot_status();
        summary.push_str(&format!(
            "## Context Status\n- Usage: {:.1}%\n- Quality: {:.0}%\n- Level: {}\n\n",
            status.usage_percent,
            pre_rot.estimated_quality * 100.0,
            pre_rot.level.description()
        ));

        // Key discoveries
        if !self.discoveries.is_empty() {
            summary.push_str("## Key Discoveries\n");
            for category in self.discovery_categories().iter().take(5) {
                let discoveries = self.discoveries_by_category(category);
                summary.push_str(&format!("\n### {}\n", category));
                for d in discoveries.iter().take(3) {
                    summary.push_str(&format!("- {}\n", d.content));
                }
            }
            summary.push('\n');
        }

        // Recent context
        if let Some(truncated) = &self.truncated_summary {
            summary.push_str("## Earlier Context\n");
            summary.push_str(truncated);
            summary.push_str("\n\n");
        }

        // Current task context
        summary.push_str("## Recent Messages\n");
        for msg in self.messages.iter().rev().take(5).rev() {
            let role = match msg.role {
                MessageRole::User => "User",
                MessageRole::Assistant => "Assistant",
                MessageRole::System => "System",
                MessageRole::Tool => "Tool",
            };
            let content = truncate_for_summary(&msg.content, 200);
            summary.push_str(&format!("**{}**: {}\n", role, content));
        }

        summary
    }

    /// Add a message to the context
    pub fn add_message(&mut self, message: ContextMessage) {
        self.total_tokens += message.token_count;
        self.messages.push(message);

        // Auto-manage if needed
        if self.config.auto_summarize && self.needs_management() {
            self.manage_context();
        }
    }

    /// Add a tool result
    pub fn add_tool_result(&mut self, result: ContextToolResult) {
        self.total_tokens += result.token_count;
        self.tool_results.push(result);

        // Auto-manage if needed
        if self.config.auto_summarize && self.needs_management() {
            self.manage_context();
        }
    }

    /// Add a discovery
    pub fn add_discovery(&mut self, discovery: Discovery) {
        self.discoveries.push(discovery);
    }

    /// Manage context window (truncate or summarize)
    pub fn manage_context(&mut self) {
        let status = self.window_status();

        if !status.needs_truncation && !status.needs_summarization {
            // Check if we should auto-restore
            if self.compression_config.enabled
                && status.usage_percent < self.compression_config.auto_restore_threshold * 100.0
                && !self.compression_checkpoints.is_empty()
            {
                self.try_auto_restore();
            }
            return;
        }

        // Calculate how many tokens to remove
        let target_tokens = self.config.min_preserved;
        let to_remove = self.total_tokens.saturating_sub(target_tokens);

        if to_remove == 0 {
            return;
        }

        // Find messages to remove (oldest first, but keep system messages)
        let mut removed_tokens = 0;
        let mut messages_to_summarize = Vec::new();
        let mut tool_results_to_backup = Vec::new();
        let mut keep_indices = Vec::new();

        for (i, msg) in self.messages.iter().enumerate() {
            // Always keep system messages
            if msg.role == MessageRole::System {
                keep_indices.push(i);
                continue;
            }

            if removed_tokens < to_remove && !msg.is_summarized {
                messages_to_summarize.push(msg.clone());
                removed_tokens += msg.token_count;
            } else {
                keep_indices.push(i);
            }
        }

        // Also compress old tool results if needed
        if removed_tokens < to_remove && !self.tool_results.is_empty() {
            let tool_count = self.tool_results.len();
            let to_remove_count = tool_count / 2; // Remove half
            tool_results_to_backup = self.tool_results.drain(..to_remove_count).collect();
            removed_tokens += tool_results_to_backup
                .iter()
                .map(|r| r.token_count)
                .sum::<usize>();
        }

        // Create summary of removed messages
        if !messages_to_summarize.is_empty() || !tool_results_to_backup.is_empty() {
            let summary = self.create_summary(&messages_to_summarize);

            // Create recoverable checkpoint if enabled
            if self.compression_config.enabled {
                self.create_compression_checkpoint(
                    messages_to_summarize.clone(),
                    tool_results_to_backup,
                    summary.clone(),
                );
            }

            // Update truncated info
            self.truncated_count += messages_to_summarize.len();
            self.truncated_summary = Some(
                self.truncated_summary
                    .take()
                    .map(|s| format!("{}\n\n{}", s, summary))
                    .unwrap_or(summary),
            );
        }

        // Keep only non-removed messages
        let new_messages: Vec<_> = keep_indices
            .into_iter()
            .map(|i| self.messages[i].clone())
            .collect();

        // Recalculate tokens
        self.messages = new_messages;
        self.recalculate_tokens();
    }

    /// Create a compression checkpoint for recovery
    fn create_compression_checkpoint(
        &mut self,
        messages: Vec<ContextMessage>,
        tool_results: Vec<ContextToolResult>,
        summary: String,
    ) {
        let checkpoint =
            CompressionCheckpoint::new(messages.clone(), tool_results.clone(), summary);

        // Update stats
        self.compression_stats.total_compressions += 1;
        self.compression_stats.total_messages_compressed += checkpoint.message_count();
        self.compression_stats.total_tokens_compressed += checkpoint.token_count;

        // Add checkpoint
        self.compression_checkpoints.push(checkpoint);

        // Enforce max checkpoints
        while self.compression_checkpoints.len() > self.compression_config.max_checkpoints {
            self.compression_checkpoints.remove(0);
        }

        // Enforce max backup tokens
        let mut total_backup_tokens: usize = self
            .compression_checkpoints
            .iter()
            .map(|c| c.token_count)
            .sum();

        while total_backup_tokens > self.compression_config.max_backup_tokens
            && !self.compression_checkpoints.is_empty()
        {
            let removed = self.compression_checkpoints.remove(0);
            total_backup_tokens -= removed.token_count;
        }
    }

    /// Try to auto-restore compressed content
    fn try_auto_restore(&mut self) {
        let available = self.config.available_tokens();
        let can_restore = available.saturating_sub(self.total_tokens);

        if can_restore < 1000 {
            return; // Not enough room
        }

        // Find a checkpoint to restore
        let checkpoint_idx = if self.compression_config.restore_recent_first {
            self.compression_checkpoints.len().saturating_sub(1)
        } else {
            0
        };

        if checkpoint_idx >= self.compression_checkpoints.len() {
            return;
        }

        let checkpoint = &self.compression_checkpoints[checkpoint_idx];

        // Check if we can fit this checkpoint
        if checkpoint.token_count <= can_restore {
            self.restore_checkpoint(checkpoint_idx);
        }
    }

    /// Restore a specific checkpoint
    pub fn restore_checkpoint(&mut self, checkpoint_idx: usize) -> bool {
        if checkpoint_idx >= self.compression_checkpoints.len() {
            return false;
        }

        let checkpoint = self.compression_checkpoints.remove(checkpoint_idx);

        // Calculate counts before moving data out of checkpoint
        let checkpoint_message_count = checkpoint.message_count();
        let checkpoint_token_count = checkpoint.token_count;

        // Restore messages (insert at appropriate position based on timestamp)
        for msg in checkpoint.original_messages {
            // Find insertion point
            let insert_idx = self
                .messages
                .iter()
                .position(|m| m.created_at > msg.created_at)
                .unwrap_or(self.messages.len());
            self.messages.insert(insert_idx, msg);
        }

        // Restore tool results
        for result in checkpoint.original_tool_results {
            let insert_idx = self
                .tool_results
                .iter()
                .position(|r| r.executed_at > result.executed_at)
                .unwrap_or(self.tool_results.len());
            self.tool_results.insert(insert_idx, result);
        }

        // Update stats
        self.compression_stats.total_restorations += 1;
        self.compression_stats.total_messages_restored += checkpoint_message_count;
        self.compression_stats.total_tokens_restored += checkpoint_token_count;

        // Recalculate tokens
        self.recalculate_tokens();

        true
    }

    /// Restore all checkpoints (full recovery)
    pub fn restore_all_checkpoints(&mut self) -> usize {
        let checkpoint_count = self.compression_checkpoints.len();

        // Restore from oldest to newest
        while !self.compression_checkpoints.is_empty() {
            self.restore_checkpoint(0);
        }

        checkpoint_count
    }

    /// Get compression statistics
    pub fn compression_statistics(&self) -> &CompressionStats {
        &self.compression_stats
    }

    /// Check if recovery is available
    pub fn has_recoverable_content(&self) -> bool {
        !self.compression_checkpoints.is_empty()
    }

    /// Get recoverable token count
    pub fn recoverable_tokens(&self) -> usize {
        self.compression_checkpoints
            .iter()
            .map(|c| c.token_count)
            .sum()
    }

    /// Get checkpoint summaries
    pub fn checkpoint_summaries(&self) -> Vec<(Uuid, DateTime<Utc>, usize, usize)> {
        self.compression_checkpoints
            .iter()
            .map(|c| (c.id, c.created_at, c.message_count(), c.token_count))
            .collect()
    }

    /// Create a summary of messages
    fn create_summary(&self, messages: &[ContextMessage]) -> String {
        let mut summary = String::new();

        // Group by role
        let user_msgs: Vec<_> = messages
            .iter()
            .filter(|m| m.role == MessageRole::User)
            .collect();
        let assistant_msgs: Vec<_> = messages
            .iter()
            .filter(|m| m.role == MessageRole::Assistant)
            .collect();
        let tool_msgs: Vec<_> = messages
            .iter()
            .filter(|m| m.role == MessageRole::Tool)
            .collect();

        if !user_msgs.is_empty() {
            summary.push_str(&format!(
                "User asked about: {}\n",
                truncate_for_summary(
                    &user_msgs
                        .iter()
                        .map(|m| m.content.as_str())
                        .collect::<Vec<_>>()
                        .join("; "),
                    200
                )
            ));
        }

        if !assistant_msgs.is_empty() {
            summary.push_str(&format!(
                "Assistant responded with: {}\n",
                truncate_for_summary(
                    &assistant_msgs
                        .iter()
                        .map(|m| m.content.as_str())
                        .collect::<Vec<_>>()
                        .join("; "),
                    200
                )
            ));
        }

        if !tool_msgs.is_empty() {
            summary.push_str(&format!("Tools executed: {} calls\n", tool_msgs.len()));
        }

        summary
    }

    /// Create a structured summary from messages and tool results
    pub fn create_structured_summary(
        &self,
        messages: &[ContextMessage],
        tool_results: &[ContextToolResult],
    ) -> StructuredSummary {
        let mut summary = StructuredSummary::new();

        // Extract task from first user message
        if let Some(first_user) = messages.iter().find(|m| m.role == MessageRole::User) {
            let task = truncate_for_summary(&first_user.content, 100);
            summary.current_task = Some(task);
        }

        // Extract facts from assistant messages
        for msg in messages.iter().filter(|m| m.role == MessageRole::Assistant) {
            // Look for statements that look like facts
            for line in msg.content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with('-') || trimmed.starts_with('*') {
                    let content = trimmed
                        .trim_start_matches('-')
                        .trim_start_matches('*')
                        .trim();
                    if !content.is_empty() && content.len() < 200 {
                        summary.add_fact(SummaryFact::new("finding", content));
                    }
                }
            }
        }

        // Extract file references from tool results
        for result in tool_results {
            match result.tool_name.as_str() {
                "read" => {
                    if let Some(path) = result.input.get("path").and_then(|v| v.as_str()) {
                        summary.add_file(SummaryFileRef::new(path, FileAction::Read));
                    }
                }
                "write" => {
                    if let Some(path) = result.input.get("path").and_then(|v| v.as_str()) {
                        summary.add_file(SummaryFileRef::new(path, FileAction::Created));
                    }
                }
                "edit" => {
                    if let Some(path) = result.input.get("path").and_then(|v| v.as_str()) {
                        summary.add_file(SummaryFileRef::new(path, FileAction::Modified));
                    }
                }
                _ => {}
            }

            // Add tool usage
            let description = if let Some(path) = result.input.get("path").and_then(|v| v.as_str())
            {
                format!("{} on {}", result.tool_name, path)
            } else {
                result.tool_name.clone()
            };
            summary.add_tool_usage(SummaryToolUsage::new(
                &result.tool_name,
                description,
                result.success,
            ));
        }

        // Limit facts to most important
        if summary.facts.len() > 10 {
            summary.facts.truncate(10);
        }

        summary.estimate_tokens();
        summary
    }

    /// Get or create the current structured summary
    pub fn get_or_create_structured_summary(&mut self) -> &mut StructuredSummary {
        if self.structured_summary.is_none() {
            self.structured_summary = Some(StructuredSummary::new());
        }
        self.structured_summary.as_mut().unwrap()
    }

    /// Set the current task in the structured summary
    pub fn set_current_task(&mut self, task: impl Into<String>) {
        self.get_or_create_structured_summary().current_task = Some(task.into());
    }

    /// Add a decision to the structured summary
    pub fn record_decision(&mut self, topic: impl Into<String>, decision: impl Into<String>) {
        let decision = SummaryDecision::new(topic, decision);
        self.get_or_create_structured_summary()
            .add_decision(decision);
    }

    /// Add a fact to the structured summary
    pub fn record_fact(&mut self, category: impl Into<String>, content: impl Into<String>) {
        let fact = SummaryFact::new(category, content);
        self.get_or_create_structured_summary().add_fact(fact);
    }

    /// Record file access
    pub fn record_file_access(&mut self, path: impl Into<String>, action: FileAction) {
        let file_ref = SummaryFileRef::new(path, action);
        self.get_or_create_structured_summary().add_file(file_ref);
    }

    /// Get the full structured summary (current + history)
    pub fn get_full_structured_summary(&self) -> String {
        let mut result = String::new();

        // Historical summaries
        if !self.summary_history.is_empty() {
            result.push_str("## Previous Context\n\n");
            for (i, summary) in self.summary_history.iter().enumerate() {
                result.push_str(&format!("### Session {}\n", i + 1));
                result.push_str(&summary.to_compact());
                result.push_str("\n\n");
            }
        }

        // Current summary
        if let Some(summary) = &self.structured_summary {
            result.push_str(&summary.to_markdown());
        }

        result
    }

    /// Archive current summary to history (during compression)
    pub fn archive_current_summary(&mut self) {
        if let Some(mut summary) = self.structured_summary.take() {
            summary.estimate_tokens();
            self.summary_history.push(summary);

            // Keep only last 5 historical summaries
            while self.summary_history.len() > 5 {
                self.summary_history.remove(0);
            }
        }
    }

    /// Recalculate total tokens
    fn recalculate_tokens(&mut self) {
        self.total_tokens = self.messages.iter().map(|m| m.token_count).sum::<usize>()
            + self
                .tool_results
                .iter()
                .map(|r| r.token_count)
                .sum::<usize>()
            + self
                .injected_context
                .as_ref()
                .map(|c| estimate_tokens(c))
                .unwrap_or(0)
            + self
                .truncated_summary
                .as_ref()
                .map(|s| estimate_tokens(s))
                .unwrap_or(0);
    }

    /// Get discoveries by category
    pub fn discoveries_by_category(&self, category: &str) -> Vec<&Discovery> {
        self.discoveries
            .iter()
            .filter(|d| d.category == category)
            .collect()
    }

    /// Get all discovery categories
    pub fn discovery_categories(&self) -> Vec<String> {
        let mut categories: Vec<String> = self
            .discoveries
            .iter()
            .map(|d| d.category.clone())
            .collect();
        categories.sort();
        categories.dedup();
        categories
    }

    /// Inject context from parent or Context Store
    pub fn inject(&mut self, context: impl Into<String>) {
        let ctx = context.into();
        let old_tokens = self
            .injected_context
            .as_ref()
            .map(|c| estimate_tokens(c))
            .unwrap_or(0);
        let new_tokens = estimate_tokens(&ctx);

        self.total_tokens = self.total_tokens - old_tokens + new_tokens;
        self.injected_context = Some(ctx);
    }

    /// Build context string for LLM
    pub fn build_context_string(&self) -> String {
        let mut parts = Vec::new();

        // Include truncated summary if any
        if let Some(ref summary) = self.truncated_summary {
            parts.push(format!(
                "## Earlier Conversation Summary ({} messages truncated)\n\n{}",
                self.truncated_count, summary
            ));
        }

        // Include injected context
        if let Some(ref injected) = self.injected_context {
            parts.push(format!("## Relevant Context\n\n{}", injected));
        }

        // Summarize discoveries
        if !self.discoveries.is_empty() {
            let summary = self.summarize_discoveries();
            parts.push(format!("## Previous Discoveries\n\n{}", summary));
        }

        parts.join("\n\n")
    }

    /// Summarize discoveries for context
    fn summarize_discoveries(&self) -> String {
        let categories = self.discovery_categories();
        let mut summary = String::new();

        for category in categories {
            let discoveries = self.discoveries_by_category(&category);
            summary.push_str(&format!("\n### {}\n", category));

            for discovery in discoveries.iter().take(5) {
                summary.push_str(&format!("- {}", discovery.content));
                if let Some(ref source) = discovery.source {
                    summary.push_str(&format!(" (source: {})", source));
                }
                summary.push('\n');
            }

            if discoveries.len() > 5 {
                summary.push_str(&format!("  ... and {} more\n", discoveries.len() - 5));
            }
        }

        summary
    }

    /// Get message count
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// Get last assistant message
    pub fn last_assistant_message(&self) -> Option<&ContextMessage> {
        self.messages
            .iter()
            .rev()
            .find(|m| m.role == MessageRole::Assistant)
    }

    /// Clear context (but keep discoveries)
    pub fn clear_messages(&mut self) {
        self.messages.clear();
        self.tool_results.clear();
        self.truncated_summary = None;
        self.truncated_count = 0;
        self.recalculate_tokens();
    }

    /// Export discoveries for sharing
    pub fn export_discoveries(&self) -> Vec<Discovery> {
        self.discoveries.clone()
    }

    /// Import discoveries from another context
    pub fn import_discoveries(&mut self, discoveries: Vec<Discovery>) {
        self.discoveries.extend(discoveries);
    }

    /// Get token usage report
    pub fn token_report(&self) -> TokenReport {
        let message_tokens: usize = self.messages.iter().map(|m| m.token_count).sum();
        let tool_tokens: usize = self.tool_results.iter().map(|r| r.token_count).sum();
        let discovery_tokens: usize = self.discoveries.iter().map(|d| d.token_count()).sum();
        let injected_tokens = self
            .injected_context
            .as_ref()
            .map(|c| estimate_tokens(c))
            .unwrap_or(0);
        let summary_tokens = self
            .truncated_summary
            .as_ref()
            .map(|s| estimate_tokens(s))
            .unwrap_or(0);

        TokenReport {
            message_tokens,
            tool_tokens,
            discovery_tokens,
            injected_tokens,
            summary_tokens,
            total_tokens: self.total_tokens,
            max_tokens: self.config.max_tokens,
            usage_percent: self.window_status().usage_percent,
        }
    }
}

/// Token usage report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenReport {
    pub message_tokens: usize,
    pub tool_tokens: usize,
    pub discovery_tokens: usize,
    pub injected_tokens: usize,
    pub summary_tokens: usize,
    pub total_tokens: usize,
    pub max_tokens: usize,
    pub usage_percent: f32,
}

/// Context Store - shared knowledge between sub-agents
#[derive(Debug, Clone, Default)]
pub struct ContextStore {
    /// All discoveries indexed by ID
    discoveries: HashMap<DiscoveryId, Discovery>,

    /// Index by category
    by_category: HashMap<String, Vec<DiscoveryId>>,

    /// Total token count in store
    total_tokens: usize,
}

impl ContextStore {
    /// Create a new context store
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a discovery
    pub fn add(&mut self, discovery: Discovery) {
        let id = discovery.id;
        let category = discovery.category.clone();
        let tokens = discovery.token_count();

        self.total_tokens += tokens;
        self.discoveries.insert(id, discovery);
        self.by_category.entry(category).or_default().push(id);
    }

    /// Add if not duplicate (check content similarity)
    pub fn add_unique(&mut self, discovery: Discovery) -> bool {
        // Check for duplicate content in same category
        let existing = self.get_by_category(&discovery.category);
        for existing_discovery in existing {
            if existing_discovery.content == discovery.content {
                return false; // Already exists
            }
        }
        self.add(discovery);
        true
    }

    /// Get a discovery by ID
    pub fn get(&self, id: DiscoveryId) -> Option<&Discovery> {
        self.discoveries.get(&id)
    }

    /// Get discoveries by category
    pub fn get_by_category(&self, category: &str) -> Vec<&Discovery> {
        self.by_category
            .get(category)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.discoveries.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all categories
    pub fn categories(&self) -> Vec<String> {
        self.by_category.keys().cloned().collect()
    }

    /// Build injection context for a sub-agent
    pub fn inject_context(&self, discovery_ids: &[DiscoveryId]) -> String {
        let mut context = String::new();

        for id in discovery_ids {
            if let Some(discovery) = self.discoveries.get(id) {
                context.push_str(&format!("[{}] {}\n", discovery.category, discovery.content));
            }
        }

        context
    }

    /// Build injection context by categories
    pub fn inject_by_categories(&self, categories: &[&str], max_tokens: usize) -> String {
        let mut context = String::new();
        let mut tokens = 0;

        for category in categories {
            for discovery in self.get_by_category(category) {
                let entry = format!("[{}] {}\n", discovery.category, discovery.content);
                let entry_tokens = estimate_tokens(&entry);

                if tokens + entry_tokens > max_tokens {
                    break;
                }

                context.push_str(&entry);
                tokens += entry_tokens;
            }
        }

        context
    }

    /// Get discovery count
    pub fn len(&self) -> usize {
        self.discoveries.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.discoveries.is_empty()
    }

    /// Get total token count
    pub fn total_tokens(&self) -> usize {
        self.total_tokens
    }
}

/// Estimate token count for a string
/// Uses rough approximation: ~4 characters per token for English
fn estimate_tokens(text: &str) -> usize {
    // More accurate estimation considering whitespace and special chars
    let char_count = text.chars().count();
    let word_count = text.split_whitespace().count();

    // Weighted average: characters / 4 with word count adjustment
    let char_estimate = char_count / 4;
    let word_estimate = word_count * 4 / 3; // ~1.33 tokens per word

    (char_estimate + word_estimate) / 2
}

/// Truncate text for summary
fn truncate_for_summary(text: &str, max_chars: usize) -> String {
    if text.len() <= max_chars {
        text.to_string()
    } else {
        format!("{}...", &text[..max_chars])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discovery() {
        let discovery = Discovery::new("api_endpoint", "GET /users - src/api.rs:25")
            .with_source("src/api.rs")
            .with_confidence(0.9);

        assert_eq!(discovery.category, "api_endpoint");
        assert_eq!(discovery.confidence, 0.9);
    }

    #[test]
    fn test_context() {
        let mut ctx = SubAgentContext::new();

        ctx.add_message(ContextMessage::user("Find API endpoints"));
        ctx.add_message(ContextMessage::assistant("I found 3 endpoints"));

        ctx.add_discovery(Discovery::new("api", "GET /users"));
        ctx.add_discovery(Discovery::new("api", "POST /auth"));
        ctx.add_discovery(Discovery::new("file", "src/api.rs"));

        assert_eq!(ctx.message_count(), 2);
        assert_eq!(ctx.discoveries.len(), 3);
        assert_eq!(ctx.discoveries_by_category("api").len(), 2);
    }

    #[test]
    fn test_context_store() {
        let mut store = ContextStore::new();

        store.add(Discovery::new("api", "GET /users"));
        store.add(Discovery::new("api", "POST /auth"));
        store.add(Discovery::new("file", "src/api.rs"));

        assert_eq!(store.len(), 3);
        assert_eq!(store.get_by_category("api").len(), 2);
        assert_eq!(store.get_by_category("file").len(), 1);
    }

    #[test]
    fn test_context_window_status() {
        let mut ctx = SubAgentContext::new();

        let status = ctx.window_status();
        assert_eq!(status.total_tokens, 0);
        assert!(!status.needs_truncation);

        // Add some messages
        ctx.add_message(ContextMessage::user("Hello"));

        let status = ctx.window_status();
        assert!(status.total_tokens > 0);
    }

    #[test]
    fn test_token_estimation() {
        let tokens = estimate_tokens("Hello, world!");
        assert!(tokens > 0);
        assert!(tokens < 10); // Should be roughly 3-4 tokens
    }

    #[test]
    fn test_context_for_model() {
        let ctx = SubAgentContext::for_model("claude-3-opus");
        assert_eq!(ctx.config.max_tokens, 200_000);

        let ctx = SubAgentContext::for_model("gpt-4");
        assert_eq!(ctx.config.max_tokens, 128_000);
    }

    #[test]
    fn test_add_unique() {
        let mut store = ContextStore::new();

        let added = store.add_unique(Discovery::new("api", "GET /users"));
        assert!(added);

        let added = store.add_unique(Discovery::new("api", "GET /users"));
        assert!(!added); // Duplicate

        let added = store.add_unique(Discovery::new("api", "POST /users"));
        assert!(added); // Different content
    }

    #[test]
    fn test_token_report() {
        let mut ctx = SubAgentContext::new();
        ctx.add_message(ContextMessage::user("Test message"));

        let report = ctx.token_report();
        assert!(report.message_tokens > 0);
        assert!(report.total_tokens > 0);
    }
}
