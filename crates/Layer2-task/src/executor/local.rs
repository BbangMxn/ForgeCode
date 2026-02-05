//! Local executor - runs tasks on the host system with log streaming
//!
//! Features:
//! - Real-time stdout/stderr streaming
//! - Process cancellation support
//! - Log integration for LLM analysis
//! - Exit code tracking
//! - Advanced timeout handling (soft/hard)
//! - Graceful shutdown with SIGTERM -> SIGKILL escalation

use crate::executor::Executor;
use crate::log::{LogEntry, TaskLogManager};
use crate::task::{ExecutionMode, Task, TaskResult};
use async_trait::async_trait;
use forge_foundation::{Error, Result};
use futures::FutureExt;
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, error, info, warn};

/// Timeout policy for task execution
#[derive(Debug, Clone)]
pub enum TimeoutPolicy {
    /// No timeout (dangerous for production)
    None,
    /// Hard timeout - kill immediately when exceeded
    Hard(Duration),
    /// Soft timeout with grace period
    /// (soft_timeout, grace_period) - sends SIGTERM at soft, SIGKILL at soft + grace
    Graceful {
        soft_timeout: Duration,
        grace_period: Duration,
    },
    /// Progressive timeout - warning, soft, hard
    Progressive {
        /// Warning timeout (log warning, continue)
        warning: Duration,
        /// Soft timeout (send SIGTERM)
        soft: Duration,
        /// Hard timeout (send SIGKILL)
        hard: Duration,
    },
}

impl Default for TimeoutPolicy {
    fn default() -> Self {
        Self::Hard(Duration::from_secs(120)) // 2 minutes default
    }
}

impl TimeoutPolicy {
    /// Create a hard timeout from seconds
    pub fn hard_secs(secs: u64) -> Self {
        Self::Hard(Duration::from_secs(secs))
    }

    /// Create a graceful timeout
    pub fn graceful(timeout_secs: u64, grace_secs: u64) -> Self {
        Self::Graceful {
            soft_timeout: Duration::from_secs(timeout_secs),
            grace_period: Duration::from_secs(grace_secs),
        }
    }

    /// Create a progressive timeout
    pub fn progressive(warning_secs: u64, soft_secs: u64, hard_secs: u64) -> Self {
        Self::Progressive {
            warning: Duration::from_secs(warning_secs),
            soft: Duration::from_secs(soft_secs),
            hard: Duration::from_secs(hard_secs),
        }
    }

    /// Get the maximum timeout duration
    pub fn max_duration(&self) -> Option<Duration> {
        match self {
            Self::None => None,
            Self::Hard(d) => Some(*d),
            Self::Graceful {
                soft_timeout,
                grace_period,
            } => Some(*soft_timeout + *grace_period),
            Self::Progressive { hard, .. } => Some(*hard),
        }
    }
}

/// Timeout state tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeoutState {
    /// Not started
    Pending,
    /// Running normally
    Running,
    /// Warning threshold reached
    Warning,
    /// Soft timeout - SIGTERM sent
    SoftTimeout,
    /// Hard timeout - SIGKILL sent
    HardTimeout,
    /// Completed before timeout
    Completed,
}

/// Running process info
struct ProcessInfo {
    /// Child process handle
    child: Child,

    /// Task ID
    task_id: String,

    /// Start time
    started_at: std::time::Instant,

    /// Kill flag
    kill_requested: bool,

    /// Timeout policy
    timeout_policy: TimeoutPolicy,

    /// Current timeout state
    timeout_state: TimeoutState,
}

/// Local executor configuration
#[derive(Debug, Clone)]
pub struct LocalExecutorConfig {
    /// Default timeout policy
    pub default_timeout_policy: TimeoutPolicy,
    /// Enable process group killing (Unix only)
    pub kill_process_group: bool,
    /// Grace period after soft kill before hard kill
    pub default_grace_period: Duration,
}

impl Default for LocalExecutorConfig {
    fn default() -> Self {
        Self {
            default_timeout_policy: TimeoutPolicy::Graceful {
                soft_timeout: Duration::from_secs(120),
                grace_period: Duration::from_secs(5),
            },
            kill_process_group: true,
            default_grace_period: Duration::from_secs(5),
        }
    }
}

/// Local executor that runs tasks directly on the host
pub struct LocalExecutor {
    /// Running processes by task ID
    processes: Arc<RwLock<HashMap<String, Arc<Mutex<ProcessInfo>>>>>,

    /// Log manager
    log_manager: Arc<TaskLogManager>,

    /// Configuration
    config: LocalExecutorConfig,
}

impl LocalExecutor {
    /// Create a new local executor
    pub fn new() -> Self {
        Self {
            // Pre-allocate for typical concurrent task count
            processes: Arc::new(RwLock::new(HashMap::with_capacity(16))),
            log_manager: Arc::new(TaskLogManager::new()),
            config: LocalExecutorConfig::default(),
        }
    }

    /// Create with configuration
    pub fn with_config(config: LocalExecutorConfig) -> Self {
        Self {
            processes: Arc::new(RwLock::new(HashMap::with_capacity(16))),
            log_manager: Arc::new(TaskLogManager::new()),
            config,
        }
    }

    /// Create with custom log manager
    pub fn with_log_manager(log_manager: Arc<TaskLogManager>) -> Self {
        Self {
            processes: Arc::new(RwLock::new(HashMap::with_capacity(16))),
            log_manager,
            config: LocalExecutorConfig::default(),
        }
    }

    /// Create with config and log manager
    pub fn with_config_and_log_manager(
        config: LocalExecutorConfig,
        log_manager: Arc<TaskLogManager>,
    ) -> Self {
        Self {
            processes: Arc::new(RwLock::new(HashMap::with_capacity(16))),
            log_manager,
            config,
        }
    }

    /// Get the log manager
    pub fn log_manager(&self) -> Arc<TaskLogManager> {
        Arc::clone(&self.log_manager)
    }

    /// Get running processes
    pub async fn running_processes(&self) -> Vec<String> {
        self.processes.read().await.keys().cloned().collect()
    }

    /// Check if a task is running
    pub async fn is_running(&self, task_id: &str) -> bool {
        self.processes.read().await.contains_key(task_id)
    }

    /// Get logs for a task
    pub async fn get_logs(&self, task_id: &str, tail: Option<usize>) -> Vec<LogEntry> {
        match tail {
            Some(n) => self.log_manager.tail(task_id, n).await,
            None => {
                if let Some(buffer) = self.log_manager.get_buffer(task_id).await {
                    buffer.entries().cloned().collect()
                } else {
                    Vec::new()
                }
            }
        }
    }

    /// Get errors for a task
    pub async fn get_errors(&self, task_id: &str) -> Vec<LogEntry> {
        self.log_manager.errors(task_id).await
    }

    /// Get log analysis for LLM
    pub async fn get_log_analysis(&self, task_id: &str) -> Option<crate::log::LogAnalysisReport> {
        self.log_manager.get_analysis(task_id).await
    }

    /// Subscribe to real-time logs
    pub async fn subscribe_logs(
        &self,
        task_id: &str,
    ) -> Option<tokio::sync::broadcast::Receiver<LogEntry>> {
        self.log_manager.subscribe(task_id).await
    }

    /// Force kill a process
    pub async fn force_kill(&self, task_id: &str) -> Result<()> {
        let process_info = {
            let processes = self.processes.read().await;
            processes.get(task_id).cloned()
        };

        if let Some(info) = process_info {
            let mut info = info.lock().await;
            info.kill_requested = true;

            // Try to kill the process
            if let Err(e) = info.child.kill().await {
                error!("Failed to kill process {}: {}", task_id, e);
                return Err(Error::Task(format!("Failed to kill process: {}", e)));
            }

            self.log_manager
                .push_system(task_id, "Process forcefully terminated")
                .await;
            info!("Force killed process {}", task_id);
        }

        Ok(())
    }

    /// Get timeout state for a task
    pub async fn get_timeout_state(&self, task_id: &str) -> Option<TimeoutState> {
        let processes = self.processes.read().await;
        if let Some(info) = processes.get(task_id) {
            let info = info.lock().await;
            Some(info.timeout_state)
        } else {
            None
        }
    }

    /// Send graceful termination signal (SIGTERM on Unix, TerminateProcess on Windows)
    #[cfg(unix)]
    async fn send_sigterm(child: &Child) -> Result<()> {
        use nix::sys::signal::{kill, Signal};
        use nix::unistd::Pid;

        if let Some(pid) = child.id() {
            kill(Pid::from_raw(pid as i32), Signal::SIGTERM)
                .map_err(|e| Error::Task(format!("Failed to send SIGTERM: {}", e)))?;
        }
        Ok(())
    }

    #[cfg(windows)]
    async fn send_sigterm(child: &Child) -> Result<()> {
        // Windows doesn't have SIGTERM, we'll use a softer approach
        // by sending Ctrl+C event or just logging and proceeding to kill
        debug!("Windows doesn't support SIGTERM, will use kill");
        Ok(())
    }

    /// Execute timeout handling based on policy
    async fn handle_timeout_policy(
        &self,
        task_id: &str,
        process_info: &Arc<Mutex<ProcessInfo>>,
        elapsed: Duration,
    ) -> Option<TimeoutState> {
        let mut info = process_info.lock().await;
        let policy = info.timeout_policy.clone();

        match policy {
            TimeoutPolicy::None => None,
            TimeoutPolicy::Hard(timeout) => {
                if elapsed >= timeout {
                    info.timeout_state = TimeoutState::HardTimeout;
                    let _ = info.child.kill().await;
                    self.log_manager
                        .push_system(
                            task_id,
                            format!(
                                "Hard timeout ({:.1}s) - process killed",
                                timeout.as_secs_f64()
                            ),
                        )
                        .await;
                    Some(TimeoutState::HardTimeout)
                } else {
                    None
                }
            }
            TimeoutPolicy::Graceful {
                soft_timeout,
                grace_period,
            } => {
                if elapsed >= soft_timeout + grace_period {
                    // Hard kill
                    info.timeout_state = TimeoutState::HardTimeout;
                    let _ = info.child.kill().await;
                    self.log_manager
                        .push_system(task_id, "Grace period expired - process killed")
                        .await;
                    Some(TimeoutState::HardTimeout)
                } else if elapsed >= soft_timeout && info.timeout_state != TimeoutState::SoftTimeout
                {
                    // Soft timeout - send SIGTERM
                    info.timeout_state = TimeoutState::SoftTimeout;
                    #[cfg(unix)]
                    {
                        let _ = Self::send_sigterm(&info.child).await;
                    }
                    #[cfg(windows)]
                    {
                        // On Windows, we don't have a graceful option
                        // Log and continue, will hard kill after grace period
                    }
                    self.log_manager
                        .push_system(
                            task_id,
                            format!(
                                "Soft timeout ({:.1}s) - termination signal sent, {:.1}s grace period",
                                soft_timeout.as_secs_f64(),
                                grace_period.as_secs_f64()
                            ),
                        )
                        .await;
                    Some(TimeoutState::SoftTimeout)
                } else {
                    None
                }
            }
            TimeoutPolicy::Progressive {
                warning,
                soft,
                hard,
            } => {
                if elapsed >= hard {
                    info.timeout_state = TimeoutState::HardTimeout;
                    let _ = info.child.kill().await;
                    self.log_manager
                        .push_system(
                            task_id,
                            format!("Hard timeout ({:.1}s) - process killed", hard.as_secs_f64()),
                        )
                        .await;
                    Some(TimeoutState::HardTimeout)
                } else if elapsed >= soft && info.timeout_state != TimeoutState::SoftTimeout {
                    info.timeout_state = TimeoutState::SoftTimeout;
                    #[cfg(unix)]
                    {
                        let _ = Self::send_sigterm(&info.child).await;
                    }
                    self.log_manager
                        .push_system(
                            task_id,
                            format!(
                                "Soft timeout ({:.1}s) - termination signal sent",
                                soft.as_secs_f64()
                            ),
                        )
                        .await;
                    Some(TimeoutState::SoftTimeout)
                } else if elapsed >= warning && info.timeout_state == TimeoutState::Running {
                    info.timeout_state = TimeoutState::Warning;
                    self.log_manager
                        .push_system(
                            task_id,
                            format!(
                                "Warning: task running for {:.1}s (soft timeout at {:.1}s)",
                                elapsed.as_secs_f64(),
                                soft.as_secs_f64()
                            ),
                        )
                        .await;
                    warn!(
                        "Task {} warning timeout at {:.1}s",
                        task_id,
                        elapsed.as_secs_f64()
                    );
                    Some(TimeoutState::Warning)
                } else {
                    None
                }
            }
        }
    }
}

impl Default for LocalExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Executor for LocalExecutor {
    async fn execute(&self, task: &Task) -> Result<TaskResult> {
        // Ensure task is for local execution
        if !matches!(task.execution_mode, ExecutionMode::Local) {
            return Err(Error::Task(
                "LocalExecutor can only execute Local tasks".to_string(),
            ));
        }

        let task_id = task.id.to_string();

        // Create log buffer
        let _log_rx = self
            .log_manager
            .create_buffer(&task_id, Some(&task.command))
            .await;

        // Determine shell
        let (shell, shell_arg) = if cfg!(windows) {
            ("cmd", "/C")
        } else {
            ("sh", "-c")
        };

        // Build command
        let mut cmd = Command::new(shell);
        cmd.arg(shell_arg)
            .arg(&task.command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        // Inherit PATH and other important environment variables from parent process
        // This ensures commands like `cargo`, `npm`, etc. are available
        for (key, value) in std::env::vars() {
            cmd.env(&key, &value);
        }

        // Override with task-specific environment variables
        for (key, value) in &task.env {
            cmd.env(key, value);
        }

        debug!("Executing task {}: {}", task_id, task.command);

        // Spawn process
        let mut child = cmd
            .spawn()
            .map_err(|e| Error::Task(format!("Failed to spawn process: {}", e)))?;

        // Get stdout/stderr handles
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        // Determine timeout policy
        let timeout_policy = if task.timeout.as_secs() > 0 {
            // Use graceful policy with task timeout as soft timeout
            TimeoutPolicy::Graceful {
                soft_timeout: task.timeout,
                grace_period: self.config.default_grace_period,
            }
        } else {
            self.config.default_timeout_policy.clone()
        };

        // Store process info
        let process_info = Arc::new(Mutex::new(ProcessInfo {
            child,
            task_id: task_id.clone(),
            started_at: std::time::Instant::now(),
            kill_requested: false,
            timeout_policy: timeout_policy.clone(),
            timeout_state: TimeoutState::Running,
        }));

        {
            let mut processes = self.processes.write().await;
            processes.insert(task_id.clone(), Arc::clone(&process_info));
        }

        // Spawn log readers
        let log_manager = Arc::clone(&self.log_manager);
        let task_id_stdout = task_id.clone();

        let stdout_handle = if let Some(stdout) = stdout {
            let log_manager = Arc::clone(&log_manager);
            Some(tokio::spawn(async move {
                let mut reader = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    log_manager.push_stdout(&task_id_stdout, &line).await;
                }
            }))
        } else {
            None
        };

        let log_manager = Arc::clone(&self.log_manager);
        let task_id_stderr = task_id.clone();

        let stderr_handle = if let Some(stderr) = stderr {
            let log_manager = Arc::clone(&log_manager);
            Some(tokio::spawn(async move {
                let mut reader = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    log_manager.push_stderr(&task_id_stderr, &line).await;
                }
            }))
        } else {
            None
        };

        // Calculate max timeout from policy
        let max_timeout = timeout_policy
            .max_duration()
            .unwrap_or(Duration::from_secs(3600));

        // Wait for completion with advanced timeout handling
        let result = {
            // Create a timeout check interval
            let check_interval = Duration::from_millis(500);
            let started_at = std::time::Instant::now();

            // Spawn the main wait task
            let process_info_clone = Arc::clone(&process_info);
            let wait_task = tokio::spawn(async move {
                // Wait for readers first
                if let Some(h) = stdout_handle {
                    let _ = h.await;
                }
                if let Some(h) = stderr_handle {
                    let _ = h.await;
                }

                // Then wait for process
                let mut info = process_info_clone.lock().await;
                info.child.wait().await
            });

            // Run timeout monitoring loop with fused wait_task
            let mut wait_task_fused = wait_task.fuse();
            let wait_result: std::result::Result<
                std::result::Result<std::process::ExitStatus, std::io::Error>,
                String,
            > = loop {
                tokio::select! {
                    // Process completed
                    result = &mut wait_task_fused => {
                        match result {
                            Ok(inner) => break Ok(inner),
                            Err(_) => break Err("Task join error".to_string()),
                        }
                    }
                    // Timeout check interval
                    _ = tokio::time::sleep(check_interval) => {
                        let elapsed = started_at.elapsed();

                        // Check timeout policy
                        if let Some(state) = self.handle_timeout_policy(
                            &task_id,
                            &process_info,
                            elapsed
                        ).await {
                            if state == TimeoutState::HardTimeout {
                                // Process was killed, wait a bit for cleanup
                                tokio::time::sleep(Duration::from_millis(100)).await;
                                break Err("Hard timeout".to_string());
                            }
                        }

                        // Check max timeout (safety net)
                        if elapsed >= max_timeout {
                            warn!("Task {} max timeout ({:.1}s) reached", task_id, max_timeout.as_secs_f64());
                            let mut info = process_info.lock().await;
                            let _ = info.child.kill().await;
                            break Err("Max timeout reached".to_string());
                        }
                    }
                }
            };

            match wait_result {
                Ok(Ok(status)) => {
                    // Mark as completed
                    {
                        let mut info = process_info.lock().await;
                        info.timeout_state = TimeoutState::Completed;
                    }

                    let exit_code = status.code().unwrap_or(-1);

                    // Get output from log buffer
                    let output = if let Some(buffer) = self.log_manager.get_buffer(&task_id).await {
                        buffer
                            .entries()
                            .filter(|e| {
                                matches!(
                                    e.level,
                                    crate::log::LogLevel::Stdout | crate::log::LogLevel::Stderr
                                )
                            })
                            .map(|e| {
                                if e.level == crate::log::LogLevel::Stderr {
                                    format!("[stderr] {}", e.content)
                                } else {
                                    e.content.clone()
                                }
                            })
                            .collect::<Vec<_>>()
                            .join("\n")
                    } else {
                        String::new()
                    };

                    if exit_code != 0 {
                        self.log_manager
                            .push_system(
                                &task_id,
                                format!("Process exited with code {}", exit_code),
                            )
                            .await;
                    }

                    Ok(TaskResult::with_exit_code(output, exit_code))
                }
                Ok(Err(e)) => Err(Error::Task(format!("Process error: {}", e))),
                Err(_) => {
                    // Task join error or timeout
                    let timeout_state = {
                        let info = process_info.lock().await;
                        info.timeout_state
                    };

                    match timeout_state {
                        TimeoutState::SoftTimeout => Err(Error::Timeout(
                            "Task soft timeout - process terminated gracefully".to_string(),
                        )),
                        TimeoutState::HardTimeout => Err(Error::Timeout(
                            "Task hard timeout - process killed forcefully".to_string(),
                        )),
                        _ => {
                            warn!("Task {} timed out", task_id);
                            self.log_manager
                                .push_system(&task_id, "Task timed out, killing process")
                                .await;

                            let mut info = process_info.lock().await;
                            let _ = info.child.kill().await;

                            Err(Error::Timeout("Task execution timed out".to_string()))
                        }
                    }
                }
            }
        };

        // Cleanup
        {
            let mut processes = self.processes.write().await;
            processes.remove(&task_id);
        }

        // Mark log as ended
        self.log_manager.mark_ended(&task_id).await;

        result
    }

    async fn cancel(&self, task: &Task) -> Result<()> {
        let task_id = task.id.to_string();

        let process_info = {
            let processes = self.processes.read().await;
            processes.get(&task_id).cloned()
        };

        if let Some(info) = process_info {
            let mut info = info.lock().await;
            info.kill_requested = true;

            info.child
                .kill()
                .await
                .map_err(|e| Error::Task(format!("Failed to kill process: {}", e)))?;

            self.log_manager
                .push_system(&task_id, "Task cancelled by user")
                .await;
            info!("Cancelled task {}", task_id);
        }

        // Remove from processes
        {
            let mut processes = self.processes.write().await;
            processes.remove(&task_id);
        }

        // Mark log as ended
        self.log_manager.mark_ended(&task_id).await;

        Ok(())
    }

    fn is_available(&self) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "local"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::Task;

    #[tokio::test]
    async fn test_local_executor() {
        let executor = LocalExecutor::new();
        assert!(executor.is_available());
        assert_eq!(executor.name(), "local");
    }

    #[tokio::test]
    async fn test_execute_simple_command() {
        let executor = LocalExecutor::new();

        let task = Task::new("session-1", "bash", "echo hello", serde_json::json!({}));

        let result = executor.execute(&task).await;
        assert!(result.is_ok());

        let result = result.unwrap();
        // On Windows, the output might differ
        if !cfg!(windows) {
            assert!(result.output.contains("hello"));
        }
    }

    #[tokio::test]
    async fn test_log_collection() {
        let executor = LocalExecutor::new();

        let task = Task::new("session-1", "bash", "echo test_line", serde_json::json!({}));

        let _result = executor.execute(&task).await;

        // Check logs
        let logs = executor.get_logs(&task.id.to_string(), None).await;
        assert!(!logs.is_empty());
    }

    #[tokio::test]
    async fn test_running_processes() {
        let executor = LocalExecutor::new();
        let processes = executor.running_processes().await;
        assert!(processes.is_empty());
    }
}
