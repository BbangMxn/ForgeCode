//! PTY executor - runs tasks with full pseudo-terminal support
//!
//! Uses portable-pty for cross-platform PTY support, enabling:
//! - Interactive command execution (vim, htop, etc.)
//! - Proper ANSI escape sequence handling
//! - Terminal environment emulation
//! - **Background execution** for long-running servers
//!
//! This executor bridges Layer2-tool's ForgeCmd with Layer2-task.
//!
//! ## Security
//!
//! Environment variables are filtered based on `EnvSecurityConfig`:
//! - Blocked patterns (e.g., `AWS_*`, `*_TOKEN`) are removed before execution
//! - Allowed patterns take precedence over blocked patterns
//! - Sensitive values can be masked in output
//!
//! ## Background Execution
//!
//! For long-running processes (servers, watch commands), use `spawn_background`:
//! - Returns immediately after process starts
//! - Logs are collected asynchronously in background
//! - Use `get_logs()` to retrieve output
//! - Use `force_kill()` to terminate

use crate::executor::Executor;
use crate::log::{LogEntry, TaskLogManager};
use crate::state::TaskState;
use crate::task::{Task, TaskResult};
use async_trait::async_trait;
use forge_foundation::{Error, Result};
use portable_pty::{native_pty_system, CommandBuilder, PtyPair, PtySize};
use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Mutex, RwLock};
use tracing::{debug, error, info, warn};

/// PTY size configuration
#[derive(Debug, Clone, Copy)]
pub struct PtySizeConfig {
    pub rows: u16,
    pub cols: u16,
}

impl Default for PtySizeConfig {
    fn default() -> Self {
        Self {
            rows: 24,
            cols: 120,
        }
    }
}

/// PTY session state
struct PtySessionState {
    /// PTY pair (master + slave)
    pty: Option<PtyPair>,

    /// Child process handle
    child: Option<Box<dyn portable_pty::Child + Send + Sync>>,

    /// Task ID
    task_id: String,

    /// Command being executed
    command: String,

    /// Start time
    started_at: Instant,

    /// Kill requested flag
    kill_requested: Arc<AtomicBool>,

    /// Task status
    status: TaskState,

    /// Exit code (set when process completes)
    exit_code: Option<i32>,

    /// Background log collector handle
    log_collector_handle: Option<tokio::task::JoinHandle<()>>,
}

/// Environment security configuration for PTY
///
/// Controls which environment variables are accessible to AI-executed commands.
/// This prevents AI from accessing sensitive credentials like API keys.
#[derive(Debug, Clone)]
pub struct PtyEnvSecurityConfig {
    /// Patterns to block (e.g., "AWS_*", "*_TOKEN")
    /// Variables matching these patterns are removed from the environment
    pub blocked_patterns: Vec<String>,

    /// Patterns to allow (takes precedence over blocked)
    /// Use this to whitelist specific variables that would otherwise be blocked
    pub allowed_patterns: Vec<String>,

    /// Whether to mask sensitive values in output
    pub mask_in_output: bool,

    /// Character(s) to use for masking
    pub mask_char: String,
}

impl Default for PtyEnvSecurityConfig {
    fn default() -> Self {
        Self {
            blocked_patterns: vec![
                // Cloud credentials
                "AWS_*".to_string(),
                "AZURE_*".to_string(),
                "GCP_*".to_string(),
                "GOOGLE_*".to_string(),
                // Generic secrets
                "*_SECRET".to_string(),
                "*_SECRET_*".to_string(),
                "*_TOKEN".to_string(),
                "*_TOKEN_*".to_string(),
                "*_KEY".to_string(),
                "*_API_KEY".to_string(),
                "*_APIKEY".to_string(),
                "*_PASSWORD".to_string(),
                "*_PASS".to_string(),
                "*_AUTH".to_string(),
                "*_PRIVATE_*".to_string(),
                "*_CREDENTIALS".to_string(),
                // Specific services
                "ANTHROPIC_API_KEY".to_string(),
                "OPENAI_API_KEY".to_string(),
                "CLAUDE_API_KEY".to_string(),
                "GITHUB_TOKEN".to_string(),
                "GITLAB_TOKEN".to_string(),
                "NPM_TOKEN".to_string(),
                // Database
                "DATABASE_URL".to_string(),
                "DATABASE_PASSWORD".to_string(),
                "MONGODB_URI".to_string(),
                "REDIS_URL".to_string(),
                "REDIS_PASSWORD".to_string(),
                // SSH/GPG
                "SSH_*".to_string(),
                "GPG_*".to_string(),
            ],
            allowed_patterns: vec![
                // System essentials
                "PATH".to_string(),
                "HOME".to_string(),
                "USER".to_string(),
                "SHELL".to_string(),
                "TERM".to_string(),
                "LANG".to_string(),
                "LC_*".to_string(),
                "TZ".to_string(),
                // Development
                "NODE_ENV".to_string(),
                "RUST_LOG".to_string(),
                "RUST_BACKTRACE".to_string(),
                "CARGO_*".to_string(),
                "RUSTUP_*".to_string(),
                "DEBUG".to_string(),
                "VERBOSE".to_string(),
                // Editor
                "EDITOR".to_string(),
                "VISUAL".to_string(),
                // Display
                "DISPLAY".to_string(),
                "COLORTERM".to_string(),
            ],
            mask_in_output: true,
            mask_char: "***".to_string(),
        }
    }
}

impl PtyEnvSecurityConfig {
    /// Check if an environment variable should be blocked
    pub fn is_blocked(&self, name: &str) -> bool {
        // Check allowed patterns first (they take precedence)
        for pattern in &self.allowed_patterns {
            if pattern_matches(pattern, name) {
                return false;
            }
        }

        // Check blocked patterns
        for pattern in &self.blocked_patterns {
            if pattern_matches(pattern, name) {
                return true;
            }
        }

        false
    }

    /// Filter environment variables, removing blocked ones
    pub fn filter_env(&self, env: &HashMap<String, String>) -> HashMap<String, String> {
        env.iter()
            .filter(|(k, _)| !self.is_blocked(k))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// Mask a value if the variable is sensitive
    pub fn mask_value(&self, name: &str, value: &str) -> String {
        if self.mask_in_output && self.is_blocked(name) {
            self.mask_char.clone()
        } else {
            value.to_string()
        }
    }

    /// Mask sensitive values in output text
    pub fn mask_output(&self, output: &str, env: &HashMap<String, String>) -> String {
        if !self.mask_in_output {
            return output.to_string();
        }

        let mut result = output.to_string();
        for (name, value) in env {
            if self.is_blocked(name) && !value.is_empty() && value.len() > 3 {
                // Only mask if value is long enough to be meaningful
                result = result.replace(value, &self.mask_char);
            }
        }
        result
    }
}

/// PTY executor configuration
#[derive(Debug, Clone)]
pub struct PtyExecutorConfig {
    /// PTY size
    pub pty_size: PtySizeConfig,

    /// Shell to use
    pub shell: String,

    /// Default timeout
    pub default_timeout: Duration,

    /// Environment security configuration
    pub env_security: PtyEnvSecurityConfig,
}

impl Default for PtyExecutorConfig {
    fn default() -> Self {
        let shell = if cfg!(windows) {
            "powershell".to_string()
        } else {
            std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())
        };

        Self {
            pty_size: PtySizeConfig::default(),
            shell,
            default_timeout: Duration::from_secs(300),
            env_security: PtyEnvSecurityConfig::default(),
        }
    }
}

impl PtyExecutorConfig {
    /// Create config with custom environment security settings
    pub fn with_env_security(mut self, env_security: PtyEnvSecurityConfig) -> Self {
        self.env_security = env_security;
        self
    }

    /// Add blocked patterns
    pub fn block_env_patterns(mut self, patterns: Vec<String>) -> Self {
        self.env_security.blocked_patterns.extend(patterns);
        self
    }

    /// Add allowed patterns
    pub fn allow_env_patterns(mut self, patterns: Vec<String>) -> Self {
        self.env_security.allowed_patterns.extend(patterns);
        self
    }
}

/// PTY executor that runs tasks with full terminal emulation
pub struct PtyExecutor {
    /// Active sessions by task ID
    sessions: Arc<RwLock<HashMap<String, Arc<Mutex<PtySessionState>>>>>,

    /// Log manager
    log_manager: Arc<TaskLogManager>,

    /// Configuration
    config: PtyExecutorConfig,
}

impl PtyExecutor {
    /// Create a new PTY executor
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            log_manager: Arc::new(TaskLogManager::new()),
            config: PtyExecutorConfig::default(),
        }
    }

    /// Create with custom configuration
    pub fn with_config(config: PtyExecutorConfig) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            log_manager: Arc::new(TaskLogManager::new()),
            config,
        }
    }

    /// Create with custom log manager
    pub fn with_log_manager(log_manager: Arc<TaskLogManager>) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            log_manager,
            config: PtyExecutorConfig::default(),
        }
    }

    /// Get the log manager
    pub fn log_manager(&self) -> Arc<TaskLogManager> {
        Arc::clone(&self.log_manager)
    }

    /// Get active sessions
    pub async fn active_sessions(&self) -> Vec<String> {
        self.sessions.read().await.keys().cloned().collect()
    }

    /// Check if a task is running
    pub async fn is_running(&self, task_id: &str) -> bool {
        self.sessions.read().await.contains_key(task_id)
    }

    /// Force kill a session
    pub async fn force_kill(&self, task_id: &str) -> Result<()> {
        let session = {
            let sessions = self.sessions.read().await;
            sessions.get(task_id).cloned()
        };

        if let Some(session) = session {
            let mut state = session.lock().await;
            state.kill_requested.store(true, Ordering::SeqCst);
            state.status = TaskState::Cancelled;

            if let Some(ref mut child) = state.child {
                if let Err(e) = child.kill() {
                    error!("Failed to kill PTY process {}: {}", task_id, e);
                    return Err(Error::Task(format!("Failed to kill process: {}", e)));
                }
            }

            // Cancel the log collector if running
            if let Some(handle) = state.log_collector_handle.take() {
                handle.abort();
            }

            self.log_manager
                .push_system(task_id, "PTY process forcefully terminated")
                .await;
            self.log_manager.mark_ended(task_id).await;
            info!("Force killed PTY session {}", task_id);
        }

        // Remove from sessions
        {
            let mut sessions = self.sessions.write().await;
            sessions.remove(task_id);
        }

        Ok(())
    }

    /// Get task status
    pub async fn get_status(&self, task_id: &str) -> Option<TaskState> {
        let sessions = self.sessions.read().await;
        if let Some(session) = sessions.get(task_id) {
            let state = session.lock().await;
            Some(state.status.clone())
        } else {
            None
        }
    }

    /// Check if a task has completed
    pub async fn is_completed(&self, task_id: &str) -> bool {
        if let Some(status) = self.get_status(task_id).await {
            status.is_terminal()
        } else {
            true // Not found = completed/cleaned up
        }
    }

    /// Get exit code for a completed task
    pub async fn get_exit_code(&self, task_id: &str) -> Option<i32> {
        let sessions = self.sessions.read().await;
        if let Some(session) = sessions.get(task_id) {
            let state = session.lock().await;
            state.exit_code
        } else {
            None
        }
    }

    /// Spawn a task in background (returns immediately)
    /// Use get_logs() to retrieve output, force_kill() to terminate
    pub async fn spawn_background(&self, task: &Task) -> Result<()> {
        let task_id = task.id.to_string();
        let pty_system = native_pty_system();

        // Create PTY
        let size = PtySize {
            rows: self.config.pty_size.rows,
            cols: self.config.pty_size.cols,
            pixel_width: 0,
            pixel_height: 0,
        };

        let pty = pty_system
            .openpty(size)
            .map_err(|e| Error::Task(format!("Failed to open PTY: {}", e)))?;

        // Build command - use appropriate shell flag for platform
        let mut cmd = CommandBuilder::new(&self.config.shell);
        if cfg!(windows) {
            // PowerShell uses -Command, not -c
            cmd.arg("-Command");
        } else {
            cmd.arg("-c");
        }
        cmd.arg(&task.command);

        // Set working directory if provided
        if let Some(cwd_value) = task.input.get("cwd") {
            if let Some(cwd_str) = cwd_value.as_str() {
                cmd.cwd(PathBuf::from(cwd_str));
            }
        } else if let Some(cwd_value) = task.input.get("working_dir") {
            if let Some(cwd_str) = cwd_value.as_str() {
                cmd.cwd(PathBuf::from(cwd_str));
            }
        }

        // Set environment
        for (key, value) in self.build_env() {
            cmd.env(key, value);
        }

        // Add task-specific environment
        for (key, value) in &task.env {
            cmd.env(key, value);
        }

        // Create log buffer
        let _log_rx = self
            .log_manager
            .create_buffer(&task_id, Some(&task.command))
            .await;

        info!("Spawning background PTY task {}: {}", task_id, task.command);

        // Spawn child process
        let child = pty
            .slave
            .spawn_command(cmd)
            .map_err(|e| Error::Task(format!("Failed to spawn PTY command: {}", e)))?;

        // Get reader for log collection
        let reader = pty
            .master
            .try_clone_reader()
            .map_err(|e| Error::Task(format!("Failed to clone PTY reader: {}", e)))?;

        let kill_requested = Arc::new(AtomicBool::new(false));

        // Store session state
        let session_state = Arc::new(Mutex::new(PtySessionState {
            pty: Some(pty),
            child: Some(child),
            task_id: task_id.clone(),
            command: task.command.clone(),
            started_at: Instant::now(),
            kill_requested: Arc::clone(&kill_requested),
            status: TaskState::Running,
            exit_code: None,
            log_collector_handle: None,
        }));

        // Spawn background log collector
        let log_manager = Arc::clone(&self.log_manager);
        let task_id_clone = task_id.clone();
        let session_clone = Arc::clone(&session_state);
        let timeout = task.timeout;
        let env_security = self.config.env_security.clone();
        let system_env = self.get_system_env();

        let log_handle = tokio::spawn(async move {
            Self::background_log_collector(
                reader,
                log_manager,
                task_id_clone,
                session_clone,
                timeout,
                kill_requested,
                env_security,
                system_env,
            )
            .await;
        });

        // Store the handle
        {
            let mut state = session_state.lock().await;
            state.log_collector_handle = Some(log_handle);
        }

        // Add to sessions
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(task_id.clone(), session_state);
        }

        info!("Background PTY task {} started successfully", task_id);
        Ok(())
    }

    /// Background log collector task
    async fn background_log_collector(
        mut reader: Box<dyn Read + Send>,
        log_manager: Arc<TaskLogManager>,
        task_id: String,
        session: Arc<Mutex<PtySessionState>>,
        timeout: Duration,
        kill_requested: Arc<AtomicBool>,
        env_security: PtyEnvSecurityConfig,
        system_env: HashMap<String, String>,
    ) {
        let start = Instant::now();
        let mut buf = [0u8; 4096];
        let mut accumulated_output = String::new();

        // Use spawn_blocking for the synchronous read operations
        let (tx, mut rx) = mpsc::channel::<Vec<u8>>(100);

        let reader_handle = tokio::task::spawn_blocking(move || {
            loop {
                if kill_requested.load(Ordering::SeqCst) {
                    break;
                }

                match reader.read(&mut buf) {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        let chunk = buf[..n].to_vec();
                        if tx.blocking_send(chunk).is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        if e.kind() == std::io::ErrorKind::WouldBlock {
                            std::thread::sleep(Duration::from_millis(10));
                            continue;
                        }
                        break;
                    }
                }
            }
        });

        // Process incoming output
        loop {
            tokio::select! {
                chunk = rx.recv() => {
                    match chunk {
                        Some(data) => {
                            let text = String::from_utf8_lossy(&data);
                            let clean_text = strip_ansi(&text);
                            let masked_text = env_security.mask_output(&clean_text, &system_env);

                            accumulated_output.push_str(&masked_text);
                            log_manager.push_stdout(&task_id, &masked_text).await;
                        }
                        None => break, // Channel closed
                    }
                }
                _ = tokio::time::sleep(Duration::from_millis(100)) => {
                    // Check timeout
                    if start.elapsed() > timeout {
                        warn!("PTY task {} timed out after {:?}", task_id, timeout);
                        let mut state = session.lock().await;
                        state.status = TaskState::Timeout;
                        state.kill_requested.store(true, Ordering::SeqCst);
                        if let Some(ref mut child) = state.child {
                            let _ = child.kill();
                        }
                        log_manager.push_system(&task_id, "Task timed out").await;
                        break;
                    }

                    // Check if process has exited
                    let mut state = session.lock().await;
                    if let Some(ref mut child) = state.child {
                        match child.try_wait() {
                            Ok(Some(status)) => {
                                let exit_code = status.exit_code() as i32;
                                state.exit_code = Some(exit_code);
                                state.status = if exit_code == 0 {
                                    TaskState::Completed(TaskResult::success(accumulated_output.clone()))
                                } else {
                                    TaskState::Failed(format!("Exit code: {}", exit_code))
                                };
                                info!("PTY task {} completed with exit code {}", task_id, exit_code);
                                break;
                            }
                            Ok(None) => {
                                // Still running
                            }
                            Err(e) => {
                                error!("Error checking process status: {}", e);
                                state.status = TaskState::Failed(e.to_string());
                                break;
                            }
                        }
                    }
                }
            }
        }

        // Wait for reader to finish
        let _ = reader_handle.await;

        // Mark log as ended
        log_manager.mark_ended(&task_id).await;

        // Update final status
        let mut state = session.lock().await;
        if matches!(state.status, TaskState::Running) {
            state.status = TaskState::Completed(TaskResult::success(accumulated_output));
        }
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

    /// Build filtered environment using security configuration
    fn build_env(&self) -> HashMap<String, String> {
        let system_env: HashMap<String, String> = std::env::vars().collect();

        // Filter using security config
        let mut env = self.config.env_security.filter_env(&system_env);

        // Ensure TERM is set for proper terminal behavior
        env.insert("TERM".to_string(), "xterm-256color".to_string());

        env
    }

    /// Get the original system environment (for output masking)
    fn get_system_env(&self) -> HashMap<String, String> {
        std::env::vars().collect()
    }

    /// Execute command in PTY (synchronous - waits for completion)
    /// For long-running processes, use spawn_background() instead
    async fn execute_in_pty(&self, task: &Task) -> Result<TaskResult> {
        let task_id = task.id.to_string();
        let pty_system = native_pty_system();

        // Create PTY
        let size = PtySize {
            rows: self.config.pty_size.rows,
            cols: self.config.pty_size.cols,
            pixel_width: 0,
            pixel_height: 0,
        };

        let pty = pty_system
            .openpty(size)
            .map_err(|e| Error::Task(format!("Failed to open PTY: {}", e)))?;

        // Build command - use appropriate shell flag for platform
        let mut cmd = CommandBuilder::new(&self.config.shell);
        if cfg!(windows) {
            // PowerShell uses -Command, not -c
            cmd.arg("-Command");
        } else {
            cmd.arg("-c");
        }
        cmd.arg(&task.command);

        // Set working directory if provided in input
        if let Some(cwd_value) = task.input.get("cwd") {
            if let Some(cwd_str) = cwd_value.as_str() {
                cmd.cwd(PathBuf::from(cwd_str));
            }
        } else if let Some(cwd_value) = task.input.get("working_dir") {
            if let Some(cwd_str) = cwd_value.as_str() {
                cmd.cwd(PathBuf::from(cwd_str));
            }
        }

        // Set environment
        for (key, value) in self.build_env() {
            cmd.env(key, value);
        }

        // Add task-specific environment
        for (key, value) in &task.env {
            cmd.env(key, value);
        }

        // Create log buffer
        let _log_rx = self
            .log_manager
            .create_buffer(&task_id, Some(&task.command))
            .await;

        debug!("Executing PTY task {}: {}", task_id, task.command);

        // Spawn child process
        let child = pty
            .slave
            .spawn_command(cmd)
            .map_err(|e| Error::Task(format!("Failed to spawn PTY command: {}", e)))?;

        // Get reader
        let mut reader = pty
            .master
            .try_clone_reader()
            .map_err(|e| Error::Task(format!("Failed to clone PTY reader: {}", e)))?;

        let kill_requested = Arc::new(AtomicBool::new(false));

        // Store session state
        let session_state = Arc::new(Mutex::new(PtySessionState {
            pty: Some(pty),
            child: Some(child),
            task_id: task_id.clone(),
            command: task.command.clone(),
            started_at: Instant::now(),
            kill_requested: Arc::clone(&kill_requested),
            status: TaskState::Running,
            exit_code: None,
            log_collector_handle: None,
        }));

        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(task_id.clone(), Arc::clone(&session_state));
        }

        // Collect output with timeout
        let timeout_duration = task.timeout;
        let kill_flag = Arc::clone(&kill_requested);

        let output_handle = tokio::task::spawn_blocking(move || {
            let mut output = Vec::new();
            let mut buf = [0u8; 4096];
            let start = Instant::now();

            loop {
                if start.elapsed() > timeout_duration || kill_flag.load(Ordering::SeqCst) {
                    break;
                }

                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let chunk = &buf[..n];
                        output.extend_from_slice(chunk);
                    }
                    Err(e) => {
                        if e.kind() == std::io::ErrorKind::WouldBlock {
                            std::thread::sleep(Duration::from_millis(10));
                            continue;
                        }
                        break;
                    }
                }
            }

            output
        });

        // Wait for output collection
        let output = output_handle
            .await
            .map_err(|e| Error::Task(format!("Output collection failed: {}", e)))?;

        // Wait for process completion
        let (exit_code, was_killed) = {
            let mut state = session_state.lock().await;
            let exit_code = if let Some(ref mut child) = state.child {
                match child.try_wait() {
                    Ok(Some(status)) => status.exit_code() as i32,
                    Ok(None) => {
                        // Still running, wait with timeout
                        match child.wait() {
                            Ok(status) => status.exit_code() as i32,
                            Err(_) => -1,
                        }
                    }
                    Err(_) => -1,
                }
            } else {
                -1
            };
            state.exit_code = Some(exit_code);
            // Don't set full status here, will be set after output processing
            (exit_code, state.kill_requested.load(Ordering::SeqCst))
        };

        // Cleanup session
        {
            let mut sessions = self.sessions.write().await;
            sessions.remove(&task_id);
        }

        // Process output
        let stdout = String::from_utf8_lossy(&output).to_string();
        let stdout_clean = strip_ansi(&stdout);

        // Mask sensitive values in output
        let system_env = self.get_system_env();
        let stdout_masked = self
            .config
            .env_security
            .mask_output(&stdout_clean, &system_env);

        // Push to log
        self.log_manager.push_stdout(&task_id, &stdout_masked).await;
        self.log_manager.mark_ended(&task_id).await;

        if was_killed {
            return Err(Error::Task("Process was killed".to_string()));
        }

        Ok(TaskResult::with_exit_code(stdout_masked, exit_code))
    }

    /// Wait for a background task to complete
    pub async fn wait_for_completion(&self, task_id: &str, timeout: Duration) -> Result<TaskState> {
        let start = Instant::now();

        loop {
            if start.elapsed() > timeout {
                return Err(Error::Task("Wait timeout exceeded".to_string()));
            }

            if let Some(status) = self.get_status(task_id).await {
                if status.is_terminal() {
                    return Ok(status);
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            } else {
                // Task not found - assume completed/cleaned up
                return Ok(TaskState::Completed(TaskResult::success(String::new())));
            }
        }
    }

    /// Wait for output to contain a specific pattern
    pub async fn wait_for_output(&self, task_id: &str, pattern: &str, timeout: Duration) -> Result<bool> {
        let start = Instant::now();

        loop {
            if start.elapsed() > timeout {
                return Ok(false);
            }

            // Check logs for pattern
            let logs = self.get_logs(task_id, None).await;
            for entry in &logs {
                if entry.content.contains(pattern) {
                    return Ok(true);
                }
            }

            // Check if task has ended without match
            if self.is_completed(task_id).await {
                return Ok(false);
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}

impl Default for PtyExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Executor for PtyExecutor {
    async fn execute(&self, task: &Task) -> Result<TaskResult> {
        // PTY executor handles all execution modes (can be used for interactive)
        self.execute_in_pty(task).await
    }

    async fn cancel(&self, task: &Task) -> Result<()> {
        self.force_kill(&task.id.to_string()).await
    }

    fn is_available(&self) -> bool {
        // PTY is available on all platforms via portable-pty
        true
    }

    fn name(&self) -> &'static str {
        "pty"
    }
}

/// Check if a pattern matches a string (simple glob-like matching)
fn pattern_matches(pattern: &str, s: &str) -> bool {
    if pattern.starts_with('*') && pattern.ends_with('*') {
        let middle = &pattern[1..pattern.len() - 1];
        s.contains(middle)
    } else if pattern.starts_with('*') {
        s.ends_with(&pattern[1..])
    } else if pattern.ends_with('*') {
        s.starts_with(&pattern[..pattern.len() - 1])
    } else {
        s == pattern
    }
}

/// Strip ANSI escape sequences from output
fn strip_ansi(input: &str) -> String {
    // Simple ANSI stripping - handle common escape sequences
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip escape sequence
            if chars.peek() == Some(&'[') {
                chars.next(); // consume '['
                              // Skip until we hit a letter
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            result.push(c);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_matches() {
        assert!(pattern_matches("AWS_*", "AWS_ACCESS_KEY"));
        assert!(pattern_matches("*_TOKEN", "GITHUB_TOKEN"));
        assert!(pattern_matches("*SECRET*", "MY_SECRET_KEY"));
        assert!(!pattern_matches("AWS_*", "OTHER_KEY"));
    }

    #[test]
    fn test_strip_ansi() {
        let input = "\x1b[32mgreen\x1b[0m text";
        let output = strip_ansi(input);
        assert_eq!(output, "green text");
    }

    #[tokio::test]
    async fn test_pty_executor() {
        let executor = PtyExecutor::new();
        assert!(executor.is_available());
        assert_eq!(executor.name(), "pty");
    }

    #[test]
    fn test_config_default() {
        let config = PtyExecutorConfig::default();
        assert_eq!(config.pty_size.rows, 24);
        assert_eq!(config.pty_size.cols, 120);
    }

    #[test]
    fn test_env_security_blocked() {
        let security = PtyEnvSecurityConfig::default();

        // Should be blocked
        assert!(security.is_blocked("AWS_ACCESS_KEY"));
        assert!(security.is_blocked("AWS_SECRET_ACCESS_KEY"));
        assert!(security.is_blocked("GITHUB_TOKEN"));
        assert!(security.is_blocked("DATABASE_PASSWORD"));
        assert!(security.is_blocked("ANTHROPIC_API_KEY"));
        assert!(security.is_blocked("MY_SECRET_VALUE"));

        // Should NOT be blocked (allowed patterns take precedence)
        assert!(!security.is_blocked("PATH"));
        assert!(!security.is_blocked("HOME"));
        assert!(!security.is_blocked("TERM"));
        assert!(!security.is_blocked("NODE_ENV"));
        assert!(!security.is_blocked("RUST_LOG"));
        assert!(!security.is_blocked("CARGO_HOME"));
    }

    #[test]
    fn test_env_security_filter() {
        let security = PtyEnvSecurityConfig::default();
        let mut env = HashMap::new();
        env.insert("PATH".to_string(), "/usr/bin".to_string());
        env.insert("HOME".to_string(), "/home/user".to_string());
        env.insert("AWS_SECRET_KEY".to_string(), "secret123".to_string());
        env.insert("GITHUB_TOKEN".to_string(), "ghp_xxx".to_string());
        env.insert("NODE_ENV".to_string(), "development".to_string());

        let filtered = security.filter_env(&env);

        assert!(filtered.contains_key("PATH"));
        assert!(filtered.contains_key("HOME"));
        assert!(filtered.contains_key("NODE_ENV"));
        assert!(!filtered.contains_key("AWS_SECRET_KEY"));
        assert!(!filtered.contains_key("GITHUB_TOKEN"));
    }

    #[test]
    fn test_env_security_mask_output() {
        let security = PtyEnvSecurityConfig::default();
        let mut env = HashMap::new();
        env.insert("AWS_SECRET_KEY".to_string(), "supersecret123".to_string());
        env.insert("PATH".to_string(), "/usr/bin".to_string());

        let output = "The key is supersecret123 and path is /usr/bin";
        let masked = security.mask_output(output, &env);

        assert!(masked.contains("***"));
        assert!(!masked.contains("supersecret123"));
        assert!(masked.contains("/usr/bin")); // PATH is allowed, not masked
    }

    #[test]
    fn test_config_builder() {
        let config = PtyExecutorConfig::default()
            .block_env_patterns(vec!["CUSTOM_SECRET_*".to_string()])
            .allow_env_patterns(vec!["MY_SAFE_VAR".to_string()]);

        assert!(config
            .env_security
            .blocked_patterns
            .contains(&"CUSTOM_SECRET_*".to_string()));
        assert!(config
            .env_security
            .allowed_patterns
            .contains(&"MY_SAFE_VAR".to_string()));
    }
}
