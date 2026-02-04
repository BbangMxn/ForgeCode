//! Container Security Module
//!
//! Provides secure container execution for tasks using Docker/Podman.
//!
//! Features:
//! - Isolated execution environment
//! - Resource limits (CPU, memory, disk)
//! - Network isolation
//! - Volume mounting with restrictions
//! - Audit logging

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use tracing::{debug, error, info, warn};

/// Container runtime type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContainerRuntime {
    Docker,
    Podman,
}

impl Default for ContainerRuntime {
    fn default() -> Self {
        Self::Docker
    }
}

impl ContainerRuntime {
    /// Get the CLI command for this runtime
    pub fn command(&self) -> &'static str {
        match self {
            Self::Docker => "docker",
            Self::Podman => "podman",
        }
    }
}

/// Resource limits for container
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// CPU limit (number of cores, e.g., 1.5)
    pub cpus: Option<f32>,
    /// Memory limit (e.g., "512m", "2g")
    pub memory: Option<String>,
    /// Memory swap limit
    pub memory_swap: Option<String>,
    /// PIDs limit
    pub pids_limit: Option<u32>,
    /// Disk quota in bytes
    pub disk_quota: Option<u64>,
    /// Read-only root filesystem
    pub read_only: bool,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            cpus: Some(2.0),
            memory: Some("2g".to_string()),
            memory_swap: Some("2g".to_string()),
            pids_limit: Some(256),
            disk_quota: Some(10 * 1024 * 1024 * 1024), // 10GB
            read_only: false,
        }
    }
}

impl ResourceLimits {
    /// Create minimal limits for quick tasks
    pub fn minimal() -> Self {
        Self {
            cpus: Some(0.5),
            memory: Some("256m".to_string()),
            memory_swap: Some("256m".to_string()),
            pids_limit: Some(64),
            disk_quota: Some(1024 * 1024 * 1024), // 1GB
            read_only: true,
        }
    }

    /// Create generous limits for heavy tasks
    pub fn generous() -> Self {
        Self {
            cpus: Some(4.0),
            memory: Some("8g".to_string()),
            memory_swap: Some("8g".to_string()),
            pids_limit: Some(1024),
            disk_quota: Some(50 * 1024 * 1024 * 1024), // 50GB
            read_only: false,
        }
    }
}

/// Network mode for container
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkMode {
    /// No network access
    None,
    /// Bridge network (default Docker)
    Bridge,
    /// Host network (full access)
    Host,
    /// Custom network
    Custom(String),
}

impl Default for NetworkMode {
    fn default() -> Self {
        Self::Bridge
    }
}

/// Volume mount configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeMount {
    /// Host path
    pub host_path: PathBuf,
    /// Container path
    pub container_path: PathBuf,
    /// Read-only mount
    pub read_only: bool,
}

impl VolumeMount {
    pub fn new(host: impl Into<PathBuf>, container: impl Into<PathBuf>) -> Self {
        Self {
            host_path: host.into(),
            container_path: container.into(),
            read_only: false,
        }
    }

    pub fn read_only(mut self) -> Self {
        self.read_only = true;
        self
    }
}

/// Security profile for container
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityProfile {
    /// Drop all capabilities except specified
    pub drop_caps: Vec<String>,
    /// Add specific capabilities
    pub add_caps: Vec<String>,
    /// Seccomp profile path
    pub seccomp_profile: Option<String>,
    /// AppArmor profile
    pub apparmor_profile: Option<String>,
    /// Run as non-root user
    pub user: Option<String>,
    /// No new privileges
    pub no_new_privileges: bool,
}

impl Default for SecurityProfile {
    fn default() -> Self {
        Self {
            drop_caps: vec!["ALL".to_string()],
            add_caps: vec![],
            seccomp_profile: None,
            apparmor_profile: None,
            user: Some("1000:1000".to_string()),
            no_new_privileges: true,
        }
    }
}

impl SecurityProfile {
    /// Create a permissive profile (for trusted code)
    pub fn permissive() -> Self {
        Self {
            drop_caps: vec![],
            add_caps: vec![],
            seccomp_profile: None,
            apparmor_profile: None,
            user: None,
            no_new_privileges: false,
        }
    }

    /// Create a strict profile (for untrusted code)
    pub fn strict() -> Self {
        Self {
            drop_caps: vec!["ALL".to_string()],
            add_caps: vec![],
            seccomp_profile: Some("default".to_string()),
            apparmor_profile: Some("docker-default".to_string()),
            user: Some("65534:65534".to_string()), // nobody
            no_new_privileges: true,
        }
    }
}

/// Container configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerConfig {
    /// Container image
    pub image: String,
    /// Container name (auto-generated if None)
    pub name: Option<String>,
    /// Command to run
    pub command: Vec<String>,
    /// Working directory in container
    pub working_dir: Option<PathBuf>,
    /// Environment variables
    pub env: HashMap<String, String>,
    /// Volume mounts
    pub volumes: Vec<VolumeMount>,
    /// Resource limits
    pub limits: ResourceLimits,
    /// Network mode
    pub network: NetworkMode,
    /// Security profile
    pub security: SecurityProfile,
    /// Timeout
    pub timeout: Duration,
    /// Auto-remove after exit
    pub auto_remove: bool,
    /// Labels
    pub labels: HashMap<String, String>,
}

impl Default for ContainerConfig {
    fn default() -> Self {
        Self {
            image: "ubuntu:22.04".to_string(),
            name: None,
            command: vec![],
            working_dir: None,
            env: HashMap::new(),
            volumes: vec![],
            limits: ResourceLimits::default(),
            network: NetworkMode::default(),
            security: SecurityProfile::default(),
            timeout: Duration::from_secs(300),
            auto_remove: true,
            labels: HashMap::new(),
        }
    }
}

impl ContainerConfig {
    pub fn new(image: impl Into<String>) -> Self {
        Self {
            image: image.into(),
            ..Default::default()
        }
    }

    pub fn with_command(mut self, cmd: Vec<String>) -> Self {
        self.command = cmd;
        self
    }

    pub fn with_working_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.working_dir = Some(dir.into());
        self
    }

    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    pub fn with_volume(mut self, mount: VolumeMount) -> Self {
        self.volumes.push(mount);
        self
    }

    pub fn with_limits(mut self, limits: ResourceLimits) -> Self {
        self.limits = limits;
        self
    }

    pub fn with_network(mut self, network: NetworkMode) -> Self {
        self.network = network;
        self
    }

    pub fn with_security(mut self, security: SecurityProfile) -> Self {
        self.security = security;
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Build docker run arguments
    pub fn build_args(&self, runtime: ContainerRuntime) -> Vec<String> {
        let mut args = vec![];

        // Auto-remove
        if self.auto_remove {
            args.push("--rm".to_string());
        }

        // Name
        if let Some(name) = &self.name {
            args.push("--name".to_string());
            args.push(name.clone());
        }

        // Working directory
        if let Some(dir) = &self.working_dir {
            args.push("-w".to_string());
            args.push(dir.to_string_lossy().to_string());
        }

        // Environment
        for (key, value) in &self.env {
            args.push("-e".to_string());
            args.push(format!("{}={}", key, value));
        }

        // Volumes
        for vol in &self.volumes {
            args.push("-v".to_string());
            let ro = if vol.read_only { ":ro" } else { "" };
            args.push(format!(
                "{}:{}{}",
                vol.host_path.to_string_lossy(),
                vol.container_path.to_string_lossy(),
                ro
            ));
        }

        // Resource limits
        if let Some(cpus) = self.limits.cpus {
            args.push("--cpus".to_string());
            args.push(cpus.to_string());
        }
        if let Some(mem) = &self.limits.memory {
            args.push("-m".to_string());
            args.push(mem.clone());
        }
        if let Some(swap) = &self.limits.memory_swap {
            args.push("--memory-swap".to_string());
            args.push(swap.clone());
        }
        if let Some(pids) = self.limits.pids_limit {
            args.push("--pids-limit".to_string());
            args.push(pids.to_string());
        }
        if self.limits.read_only {
            args.push("--read-only".to_string());
        }

        // Network
        match &self.network {
            NetworkMode::None => {
                args.push("--network".to_string());
                args.push("none".to_string());
            }
            NetworkMode::Bridge => {
                // Default, no arg needed
            }
            NetworkMode::Host => {
                args.push("--network".to_string());
                args.push("host".to_string());
            }
            NetworkMode::Custom(name) => {
                args.push("--network".to_string());
                args.push(name.clone());
            }
        }

        // Security
        if !self.security.drop_caps.is_empty() {
            for cap in &self.security.drop_caps {
                args.push("--cap-drop".to_string());
                args.push(cap.clone());
            }
        }
        for cap in &self.security.add_caps {
            args.push("--cap-add".to_string());
            args.push(cap.clone());
        }
        if let Some(user) = &self.security.user {
            args.push("-u".to_string());
            args.push(user.clone());
        }
        if self.security.no_new_privileges {
            args.push("--security-opt".to_string());
            args.push("no-new-privileges:true".to_string());
        }
        if let Some(seccomp) = &self.security.seccomp_profile {
            args.push("--security-opt".to_string());
            args.push(format!("seccomp={}", seccomp));
        }

        // Labels
        for (key, value) in &self.labels {
            args.push("--label".to_string());
            args.push(format!("{}={}", key, value));
        }

        // Image
        args.push(self.image.clone());

        // Command
        args.extend(self.command.clone());

        args
    }
}

/// Container execution result
#[derive(Debug, Clone)]
pub struct ContainerResult {
    /// Container ID
    pub container_id: String,
    /// Exit code
    pub exit_code: i32,
    /// Stdout
    pub stdout: String,
    /// Stderr
    pub stderr: String,
    /// Execution duration
    pub duration: Duration,
    /// Whether timed out
    pub timed_out: bool,
}

/// Container executor trait
#[async_trait]
pub trait ContainerExecutor: Send + Sync {
    /// Run a container
    async fn run(&self, config: ContainerConfig) -> Result<ContainerResult, ContainerError>;

    /// Check if runtime is available
    async fn is_available(&self) -> bool;

    /// Pull an image
    async fn pull_image(&self, image: &str) -> Result<(), ContainerError>;

    /// List running containers
    async fn list_containers(
        &self,
        label_filter: Option<&str>,
    ) -> Result<Vec<String>, ContainerError>;

    /// Stop a container
    async fn stop_container(&self, id: &str, timeout: Duration) -> Result<(), ContainerError>;

    /// Remove a container
    async fn remove_container(&self, id: &str, force: bool) -> Result<(), ContainerError>;
}

/// Container error types
#[derive(Debug, Clone)]
pub enum ContainerError {
    RuntimeNotFound,
    ImageNotFound(String),
    StartFailed(String),
    Timeout,
    ExecutionFailed(String),
    ResourceExceeded(String),
    PermissionDenied(String),
    Other(String),
}

impl std::fmt::Display for ContainerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RuntimeNotFound => write!(f, "Container runtime not found"),
            Self::ImageNotFound(img) => write!(f, "Image not found: {}", img),
            Self::StartFailed(msg) => write!(f, "Failed to start container: {}", msg),
            Self::Timeout => write!(f, "Container execution timed out"),
            Self::ExecutionFailed(msg) => write!(f, "Execution failed: {}", msg),
            Self::ResourceExceeded(msg) => write!(f, "Resource limit exceeded: {}", msg),
            Self::PermissionDenied(msg) => write!(f, "Permission denied: {}", msg),
            Self::Other(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for ContainerError {}

/// Docker/Podman executor implementation
pub struct DockerExecutor {
    runtime: ContainerRuntime,
}

impl DockerExecutor {
    pub fn new(runtime: ContainerRuntime) -> Self {
        Self { runtime }
    }

    pub fn docker() -> Self {
        Self::new(ContainerRuntime::Docker)
    }

    pub fn podman() -> Self {
        Self::new(ContainerRuntime::Podman)
    }

    /// Detect available runtime
    pub async fn detect() -> Option<Self> {
        // Try Docker first
        if Self::docker().is_available().await {
            return Some(Self::docker());
        }
        // Try Podman
        if Self::podman().is_available().await {
            return Some(Self::podman());
        }
        None
    }
}

#[async_trait]
impl ContainerExecutor for DockerExecutor {
    async fn run(&self, config: ContainerConfig) -> Result<ContainerResult, ContainerError> {
        let start = std::time::Instant::now();
        let args = config.build_args(self.runtime);

        info!(
            "Running container with {}: {} {}",
            self.runtime.command(),
            config.image,
            config.command.join(" ")
        );

        let output = tokio::process::Command::new(self.runtime.command())
            .arg("run")
            .args(&args)
            .output()
            .await
            .map_err(|e| ContainerError::ExecutionFailed(e.to_string()))?;

        let duration = start.elapsed();

        Ok(ContainerResult {
            container_id: String::new(), // Would need to parse from output
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            duration,
            timed_out: false,
        })
    }

    async fn is_available(&self) -> bool {
        tokio::process::Command::new(self.runtime.command())
            .arg("--version")
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    async fn pull_image(&self, image: &str) -> Result<(), ContainerError> {
        info!("Pulling image: {}", image);

        let output = tokio::process::Command::new(self.runtime.command())
            .args(["pull", image])
            .output()
            .await
            .map_err(|e| ContainerError::ExecutionFailed(e.to_string()))?;

        if output.status.success() {
            Ok(())
        } else {
            Err(ContainerError::ImageNotFound(image.to_string()))
        }
    }

    async fn list_containers(
        &self,
        label_filter: Option<&str>,
    ) -> Result<Vec<String>, ContainerError> {
        let mut cmd = tokio::process::Command::new(self.runtime.command());
        cmd.args(["ps", "-q"]);

        if let Some(label) = label_filter {
            cmd.args(["--filter", &format!("label={}", label)]);
        }

        let output = cmd
            .output()
            .await
            .map_err(|e| ContainerError::ExecutionFailed(e.to_string()))?;

        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|s| s.to_string())
            .collect())
    }

    async fn stop_container(&self, id: &str, timeout: Duration) -> Result<(), ContainerError> {
        info!("Stopping container: {}", id);

        let output = tokio::process::Command::new(self.runtime.command())
            .args(["stop", "-t", &timeout.as_secs().to_string(), id])
            .output()
            .await
            .map_err(|e| ContainerError::ExecutionFailed(e.to_string()))?;

        if output.status.success() {
            Ok(())
        } else {
            Err(ContainerError::ExecutionFailed(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ))
        }
    }

    async fn remove_container(&self, id: &str, force: bool) -> Result<(), ContainerError> {
        info!("Removing container: {}", id);

        let mut args = vec!["rm"];
        if force {
            args.push("-f");
        }
        args.push(id);

        let output = tokio::process::Command::new(self.runtime.command())
            .args(&args)
            .output()
            .await
            .map_err(|e| ContainerError::ExecutionFailed(e.to_string()))?;

        if output.status.success() {
            Ok(())
        } else {
            Err(ContainerError::ExecutionFailed(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ))
        }
    }
}

/// Pre-configured container templates
pub struct ContainerTemplates;

impl ContainerTemplates {
    /// Node.js development container
    pub fn nodejs() -> ContainerConfig {
        ContainerConfig::new("node:20-slim")
            .with_limits(ResourceLimits::default())
            .with_network(NetworkMode::Bridge)
    }

    /// Python development container
    pub fn python() -> ContainerConfig {
        ContainerConfig::new("python:3.12-slim")
            .with_limits(ResourceLimits::default())
            .with_network(NetworkMode::Bridge)
    }

    /// Rust development container
    pub fn rust() -> ContainerConfig {
        ContainerConfig::new("rust:1.75-slim")
            .with_limits(ResourceLimits::generous())
            .with_network(NetworkMode::Bridge)
    }

    /// Isolated shell (no network)
    pub fn isolated_shell() -> ContainerConfig {
        ContainerConfig::new("alpine:latest")
            .with_limits(ResourceLimits::minimal())
            .with_network(NetworkMode::None)
            .with_security(SecurityProfile::strict())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_container_config_build_args() {
        let config = ContainerConfig::new("ubuntu:22.04")
            .with_command(vec!["echo".to_string(), "hello".to_string()])
            .with_env("FOO", "bar")
            .with_limits(ResourceLimits {
                cpus: Some(1.0),
                memory: Some("512m".to_string()),
                ..Default::default()
            });

        let args = config.build_args(ContainerRuntime::Docker);

        assert!(args.contains(&"--rm".to_string()));
        assert!(args.contains(&"-e".to_string()));
        assert!(args.contains(&"FOO=bar".to_string()));
        assert!(args.contains(&"--cpus".to_string()));
        assert!(args.contains(&"1".to_string()));
        assert!(args.contains(&"ubuntu:22.04".to_string()));
        assert!(args.contains(&"echo".to_string()));
    }

    #[test]
    fn test_resource_limits() {
        let minimal = ResourceLimits::minimal();
        assert_eq!(minimal.cpus, Some(0.5));
        assert!(minimal.read_only);

        let generous = ResourceLimits::generous();
        assert_eq!(generous.cpus, Some(4.0));
        assert!(!generous.read_only);
    }

    #[test]
    fn test_security_profiles() {
        let strict = SecurityProfile::strict();
        assert!(strict.no_new_privileges);
        assert!(strict.drop_caps.contains(&"ALL".to_string()));

        let permissive = SecurityProfile::permissive();
        assert!(!permissive.no_new_privileges);
        assert!(permissive.drop_caps.is_empty());
    }

    #[test]
    fn test_volume_mount() {
        let mount = VolumeMount::new("/host/path", "/container/path").read_only();
        assert!(mount.read_only);
    }

    #[test]
    fn test_templates() {
        let nodejs = ContainerTemplates::nodejs();
        assert!(nodejs.image.contains("node"));

        let isolated = ContainerTemplates::isolated_shell();
        assert!(matches!(isolated.network, NetworkMode::None));
    }
}
