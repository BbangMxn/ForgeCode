//! PTY shell session management for forgecmd
//!
//! This module wraps portable-pty to provide:
//! - Cross-platform PTY support (Unix + Windows)
//! - Async command execution with timeout
//! - ANSI escape sequence handling
//! - Environment variable management

use crate::forgecmd::config::ForgeCmdConfig;
use crate::forgecmd::error::{CommandResult, ForgeCmdError};
use portable_pty::{native_pty_system, CommandBuilder, PtyPair, PtySize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// PTY session for interactive command execution
pub struct PtySession {
    /// PTY pair (master + slave)
    pty: Option<PtyPair>,

    /// Configuration
    config: ForgeCmdConfig,

    /// Current working directory
    working_dir: PathBuf,

    /// Environment variables
    env: HashMap<String, String>,

    /// Session active flag
    active: Arc<Mutex<bool>>,
}

impl PtySession {
    /// Create a new PTY session
    pub fn new(config: ForgeCmdConfig, working_dir: PathBuf) -> Result<Self, ForgeCmdError> {
        let pty_system = native_pty_system();

        let size = PtySize {
            rows: config.pty_size.rows,
            cols: config.pty_size.cols,
            pixel_width: 0,
            pixel_height: 0,
        };

        let pty = pty_system
            .openpty(size)
            .map_err(|e| ForgeCmdError::PtyCreationFailed(format!("Failed to open PTY: {}", e)))?;

        // Build initial environment
        let mut env = std::env::vars().collect::<HashMap<_, _>>();

        // Set TERM for proper terminal behavior
        env.insert("TERM".to_string(), "xterm-256color".to_string());

        // Remove blocked environment variables
        for pattern in &config.blocked_env_vars {
            env.retain(|k, _| !crate::forgecmd::config::pattern_matches(pattern, k));
        }

        Ok(Self {
            pty: Some(pty),
            config,
            working_dir,
            env,
            active: Arc::new(Mutex::new(true)),
        })
    }

    /// Create a session with default config
    pub fn with_defaults(working_dir: PathBuf) -> Result<Self, ForgeCmdError> {
        Self::new(ForgeCmdConfig::default(), working_dir)
    }

    /// Execute a command and wait for completion
    pub fn execute(&mut self, command: &str) -> Result<CommandResult, ForgeCmdError> {
        self.execute_with_timeout(command, Duration::from_secs(self.config.timeout))
    }

    /// Execute a command with custom timeout
    pub fn execute_with_timeout(
        &mut self,
        command: &str,
        timeout: Duration,
    ) -> Result<CommandResult, ForgeCmdError> {
        let pty = self
            .pty
            .as_ref()
            .ok_or_else(|| ForgeCmdError::SessionNotStarted)?;

        // Build command
        let mut cmd = CommandBuilder::new(&self.config.shell);
        cmd.arg("-c");
        cmd.arg(command);
        cmd.cwd(&self.working_dir);

        // Set environment
        for (key, value) in &self.env {
            cmd.env(key, value);
        }

        // Spawn child process
        let mut child = pty.slave.spawn_command(cmd).map_err(|e| {
            ForgeCmdError::ShellSpawnFailed(format!("Failed to spawn command: {}", e))
        })?;

        // Get reader for output
        let reader = pty.master.try_clone_reader().map_err(|e| {
            ForgeCmdError::ExecutionFailed(format!("Failed to clone reader: {}", e))
        })?;

        // Collect output with timeout
        let start = Instant::now();
        let (stdout, stderr) = self.collect_output(reader, timeout)?;

        // Wait for process to complete
        let status = child.wait().map_err(|e| {
            ForgeCmdError::ExecutionFailed(format!("Failed to wait for command: {}", e))
        })?;

        let duration = start.elapsed();
        let exit_code = status.exit_code() as i32;

        // Strip ANSI escape sequences from output
        let stdout_clean = strip_ansi(&stdout);
        let stderr_clean = strip_ansi(&stderr);

        Ok(CommandResult {
            command: command.to_string(),
            exit_code: Some(exit_code),
            stdout: stdout_clean,
            stderr: stderr_clean,
            duration_ms: duration.as_millis() as u64,
            truncated: false,
        })
    }

    /// Execute a command asynchronously (non-blocking)
    pub fn spawn(&mut self, command: &str) -> Result<SpawnedCommand, ForgeCmdError> {
        let pty = self
            .pty
            .as_ref()
            .ok_or_else(|| ForgeCmdError::SessionNotStarted)?;

        let mut cmd = CommandBuilder::new(&self.config.shell);
        cmd.arg("-c");
        cmd.arg(command);
        cmd.cwd(&self.working_dir);

        for (key, value) in &self.env {
            cmd.env(key, value);
        }

        let child = pty.slave.spawn_command(cmd).map_err(|e| {
            ForgeCmdError::ShellSpawnFailed(format!("Failed to spawn command: {}", e))
        })?;

        let reader = pty.master.try_clone_reader().map_err(|e| {
            ForgeCmdError::ExecutionFailed(format!("Failed to clone reader: {}", e))
        })?;

        let writer = pty
            .master
            .take_writer()
            .map_err(|e| ForgeCmdError::ExecutionFailed(format!("Failed to take writer: {}", e)))?;

        Ok(SpawnedCommand {
            command: command.to_string(),
            child,
            reader: Some(reader),
            writer: Some(writer),
            started_at: Instant::now(),
        })
    }

    /// Collect output from PTY with timeout
    fn collect_output(
        &self,
        reader: Box<dyn Read + Send>,
        timeout: Duration,
    ) -> Result<(String, String), ForgeCmdError> {
        let output = Arc::new(Mutex::new(Vec::new()));
        let output_clone = Arc::clone(&output);

        let handle = thread::spawn(move || {
            let mut buf_reader = std::io::BufReader::new(reader);
            let mut buf = [0u8; 4096];

            loop {
                match buf_reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if let Ok(mut output) = output_clone.lock() {
                            output.extend_from_slice(&buf[..n]);
                        }
                    }
                    Err(e) => {
                        // Check for expected errors (e.g., broken pipe on completion)
                        if e.kind() != std::io::ErrorKind::BrokenPipe {
                            break;
                        }
                        break;
                    }
                }
            }
        });

        // Wait for thread with timeout
        let start = Instant::now();
        loop {
            if handle.is_finished() {
                break;
            }
            if start.elapsed() > timeout {
                // Timeout - return what we have
                let output_data = output.lock().map(|o| o.clone()).unwrap_or_default();
                let stdout = String::from_utf8_lossy(&output_data).to_string();
                return Ok((stdout, String::new()));
            }
            thread::sleep(Duration::from_millis(10));
        }

        let _ = handle.join();

        let output_data = output.lock().map(|o| o.clone()).unwrap_or_default();
        let stdout = String::from_utf8_lossy(&output_data).to_string();

        // PTY combines stdout/stderr, so we return all as stdout
        Ok((stdout, String::new()))
    }

    /// Resize the PTY
    pub fn resize(&self, rows: u16, cols: u16) -> Result<(), ForgeCmdError> {
        if let Some(ref pty) = self.pty {
            let size = PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            };
            pty.master.resize(size).map_err(|e| {
                ForgeCmdError::PtyCreationFailed(format!("Failed to resize PTY: {}", e))
            })?;
        }
        Ok(())
    }

    /// Set working directory
    pub fn set_working_dir(&mut self, dir: PathBuf) {
        self.working_dir = dir;
    }

    /// Get current working directory
    pub fn working_dir(&self) -> &PathBuf {
        &self.working_dir
    }

    /// Set environment variable
    pub fn set_env(&mut self, key: &str, value: &str) {
        // Check if blocked
        for pattern in &self.config.blocked_env_vars {
            if crate::forgecmd::config::pattern_matches(pattern, key) {
                return; // Don't set blocked env vars
            }
        }
        self.env.insert(key.to_string(), value.to_string());
    }

    /// Remove environment variable
    pub fn remove_env(&mut self, key: &str) {
        self.env.remove(key);
    }

    /// Get environment variable
    pub fn get_env(&self, key: &str) -> Option<&String> {
        self.env.get(key)
    }

    /// Check if session is active
    pub fn is_active(&self) -> bool {
        self.active.lock().map(|a| *a).unwrap_or(false)
    }

    /// Close the session
    pub fn close(&mut self) {
        if let Ok(mut active) = self.active.lock() {
            *active = false;
        }
        self.pty = None;
    }
}

impl Drop for PtySession {
    fn drop(&mut self) {
        self.close();
    }
}

/// A spawned command (for async execution)
pub struct SpawnedCommand {
    command: String,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    reader: Option<Box<dyn Read + Send>>,
    writer: Option<Box<dyn Write + Send>>,
    started_at: Instant,
}

impl SpawnedCommand {
    /// Send input to the command
    pub fn send(&mut self, input: &str) -> Result<(), ForgeCmdError> {
        if let Some(ref mut writer) = self.writer {
            writer.write_all(input.as_bytes()).map_err(|e| {
                ForgeCmdError::ExecutionFailed(format!("Failed to send input: {}", e))
            })?;
            writer
                .flush()
                .map_err(|e| ForgeCmdError::ExecutionFailed(format!("Failed to flush: {}", e)))?;
        }
        Ok(())
    }

    /// Read available output (non-blocking)
    pub fn read_output(&mut self) -> Result<String, ForgeCmdError> {
        if let Some(ref mut reader) = self.reader {
            let mut buf = [0u8; 4096];
            // Note: This is blocking in the current implementation
            // For true non-blocking, we'd need async I/O
            match reader.read(&mut buf) {
                Ok(0) => Ok(String::new()),
                Ok(n) => {
                    let output = String::from_utf8_lossy(&buf[..n]).to_string();
                    Ok(strip_ansi(&output))
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(String::new()),
                Err(e) => Err(ForgeCmdError::ExecutionFailed(format!(
                    "Failed to read output: {}",
                    e
                ))),
            }
        } else {
            Ok(String::new())
        }
    }

    /// Wait for the command to complete
    pub fn wait(mut self) -> Result<CommandResult, ForgeCmdError> {
        // Close writer to signal EOF
        self.writer = None;

        // Read remaining output
        let mut output = String::new();
        if let Some(mut reader) = self.reader.take() {
            let _ = reader.read_to_string(&mut output);
        }

        let status = self
            .child
            .wait()
            .map_err(|e| ForgeCmdError::ExecutionFailed(format!("Failed to wait: {}", e)))?;

        let duration = self.started_at.elapsed();
        let exit_code = status.exit_code() as i32;
        let stdout_clean = strip_ansi(&output);

        Ok(CommandResult {
            command: self.command,
            exit_code: Some(exit_code),
            stdout: stdout_clean,
            stderr: String::new(),
            duration_ms: duration.as_millis() as u64,
            truncated: false,
        })
    }

    /// Kill the command
    pub fn kill(&mut self) -> Result<(), ForgeCmdError> {
        self.child
            .kill()
            .map_err(|e| ForgeCmdError::ExecutionFailed(format!("Failed to kill: {}", e)))
    }

    /// Check if the command is still running
    pub fn try_wait(&mut self) -> Result<Option<i32>, ForgeCmdError> {
        match self.child.try_wait() {
            Ok(Some(status)) => Ok(Some(status.exit_code() as i32)),
            Ok(None) => Ok(None),
            Err(e) => Err(ForgeCmdError::ExecutionFailed(format!(
                "Failed to check status: {}",
                e
            ))),
        }
    }

    /// Get elapsed time since start
    pub fn elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }
}

/// Strip ANSI escape sequences from output
fn strip_ansi(input: &str) -> String {
    strip_ansi_escapes::strip_str(input).to_string()
}

/// Simple command execution without full PTY session
/// (For when you just need quick command execution)
pub fn execute_simple(
    command: &str,
    working_dir: &PathBuf,
    timeout: Duration,
) -> Result<CommandResult, ForgeCmdError> {
    use std::process::Command;

    let start = Instant::now();

    let shell = if cfg!(windows) { "cmd" } else { "sh" };
    let shell_arg = if cfg!(windows) { "/C" } else { "-c" };

    let output = Command::new(shell)
        .arg(shell_arg)
        .arg(command)
        .current_dir(working_dir)
        .output()
        .map_err(|e| ForgeCmdError::ExecutionFailed(format!("Failed to execute: {}", e)))?;

    let duration = start.elapsed();

    if duration > timeout {
        return Err(ForgeCmdError::Timeout(timeout.as_secs()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code();

    Ok(CommandResult {
        command: command.to_string(),
        exit_code,
        stdout,
        stderr,
        duration_ms: duration.as_millis() as u64,
        truncated: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_execute() {
        let working_dir = std::env::current_dir().unwrap();
        let result = execute_simple("echo hello", &working_dir, Duration::from_secs(5)).unwrap();

        assert!(result.success());
        assert!(result.stdout.contains("hello"));
    }

    #[test]
    fn test_strip_ansi() {
        let input = "\x1b[32mgreen\x1b[0m text";
        let output = strip_ansi(input);
        assert_eq!(output, "green text");
    }

    #[test]
    #[cfg(not(windows))] // PTY tests may behave differently on Windows
    fn test_pty_session() {
        let working_dir = std::env::current_dir().unwrap();
        let mut session = PtySession::with_defaults(working_dir).unwrap();

        let result = session.execute("echo hello").unwrap();
        assert!(result.stdout.contains("hello"));
    }

    #[test]
    fn test_command_result() {
        let result = CommandResult {
            command: "test".to_string(),
            exit_code: Some(0),
            stdout: "output".to_string(),
            stderr: String::new(),
            duration_ms: 1000,
            truncated: false,
        };
        assert!(result.success());
        assert_eq!(result.exit_code, Some(0));
    }
}
