//! Task Progress Widget
//!
//! Displays running tasks with progress bars.
//! Implements Layer1's TaskObserver trait for TUI.

#![allow(dead_code)]

use forge_foundation::core::traits::{TaskObserver, TaskResult, TaskState};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph},
    Frame,
};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Instant;

/// Information about a running task
#[derive(Clone, Debug)]
pub struct TaskInfo {
    /// Task ID
    pub id: String,
    /// Current state
    pub state: TaskState,
    /// Progress (0.0 - 1.0)
    pub progress: f32,
    /// Current status message
    pub message: String,
    /// When the task started
    pub start_time: Instant,
    /// Tool name (if applicable)
    pub tool_name: Option<String>,
}

impl TaskInfo {
    /// Create a new task info
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            state: TaskState::Pending,
            progress: 0.0,
            message: "Starting...".to_string(),
            start_time: Instant::now(),
            tool_name: None,
        }
    }

    /// Get elapsed time in seconds
    pub fn elapsed_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    /// Format elapsed time as human-readable string
    pub fn elapsed_string(&self) -> String {
        let secs = self.elapsed_secs();
        if secs < 60 {
            format!("{}s", secs)
        } else {
            format!("{}m {}s", secs / 60, secs % 60)
        }
    }

    /// Get color based on state
    pub fn state_color(&self) -> Color {
        match self.state {
            TaskState::Pending => Color::DarkGray,
            TaskState::Running => Color::Cyan,
            TaskState::Paused => Color::Yellow,
            TaskState::Completed => Color::Green,
            TaskState::Failed => Color::Red,
            TaskState::Cancelled => Color::Magenta,
        }
    }

    /// Get state as short string
    pub fn state_str(&self) -> &'static str {
        match self.state {
            TaskState::Pending => "PEND",
            TaskState::Running => "RUN",
            TaskState::Paused => "PAUSE",
            TaskState::Completed => "DONE",
            TaskState::Failed => "FAIL",
            TaskState::Cancelled => "STOP",
        }
    }
}

/// Widget for displaying task progress
pub struct TaskProgressWidget {
    /// Active tasks
    tasks: Arc<RwLock<HashMap<String, TaskInfo>>>,
    /// Maximum tasks to display
    max_display: usize,
    /// Whether to show completed tasks briefly
    show_completed: bool,
}

impl TaskProgressWidget {
    /// Create a new task progress widget
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
            max_display: 5,
            show_completed: true,
        }
    }

    /// Create with shared task state
    pub fn with_tasks(tasks: Arc<RwLock<HashMap<String, TaskInfo>>>) -> Self {
        Self {
            tasks,
            max_display: 5,
            show_completed: true,
        }
    }

    /// Add a new task
    pub fn add_task(&self, task_id: &str, tool_name: Option<&str>) {
        if let Ok(mut tasks) = self.tasks.write() {
            let mut info = TaskInfo::new(task_id);
            info.tool_name = tool_name.map(String::from);
            tasks.insert(task_id.to_string(), info);
        }
    }

    /// Update task state
    pub fn update_state(&self, task_id: &str, state: TaskState) {
        if let Ok(mut tasks) = self.tasks.write() {
            if let Some(info) = tasks.get_mut(task_id) {
                info.state = state;
            }
        }
    }

    /// Update task progress
    pub fn update_progress(&self, task_id: &str, progress: f32, message: &str) {
        if let Ok(mut tasks) = self.tasks.write() {
            if let Some(info) = tasks.get_mut(task_id) {
                info.progress = progress.clamp(0.0, 1.0);
                info.message = message.to_string();
            }
        }
    }

    /// Remove a task
    pub fn remove_task(&self, task_id: &str) {
        if let Ok(mut tasks) = self.tasks.write() {
            tasks.remove(task_id);
        }
    }

    /// Get number of active tasks
    pub fn active_count(&self) -> usize {
        self.tasks
            .read()
            .map(|t| {
                t.values()
                    .filter(|i| matches!(i.state, TaskState::Running | TaskState::Pending))
                    .count()
            })
            .unwrap_or(0)
    }

    /// Check if there are any tasks to display
    pub fn has_tasks(&self) -> bool {
        self.tasks.read().map(|t| !t.is_empty()).unwrap_or(false)
    }

    /// Calculate required height for rendering
    pub fn required_height(&self) -> u16 {
        let count = self.tasks.read().map(|t| t.len()).unwrap_or(0);
        if count == 0 {
            0
        } else {
            // Border (2) + tasks (2 lines each, max 5)
            2 + (count.min(self.max_display) * 2) as u16
        }
    }

    /// Render the widget
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let tasks = match self.tasks.read() {
            Ok(t) => t,
            Err(_) => return,
        };

        if tasks.is_empty() {
            return;
        }

        // Create block
        let active = tasks
            .values()
            .filter(|i| matches!(i.state, TaskState::Running | TaskState::Pending))
            .count();
        let title = format!(" Tasks ({} active) ", active);
        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Sort tasks: running first, then by start time
        let mut sorted_tasks: Vec<_> = tasks.values().collect();
        sorted_tasks.sort_by(|a, b| {
            let a_running = matches!(a.state, TaskState::Running);
            let b_running = matches!(b.state, TaskState::Running);
            match (a_running, b_running) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.start_time.cmp(&b.start_time),
            }
        });

        // Render each task (2 lines per task)
        let task_height = 2u16;
        for (i, task) in sorted_tasks.iter().take(self.max_display).enumerate() {
            let y = inner.y + (i as u16 * task_height);
            if y + task_height > inner.y + inner.height {
                break;
            }

            let task_area = Rect::new(inner.x, y, inner.width, task_height);
            self.render_task(frame, task_area, task);
        }

        // Show "+N more" if there are hidden tasks
        if sorted_tasks.len() > self.max_display {
            let more_count = sorted_tasks.len() - self.max_display;
            let more_text = format!("+{} more...", more_count);
            let more_para = Paragraph::new(more_text).style(Style::default().fg(Color::DarkGray));
            let more_area = Rect::new(inner.x, inner.y + inner.height - 1, inner.width, 1);
            frame.render_widget(more_para, more_area);
        }
    }

    /// Render a single task
    fn render_task(&self, frame: &mut Frame, area: Rect, task: &TaskInfo) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(1)])
            .split(area);

        // First line: state, id, tool name, elapsed time
        let id_short = if task.id.len() > 8 {
            &task.id[..8]
        } else {
            &task.id
        };

        let mut spans = vec![
            Span::styled(
                format!("[{}] ", task.state_str()),
                Style::default()
                    .fg(task.state_color())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(id_short, Style::default().fg(Color::White)),
        ];

        if let Some(tool) = &task.tool_name {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                format!("({})", tool),
                Style::default().fg(Color::Magenta),
            ));
        }

        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            task.elapsed_string(),
            Style::default().fg(Color::DarkGray),
        ));

        let header = Paragraph::new(Line::from(spans));
        frame.render_widget(header, chunks[0]);

        // Second line: progress bar or message
        if matches!(task.state, TaskState::Running) && task.progress > 0.0 {
            let gauge = Gauge::default()
                .ratio(task.progress as f64)
                .gauge_style(Style::default().fg(Color::Cyan).bg(Color::DarkGray))
                .label(format!(
                    "{:.0}% - {}",
                    task.progress * 100.0,
                    truncate_string(&task.message, 30)
                ));
            frame.render_widget(gauge, chunks[1]);
        } else {
            let msg = Paragraph::new(truncate_string(&task.message, area.width as usize - 2))
                .style(Style::default().fg(Color::Gray));
            frame.render_widget(msg, chunks[1]);
        }
    }
}

impl Default for TaskProgressWidget {
    fn default() -> Self {
        Self::new()
    }
}

/// TUI implementation of TaskObserver trait
pub struct TuiTaskObserver {
    /// Shared widget state
    widget: Arc<RwLock<HashMap<String, TaskInfo>>>,
}

impl TuiTaskObserver {
    /// Create a new observer
    pub fn new() -> Self {
        Self {
            widget: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get the shared task state for use with TaskProgressWidget
    pub fn tasks(&self) -> Arc<RwLock<HashMap<String, TaskInfo>>> {
        self.widget.clone()
    }

    /// Create a widget that shares state with this observer
    pub fn create_widget(&self) -> TaskProgressWidget {
        TaskProgressWidget::with_tasks(self.widget.clone())
    }
}

impl Default for TuiTaskObserver {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskObserver for TuiTaskObserver {
    fn on_state_change(&self, task_id: &str, state: TaskState) {
        if let Ok(mut tasks) = self.widget.write() {
            if let Some(info) = tasks.get_mut(task_id) {
                info.state = state;
            } else {
                // New task
                let mut info = TaskInfo::new(task_id);
                info.state = state;
                tasks.insert(task_id.to_string(), info);
            }
        }
    }

    fn on_progress(&self, task_id: &str, progress: f32, message: &str) {
        if let Ok(mut tasks) = self.widget.write() {
            if let Some(info) = tasks.get_mut(task_id) {
                info.progress = progress.clamp(0.0, 1.0);
                info.message = message.to_string();
            }
        }
    }

    fn on_complete(&self, task_id: &str, _result: &TaskResult) {
        if let Ok(mut tasks) = self.widget.write() {
            // Keep completed task briefly for display, then remove
            if let Some(info) = tasks.get_mut(task_id) {
                info.state = TaskState::Completed;
                info.progress = 1.0;
                info.message = "Completed".to_string();
            }
            // Note: In practice, you'd want to schedule removal after a delay
        }
    }
}

/// Truncate a string to fit within a certain width
fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else if max_len > 3 {
        format!("{}...", &s[..max_len - 3])
    } else {
        s[..max_len].to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_info_creation() {
        let info = TaskInfo::new("test-123");
        assert_eq!(info.id, "test-123");
        assert_eq!(info.state, TaskState::Pending);
        assert_eq!(info.progress, 0.0);
    }

    #[test]
    fn test_widget_add_remove() {
        let widget = TaskProgressWidget::new();

        widget.add_task("task-1", Some("bash"));
        assert!(widget.has_tasks());
        assert_eq!(widget.active_count(), 1);

        widget.update_state("task-1", TaskState::Running);
        widget.update_progress("task-1", 0.5, "Working...");

        widget.remove_task("task-1");
        assert!(!widget.has_tasks());
    }

    #[test]
    fn test_observer_state_change() {
        let observer = TuiTaskObserver::new();

        observer.on_state_change("task-1", TaskState::Running);

        let tasks = observer.tasks();
        let guard = tasks.read().unwrap();
        assert!(guard.contains_key("task-1"));
        assert_eq!(guard.get("task-1").unwrap().state, TaskState::Running);
    }

    #[test]
    fn test_truncate_string() {
        assert_eq!(truncate_string("hello", 10), "hello");
        assert_eq!(truncate_string("hello world", 8), "hello...");
        assert_eq!(truncate_string("hi", 2), "hi");
    }
}
