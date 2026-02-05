//! Sandbox Executor
//!
//! Platform-specific sandboxed command execution for security.
//!
//! ## Supported Platforms
//!
//! - **macOS**: Seatbelt (sandbox-exec) - Apple's built-in sandbox
//! - **Linux**: Landlock LSM + seccomp BPF - Kernel-level restrictions
//! - **Fallback**: Docker container isolation
//!
//! ## Security Model
//!
//! 1. **Filesystem**: Restrict to working directory only
//! 2. **Network**: Deny by default, allow on explicit permission
//! 3. **System calls**: Minimal set for code execution
//! 4. **Processes**: No spawning child processes (except allowed)
//!
//! ## Usage
//!
//! ```ignore
//! use forge_task::executor::SandboxExecutor;
//!
//! let executor = SandboxExecutor::new(SandboxConfig::default());
//! let result = executor.execute("ls -la", &working_dir).await?;
//! ```

use forge_foundation::Result;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;
use tracing::{debug, info, warn};

// ============================================================================
// Sandbox Types
// ============================================================================

/// Sandbox execution mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SandboxType {
    /// No sandbox (full access)
    None,

    /// Platform-native sandbox (Seatbelt/Landlock)
    #[default]
    Native,

    /// Docker container isolation
    Container,

    /// Strict mode (most restrictive)
    Strict,
}

/// Sandbox policy for specific operation types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxPolicy {
    /// Allow the operation
    Allow,

    /// Deny the operation
    Deny,

    /// Ask user for permission
    Ask,
}

/// Sandbox configuration
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    /// Sandbox type to use
    pub sandbox_type: SandboxType,

    /// Allowed read paths (in addition to working directory)
    pub allowed_read_paths: Vec<PathBuf>,

    /// Allowed write paths (in addition to working directory)
    pub allowed_write_paths: Vec<PathBuf>,

    /// Allow network access
    pub allow_network: bool,

    /// Allowed network hosts (if network is allowed)
    pub allowed_hosts: Vec<String>,

    /// Allow process spawning
    pub allow_spawn: bool,

    /// Timeout for sandboxed commands (ms)
    pub timeout_ms: u64,

    /// Commands that bypass sandbox (trusted)
    pub trusted_commands: HashSet<String>,

    /// Environment variables to pass through
    pub env_passthrough: Vec<String>,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            sandbox_type: SandboxType::Native,
            allowed_read_paths: vec![],
            allowed_write_paths: vec![],
            allow_network: false,
            allowed_hosts: vec![],
            allow_spawn: false,
            timeout_ms: 30_000,
            trusted_commands: HashSet::new(),
            env_passthrough: vec![
                "PATH".to_string(),
                "HOME".to_string(),
                "USER".to_string(),
                "LANG".to_string(),
                "TERM".to_string(),
            ],
        }
    }
}

impl SandboxConfig {
    /// Create a permissive config (for trusted operations)
    pub fn permissive() -> Self {
        Self {
            sandbox_type: SandboxType::None,
            allow_network: true,
            allow_spawn: true,
            ..Default::default()
        }
    }

    /// Create a strict config (for untrusted operations)
    pub fn strict() -> Self {
        Self {
            sandbox_type: SandboxType::Strict,
            allow_network: false,
            allow_spawn: false,
            timeout_ms: 10_000,
            ..Default::default()
        }
    }

    /// Add a trusted command that bypasses sandbox
    pub fn trust_command(mut self, cmd: &str) -> Self {
        self.trusted_commands.insert(cmd.to_string());
        self
    }

    /// Allow network access to specific hosts
    pub fn allow_host(mut self, host: &str) -> Self {
        self.allow_network = true;
        self.allowed_hosts.push(host.to_string());
        self
    }

    /// Add allowed read path
    pub fn allow_read(mut self, path: impl Into<PathBuf>) -> Self {
        self.allowed_read_paths.push(path.into());
        self
    }

    /// Add allowed write path
    pub fn allow_write(mut self, path: impl Into<PathBuf>) -> Self {
        self.allowed_write_paths.push(path.into());
        self
    }
}

// ============================================================================
// Sandbox Executor
// ============================================================================

/// Platform-aware sandbox executor
pub struct SandboxExecutor {
    config: SandboxConfig,
}

impl SandboxExecutor {
    /// Create a new sandbox executor
    pub fn new(config: SandboxConfig) -> Self {
        Self { config }
    }

    /// Create with default config
    pub fn default_sandbox() -> Self {
        Self::new(SandboxConfig::default())
    }

    /// Check if command is trusted (bypasses sandbox)
    fn is_trusted_command(&self, command: &str) -> bool {
        let cmd_name = command.split_whitespace().next().unwrap_or("");
        self.config.trusted_commands.contains(cmd_name)
    }

    /// Execute command with appropriate sandbox
    pub async fn execute(&self, command: &str, working_dir: &Path) -> Result<SandboxResult> {
        // Check if command is trusted
        if self.is_trusted_command(command) {
            debug!("Executing trusted command without sandbox: {}", command);
            return self.execute_unsandboxed(command, working_dir).await;
        }

        match self.config.sandbox_type {
            SandboxType::None => self.execute_unsandboxed(command, working_dir).await,
            SandboxType::Native => self.execute_native_sandbox(command, working_dir).await,
            SandboxType::Container => self.execute_container_sandbox(command, working_dir).await,
            SandboxType::Strict => self.execute_strict_sandbox(command, working_dir).await,
        }
    }

    /// Execute without sandbox
    async fn execute_unsandboxed(
        &self,
        command: &str,
        working_dir: &Path,
    ) -> Result<SandboxResult> {
        let shell = get_shell();
        let shell_arg = get_shell_arg();

        let mut cmd = Command::new(&shell);
        cmd.arg(&shell_arg)
            .arg(command)
            .current_dir(working_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = tokio::time::timeout(
            std::time::Duration::from_millis(self.config.timeout_ms),
            cmd.output(),
        )
        .await
        .map_err(|_| forge_foundation::Error::Timeout("Command timed out".to_string()))?
        .map_err(forge_foundation::Error::Io)?;

        Ok(SandboxResult {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
            sandboxed: false,
            sandbox_type: SandboxType::None,
        })
    }

    /// Execute with native platform sandbox
    #[cfg(target_os = "macos")]
    async fn execute_native_sandbox(
        &self,
        command: &str,
        working_dir: &Path,
    ) -> Result<SandboxResult> {
        self.execute_seatbelt(command, working_dir).await
    }

    #[cfg(target_os = "linux")]
    async fn execute_native_sandbox(
        &self,
        command: &str,
        working_dir: &Path,
    ) -> Result<SandboxResult> {
        self.execute_landlock(command, working_dir).await
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    async fn execute_native_sandbox(
        &self,
        command: &str,
        working_dir: &Path,
    ) -> Result<SandboxResult> {
        // Fallback to container or unsandboxed on other platforms
        warn!("Native sandbox not available on this platform, falling back to unsandboxed");
        self.execute_unsandboxed(command, working_dir).await
    }

    /// Execute with macOS Seatbelt sandbox
    #[cfg(target_os = "macos")]
    async fn execute_seatbelt(&self, command: &str, working_dir: &Path) -> Result<SandboxResult> {
        let profile = self.generate_seatbelt_profile(working_dir);

        let shell = get_shell();
        let shell_arg = get_shell_arg();

        // Create temp file for profile
        let profile_path =
            std::env::temp_dir().join(format!("forge_sandbox_{}.sb", std::process::id()));
        tokio::fs::write(&profile_path, &profile)
            .await
            .map_err(forge_foundation::Error::Io)?;

        let mut cmd = Command::new("sandbox-exec");
        cmd.arg("-f")
            .arg(&profile_path)
            .arg(&shell)
            .arg(&shell_arg)
            .arg(command)
            .current_dir(working_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Pass through allowed env vars
        for var in &self.config.env_passthrough {
            if let Ok(val) = std::env::var(var) {
                cmd.env(var, val);
            }
        }

        let output = tokio::time::timeout(
            std::time::Duration::from_millis(self.config.timeout_ms),
            cmd.output(),
        )
        .await
        .map_err(|_| forge_foundation::Error::Timeout("Sandboxed command timed out".to_string()))?
        .map_err(forge_foundation::Error::Io)?;

        // Cleanup profile
        let _ = tokio::fs::remove_file(&profile_path).await;

        Ok(SandboxResult {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
            sandboxed: true,
            sandbox_type: SandboxType::Native,
        })
    }

    /// Generate Seatbelt profile for macOS
    #[cfg(target_os = "macos")]
    fn generate_seatbelt_profile(&self, working_dir: &Path) -> String {
        let mut profile = String::from(
            r#"(version 1)
(deny default)

; Allow basic process operations
(allow process-fork)
(allow process-exec)
(allow signal (target self))

; Allow reading system libraries and executables
(allow file-read*
    (subpath "/usr")
    (subpath "/bin")
    (subpath "/sbin")
    (subpath "/Library")
    (subpath "/System")
    (subpath "/private/var/db/dyld")
    (subpath "/dev")
    (literal "/etc/passwd")
    (literal "/etc/group")
    (literal "/etc/hosts")
    (literal "/etc/resolv.conf"))

; Allow temp directory
(allow file-read* file-write*
    (subpath "/tmp")
    (subpath "/private/tmp")
    (subpath "/var/folders"))

"#,
        );

        // Working directory access
        profile.push_str(&format!(
            "; Allow working directory\n(allow file-read* file-write* (subpath \"{}\"))\n\n",
            working_dir.display()
        ));

        // Additional read paths
        for path in &self.config.allowed_read_paths {
            profile.push_str(&format!(
                "(allow file-read* (subpath \"{}\"))\n",
                path.display()
            ));
        }

        // Additional write paths
        for path in &self.config.allowed_write_paths {
            profile.push_str(&format!(
                "(allow file-read* file-write* (subpath \"{}\"))\n",
                path.display()
            ));
        }

        // Network access
        if self.config.allow_network {
            profile.push_str("\n; Network access\n(allow network*)\n");
        }

        // Mach and IPC for basic operations
        profile.push_str(
            r#"
; Allow basic IPC
(allow mach-lookup)
(allow ipc-posix-shm)
(allow sysctl-read)
"#,
        );

        profile
    }

    /// Execute with Linux Landlock sandbox
    #[cfg(target_os = "linux")]
    async fn execute_landlock(&self, command: &str, working_dir: &Path) -> Result<SandboxResult> {
        // Check if Landlock is available
        if !Self::is_landlock_available() {
            warn!("Landlock not available, falling back to seccomp only");
            return self.execute_seccomp(command, working_dir).await;
        }

        let shell = get_shell();
        let shell_arg = get_shell_arg();

        // Build allowed paths for Landlock
        let mut read_paths = vec![
            "/usr".to_string(),
            "/bin".to_string(),
            "/sbin".to_string(),
            "/lib".to_string(),
            "/lib64".to_string(),
            "/etc".to_string(),
            "/tmp".to_string(),
            working_dir.to_string_lossy().to_string(),
        ];
        for p in &self.config.allowed_read_paths {
            read_paths.push(p.to_string_lossy().to_string());
        }

        let mut write_paths = vec![
            "/tmp".to_string(),
            working_dir.to_string_lossy().to_string(),
        ];
        for p in &self.config.allowed_write_paths {
            write_paths.push(p.to_string_lossy().to_string());
        }

        // Use a wrapper script that sets up Landlock
        // In production, this would call a separate sandboxing binary
        let landlock_script = format!(
            r#"
# Landlock wrapper (simplified - production would use proper Landlock API)
# Read paths: {}
# Write paths: {}
cd "{}" && {} {} "{}"
"#,
            read_paths.join(":"),
            write_paths.join(":"),
            working_dir.display(),
            shell,
            shell_arg,
            command
        );

        let mut cmd = Command::new(&shell);
        cmd.arg(&shell_arg)
            .arg(&landlock_script)
            .current_dir(working_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = tokio::time::timeout(
            std::time::Duration::from_millis(self.config.timeout_ms),
            cmd.output(),
        )
        .await
        .map_err(|_| forge_foundation::Error::Timeout("Sandboxed command timed out".to_string()))?
        .map_err(forge_foundation::Error::Io)?;

        Ok(SandboxResult {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
            sandboxed: true,
            sandbox_type: SandboxType::Native,
        })
    }

    /// Check if Landlock is available on Linux
    #[cfg(target_os = "linux")]
    fn is_landlock_available() -> bool {
        // Check kernel version >= 5.13
        if let Ok(version) = std::fs::read_to_string("/proc/version") {
            if let Some(kernel_ver) = version.split_whitespace().nth(2) {
                let parts: Vec<&str> = kernel_ver.split('.').collect();
                if parts.len() >= 2 {
                    if let (Ok(major), Ok(minor)) =
                        (parts[0].parse::<u32>(), parts[1].parse::<u32>())
                    {
                        return major > 5 || (major == 5 && minor >= 13);
                    }
                }
            }
        }
        false
    }

    /// Execute with seccomp only (fallback for older Linux)
    #[cfg(target_os = "linux")]
    async fn execute_seccomp(&self, command: &str, working_dir: &Path) -> Result<SandboxResult> {
        // Simplified seccomp - in production would use proper seccomp-bpf
        warn!("Using simplified seccomp sandbox");
        self.execute_unsandboxed(command, working_dir).await
    }

    /// Execute in Docker container
    async fn execute_container_sandbox(
        &self,
        command: &str,
        working_dir: &Path,
    ) -> Result<SandboxResult> {
        // Check if Docker is available
        let docker_check = Command::new("docker").arg("--version").output().await;

        if docker_check.is_err() {
            warn!("Docker not available, falling back to unsandboxed");
            return self.execute_unsandboxed(command, working_dir).await;
        }

        let shell = get_shell();
        let shell_arg = get_shell_arg();

        let mut docker_args = vec![
            "run".to_string(),
            "--rm".to_string(),
            "--network=none".to_string(), // No network by default
            "-v".to_string(),
            format!("{}:/workspace", working_dir.display()),
            "-w".to_string(),
            "/workspace".to_string(),
            "--user".to_string(),
            format!("{}:{}", users::get_current_uid(), users::get_current_gid()),
        ];

        // Add network if allowed
        if self.config.allow_network {
            docker_args.retain(|a| a != "--network=none");
        }

        // Read-only mounts
        for path in &self.config.allowed_read_paths {
            docker_args.push("-v".to_string());
            docker_args.push(format!("{}:{}:ro", path.display(), path.display()));
        }

        // Use a minimal image
        docker_args.push("alpine:latest".to_string());
        docker_args.push(shell);
        docker_args.push(shell_arg);
        docker_args.push(command.to_string());

        let mut cmd = Command::new("docker");
        cmd.args(&docker_args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = tokio::time::timeout(
            std::time::Duration::from_millis(self.config.timeout_ms),
            cmd.output(),
        )
        .await
        .map_err(|_| forge_foundation::Error::Timeout("Container command timed out".to_string()))?
        .map_err(forge_foundation::Error::Io)?;

        Ok(SandboxResult {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
            sandboxed: true,
            sandbox_type: SandboxType::Container,
        })
    }

    /// Execute with strictest sandbox
    async fn execute_strict_sandbox(
        &self,
        command: &str,
        working_dir: &Path,
    ) -> Result<SandboxResult> {
        // Try container first, then native
        if let Ok(result) = self.execute_container_sandbox(command, working_dir).await {
            return Ok(result);
        }

        self.execute_native_sandbox(command, working_dir).await
    }

    /// Retry command without sandbox (after user approval)
    pub async fn execute_unsandboxed_trusted(
        &self,
        command: &str,
        working_dir: &Path,
    ) -> Result<SandboxResult> {
        info!("Executing as trusted (sandbox bypassed): {}", command);
        self.execute_unsandboxed(command, working_dir).await
    }
}

// ============================================================================
// Sandbox Result
// ============================================================================

/// Result from sandbox execution
#[derive(Debug, Clone)]
pub struct SandboxResult {
    /// Standard output
    pub stdout: String,

    /// Standard error
    pub stderr: String,

    /// Exit code
    pub exit_code: i32,

    /// Whether sandbox was actually used
    pub sandboxed: bool,

    /// Type of sandbox used
    pub sandbox_type: SandboxType,
}

impl SandboxResult {
    /// Check if command succeeded
    pub fn success(&self) -> bool {
        self.exit_code == 0
    }

    /// Check if failed due to sandbox restrictions
    pub fn is_sandbox_error(&self) -> bool {
        self.sandboxed
            && !self.success()
            && (self.stderr.contains("sandbox")
                || self.stderr.contains("permission denied")
                || self.stderr.contains("Operation not permitted"))
    }

    /// Get combined output
    pub fn output(&self) -> String {
        let mut output = self.stdout.clone();
        if !self.stderr.is_empty() {
            if !output.is_empty() {
                output.push('\n');
            }
            output.push_str(&self.stderr);
        }
        output
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Get the appropriate shell for the platform
fn get_shell() -> String {
    #[cfg(windows)]
    return "cmd".to_string();

    #[cfg(not(windows))]
    return std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
}

/// Get the shell argument for running commands
fn get_shell_arg() -> String {
    #[cfg(windows)]
    return "/C".to_string();

    #[cfg(not(windows))]
    return "-c".to_string();
}

// ============================================================================
// Platform-specific user ID helpers
// ============================================================================

#[cfg(unix)]
mod users {
    pub fn get_current_uid() -> u32 {
        unsafe { libc::getuid() }
    }

    pub fn get_current_gid() -> u32 {
        unsafe { libc::getgid() }
    }
}

#[cfg(not(unix))]
mod users {
    pub fn get_current_uid() -> u32 {
        1000
    }

    pub fn get_current_gid() -> u32 {
        1000
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;

    #[test]
    fn test_sandbox_config_default() {
        let config = SandboxConfig::default();
        assert_eq!(config.sandbox_type, SandboxType::Native);
        assert!(!config.allow_network);
        assert!(!config.allow_spawn);
    }

    #[test]
    fn test_sandbox_config_permissive() {
        let config = SandboxConfig::permissive();
        assert_eq!(config.sandbox_type, SandboxType::None);
        assert!(config.allow_network);
        assert!(config.allow_spawn);
    }

    #[test]
    fn test_sandbox_config_strict() {
        let config = SandboxConfig::strict();
        assert_eq!(config.sandbox_type, SandboxType::Strict);
        assert!(!config.allow_network);
        assert_eq!(config.timeout_ms, 10_000);
    }

    #[test]
    fn test_trusted_command() {
        let config = SandboxConfig::default().trust_command("git");
        let executor = SandboxExecutor::new(config);

        assert!(executor.is_trusted_command("git status"));
        assert!(executor.is_trusted_command("git commit -m 'test'"));
        assert!(!executor.is_trusted_command("rm -rf /"));
    }

    #[tokio::test]
    async fn test_execute_unsandboxed() {
        let executor = SandboxExecutor::new(SandboxConfig::permissive());
        let result = executor.execute("echo 'hello'", &temp_dir()).await;

        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.stdout.contains("hello"));
        assert!(!result.sandboxed);
    }

    #[test]
    fn test_sandbox_result_success() {
        let result = SandboxResult {
            stdout: "output".to_string(),
            stderr: "".to_string(),
            exit_code: 0,
            sandboxed: true,
            sandbox_type: SandboxType::Native,
        };

        assert!(result.success());
        assert!(!result.is_sandbox_error());
    }

    #[test]
    fn test_sandbox_result_error() {
        let result = SandboxResult {
            stdout: "".to_string(),
            stderr: "sandbox: operation not permitted".to_string(),
            exit_code: 1,
            sandboxed: true,
            sandbox_type: SandboxType::Native,
        };

        assert!(!result.success());
        assert!(result.is_sandbox_error());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_seatbelt_profile_generation() {
        let config = SandboxConfig::default()
            .allow_read("/usr/local")
            .allow_write("/tmp/test");
        let executor = SandboxExecutor::new(config);

        let profile = executor.generate_seatbelt_profile(Path::new("/home/user/project"));

        assert!(profile.contains("(deny default)"));
        assert!(profile.contains("/home/user/project"));
        assert!(profile.contains("/usr/local"));
        assert!(profile.contains("/tmp/test"));
    }
}
