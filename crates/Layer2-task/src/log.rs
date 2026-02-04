//! Task Log System - Real-time log capture and access
//!
//! Provides:
//! - Real-time log streaming from running tasks
//! - Log history with structured entries
//! - Log analysis support for LLM debugging
//! - Configurable retention and filtering

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::debug;

/// Maximum log entries per task
const DEFAULT_MAX_ENTRIES: usize = 10000;

/// Broadcast channel capacity
const BROADCAST_CAPACITY: usize = 1000;

/// Log level for task output
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogLevel {
    /// Standard output
    Stdout,
    /// Standard error
    Stderr,
    /// System messages (start, stop, etc.)
    System,
    /// Debug information
    Debug,
    /// Error information
    Error,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Stdout => "stdout",
            LogLevel::Stderr => "stderr",
            LogLevel::System => "system",
            LogLevel::Debug => "debug",
            LogLevel::Error => "error",
        }
    }

    pub fn is_error(&self) -> bool {
        matches!(self, LogLevel::Stderr | LogLevel::Error)
    }
}

/// A single log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// Timestamp
    pub timestamp: DateTime<Utc>,

    /// Log level
    pub level: LogLevel,

    /// Log content
    pub content: String,

    /// Line number in task output
    pub line_number: usize,

    /// Associated metadata (e.g., exit code, command)
    pub metadata: Option<serde_json::Value>,
}

impl LogEntry {
    pub fn new(level: LogLevel, content: impl Into<String>, line_number: usize) -> Self {
        Self {
            timestamp: Utc::now(),
            level,
            content: content.into(),
            line_number,
            metadata: None,
        }
    }

    pub fn stdout(content: impl Into<String>, line_number: usize) -> Self {
        Self::new(LogLevel::Stdout, content, line_number)
    }

    pub fn stderr(content: impl Into<String>, line_number: usize) -> Self {
        Self::new(LogLevel::Stderr, content, line_number)
    }

    pub fn system(content: impl Into<String>, line_number: usize) -> Self {
        Self::new(LogLevel::System, content, line_number)
    }

    pub fn error(content: impl Into<String>, line_number: usize) -> Self {
        Self::new(LogLevel::Error, content, line_number)
    }

    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Format for LLM analysis
    pub fn format_for_analysis(&self) -> String {
        format!(
            "[{}] [{}] L{}: {}",
            self.timestamp.format("%H:%M:%S%.3f"),
            self.level.as_str(),
            self.line_number,
            self.content
        )
    }
}

/// Log buffer for a single task
#[derive(Debug)]
pub struct TaskLogBuffer {
    /// Task ID
    pub task_id: String,

    /// Log entries
    entries: VecDeque<LogEntry>,

    /// Maximum entries to keep
    max_entries: usize,

    /// Current line count
    line_count: usize,

    /// Real-time broadcast sender
    tx: broadcast::Sender<LogEntry>,

    /// Start time
    started_at: DateTime<Utc>,

    /// End time
    ended_at: Option<DateTime<Utc>>,

    /// Associated command
    command: Option<String>,
}

impl TaskLogBuffer {
    pub fn new(task_id: impl Into<String>) -> Self {
        let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);
        Self {
            task_id: task_id.into(),
            entries: VecDeque::with_capacity(DEFAULT_MAX_ENTRIES),
            max_entries: DEFAULT_MAX_ENTRIES,
            line_count: 0,
            tx,
            started_at: Utc::now(),
            ended_at: None,
            command: None,
        }
    }

    pub fn with_max_entries(mut self, max: usize) -> Self {
        self.max_entries = max;
        self
    }

    pub fn with_command(mut self, command: impl Into<String>) -> Self {
        self.command = Some(command.into());
        self
    }

    /// Add a log entry
    pub fn push(&mut self, mut entry: LogEntry) {
        self.line_count += 1;
        entry.line_number = self.line_count;

        // Send to real-time subscribers
        let _ = self.tx.send(entry.clone());

        // Store in buffer
        if self.entries.len() >= self.max_entries {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    /// Add stdout line
    pub fn push_stdout(&mut self, content: impl Into<String>) {
        self.push(LogEntry::stdout(content, 0));
    }

    /// Add stderr line
    pub fn push_stderr(&mut self, content: impl Into<String>) {
        self.push(LogEntry::stderr(content, 0));
    }

    /// Add system message
    pub fn push_system(&mut self, content: impl Into<String>) {
        self.push(LogEntry::system(content, 0));
    }

    /// Mark as ended
    pub fn mark_ended(&mut self) {
        self.ended_at = Some(Utc::now());
    }

    /// Subscribe to real-time logs
    pub fn subscribe(&self) -> broadcast::Receiver<LogEntry> {
        self.tx.subscribe()
    }

    /// Get all entries
    pub fn entries(&self) -> impl Iterator<Item = &LogEntry> {
        self.entries.iter()
    }

    /// Get entries with filtering
    pub fn entries_filtered(&self, level: Option<LogLevel>, search: Option<&str>) -> Vec<&LogEntry> {
        self.entries
            .iter()
            .filter(|e| level.map_or(true, |l| e.level == l))
            .filter(|e| search.map_or(true, |s| e.content.contains(s)))
            .collect()
    }

    /// Get last N entries
    pub fn tail(&self, n: usize) -> Vec<&LogEntry> {
        self.entries.iter().rev().take(n).rev().collect()
    }

    /// Get entries from line number
    pub fn from_line(&self, line: usize) -> Vec<&LogEntry> {
        self.entries.iter().filter(|e| e.line_number >= line).collect()
    }

    /// Get only errors
    pub fn errors(&self) -> Vec<&LogEntry> {
        self.entries.iter().filter(|e| e.level.is_error()).collect()
    }

    /// Total line count
    pub fn line_count(&self) -> usize {
        self.line_count
    }

    /// Check if active
    pub fn is_active(&self) -> bool {
        self.ended_at.is_none()
    }

    /// Duration in seconds
    pub fn duration_secs(&self) -> f64 {
        let end = self.ended_at.unwrap_or_else(Utc::now);
        (end - self.started_at).num_milliseconds() as f64 / 1000.0
    }

    /// Generate analysis report for LLM
    pub fn generate_analysis_report(&self) -> LogAnalysisReport {
        let errors = self.errors();
        let error_count = errors.len();

        // Find patterns in errors
        let mut error_patterns: HashMap<String, usize> = HashMap::new();
        for error in &errors {
            // Extract first word or pattern from error
            let pattern = error
                .content
                .split_whitespace()
                .next()
                .unwrap_or("unknown")
                .to_string();
            *error_patterns.entry(pattern).or_insert(0) += 1;
        }

        // Get last errors for context
        let last_errors: Vec<String> = errors
            .iter()
            .rev()
            .take(5)
            .map(|e| e.format_for_analysis())
            .collect();

        // Get log excerpt around first error
        let error_context = if let Some(first_error) = errors.first() {
            let line = first_error.line_number;
            let start = line.saturating_sub(5);
            let end = line + 5;
            self.entries
                .iter()
                .filter(|e| e.line_number >= start && e.line_number <= end)
                .map(|e| e.format_for_analysis())
                .collect()
        } else {
            Vec::new()
        };

        LogAnalysisReport {
            task_id: self.task_id.clone(),
            command: self.command.clone(),
            total_lines: self.line_count,
            error_count,
            duration_secs: self.duration_secs(),
            is_active: self.is_active(),
            error_patterns,
            last_errors,
            error_context,
            summary: self.generate_summary(),
        }
    }

    /// Generate a brief summary
    fn generate_summary(&self) -> String {
        let status = if self.is_active() {
            "running"
        } else {
            "completed"
        };
        let errors = self.errors().len();
        format!(
            "Task {} ({}): {} lines, {} errors, {:.1}s",
            self.task_id, status, self.line_count, errors, self.duration_secs()
        )
    }
}

/// Analysis report for LLM debugging
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogAnalysisReport {
    /// Task identifier
    pub task_id: String,

    /// Command that was executed
    pub command: Option<String>,

    /// Total log lines
    pub total_lines: usize,

    /// Number of error lines
    pub error_count: usize,

    /// Execution duration
    pub duration_secs: f64,

    /// Whether task is still running
    pub is_active: bool,

    /// Common error patterns (pattern -> count)
    pub error_patterns: HashMap<String, usize>,

    /// Last few error messages
    pub last_errors: Vec<String>,

    /// Context around first error
    pub error_context: Vec<String>,

    /// Brief summary
    pub summary: String,
}

impl LogAnalysisReport {
    /// Format for LLM analysis
    pub fn format_for_llm(&self) -> String {
        let mut output = String::new();

        output.push_str(&format!("=== Task Log Analysis: {} ===\n\n", self.task_id));

        if let Some(cmd) = &self.command {
            output.push_str(&format!("Command: {}\n", cmd));
        }
        output.push_str(&format!("Status: {}\n", if self.is_active { "Running" } else { "Completed" }));
        output.push_str(&format!("Duration: {:.1}s\n", self.duration_secs));
        output.push_str(&format!("Total Lines: {}\n", self.total_lines));
        output.push_str(&format!("Errors: {}\n\n", self.error_count));

        if !self.error_patterns.is_empty() {
            output.push_str("Error Patterns:\n");
            for (pattern, count) in &self.error_patterns {
                output.push_str(&format!("  - {}: {} occurrences\n", pattern, count));
            }
            output.push('\n');
        }

        if !self.last_errors.is_empty() {
            output.push_str("Recent Errors:\n");
            for error in &self.last_errors {
                output.push_str(&format!("  {}\n", error));
            }
            output.push('\n');
        }

        if !self.error_context.is_empty() {
            output.push_str("Context Around First Error:\n");
            for line in &self.error_context {
                output.push_str(&format!("  {}\n", line));
            }
        }

        output
    }
}

/// Task log manager - manages logs for all tasks
pub struct TaskLogManager {
    /// Log buffers by task ID
    buffers: Arc<RwLock<HashMap<String, TaskLogBuffer>>>,

    /// Maximum buffers to keep
    max_buffers: usize,

    /// Persist logs to disk
    persist_dir: Option<PathBuf>,
}

impl TaskLogManager {
    pub fn new() -> Self {
        Self {
            buffers: Arc::new(RwLock::new(HashMap::new())),
            max_buffers: 100,
            persist_dir: None,
        }
    }

    pub fn with_persistence(mut self, dir: PathBuf) -> Self {
        self.persist_dir = Some(dir);
        self
    }

    pub fn with_max_buffers(mut self, max: usize) -> Self {
        self.max_buffers = max;
        self
    }

    /// Create a new log buffer for a task
    pub async fn create_buffer(&self, task_id: impl Into<String>, command: Option<&str>) -> broadcast::Receiver<LogEntry> {
        let task_id = task_id.into();
        let mut buffer = TaskLogBuffer::new(&task_id);

        if let Some(cmd) = command {
            buffer = buffer.with_command(cmd);
        }

        // Subscribe BEFORE pushing system message so subscriber receives it
        let receiver = buffer.subscribe();

        if let Some(cmd) = command {
            buffer.push_system(format!("Starting task: {}", cmd));
        }

        let mut buffers = self.buffers.write().await;
        buffers.insert(task_id.clone(), buffer);

        // Cleanup old buffers if needed
        if buffers.len() > self.max_buffers {
            self.cleanup_old_buffers_inner(&mut buffers);
        }

        debug!("Created log buffer for task {}", task_id);
        receiver
    }

    fn cleanup_old_buffers_inner(&self, buffers: &mut HashMap<String, TaskLogBuffer>) {
        // Remove oldest completed buffers
        let mut completed: Vec<(String, DateTime<Utc>)> = buffers
            .iter()
            .filter_map(|(id, buf)| buf.ended_at.map(|t| (id.clone(), t)))
            .collect();

        completed.sort_by_key(|(_, t)| *t);

        let to_remove = completed.len().saturating_sub(self.max_buffers / 2);
        for (id, _) in completed.into_iter().take(to_remove) {
            buffers.remove(&id);
            debug!("Cleaned up log buffer for task {}", id);
        }
    }

    /// Get log buffer for a task
    pub async fn get_buffer(&self, task_id: &str) -> Option<TaskLogBuffer> {
        let buffers = self.buffers.read().await;
        buffers.get(task_id).map(|b| {
            // Clone the buffer data (not the broadcast channel)
            TaskLogBuffer {
                task_id: b.task_id.clone(),
                entries: b.entries.clone(),
                max_entries: b.max_entries,
                line_count: b.line_count,
                tx: b.tx.clone(),
                started_at: b.started_at,
                ended_at: b.ended_at,
                command: b.command.clone(),
            }
        })
    }

    /// Push log entry
    pub async fn push(&self, task_id: &str, entry: LogEntry) {
        let mut buffers = self.buffers.write().await;
        if let Some(buffer) = buffers.get_mut(task_id) {
            buffer.push(entry);
        }
    }

    /// Push stdout
    pub async fn push_stdout(&self, task_id: &str, content: impl Into<String>) {
        self.push(task_id, LogEntry::stdout(content, 0)).await;
    }

    /// Push stderr
    pub async fn push_stderr(&self, task_id: &str, content: impl Into<String>) {
        self.push(task_id, LogEntry::stderr(content, 0)).await;
    }

    /// Push system message
    pub async fn push_system(&self, task_id: &str, content: impl Into<String>) {
        self.push(task_id, LogEntry::system(content, 0)).await;
    }

    /// Mark task as ended
    pub async fn mark_ended(&self, task_id: &str) {
        let mut buffers = self.buffers.write().await;
        if let Some(buffer) = buffers.get_mut(task_id) {
            buffer.push_system("Task completed");
            buffer.mark_ended();

            // Persist if configured
            if let Some(ref dir) = self.persist_dir {
                self.persist_buffer_inner(buffer, dir);
            }
        }
    }

    fn persist_buffer_inner(&self, buffer: &TaskLogBuffer, dir: &PathBuf) {
        let path = dir.join(format!("{}.log.json", buffer.task_id));
        if let Ok(json) = serde_json::to_string_pretty(&buffer.entries.iter().collect::<Vec<_>>()) {
            if let Err(e) = std::fs::write(&path, json) {
                debug!("Failed to persist log: {}", e);
            }
        }
    }

    /// Subscribe to task logs
    pub async fn subscribe(&self, task_id: &str) -> Option<broadcast::Receiver<LogEntry>> {
        let buffers = self.buffers.read().await;
        buffers.get(task_id).map(|b| b.subscribe())
    }

    /// Get analysis report for a task
    pub async fn get_analysis(&self, task_id: &str) -> Option<LogAnalysisReport> {
        let buffers = self.buffers.read().await;
        buffers.get(task_id).map(|b| b.generate_analysis_report())
    }

    /// Get tail of logs
    pub async fn tail(&self, task_id: &str, n: usize) -> Vec<LogEntry> {
        let buffers = self.buffers.read().await;
        buffers
            .get(task_id)
            .map(|b| b.tail(n).into_iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Get errors
    pub async fn errors(&self, task_id: &str) -> Vec<LogEntry> {
        let buffers = self.buffers.read().await;
        buffers
            .get(task_id)
            .map(|b| b.errors().into_iter().cloned().collect())
            .unwrap_or_default()
    }

    /// List all active task IDs
    pub async fn active_tasks(&self) -> Vec<String> {
        let buffers = self.buffers.read().await;
        buffers
            .iter()
            .filter(|(_, b)| b.is_active())
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// List all task IDs
    pub async fn all_tasks(&self) -> Vec<String> {
        let buffers = self.buffers.read().await;
        buffers.keys().cloned().collect()
    }

    /// Cleanup old buffers
    pub async fn cleanup(&self) {
        let mut buffers = self.buffers.write().await;
        self.cleanup_old_buffers_inner(&mut buffers);
    }
}

impl Default for TaskLogManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_entry() {
        let entry = LogEntry::stdout("Hello world", 1);
        assert_eq!(entry.level, LogLevel::Stdout);
        assert_eq!(entry.content, "Hello world");
        assert_eq!(entry.line_number, 1);
    }

    #[test]
    fn test_log_buffer() {
        let mut buffer = TaskLogBuffer::new("task-1");
        buffer.push_stdout("Line 1");
        buffer.push_stderr("Error");
        buffer.push_stdout("Line 2");

        assert_eq!(buffer.line_count(), 3);
        assert_eq!(buffer.errors().len(), 1);
        assert_eq!(buffer.tail(2).len(), 2);
    }

    #[test]
    fn test_analysis_report() {
        let mut buffer = TaskLogBuffer::new("task-1").with_command("cargo build");
        buffer.push_stdout("Compiling...");
        buffer.push_stderr("error[E0001]: test error");
        buffer.push_stderr("error[E0001]: another error");
        buffer.push_stdout("Done");
        buffer.mark_ended();

        let report = buffer.generate_analysis_report();
        assert_eq!(report.total_lines, 4);
        assert_eq!(report.error_count, 2);
        assert!(!report.is_active);
    }

    #[tokio::test]
    async fn test_log_manager() {
        let manager = TaskLogManager::new();

        let _rx = manager.create_buffer("task-1", Some("cargo test")).await;
        manager.push_stdout("task-1", "Running tests...").await;
        manager.push_stderr("task-1", "1 test failed").await;
        manager.mark_ended("task-1").await;

        let report = manager.get_analysis("task-1").await.unwrap();
        assert_eq!(report.total_lines, 4); // Including system messages
        assert!(!report.is_active);
    }

    #[tokio::test]
    async fn test_subscribe() {
        let manager = TaskLogManager::new();

        // Create buffer with a command so that the system message is sent
        let mut rx = manager.create_buffer("task-1", Some("echo test")).await;

        // Receive the system message first
        let entry = rx.recv().await.unwrap();
        assert_eq!(entry.content, "Starting task: echo test");
        assert_eq!(entry.level, LogLevel::System);

        // Now push and receive stdout
        manager.push_stdout("task-1", "Hello").await;

        let entry = rx.recv().await.unwrap();
        assert_eq!(entry.content, "Hello");
        assert_eq!(entry.level, LogLevel::Stdout);
    }
}
