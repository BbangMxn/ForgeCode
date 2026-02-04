//! Agent Context - Layer2 통합 인터페이스
//!
//! Agent가 Layer2의 모든 기능(Provider, Tool, Task)을 사용하기 위한
//! 통합 컨텍스트를 제공합니다.
//!
//! ## 기능
//! - Provider (LLM) 호출
//! - Tool 실행 (권한 검사 포함)
//! - Task 관리 (로그 및 종료)
//! - MCP 브릿지 통합
//! - 에러 처리 및 복구
//!
//! ## 사용 예시
//! ```ignore
//! let ctx = AgentContext::builder()
//!     .with_provider_config(&provider_config)
//!     .with_permission_service(permissions)
//!     .build()
//!     .await?;
//!
//! // LLM 호출
//! let response = ctx.complete(&request).await?;
//!
//! // Tool 실행
//! let result = ctx.execute_tool("read", json!({"path": "file.txt"})).await?;
//!
//! // Task 실행 (로그 수집)
//! let task_result = ctx.execute_task("cargo build").await?;
//! let logs = ctx.get_task_logs(task_result.task_id).await;
//! ```

use crate::mcp::{McpBridge, McpClient, McpTransportConfig};
use crate::tool::{RuntimeContext, ToolRegistry};
use forge_foundation::{Error, PermissionAction, PermissionService, PermissionStatus, Result, Tool, ToolResult};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

// ============================================================================
// Configuration
// ============================================================================

/// Agent Context 설정
#[derive(Debug, Clone)]
pub struct AgentContextConfig {
    /// 작업 디렉토리
    pub working_directory: PathBuf,

    /// 세션 ID
    pub session_id: String,

    /// 최대 동시 태스크
    pub max_concurrent_tasks: usize,

    /// 기본 태스크 타임아웃
    pub default_task_timeout: Duration,

    /// MCP 서버 자동 연결
    pub auto_connect_mcp: bool,

    /// LSP 활성화
    pub enable_lsp: bool,

    /// 도구 실행 전 권한 확인
    pub check_permissions: bool,
}

impl Default for AgentContextConfig {
    fn default() -> Self {
        Self {
            working_directory: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            session_id: uuid::Uuid::new_v4().to_string(),
            max_concurrent_tasks: 4,
            default_task_timeout: Duration::from_secs(300),
            auto_connect_mcp: false,
            enable_lsp: false,
            check_permissions: true,
        }
    }
}

// ============================================================================
// MCP Tool Statistics
// ============================================================================

/// MCP 도구 통계
#[derive(Debug, Clone)]
pub struct McpToolStats {
    /// 총 MCP 도구 수
    pub total_tools: usize,

    /// 연결된 MCP 서버 수
    pub server_count: usize,

    /// Builtin 도구 수
    pub builtin_count: usize,
}

// ============================================================================
// Tool Execution Result
// ============================================================================

/// 도구 실행 결과 (통합)
#[derive(Debug, Clone)]
pub struct ToolExecutionResult {
    /// 도구 이름
    pub tool_name: String,

    /// 실행 성공 여부
    pub success: bool,

    /// 출력 내용
    pub output: String,

    /// 에러 메시지 (실패 시)
    pub error: Option<String>,

    /// 실행 시간 (ms)
    pub duration_ms: u64,

    /// 권한이 필요했는지
    pub permission_required: bool,

    /// 권한이 부여되었는지
    pub permission_granted: bool,
}

impl From<ToolResult> for ToolExecutionResult {
    fn from(result: ToolResult) -> Self {
        Self {
            tool_name: String::new(),
            success: result.success,
            output: result.output,
            error: result.error,
            duration_ms: 0,
            permission_required: false,
            permission_granted: false,
        }
    }
}

// ============================================================================
// Agent Context
// ============================================================================

/// Agent Context - Layer2 통합 인터페이스
///
/// Provider, Tool, Task를 통합하여 Agent에게 단일 인터페이스 제공
pub struct AgentContext {
    /// 설정
    config: AgentContextConfig,

    /// 도구 레지스트리
    tools: Arc<RwLock<ToolRegistry>>,

    /// 권한 서비스
    permissions: Option<Arc<PermissionService>>,

    /// MCP 브릿지
    mcp_bridge: Arc<RwLock<McpBridge>>,

    /// 실행 통계
    stats: Arc<RwLock<ExecutionStats>>,
}

/// 실행 통계
#[derive(Debug, Default)]
pub struct ExecutionStats {
    /// 도구 실행 횟수
    pub tool_executions: usize,

    /// 성공한 도구 실행
    pub tool_successes: usize,

    /// 실패한 도구 실행
    pub tool_failures: usize,

    /// 권한 거부 횟수
    pub permission_denials: usize,

    /// 총 실행 시간 (ms)
    pub total_duration_ms: u64,
}

impl AgentContext {
    /// 빌더 생성
    pub fn builder() -> AgentContextBuilder {
        AgentContextBuilder::new()
    }

    /// 기본 설정으로 생성
    pub fn new() -> Self {
        Self {
            config: AgentContextConfig::default(),
            tools: Arc::new(RwLock::new(ToolRegistry::with_builtins())),
            permissions: None,
            mcp_bridge: Arc::new(RwLock::new(McpBridge::new())),
            stats: Arc::new(RwLock::new(ExecutionStats::default())),
        }
    }

    /// 설정과 함께 생성
    pub fn with_config(config: AgentContextConfig) -> Self {
        Self {
            config,
            tools: Arc::new(RwLock::new(ToolRegistry::with_builtins())),
            permissions: None,
            mcp_bridge: Arc::new(RwLock::new(McpBridge::new())),
            stats: Arc::new(RwLock::new(ExecutionStats::default())),
        }
    }

    // ========================================================================
    // Tool Execution
    // ========================================================================

    /// 도구 실행
    pub async fn execute_tool(&self, name: &str, input: Value) -> Result<ToolExecutionResult> {
        let start = std::time::Instant::now();

        // 도구 조회
        let tool = {
            let tools = self.tools.read().await;
            tools.get(name).ok_or_else(|| {
                Error::NotFound(format!("Tool '{}' not found", name))
            })?
        };

        // 권한 확인
        let permission_required = tool.required_permission(&input).is_some();
        let mut permission_granted = !permission_required;

        if permission_required && self.config.check_permissions {
            if let Some(ref permissions) = self.permissions {
                if let Some(action) = tool.required_permission(&input) {
                    let status = permissions.check(name, &action);
                    match status {
                        PermissionStatus::Granted | PermissionStatus::AutoApproved => {
                            permission_granted = true;
                        }
                        PermissionStatus::Denied => {
                            permission_granted = false;
                            let mut stats = self.stats.write().await;
                            stats.permission_denials += 1;

                            return Ok(ToolExecutionResult {
                                tool_name: name.to_string(),
                                success: false,
                                output: String::new(),
                                error: Some(format!(
                                    "Permission denied for action: {:?}",
                                    action
                                )),
                                duration_ms: start.elapsed().as_millis() as u64,
                                permission_required,
                                permission_granted,
                            });
                        }
                        PermissionStatus::Unknown => {
                            // 자동 허용 (Agent에서 UI 프롬프트 처리)
                            permission_granted = true;
                        }
                    }
                }
            } else {
                // 권한 서비스 없으면 허용
                permission_granted = true;
            }
        }

        // RuntimeContext 생성
        let runtime_ctx = RuntimeContext::new(
            &self.config.session_id,
            self.config.working_directory.clone(),
            self.permissions.clone().unwrap_or_else(|| Arc::new(PermissionService::new())),
        );

        // 도구 실행
        debug!("Executing tool '{}' with input: {:?}", name, input);

        let result = tool.execute(input, &runtime_ctx).await;
        let duration_ms = start.elapsed().as_millis() as u64;

        // 통계 업데이트
        {
            let mut stats = self.stats.write().await;
            stats.tool_executions += 1;
            stats.total_duration_ms += duration_ms;

            match &result {
                Ok(r) if r.success => stats.tool_successes += 1,
                _ => stats.tool_failures += 1,
            }
        }

        match result {
            Ok(tool_result) => Ok(ToolExecutionResult {
                tool_name: name.to_string(),
                success: tool_result.success,
                output: tool_result.output,
                error: tool_result.error,
                duration_ms,
                permission_required,
                permission_granted,
            }),
            Err(e) => {
                error!("Tool '{}' execution failed: {}", name, e);
                Ok(ToolExecutionResult {
                    tool_name: name.to_string(),
                    success: false,
                    output: String::new(),
                    error: Some(e.to_string()),
                    duration_ms,
                    permission_required,
                    permission_granted,
                })
            }
        }
    }

    /// 여러 도구 병렬 실행
    pub async fn execute_tools_parallel(
        &self,
        calls: Vec<(&str, Value)>,
    ) -> Vec<Result<ToolExecutionResult>> {
        let futures: Vec<_> = calls
            .into_iter()
            .map(|(name, input)| {
                let name = name.to_string();
                async move { self.execute_tool(&name, input).await }
            })
            .collect();

        futures::future::join_all(futures).await
    }

    // ========================================================================
    // Tool Registry Access
    // ========================================================================

    /// 도구 목록 조회
    pub async fn list_tools(&self) -> Vec<(String, String)> {
        let tools = self.tools.read().await;
        tools
            .list()
            .into_iter()
            .map(|(name, desc)| (name.to_string(), desc))
            .collect()
    }

    /// 도구 스키마 조회
    pub async fn get_tool_schemas(&self) -> Vec<Value> {
        let tools = self.tools.read().await;
        tools.schemas()
    }

    /// 도구 등록
    pub async fn register_tool(&self, tool: Arc<dyn Tool>) {
        let mut tools = self.tools.write().await;
        let name = tool.name().to_string();
        tools.register(tool);
        info!("Registered tool: {}", name);
    }

    /// 도구 존재 여부 확인
    pub async fn has_tool(&self, name: &str) -> bool {
        let tools = self.tools.read().await;
        tools.contains(name)
    }

    // ========================================================================
    // Permission Management
    // ========================================================================

    /// 권한 요청
    pub async fn request_permission(&self, tool_name: &str, description: &str, action: PermissionAction) -> Result<bool> {
        if let Some(ref permissions) = self.permissions {
            permissions.request(&self.config.session_id, tool_name, description, action).await
        } else {
            // 권한 서비스 없으면 자동 허용
            Ok(true)
        }
    }

    /// 권한 확인
    pub fn check_permission(&self, tool_name: &str, action: &PermissionAction) -> PermissionStatus {
        if let Some(ref permissions) = self.permissions {
            permissions.check(tool_name, action)
        } else {
            PermissionStatus::Granted
        }
    }

    // ========================================================================
    // Context Information
    // ========================================================================

    /// 세션 ID
    pub fn session_id(&self) -> &str {
        &self.config.session_id
    }

    /// 작업 디렉토리
    pub fn working_directory(&self) -> &PathBuf {
        &self.config.working_directory
    }

    /// 실행 통계
    pub async fn stats(&self) -> ExecutionStats {
        let stats = self.stats.read().await;
        ExecutionStats {
            tool_executions: stats.tool_executions,
            tool_successes: stats.tool_successes,
            tool_failures: stats.tool_failures,
            permission_denials: stats.permission_denials,
            total_duration_ms: stats.total_duration_ms,
        }
    }

    /// 통계 초기화
    pub async fn reset_stats(&self) {
        let mut stats = self.stats.write().await;
        *stats = ExecutionStats::default();
    }

    // ========================================================================
    // MCP Integration
    // ========================================================================

    /// MCP 서버 연결
    pub async fn connect_mcp_server(
        &self,
        name: &str,
        config: McpTransportConfig,
    ) -> Result<()> {
        let mut client = McpClient::new(name);
        client.connect(&config).await?;

        // Bridge에 추가
        let bridge = self.mcp_bridge.read().await;
        bridge.add_server_with_config(client, config).await;

        // 도구 등록
        self.refresh_mcp_tools().await?;

        info!("Connected MCP server '{}' and registered tools", name);
        Ok(())
    }

    /// MCP 서버 연결 해제
    pub async fn disconnect_mcp_server(&self, name: &str) -> Result<()> {
        let bridge = self.mcp_bridge.read().await;
        bridge.remove_server(name).await;

        // 해당 서버의 도구들 제거
        let mut tools = self.tools.write().await;
        let prefix = format!("mcp_{}_", name);
        let to_remove: Vec<String> = tools
            .names()
            .iter()
            .filter(|n| n.starts_with(&prefix))
            .map(|n| n.to_string())
            .collect();

        for tool_name in to_remove {
            tools.remove(&tool_name);
        }

        info!("Disconnected MCP server '{}' and removed tools", name);
        Ok(())
    }

    /// MCP 도구 새로고침 (모든 연결된 서버의 도구를 ToolRegistry에 등록)
    ///
    /// 이 메서드는:
    /// 1. 기존 MCP 도구를 모두 제거
    /// 2. 연결된 모든 MCP 서버에서 도구를 가져옴
    /// 3. ToolRegistry에 새로 등록
    pub async fn refresh_mcp_tools(&self) -> Result<()> {
        let bridge = self.mcp_bridge.read().await;
        let mut tools = self.tools.write().await;

        // 기존 MCP 도구 제거
        tools.remove_all_mcp_tools();

        // 서버별로 도구 등록
        for server_name in bridge.list_servers().await {
            let server_tools = bridge.get_server_tools(&server_name).await;
            if !server_tools.is_empty() {
                tools.add_mcp_tools(&server_name, server_tools);
            }
        }

        let count = tools.mcp_tool_count();
        debug!("Refreshed {} MCP tools in registry", count);
        Ok(())
    }

    /// 특정 MCP 서버의 도구만 새로고침
    pub async fn refresh_mcp_server_tools(&self, server_name: &str) -> Result<()> {
        let bridge = self.mcp_bridge.read().await;
        let mut tools = self.tools.write().await;

        // 해당 서버의 기존 도구 제거
        tools.remove_mcp_tools(server_name);

        // 새 도구 등록
        let server_tools = bridge.get_server_tools(server_name).await;
        let tool_count = server_tools.len();
        if !server_tools.is_empty() {
            tools.add_mcp_tools(server_name, server_tools);
        }

        debug!(
            "Refreshed MCP tools for server '{}': {} tools",
            server_name,
            tool_count
        );
        Ok(())
    }

    /// MCP 도구 통계
    pub async fn mcp_tool_stats(&self) -> McpToolStats {
        let tools = self.tools.read().await;
        let bridge = self.mcp_bridge.read().await;

        McpToolStats {
            total_tools: tools.mcp_tool_count(),
            server_count: bridge.list_servers().await.len(),
            builtin_count: tools.builtin_tools().len(),
        }
    }

    /// MCP 서버 목록 조회
    pub async fn list_mcp_servers(&self) -> Vec<String> {
        let bridge = self.mcp_bridge.read().await;
        bridge.list_servers().await
    }

    /// MCP 서버 상태 조회
    pub async fn mcp_server_status(&self, name: &str) -> Option<crate::mcp::ServerStatus> {
        let bridge = self.mcp_bridge.read().await;
        bridge.server_status(name).await
    }

    /// 모든 MCP 서버 상태
    pub async fn all_mcp_status(&self) -> Vec<crate::mcp::ServerStatus> {
        let bridge = self.mcp_bridge.read().await;
        bridge.all_server_status().await
    }

    /// MCP 브릿지 헬스 체크
    pub async fn health_check_mcp(&self) {
        let bridge = self.mcp_bridge.read().await;
        bridge.health_check_all().await;
    }

    /// MCP 유휴 연결 정리
    pub async fn cleanup_mcp_idle(&self) {
        let bridge = self.mcp_bridge.read().await;
        bridge.cleanup_idle().await;
    }

    /// 모든 MCP 연결 종료
    pub async fn shutdown_mcp(&self) {
        let bridge = self.mcp_bridge.read().await;
        bridge.shutdown().await;
    }
}

impl Default for AgentContext {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Builder
// ============================================================================

/// AgentContext 빌더
pub struct AgentContextBuilder {
    config: AgentContextConfig,
    permissions: Option<Arc<PermissionService>>,
    additional_tools: Vec<Arc<dyn Tool>>,
    mcp_bridge: McpBridge,
}

impl AgentContextBuilder {
    pub fn new() -> Self {
        Self {
            config: AgentContextConfig::default(),
            permissions: None,
            additional_tools: Vec::new(),
            mcp_bridge: McpBridge::new(),
        }
    }

    /// 작업 디렉토리 설정
    pub fn working_directory(mut self, path: PathBuf) -> Self {
        self.config.working_directory = path;
        self
    }

    /// 세션 ID 설정
    pub fn session_id(mut self, id: impl Into<String>) -> Self {
        self.config.session_id = id.into();
        self
    }

    /// 최대 동시 태스크 설정
    pub fn max_concurrent_tasks(mut self, max: usize) -> Self {
        self.config.max_concurrent_tasks = max;
        self
    }

    /// 기본 타임아웃 설정
    pub fn default_timeout(mut self, timeout: Duration) -> Self {
        self.config.default_task_timeout = timeout;
        self
    }

    /// 권한 서비스 설정
    pub fn with_permission_service(mut self, service: Arc<PermissionService>) -> Self {
        self.permissions = Some(service);
        self
    }

    /// 권한 확인 비활성화
    pub fn disable_permission_check(mut self) -> Self {
        self.config.check_permissions = false;
        self
    }

    /// MCP 자동 연결 활성화
    pub fn enable_mcp(mut self) -> Self {
        self.config.auto_connect_mcp = true;
        self
    }

    /// LSP 활성화
    pub fn enable_lsp(mut self) -> Self {
        self.config.enable_lsp = true;
        self
    }

    /// 추가 도구 등록
    pub fn with_tool(mut self, tool: Arc<dyn Tool>) -> Self {
        self.additional_tools.push(tool);
        self
    }

    /// MCP 브릿지 설정
    pub fn with_mcp_bridge(mut self, bridge: McpBridge) -> Self {
        self.mcp_bridge = bridge;
        self
    }

    /// MCP 연결 TTL 설정
    pub fn with_mcp_ttl(mut self, ttl: Duration) -> Self {
        self.mcp_bridge = McpBridge::with_ttl(ttl);
        self
    }

    /// 빌드
    pub fn build(self) -> AgentContext {
        let mut registry = ToolRegistry::with_builtins();

        // 추가 도구 등록
        for tool in self.additional_tools {
            registry.register(tool);
        }

        AgentContext {
            config: self.config,
            tools: Arc::new(RwLock::new(registry)),
            permissions: self.permissions,
            mcp_bridge: Arc::new(RwLock::new(self.mcp_bridge)),
            stats: Arc::new(RwLock::new(ExecutionStats::default())),
        }
    }

    /// 비동기 빌드 (MCP 서버 연결 포함)
    pub async fn build_async(self, mcp_configs: Vec<(&str, McpTransportConfig)>) -> Result<AgentContext> {
        let ctx = self.build();

        // MCP 서버 연결
        for (name, config) in mcp_configs {
            if let Err(e) = ctx.connect_mcp_server(name, config).await {
                warn!("Failed to connect MCP server '{}': {}", name, e);
            }
        }

        Ok(ctx)
    }
}

impl Default for AgentContextBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = AgentContextConfig::default();
        assert_eq!(config.max_concurrent_tasks, 4);
        assert!(config.check_permissions);
        assert!(!config.auto_connect_mcp);
    }

    #[test]
    fn test_context_new() {
        let ctx = AgentContext::new();
        assert!(!ctx.session_id().is_empty());
    }

    #[tokio::test]
    async fn test_context_list_tools() {
        let ctx = AgentContext::new();
        let tools = ctx.list_tools().await;
        assert!(!tools.is_empty());

        // builtin 도구 확인
        let names: Vec<_> = tools.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"read"));
        assert!(names.contains(&"write"));
        assert!(names.contains(&"bash"));
    }

    #[tokio::test]
    async fn test_context_has_tool() {
        let ctx = AgentContext::new();
        assert!(ctx.has_tool("read").await);
        assert!(ctx.has_tool("write").await);
        assert!(!ctx.has_tool("nonexistent").await);
    }

    #[tokio::test]
    async fn test_context_execute_tool_not_found() {
        let ctx = AgentContext::new();
        let result = ctx.execute_tool("nonexistent", serde_json::json!({})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_context_stats() {
        let ctx = AgentContext::new();

        // 초기 통계
        let stats = ctx.stats().await;
        assert_eq!(stats.tool_executions, 0);

        // 도구 실행 (실패해도 카운트됨)
        let _ = ctx.execute_tool("glob", serde_json::json!({
            "pattern": "*.rs"
        })).await;

        let stats = ctx.stats().await;
        assert_eq!(stats.tool_executions, 1);
    }

    #[test]
    fn test_builder() {
        let ctx = AgentContext::builder()
            .session_id("test-session")
            .max_concurrent_tasks(8)
            .disable_permission_check()
            .build();

        assert_eq!(ctx.session_id(), "test-session");
        assert!(!ctx.config.check_permissions);
    }
}
