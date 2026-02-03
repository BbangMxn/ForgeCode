//! Command history tracker for forgecmd
//!
//! Tracks executed commands with their results, risk levels, and timing.
//! Integrates with forge-foundation's Storage for SQLite persistence.

use crate::forgecmd::filter::{CommandCategory, RiskAnalysis};
use chrono::{DateTime, Utc};
use forge_foundation::{Storage, ToolExecutionRecord};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::{Arc, RwLock};

/// Maximum in-memory history size
const MAX_MEMORY_HISTORY: usize = 1000;

/// Command execution status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    /// Command executed successfully
    Success,
    /// Command failed with non-zero exit code
    Failed,
    /// Command timed out
    Timeout,
    /// Command was cancelled by user
    Cancelled,
    /// Command was denied (permission)
    Denied,
    /// Command is currently running
    Running,
}

impl ExecutionStatus {
    /// Check if execution completed (success or failure)
    pub fn is_completed(&self) -> bool {
        matches!(
            self,
            Self::Success | Self::Failed | Self::Timeout | Self::Cancelled | Self::Denied
        )
    }

    /// Check if execution was successful
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success)
    }
}

/// A single command execution record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandRecord {
    /// Unique identifier
    pub id: String,

    /// Session ID (from ForgeCmd session)
    pub session_id: String,

    /// The command that was executed
    pub command: String,

    /// Working directory when executed
    pub working_dir: String,

    /// Risk category when analyzed
    pub category: String,

    /// Risk score (0-10)
    pub risk_score: u8,

    /// Execution status
    pub status: ExecutionStatus,

    /// Exit code (if completed)
    pub exit_code: Option<i32>,

    /// Standard output (truncated)
    pub stdout: Option<String>,

    /// Standard error (truncated)
    pub stderr: Option<String>,

    /// Duration in milliseconds
    pub duration_ms: Option<u64>,

    /// When the command was started
    pub started_at: DateTime<Utc>,

    /// When the command completed
    pub completed_at: Option<DateTime<Utc>>,

    /// Who approved the command (if confirmation was required)
    pub approved_by: Option<String>,
}

impl CommandRecord {
    /// Create a new command record (starting execution)
    pub fn new(
        session_id: &str,
        command: &str,
        working_dir: &str,
        analysis: &RiskAnalysis,
    ) -> Self {
        Self {
            id: generate_record_id(),
            session_id: session_id.to_string(),
            command: command.to_string(),
            working_dir: working_dir.to_string(),
            category: format!("{:?}", analysis.category),
            risk_score: analysis.risk_score,
            status: ExecutionStatus::Running,
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: None,
            started_at: Utc::now(),
            completed_at: None,
            approved_by: None,
        }
    }

    /// Mark as completed with success
    pub fn complete_success(&mut self, exit_code: i32, stdout: &str, stderr: &str) {
        self.status = ExecutionStatus::Success;
        self.exit_code = Some(exit_code);
        self.stdout = Some(truncate_output(stdout));
        self.stderr = Some(truncate_output(stderr));
        self.completed_at = Some(Utc::now());
        self.duration_ms = Some(self.calculate_duration());
    }

    /// Mark as failed
    pub fn complete_failed(&mut self, exit_code: i32, stdout: &str, stderr: &str) {
        self.status = ExecutionStatus::Failed;
        self.exit_code = Some(exit_code);
        self.stdout = Some(truncate_output(stdout));
        self.stderr = Some(truncate_output(stderr));
        self.completed_at = Some(Utc::now());
        self.duration_ms = Some(self.calculate_duration());
    }

    /// Mark as timed out
    pub fn complete_timeout(&mut self, stdout: &str, stderr: &str) {
        self.status = ExecutionStatus::Timeout;
        self.stdout = Some(truncate_output(stdout));
        self.stderr = Some(truncate_output(stderr));
        self.completed_at = Some(Utc::now());
        self.duration_ms = Some(self.calculate_duration());
    }

    /// Mark as cancelled
    pub fn complete_cancelled(&mut self) {
        self.status = ExecutionStatus::Cancelled;
        self.completed_at = Some(Utc::now());
        self.duration_ms = Some(self.calculate_duration());
    }

    /// Mark as denied
    pub fn mark_denied(&mut self, reason: &str) {
        self.status = ExecutionStatus::Denied;
        self.stderr = Some(reason.to_string());
        self.completed_at = Some(Utc::now());
        self.duration_ms = Some(0);
    }

    fn calculate_duration(&self) -> u64 {
        let end = self.completed_at.unwrap_or_else(Utc::now);
        (end - self.started_at).num_milliseconds().max(0) as u64
    }

    /// Convert to ToolExecutionRecord for Storage persistence
    pub fn to_tool_execution_record(&self) -> ToolExecutionRecord {
        let status = match self.status {
            ExecutionStatus::Success => "success",
            ExecutionStatus::Failed => "error",
            ExecutionStatus::Timeout => "timeout",
            ExecutionStatus::Cancelled => "cancelled",
            ExecutionStatus::Denied => "error",
            ExecutionStatus::Running => "running",
        };

        // Build input JSON with command metadata
        let input = serde_json::json!({
            "command": self.command,
            "working_dir": self.working_dir,
            "category": self.category,
            "risk_score": self.risk_score,
            "approved_by": self.approved_by,
        });

        ToolExecutionRecord {
            id: None,
            session_id: Some(self.session_id.clone()),
            message_id: None,
            tool_name: "forgecmd".to_string(),
            tool_call_id: self.id.clone(),
            input_json: input.to_string(),
            output_text: self.stdout.clone(),
            status: status.to_string(),
            error_message: self.stderr.clone(),
            duration_ms: self.duration_ms.map(|d| d as i64),
            created_at: Some(self.started_at.to_rfc3339()),
            completed_at: self.completed_at.map(|t| t.to_rfc3339()),
        }
    }
}

/// Command history tracker
pub struct CommandTracker {
    /// In-memory history (ring buffer)
    history: Arc<RwLock<VecDeque<CommandRecord>>>,

    /// Current session ID
    session_id: String,

    /// Maximum history size
    max_size: usize,
}

impl CommandTracker {
    /// Create a new command tracker
    pub fn new(session_id: &str) -> Self {
        Self {
            history: Arc::new(RwLock::new(VecDeque::with_capacity(MAX_MEMORY_HISTORY))),
            session_id: session_id.to_string(),
            max_size: MAX_MEMORY_HISTORY,
        }
    }

    /// Create with custom max size
    pub fn with_max_size(session_id: &str, max_size: usize) -> Self {
        Self {
            history: Arc::new(RwLock::new(VecDeque::with_capacity(max_size))),
            session_id: session_id.to_string(),
            max_size,
        }
    }

    /// Start tracking a command execution
    pub fn start(&self, command: &str, working_dir: &str, analysis: &RiskAnalysis) -> String {
        let record = CommandRecord::new(&self.session_id, command, working_dir, analysis);
        let id = record.id.clone();

        if let Ok(mut history) = self.history.write() {
            if history.len() >= self.max_size {
                history.pop_front();
            }
            history.push_back(record);
        }

        id
    }

    /// Update a record by ID
    pub fn update<F>(&self, id: &str, updater: F)
    where
        F: FnOnce(&mut CommandRecord),
    {
        if let Ok(mut history) = self.history.write() {
            if let Some(record) = history.iter_mut().find(|r| r.id == id) {
                updater(record);
            }
        }
    }

    /// Mark a command as completed successfully
    pub fn complete_success(&self, id: &str, exit_code: i32, stdout: &str, stderr: &str) {
        self.update(id, |record| {
            record.complete_success(exit_code, stdout, stderr);
        });
    }

    /// Mark a command as failed
    pub fn complete_failed(&self, id: &str, exit_code: i32, stdout: &str, stderr: &str) {
        self.update(id, |record| {
            record.complete_failed(exit_code, stdout, stderr);
        });
    }

    /// Mark a command as timed out
    pub fn complete_timeout(&self, id: &str, stdout: &str, stderr: &str) {
        self.update(id, |record| {
            record.complete_timeout(stdout, stderr);
        });
    }

    /// Mark a command as cancelled
    pub fn complete_cancelled(&self, id: &str) {
        self.update(id, |record| {
            record.complete_cancelled();
        });
    }

    /// Mark a command as denied
    pub fn mark_denied(&self, id: &str, reason: &str) {
        self.update(id, |record| {
            record.mark_denied(reason);
        });
    }

    /// Record a denied command directly (shorthand)
    pub fn record_denied(&self, command: &str, working_dir: &str, reason: &str) -> String {
        // Create a minimal analysis for denied commands
        let analysis = RiskAnalysis {
            category: CommandCategory::Forbidden,
            risk_score: 10,
            requires_confirmation: true,
            reason: Some(reason.to_string()),
            matched_rule: None,
        };

        let mut record = CommandRecord::new(&self.session_id, command, working_dir, &analysis);
        record.mark_denied(reason);
        let id = record.id.clone();

        if let Ok(mut history) = self.history.write() {
            if history.len() >= self.max_size {
                history.pop_front();
            }
            history.push_back(record);
        }

        id
    }

    /// Get a record by ID
    pub fn get(&self, id: &str) -> Option<CommandRecord> {
        self.history
            .read()
            .ok()
            .and_then(|h| h.iter().find(|r| r.id == id).cloned())
    }

    /// Get the last N commands
    pub fn get_recent(&self, count: usize) -> Vec<CommandRecord> {
        self.history
            .read()
            .map(|h| h.iter().rev().take(count).cloned().collect())
            .unwrap_or_default()
    }

    /// Get all commands in this session
    pub fn get_all(&self) -> Vec<CommandRecord> {
        self.history
            .read()
            .map(|h| h.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Get commands by status
    pub fn get_by_status(&self, status: ExecutionStatus) -> Vec<CommandRecord> {
        self.history
            .read()
            .map(|h| h.iter().filter(|r| r.status == status).cloned().collect())
            .unwrap_or_default()
    }

    /// Get failed commands
    pub fn get_failed(&self) -> Vec<CommandRecord> {
        self.history
            .read()
            .map(|h| {
                h.iter()
                    .filter(|r| {
                        matches!(
                            r.status,
                            ExecutionStatus::Failed
                                | ExecutionStatus::Timeout
                                | ExecutionStatus::Denied
                        )
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get running commands
    pub fn get_running(&self) -> Vec<CommandRecord> {
        self.get_by_status(ExecutionStatus::Running)
    }

    /// Get statistics
    pub fn stats(&self) -> TrackerStats {
        self.history
            .read()
            .map(|h| {
                let mut stats = TrackerStats::default();
                for record in h.iter() {
                    stats.total_commands += 1;
                    match record.status {
                        ExecutionStatus::Success => stats.successful += 1,
                        ExecutionStatus::Failed => stats.failed += 1,
                        ExecutionStatus::Timeout => stats.timed_out += 1,
                        ExecutionStatus::Cancelled => stats.cancelled += 1,
                        ExecutionStatus::Denied => stats.denied += 1,
                        ExecutionStatus::Running => stats.running += 1,
                    }
                    if let Some(duration) = record.duration_ms {
                        stats.total_duration_ms += duration;
                    }
                }
                if stats.total_commands > 0 {
                    stats.average_duration_ms =
                        stats.total_duration_ms / (stats.total_commands as u64);
                }
                stats
            })
            .unwrap_or_default()
    }

    /// Clear all history
    pub fn clear(&self) {
        if let Ok(mut history) = self.history.write() {
            history.clear();
        }
    }

    /// Get session ID
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Export history as JSON
    pub fn export_json(&self) -> Result<String, serde_json::Error> {
        let records = self.get_all();
        serde_json::to_string_pretty(&records)
    }

    /// Search commands by pattern
    pub fn search(&self, pattern: &str) -> Vec<CommandRecord> {
        let pattern_lower = pattern.to_lowercase();
        self.history
            .read()
            .map(|h| {
                h.iter()
                    .filter(|r| r.command.to_lowercase().contains(&pattern_lower))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    // ========================================================================
    // Storage Integration (Layer1)
    // ========================================================================

    /// Save a command record to Layer1 Storage
    ///
    /// Converts CommandRecord to ToolExecutionRecord and persists to SQLite.
    pub fn save_to_storage(
        &self,
        storage: &Storage,
        record: &CommandRecord,
    ) -> Result<i64, String> {
        let tool_record = record.to_tool_execution_record();

        // Start the execution record
        let id = storage
            .start_tool_execution(&tool_record)
            .map_err(|e| format!("Failed to start tool execution record: {}", e))?;

        // If already completed, update with results
        if record.status.is_completed() {
            let status_str = match record.status {
                ExecutionStatus::Success => "success",
                ExecutionStatus::Failed => "error",
                ExecutionStatus::Timeout => "timeout",
                ExecutionStatus::Cancelled => "cancelled",
                ExecutionStatus::Denied => "error",
                ExecutionStatus::Running => "running",
            };

            let output = record.stdout.as_deref();
            let error = match record.status {
                ExecutionStatus::Denied | ExecutionStatus::Failed => record.stderr.as_deref(),
                _ => None,
            };

            storage
                .complete_tool_execution(
                    id,
                    output,
                    status_str,
                    error,
                    record.duration_ms.map(|d| d as i64),
                )
                .map_err(|e| format!("Failed to complete tool execution record: {}", e))?;
        }

        Ok(id)
    }

    /// Save all in-memory records to Storage
    pub fn save_all_to_storage(&self, storage: &Storage) -> Result<usize, String> {
        let records = self.get_all();
        let mut saved = 0;

        for record in &records {
            if record.status.is_completed() {
                self.save_to_storage(storage, record)?;
                saved += 1;
            }
        }

        Ok(saved)
    }

    /// Complete a record and immediately save to Storage
    pub fn complete_and_save(
        &self,
        storage: &Storage,
        id: &str,
        exit_code: i32,
        stdout: &str,
        stderr: &str,
    ) -> Result<i64, String> {
        // Update in-memory record
        if exit_code == 0 {
            self.complete_success(id, exit_code, stdout, stderr);
        } else {
            self.complete_failed(id, exit_code, stdout, stderr);
        }

        // Get updated record and save
        let record = self
            .get(id)
            .ok_or_else(|| format!("Record not found: {}", id))?;

        self.save_to_storage(storage, &record)
    }
}

/// Tracker statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrackerStats {
    pub total_commands: usize,
    pub successful: usize,
    pub failed: usize,
    pub timed_out: usize,
    pub cancelled: usize,
    pub denied: usize,
    pub running: usize,
    pub total_duration_ms: u64,
    pub average_duration_ms: u64,
}

impl TrackerStats {
    /// Get success rate as percentage
    pub fn success_rate(&self) -> f64 {
        let completed = self.successful + self.failed + self.timed_out + self.denied;
        if completed == 0 {
            100.0
        } else {
            (self.successful as f64 / completed as f64) * 100.0
        }
    }
}

// === Helper functions ===

/// Generate a unique record ID
fn generate_record_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);

    format!("cmd-{:x}", timestamp)
}

/// Truncate output to reasonable size
fn truncate_output(output: &str) -> String {
    const MAX_OUTPUT_SIZE: usize = 10_000;

    if output.len() <= MAX_OUTPUT_SIZE {
        output.to_string()
    } else {
        let truncated = &output[..MAX_OUTPUT_SIZE];
        format!(
            "{}...[truncated {} bytes]",
            truncated,
            output.len() - MAX_OUTPUT_SIZE
        )
    }
}

/// Format duration for display
pub fn format_duration(ms: u64) -> String {
    if ms < 1000 {
        format!("{}ms", ms)
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else if ms < 3_600_000 {
        format!("{:.1}m", ms as f64 / 60_000.0)
    } else {
        format!("{:.1}h", ms as f64 / 3_600_000.0)
    }
}

// Helper function to create RiskAnalysis for a category (used in tests)
#[cfg(test)]
fn default_analysis_for_category(category: CommandCategory) -> RiskAnalysis {
    let risk_score = match category {
        CommandCategory::ReadOnly => 0,
        CommandCategory::SafeWrite => 2,
        CommandCategory::Caution => 5,
        CommandCategory::Dangerous => 8,
        CommandCategory::Forbidden => 10,
        CommandCategory::Interactive => 5,
        CommandCategory::Unknown => 4,
    };

    RiskAnalysis {
        category,
        risk_score,
        requires_confirmation: risk_score >= 5,
        reason: None,
        matched_rule: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_tracker() -> CommandTracker {
        CommandTracker::new("test-session")
    }

    fn mock_analysis() -> RiskAnalysis {
        default_analysis_for_category(CommandCategory::ReadOnly)
    }

    #[test]
    fn test_start_and_complete() {
        let tracker = create_tracker();
        let analysis = mock_analysis();

        let id = tracker.start("ls -la", "/tmp", &analysis);
        tracker.complete_success(&id, 0, "file1\nfile2", "");

        let record = tracker.get(&id).expect("Record not found");
        assert_eq!(record.command, "ls -la");
        assert!(record.status.is_success());
        assert_eq!(record.exit_code, Some(0));
    }

    #[test]
    fn test_history_limit() {
        let tracker = CommandTracker::with_max_size("test", 3);
        let analysis = mock_analysis();

        tracker.start("cmd1", "/tmp", &analysis);
        tracker.start("cmd2", "/tmp", &analysis);
        tracker.start("cmd3", "/tmp", &analysis);
        tracker.start("cmd4", "/tmp", &analysis);

        let all = tracker.get_all();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].command, "cmd2"); // cmd1 was evicted
    }

    #[test]
    fn test_stats() {
        let tracker = create_tracker();
        let analysis = mock_analysis();

        let id1 = tracker.start("ls", "/tmp", &analysis);
        tracker.complete_success(&id1, 0, "", "");

        let id2 = tracker.start("false", "/tmp", &analysis);
        tracker.complete_failed(&id2, 1, "", "error");

        let stats = tracker.stats();
        assert_eq!(stats.total_commands, 2);
        assert_eq!(stats.successful, 1);
        assert_eq!(stats.failed, 1);
        assert_eq!(stats.success_rate(), 50.0);
    }

    #[test]
    fn test_search() {
        let tracker = create_tracker();
        let analysis = mock_analysis();

        tracker.start("git status", "/tmp", &analysis);
        tracker.start("git commit", "/tmp", &analysis);
        tracker.start("ls -la", "/tmp", &analysis);

        let results = tracker.search("git");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(500), "500ms");
        assert_eq!(format_duration(2500), "2.5s");
        assert_eq!(format_duration(90_000), "1.5m");
    }

    #[test]
    fn test_record_denied() {
        let tracker = create_tracker();

        let id = tracker.record_denied("rm -rf /", "/tmp", "Forbidden command");
        let record = tracker.get(&id).unwrap();

        assert_eq!(record.status, ExecutionStatus::Denied);
        assert_eq!(record.category, "Forbidden");
    }

    #[test]
    fn test_to_tool_execution_record() {
        let tracker = create_tracker();
        let analysis = mock_analysis();

        let id = tracker.start("echo hello", "/tmp", &analysis);
        tracker.complete_success(&id, 0, "hello\n", "");

        let record = tracker.get(&id).unwrap();
        let tool_record = record.to_tool_execution_record();

        assert_eq!(tool_record.tool_name, "forgecmd");
        assert_eq!(tool_record.tool_call_id, id);
        assert_eq!(tool_record.status, "success");
        assert_eq!(tool_record.output_text, Some("hello\n".to_string()));
        assert!(tool_record.input_json.contains("echo hello"));
    }

    #[test]
    fn test_save_to_storage() {
        use forge_foundation::SessionRecord;

        let tracker = create_tracker();
        let analysis = mock_analysis();
        let storage = Storage::in_memory().expect("Failed to create storage");

        // Create session first (required by foreign key constraint)
        let session = SessionRecord {
            id: "test-session".to_string(),
            ..Default::default()
        };
        storage
            .create_session(&session)
            .expect("Failed to create session");

        // Create and complete a record
        let id = tracker.start("ls -la", "/tmp", &analysis);
        tracker.complete_success(&id, 0, "file1\nfile2", "");

        let record = tracker.get(&id).unwrap();
        let storage_id = tracker
            .save_to_storage(&storage, &record)
            .expect("Failed to save");

        assert!(storage_id > 0);
    }

    #[test]
    fn test_complete_and_save() {
        use forge_foundation::SessionRecord;

        let tracker = create_tracker();
        let analysis = mock_analysis();
        let storage = Storage::in_memory().expect("Failed to create storage");

        // Create session first (required by foreign key constraint)
        let session = SessionRecord {
            id: "test-session".to_string(),
            ..Default::default()
        };
        storage
            .create_session(&session)
            .expect("Failed to create session");

        let id = tracker.start("pwd", "/tmp", &analysis);
        let storage_id = tracker
            .complete_and_save(&storage, &id, 0, "/tmp\n", "")
            .expect("Failed to complete and save");

        assert!(storage_id > 0);

        let record = tracker.get(&id).unwrap();
        assert!(record.status.is_success());
    }
}
