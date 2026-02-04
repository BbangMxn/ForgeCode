//! Task Server Cluster Module
//!
//! Provides distributed task execution across multiple server instances.
//!
//! Features:
//! - Multi-server management
//! - API health checking and validation
//! - Load balancing strategies
//! - Automatic failover
//! - Server discovery

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Server status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServerStatus {
    /// Server is healthy and accepting tasks
    Healthy,
    /// Server is degraded but still working
    Degraded,
    /// Server is unhealthy
    Unhealthy,
    /// Server is unreachable
    Unreachable,
    /// Server is being drained (no new tasks)
    Draining,
    /// Server is offline
    Offline,
}

impl ServerStatus {
    pub fn is_available(&self) -> bool {
        matches!(self, Self::Healthy | Self::Degraded)
    }
}

/// Server information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    /// Unique server ID
    pub id: String,
    /// Server name/label
    pub name: String,
    /// Server endpoint URL
    pub endpoint: String,
    /// API key (if required)
    #[serde(skip_serializing)]
    pub api_key: Option<String>,
    /// Server capabilities
    pub capabilities: Vec<String>,
    /// Server metadata
    pub metadata: HashMap<String, String>,
}

impl ServerInfo {
    pub fn new(id: impl Into<String>, endpoint: impl Into<String>) -> Self {
        let id = id.into();
        Self {
            name: id.clone(),
            id,
            endpoint: endpoint.into(),
            api_key: None,
            capabilities: vec![],
            metadata: HashMap::new(),
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    pub fn with_api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    pub fn with_capability(mut self, cap: impl Into<String>) -> Self {
        self.capabilities.push(cap.into());
        self
    }
}

/// Health check result
#[derive(Debug, Clone)]
pub struct HealthCheckResult {
    pub status: ServerStatus,
    pub latency: Duration,
    pub message: Option<String>,
    pub checked_at: Instant,
    /// Server-reported load (0.0 - 1.0)
    pub load: Option<f32>,
    /// Available capacity
    pub capacity: Option<u32>,
}

impl Default for HealthCheckResult {
    fn default() -> Self {
        Self {
            status: ServerStatus::Unreachable,
            latency: Duration::ZERO,
            message: None,
            checked_at: Instant::now(),
            load: None,
            capacity: None,
        }
    }
}

/// Server state tracked by cluster
#[derive(Debug)]
pub struct ServerState {
    pub info: ServerInfo,
    pub status: ServerStatus,
    pub last_health_check: Option<HealthCheckResult>,
    pub consecutive_failures: u32,
    pub total_requests: AtomicU64,
    pub active_requests: AtomicU64,
    pub total_errors: AtomicU64,
    pub added_at: Instant,
}

impl ServerState {
    pub fn new(info: ServerInfo) -> Self {
        Self {
            info,
            status: ServerStatus::Healthy,
            last_health_check: None,
            consecutive_failures: 0,
            total_requests: AtomicU64::new(0),
            active_requests: AtomicU64::new(0),
            total_errors: AtomicU64::new(0),
            added_at: Instant::now(),
        }
    }

    pub fn error_rate(&self) -> f64 {
        let total = self.total_requests.load(Ordering::Relaxed);
        let errors = self.total_errors.load(Ordering::Relaxed);
        if total == 0 {
            0.0
        } else {
            errors as f64 / total as f64
        }
    }

    pub fn current_load(&self) -> f64 {
        self.last_health_check
            .as_ref()
            .and_then(|h| h.load)
            .map(|l| l as f64)
            .unwrap_or(0.0)
    }
}

/// Load balancing strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LoadBalanceStrategy {
    /// Round-robin selection
    RoundRobin,
    /// Select server with least active connections
    LeastConnections,
    /// Select server with lowest latency
    LeastLatency,
    /// Select server with lowest load
    LeastLoad,
    /// Weighted random selection
    WeightedRandom,
    /// Hash-based sticky selection
    Sticky,
}

impl Default for LoadBalanceStrategy {
    fn default() -> Self {
        Self::LeastConnections
    }
}

/// Cluster configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterConfig {
    /// Health check interval
    pub health_check_interval: Duration,
    /// Health check timeout
    pub health_check_timeout: Duration,
    /// Max consecutive failures before marking unhealthy
    pub max_failures: u32,
    /// Load balancing strategy
    pub strategy: LoadBalanceStrategy,
    /// Enable automatic failover
    pub auto_failover: bool,
    /// Retry count on failure
    pub retry_count: u32,
    /// Retry delay
    pub retry_delay: Duration,
}

impl Default for ClusterConfig {
    fn default() -> Self {
        Self {
            health_check_interval: Duration::from_secs(30),
            health_check_timeout: Duration::from_secs(10),
            max_failures: 3,
            strategy: LoadBalanceStrategy::LeastConnections,
            auto_failover: true,
            retry_count: 2,
            retry_delay: Duration::from_millis(500),
        }
    }
}

/// API validation result
#[derive(Debug, Clone)]
pub struct ApiValidationResult {
    pub server_id: String,
    pub endpoint: String,
    pub valid: bool,
    pub api_version: Option<String>,
    pub supported_methods: Vec<String>,
    pub errors: Vec<String>,
    pub latency: Duration,
}

/// Health checker trait
#[async_trait]
pub trait HealthChecker: Send + Sync {
    async fn check(&self, server: &ServerInfo) -> HealthCheckResult;
    async fn validate_api(&self, server: &ServerInfo) -> ApiValidationResult;
}

/// Default HTTP health checker
pub struct HttpHealthChecker {
    client: reqwest::Client,
    timeout: Duration,
}

impl HttpHealthChecker {
    pub fn new(timeout: Duration) -> Self {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .unwrap_or_default();
        Self { client, timeout }
    }
}

#[async_trait]
impl HealthChecker for HttpHealthChecker {
    async fn check(&self, server: &ServerInfo) -> HealthCheckResult {
        let start = Instant::now();
        let health_url = format!("{}/health", server.endpoint.trim_end_matches('/'));

        let mut request = self.client.get(&health_url);
        if let Some(key) = &server.api_key {
            request = request.header("Authorization", format!("Bearer {}", key));
        }

        match request.send().await {
            Ok(response) => {
                let latency = start.elapsed();
                let status = if response.status().is_success() {
                    ServerStatus::Healthy
                } else if response.status().is_server_error() {
                    ServerStatus::Unhealthy
                } else {
                    ServerStatus::Degraded
                };

                // Try to parse health response
                let (load, capacity) = if let Ok(body) = response.json::<serde_json::Value>().await
                {
                    let load = body.get("load").and_then(|v| v.as_f64()).map(|l| l as f32);
                    let capacity = body
                        .get("capacity")
                        .and_then(|v| v.as_u64())
                        .map(|c| c as u32);
                    (load, capacity)
                } else {
                    (None, None)
                };

                HealthCheckResult {
                    status,
                    latency,
                    message: None,
                    checked_at: Instant::now(),
                    load,
                    capacity,
                }
            }
            Err(e) => HealthCheckResult {
                status: ServerStatus::Unreachable,
                latency: start.elapsed(),
                message: Some(e.to_string()),
                checked_at: Instant::now(),
                load: None,
                capacity: None,
            },
        }
    }

    async fn validate_api(&self, server: &ServerInfo) -> ApiValidationResult {
        let start = Instant::now();
        let api_url = format!("{}/api/info", server.endpoint.trim_end_matches('/'));

        let mut request = self.client.get(&api_url);
        if let Some(key) = &server.api_key {
            request = request.header("Authorization", format!("Bearer {}", key));
        }

        match request.send().await {
            Ok(response) => {
                let latency = start.elapsed();
                if response.status().is_success() {
                    if let Ok(body) = response.json::<serde_json::Value>().await {
                        let version = body
                            .get("version")
                            .and_then(|v| v.as_str())
                            .map(String::from);
                        let methods = body
                            .get("methods")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| v.as_str().map(String::from))
                                    .collect()
                            })
                            .unwrap_or_default();

                        ApiValidationResult {
                            server_id: server.id.clone(),
                            endpoint: server.endpoint.clone(),
                            valid: true,
                            api_version: version,
                            supported_methods: methods,
                            errors: vec![],
                            latency,
                        }
                    } else {
                        ApiValidationResult {
                            server_id: server.id.clone(),
                            endpoint: server.endpoint.clone(),
                            valid: false,
                            api_version: None,
                            supported_methods: vec![],
                            errors: vec!["Invalid API response format".to_string()],
                            latency,
                        }
                    }
                } else {
                    ApiValidationResult {
                        server_id: server.id.clone(),
                        endpoint: server.endpoint.clone(),
                        valid: false,
                        api_version: None,
                        supported_methods: vec![],
                        errors: vec![format!("HTTP {}", response.status())],
                        latency,
                    }
                }
            }
            Err(e) => ApiValidationResult {
                server_id: server.id.clone(),
                endpoint: server.endpoint.clone(),
                valid: false,
                api_version: None,
                supported_methods: vec![],
                errors: vec![e.to_string()],
                latency: start.elapsed(),
            },
        }
    }
}

/// Task Server Cluster
pub struct TaskCluster {
    /// Cluster configuration
    config: ClusterConfig,
    /// Registered servers
    servers: Arc<RwLock<HashMap<String, ServerState>>>,
    /// Health checker
    health_checker: Arc<dyn HealthChecker>,
    /// Round-robin counter
    rr_counter: AtomicUsize,
    /// Health check task handle
    health_check_handle: RwLock<Option<tokio::task::JoinHandle<()>>>,
}

impl TaskCluster {
    /// Create a new cluster
    pub fn new(config: ClusterConfig) -> Self {
        let health_checker = Arc::new(HttpHealthChecker::new(config.health_check_timeout));
        Self {
            config,
            servers: Arc::new(RwLock::new(HashMap::new())),
            health_checker,
            rr_counter: AtomicUsize::new(0),
            health_check_handle: RwLock::new(None),
        }
    }

    /// Create with default config
    pub fn default() -> Self {
        Self::new(ClusterConfig::default())
    }

    /// Create with custom health checker
    pub fn with_health_checker(mut self, checker: Arc<dyn HealthChecker>) -> Self {
        self.health_checker = checker;
        self
    }

    /// Add a server to the cluster
    pub async fn add_server(&self, info: ServerInfo) -> Result<(), ClusterError> {
        let server_id = info.id.clone();

        // Validate API first
        let validation = self.health_checker.validate_api(&info).await;
        if !validation.valid {
            warn!(
                "Server {} API validation failed: {:?}",
                server_id, validation.errors
            );
            // Still add but mark as degraded
        }

        let state = ServerState::new(info);

        let mut servers = self.servers.write().await;
        if servers.contains_key(&server_id) {
            return Err(ClusterError::ServerAlreadyExists(server_id));
        }

        info!("Adding server {} to cluster", server_id);
        servers.insert(server_id, state);

        Ok(())
    }

    /// Remove a server from the cluster
    pub async fn remove_server(&self, server_id: &str) -> Result<ServerInfo, ClusterError> {
        let mut servers = self.servers.write().await;
        let state = servers
            .remove(server_id)
            .ok_or_else(|| ClusterError::ServerNotFound(server_id.to_string()))?;

        info!("Removed server {} from cluster", server_id);
        Ok(state.info)
    }

    /// Drain a server (stop sending new tasks)
    pub async fn drain_server(&self, server_id: &str) -> Result<(), ClusterError> {
        let mut servers = self.servers.write().await;
        let state = servers
            .get_mut(server_id)
            .ok_or_else(|| ClusterError::ServerNotFound(server_id.to_string()))?;

        state.status = ServerStatus::Draining;
        info!("Draining server {}", server_id);
        Ok(())
    }

    /// Get all server statuses
    pub async fn get_servers(&self) -> Vec<(ServerInfo, ServerStatus)> {
        let servers = self.servers.read().await;
        servers
            .values()
            .map(|s| (s.info.clone(), s.status))
            .collect()
    }

    /// Get available servers
    pub async fn get_available_servers(&self) -> Vec<ServerInfo> {
        let servers = self.servers.read().await;
        servers
            .values()
            .filter(|s| s.status.is_available())
            .map(|s| s.info.clone())
            .collect()
    }

    /// Select a server using the configured strategy
    pub async fn select_server(&self, sticky_key: Option<&str>) -> Option<ServerInfo> {
        let servers = self.servers.read().await;
        let available: Vec<_> = servers
            .values()
            .filter(|s| s.status.is_available())
            .collect();

        if available.is_empty() {
            return None;
        }

        let selected = match self.config.strategy {
            LoadBalanceStrategy::RoundRobin => {
                let idx = self.rr_counter.fetch_add(1, Ordering::Relaxed) % available.len();
                available.get(idx)
            }
            LoadBalanceStrategy::LeastConnections => available
                .iter()
                .min_by_key(|s| s.active_requests.load(Ordering::Relaxed)),
            LoadBalanceStrategy::LeastLatency => available.iter().min_by_key(|s| {
                s.last_health_check
                    .as_ref()
                    .map(|h| h.latency)
                    .unwrap_or(Duration::MAX)
            }),
            LoadBalanceStrategy::LeastLoad => available.iter().min_by(|a, b| {
                a.current_load()
                    .partial_cmp(&b.current_load())
                    .unwrap_or(std::cmp::Ordering::Equal)
            }),
            LoadBalanceStrategy::WeightedRandom => {
                // Simple weighted random based on inverse load
                use rand::Rng;
                let weights: Vec<f64> = available.iter().map(|s| 1.0 - s.current_load()).collect();
                let total: f64 = weights.iter().sum();

                if total > 0.0 {
                    let mut rng = rand::thread_rng();
                    let mut r = rng.gen::<f64>() * total;
                    available
                        .iter()
                        .zip(weights.iter())
                        .find(|(_, w)| {
                            r -= **w;
                            r <= 0.0
                        })
                        .map(|(s, _)| s)
                } else {
                    available.first()
                }
            }
            LoadBalanceStrategy::Sticky => {
                if let Some(key) = sticky_key {
                    // Hash the key to select a server
                    use std::hash::{Hash, Hasher};
                    let mut hasher = std::collections::hash_map::DefaultHasher::new();
                    key.hash(&mut hasher);
                    let hash = hasher.finish() as usize;
                    available.get(hash % available.len())
                } else {
                    available.first()
                }
            }
        };

        selected.map(|s| s.info.clone())
    }

    /// Select a server with capability requirement
    pub async fn select_server_with_capability(&self, capability: &str) -> Option<ServerInfo> {
        let servers = self.servers.read().await;
        let available: Vec<_> = servers
            .values()
            .filter(|s| {
                s.status.is_available() && s.info.capabilities.contains(&capability.to_string())
            })
            .collect();

        if available.is_empty() {
            return None;
        }

        // Use least connections for capability-based selection
        available
            .iter()
            .min_by_key(|s| s.active_requests.load(Ordering::Relaxed))
            .map(|s| s.info.clone())
    }

    /// Record a request to a server
    pub async fn record_request(&self, server_id: &str) {
        let servers = self.servers.read().await;
        if let Some(state) = servers.get(server_id) {
            state.total_requests.fetch_add(1, Ordering::Relaxed);
            state.active_requests.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Record request completion
    pub async fn record_completion(&self, server_id: &str, success: bool) {
        let servers = self.servers.read().await;
        if let Some(state) = servers.get(server_id) {
            state.active_requests.fetch_sub(1, Ordering::Relaxed);
            if !success {
                state.total_errors.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// Run health check on all servers
    pub async fn check_health(&self) {
        let server_ids: Vec<String> = {
            let servers = self.servers.read().await;
            servers.keys().cloned().collect()
        };

        for server_id in server_ids {
            self.check_server_health(&server_id).await;
        }
    }

    /// Check health of a specific server
    async fn check_server_health(&self, server_id: &str) {
        let info = {
            let servers = self.servers.read().await;
            servers.get(server_id).map(|s| s.info.clone())
        };

        let info = match info {
            Some(i) => i,
            None => return,
        };

        let result = self.health_checker.check(&info).await;

        let mut servers = self.servers.write().await;
        if let Some(state) = servers.get_mut(server_id) {
            if result.status.is_available() {
                state.consecutive_failures = 0;
                state.status = result.status;
            } else {
                state.consecutive_failures += 1;
                if state.consecutive_failures >= self.config.max_failures {
                    state.status = ServerStatus::Unhealthy;
                    warn!(
                        "Server {} marked unhealthy after {} failures",
                        server_id, state.consecutive_failures
                    );
                }
            }
            state.last_health_check = Some(result);
        }
    }

    /// Validate API for all servers
    pub async fn validate_all_apis(&self) -> Vec<ApiValidationResult> {
        let servers = self.servers.read().await;
        let mut results = Vec::new();

        for state in servers.values() {
            let result = self.health_checker.validate_api(&state.info).await;
            results.push(result);
        }

        results
    }

    /// Start periodic health checks
    pub async fn start_health_checks(&self) {
        let servers = Arc::clone(&self.servers);
        let health_checker = Arc::clone(&self.health_checker);
        let interval = self.config.health_check_interval;
        let max_failures = self.config.max_failures;

        let handle = tokio::spawn(async move {
            let mut timer = tokio::time::interval(interval);

            loop {
                timer.tick().await;

                let server_ids: Vec<String> = {
                    let servers = servers.read().await;
                    servers.keys().cloned().collect()
                };

                for server_id in server_ids {
                    let info = {
                        let servers = servers.read().await;
                        servers.get(&server_id).map(|s| s.info.clone())
                    };

                    if let Some(info) = info {
                        let result = health_checker.check(&info).await;

                        let mut servers = servers.write().await;
                        if let Some(state) = servers.get_mut(&server_id) {
                            if result.status.is_available() {
                                state.consecutive_failures = 0;
                                state.status = result.status;
                            } else {
                                state.consecutive_failures += 1;
                                if state.consecutive_failures >= max_failures {
                                    state.status = ServerStatus::Unhealthy;
                                }
                            }
                            state.last_health_check = Some(result);
                        }
                    }
                }

                debug!("Completed health check cycle");
            }
        });

        let mut guard = self.health_check_handle.write().await;
        *guard = Some(handle);
    }

    /// Stop health checks
    pub async fn stop_health_checks(&self) {
        let mut guard = self.health_check_handle.write().await;
        if let Some(handle) = guard.take() {
            handle.abort();
        }
    }

    /// Get cluster statistics
    pub async fn stats(&self) -> ClusterStats {
        let servers = self.servers.read().await;

        let mut total = 0;
        let mut healthy = 0;
        let mut degraded = 0;
        let mut unhealthy = 0;
        let mut total_requests = 0u64;
        let mut active_requests = 0u64;
        let mut total_errors = 0u64;

        for state in servers.values() {
            total += 1;
            match state.status {
                ServerStatus::Healthy => healthy += 1,
                ServerStatus::Degraded => degraded += 1,
                ServerStatus::Unhealthy | ServerStatus::Unreachable => unhealthy += 1,
                _ => {}
            }
            total_requests += state.total_requests.load(Ordering::Relaxed);
            active_requests += state.active_requests.load(Ordering::Relaxed);
            total_errors += state.total_errors.load(Ordering::Relaxed);
        }

        ClusterStats {
            total_servers: total,
            healthy_servers: healthy,
            degraded_servers: degraded,
            unhealthy_servers: unhealthy,
            total_requests,
            active_requests,
            total_errors,
            error_rate: if total_requests > 0 {
                total_errors as f64 / total_requests as f64
            } else {
                0.0
            },
        }
    }
}

/// Cluster statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterStats {
    pub total_servers: usize,
    pub healthy_servers: usize,
    pub degraded_servers: usize,
    pub unhealthy_servers: usize,
    pub total_requests: u64,
    pub active_requests: u64,
    pub total_errors: u64,
    pub error_rate: f64,
}

/// Cluster error types
#[derive(Debug, Clone)]
pub enum ClusterError {
    ServerNotFound(String),
    ServerAlreadyExists(String),
    NoAvailableServers,
    HealthCheckFailed(String),
    ApiValidationFailed(String),
}

impl std::fmt::Display for ClusterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ServerNotFound(id) => write!(f, "Server not found: {}", id),
            Self::ServerAlreadyExists(id) => write!(f, "Server already exists: {}", id),
            Self::NoAvailableServers => write!(f, "No available servers in cluster"),
            Self::HealthCheckFailed(msg) => write!(f, "Health check failed: {}", msg),
            Self::ApiValidationFailed(msg) => write!(f, "API validation failed: {}", msg),
        }
    }
}

impl std::error::Error for ClusterError {}

/// Request context for cluster routing
pub struct RequestContext {
    pub sticky_key: Option<String>,
    pub required_capability: Option<String>,
    pub preferred_server: Option<String>,
    pub timeout: Option<Duration>,
}

impl Default for RequestContext {
    fn default() -> Self {
        Self {
            sticky_key: None,
            required_capability: None,
            preferred_server: None,
            timeout: None,
        }
    }
}

impl RequestContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_sticky_key(mut self, key: impl Into<String>) -> Self {
        self.sticky_key = Some(key.into());
        self
    }

    pub fn with_capability(mut self, cap: impl Into<String>) -> Self {
        self.required_capability = Some(cap.into());
        self
    }

    pub fn with_preferred_server(mut self, server: impl Into<String>) -> Self {
        self.preferred_server = Some(server.into());
        self
    }
}

/// Cluster-aware task executor
pub struct ClusterExecutor {
    cluster: Arc<TaskCluster>,
    config: ClusterConfig,
}

impl ClusterExecutor {
    pub fn new(cluster: Arc<TaskCluster>) -> Self {
        let config = cluster.config.clone();
        Self { cluster, config }
    }

    /// Execute a task on the cluster with automatic retry and failover
    pub async fn execute<F, T, E>(&self, ctx: RequestContext, task: F) -> Result<T, ClusterError>
    where
        F: Fn(
                &ServerInfo,
            )
                -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, E>> + Send>>
            + Send
            + Sync,
        E: std::fmt::Display,
    {
        let mut attempts = 0;
        let mut last_error: Option<String> = None;

        while attempts <= self.config.retry_count {
            // Select server
            let server = if let Some(ref preferred) = ctx.preferred_server {
                let servers = self.cluster.get_servers().await;
                servers
                    .into_iter()
                    .find(|(s, status)| s.id == *preferred && status.is_available())
                    .map(|(s, _)| s)
            } else if let Some(ref cap) = ctx.required_capability {
                self.cluster.select_server_with_capability(cap).await
            } else {
                self.cluster.select_server(ctx.sticky_key.as_deref()).await
            };

            let server = match server {
                Some(s) => s,
                None => {
                    if attempts == 0 {
                        return Err(ClusterError::NoAvailableServers);
                    }
                    break;
                }
            };

            // Record request
            self.cluster.record_request(&server.id).await;

            // Execute
            let result = task(&server).await;

            match result {
                Ok(value) => {
                    self.cluster.record_completion(&server.id, true).await;
                    return Ok(value);
                }
                Err(e) => {
                    self.cluster.record_completion(&server.id, false).await;
                    last_error = Some(e.to_string());
                    warn!(
                        "Task failed on server {}: {} (attempt {})",
                        server.id,
                        e,
                        attempts + 1
                    );

                    if self.config.auto_failover {
                        attempts += 1;
                        if attempts <= self.config.retry_count {
                            tokio::time::sleep(self.config.retry_delay).await;
                        }
                    } else {
                        break;
                    }
                }
            }
        }

        Err(ClusterError::ApiValidationFailed(
            last_error.unwrap_or_else(|| "Unknown error".to_string()),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cluster_creation() {
        let cluster = TaskCluster::default();
        let stats = cluster.stats().await;
        assert_eq!(stats.total_servers, 0);
    }

    #[tokio::test]
    async fn test_add_remove_server() {
        let cluster = TaskCluster::default();

        let server = ServerInfo::new("server-1", "http://localhost:8080");
        cluster.add_server(server).await.unwrap();

        let stats = cluster.stats().await;
        assert_eq!(stats.total_servers, 1);

        cluster.remove_server("server-1").await.unwrap();
        let stats = cluster.stats().await;
        assert_eq!(stats.total_servers, 0);
    }

    #[tokio::test]
    async fn test_server_selection_round_robin() {
        let config = ClusterConfig {
            strategy: LoadBalanceStrategy::RoundRobin,
            ..Default::default()
        };
        let cluster = TaskCluster::new(config);

        cluster
            .add_server(ServerInfo::new("server-1", "http://localhost:8081"))
            .await
            .unwrap();
        cluster
            .add_server(ServerInfo::new("server-2", "http://localhost:8082"))
            .await
            .unwrap();

        // Multiple selections should alternate
        let s1 = cluster.select_server(None).await.unwrap();
        let s2 = cluster.select_server(None).await.unwrap();

        assert_ne!(s1.id, s2.id);
    }

    #[tokio::test]
    async fn test_capability_selection() {
        let cluster = TaskCluster::default();

        cluster
            .add_server(
                ServerInfo::new("gpu-server", "http://localhost:8081").with_capability("gpu"),
            )
            .await
            .unwrap();
        cluster
            .add_server(ServerInfo::new("cpu-server", "http://localhost:8082"))
            .await
            .unwrap();

        let selected = cluster.select_server_with_capability("gpu").await;
        assert!(selected.is_some());
        assert_eq!(selected.unwrap().id, "gpu-server");
    }

    #[tokio::test]
    async fn test_request_tracking() {
        let cluster = TaskCluster::default();

        cluster
            .add_server(ServerInfo::new("server-1", "http://localhost:8080"))
            .await
            .unwrap();

        cluster.record_request("server-1").await;
        cluster.record_request("server-1").await;
        cluster.record_completion("server-1", true).await;
        cluster.record_completion("server-1", false).await;

        let stats = cluster.stats().await;
        assert_eq!(stats.total_requests, 2);
        assert_eq!(stats.total_errors, 1);
    }

    #[test]
    fn test_server_status() {
        assert!(ServerStatus::Healthy.is_available());
        assert!(ServerStatus::Degraded.is_available());
        assert!(!ServerStatus::Unhealthy.is_available());
        assert!(!ServerStatus::Unreachable.is_available());
    }
}
