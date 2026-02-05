//! Agent context - shared state for agent execution
//!
//! Layer3의 AgentContext는 Layer2-core의 AgentContext를 위임하여
//! Tool/MCP 실행 기능을 사용하고, LLM Gateway 조율에 집중합니다.
//!
//! ## 아키텍처
//! ```text
//! Layer3-agent::AgentContext
//! ├── gateway: Arc<Gateway>           // LLM 프로바이더 (Layer2-provider)
//! ├── core_ctx: Arc<CoreAgentContext> // Tool/MCP 실행 위임 (Layer2-core)
//! ├── task_manager: Option<Arc<TaskManager>> // Task 시스템 (Layer2-task)
//! └── system_prompt: String           // 시스템 프롬프트
//! ```

use crate::parallel::{ExecutionStrategy, ToolClassifier};
use forge_core::AgentContext as CoreAgentContext;
use forge_core::ToolRegistry;
use forge_foundation::permission::PermissionService;
use forge_foundation::env_detect::Environment;
use forge_foundation::Result;
use forge_provider::Gateway;
use forge_task::{TaskManager, Task, ExecutionMode};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Context shared across agent execution
///
/// Layer3의 AgentContext는 세 가지 역할을 통합합니다:
/// 1. LLM Gateway 조율 (프로바이더 선택, 스트리밍 등)
/// 2. Tool/MCP 실행 위임 (Layer2-core::AgentContext 활용)
/// 3. Task 시스템 통합 (장시간 실행, PTY 명령어)
pub struct AgentContext {
    /// LLM provider gateway
    pub gateway: Arc<Gateway>,

    /// Core context for Tool/MCP execution (Layer2-core)
    core_ctx: Arc<CoreAgentContext>,

    /// Task manager for long-running/PTY commands (Layer2-task)
    task_manager: Option<Arc<TaskManager>>,

    /// Tool classifier for execution strategy
    tool_classifier: ToolClassifier,

    /// Working directory
    pub working_dir: PathBuf,

    /// System prompt template
    pub system_prompt: String,
}

impl AgentContext {
    /// Create a new agent context
    pub fn new(
        gateway: Arc<Gateway>,
        _tools: Arc<ToolRegistry>,
        permissions: Arc<PermissionService>,
        working_dir: PathBuf,
    ) -> Self {
        // Layer2-core의 AgentContext 생성
        let core_ctx = CoreAgentContext::builder()
            .working_directory(working_dir.clone())
            .with_permission_service(permissions)
            .build();

        Self {
            gateway,
            core_ctx: Arc::new(core_ctx),
            task_manager: None, // 기본값: TaskManager 없음
            tool_classifier: ToolClassifier::new(),
            working_dir,
            system_prompt: default_system_prompt(),
        }
    }

    /// Set task manager for Task/PTY execution
    pub fn with_task_manager(mut self, task_manager: Arc<TaskManager>) -> Self {
        self.task_manager = Some(task_manager);
        self
    }

    /// Create with builder pattern
    pub fn builder() -> AgentContextBuilder {
        AgentContextBuilder::new()
    }

    /// Set custom system prompt
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = prompt.into();
        self
    }

    // ========================================================================
    // Tool Execution (위임 to Layer2-core + Task 라우팅)
    // ========================================================================

    /// Execute a tool by name
    ///
    /// bash 도구의 경우 ExecutionStrategy에 따라 적절한 executor로 라우팅:
    /// - Direct: 직접 실행 (기존 방식)
    /// - Task: TaskManager로 백그라운드 실행
    /// - TaskPty: PTY executor로 대화형 실행
    /// - RequiresConfirmation: 확인 후 실행
    /// - Blocked: 차단
    pub async fn execute_tool(
        &self,
        name: &str,
        input: Value,
    ) -> Result<forge_core::ToolExecutionResult> {
        // bash 도구일 때 실행 전략 확인
        if name == "bash" {
            let strategy = self.tool_classifier.determine_strategy(name, &input);
            return self.execute_bash_with_strategy(input, strategy).await;
        }

        // 일반 도구는 기존 방식대로 실행
        self.core_ctx.execute_tool(name, input).await
    }

    /// Execute bash command with appropriate strategy
    async fn execute_bash_with_strategy(
        &self,
        input: Value,
        strategy: ExecutionStrategy,
    ) -> Result<forge_core::ToolExecutionResult> {
        let command = input
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        match strategy {
            ExecutionStrategy::Direct => {
                // 기존 방식: core_ctx로 직접 실행
                debug!("Bash direct execution: {}", command);
                self.core_ctx.execute_tool("bash", input).await
            }

            ExecutionStrategy::Task => {
                // 장시간 실행 명령어: TaskManager로 백그라운드 실행
                info!("Bash via Task system: {}", command);
                self.execute_bash_via_task(command, false).await
            }

            ExecutionStrategy::TaskPty => {
                // 대화형 명령어: PTY executor로 실행
                info!("Bash via PTY: {}", command);
                self.execute_bash_via_task(command, true).await
            }

            ExecutionStrategy::RequiresConfirmation => {
                // 위험한 명령어: 확인 필요 메시지 반환
                // (실제 확인은 hook 시스템에서 처리)
                warn!("Bash requires confirmation: {}", command);
                // 일단 직접 실행 시도 (권한 시스템이 차단할 수 있음)
                self.core_ctx.execute_tool("bash", input).await
            }

            ExecutionStrategy::Blocked => {
                // 금지된 명령어: 차단
                warn!("Bash command blocked: {}", command);
                Ok(forge_core::ToolExecutionResult {
                    tool_name: "bash".to_string(),
                    success: false,
                    output: String::new(),
                    error: Some(format!(
                        "Command blocked for safety: '{}'. This command is not allowed.",
                        command
                    )),
                    duration_ms: 0,
                    permission_required: false,
                    permission_granted: false,
                })
            }
        }
    }

    /// Execute bash command via Task system
    async fn execute_bash_via_task(
        &self,
        command: &str,
        use_pty: bool,
    ) -> Result<forge_core::ToolExecutionResult> {
        let Some(task_manager) = &self.task_manager else {
            // TaskManager가 없으면 직접 실행으로 폴백
            warn!("TaskManager not available, falling back to direct execution");
            return self.core_ctx.execute_tool("bash", serde_json::json!({
                "command": command
            })).await;
        };

        // Task 생성
        let mode = if use_pty {
            ExecutionMode::Pty
        } else {
            ExecutionMode::Local
        };

        let task = Task::new(
            "agent",
            "bash",
            command,
            serde_json::json!({"command": command}),
        ).with_execution_mode(mode);

        let start = std::time::Instant::now();

        // Task 제출
        let task_id = task_manager.submit(task).await;
        info!("Task submitted: {}", task_id);

        // Task 실행 시작
        task_manager.execute_task(task_id).await;

        // Task 완료 대기
        if let Some(result) = task_manager.wait(task_id).await {
            let duration_ms = start.elapsed().as_millis() as u64;
            let success = result.exit_code.map_or(false, |c| c == 0);
            let error_msg = if success {
                None
            } else {
                Some(format!("Exit code: {:?}", result.exit_code))
            };
            Ok(forge_core::ToolExecutionResult {
                tool_name: "bash".to_string(),
                success,
                output: result.output,
                error: error_msg,
                duration_ms,
                permission_required: false,
                permission_granted: false,
            })
        } else {
            Ok(forge_core::ToolExecutionResult {
                tool_name: "bash".to_string(),
                success: false,
                output: String::new(),
                error: Some("Task did not complete".to_string()),
                duration_ms: start.elapsed().as_millis() as u64,
                permission_required: false,
                permission_granted: false,
            })
        }
    }

    /// Execute multiple tools in parallel
    pub async fn execute_tools_parallel(
        &self,
        calls: Vec<(&str, Value)>,
    ) -> Vec<Result<forge_core::ToolExecutionResult>> {
        self.core_ctx.execute_tools_parallel(calls).await
    }

    /// Get tool definitions for LLM
    pub async fn tool_definitions(&self) -> Vec<forge_provider::ToolDef> {
        // core_ctx에서 스키마를 가져와 변환
        self.core_ctx
            .get_tool_schemas()
            .await
            .into_iter()
            .map(|schema| {
                let name = schema["name"].as_str().unwrap_or("").to_string();
                let description = schema["description"].as_str().unwrap_or("").to_string();

                forge_provider::ToolDef {
                    name,
                    description,
                    parameters: forge_provider::tool_def::ToolParameters {
                        schema_type: schema["parameters"]["type"]
                            .as_str()
                            .unwrap_or("object")
                            .to_string(),
                        properties: schema["parameters"]["properties"].clone(),
                        required: schema["parameters"]["required"]
                            .as_array()
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| v.as_str())
                                    .map(String::from)
                                    .collect()
                            })
                            .unwrap_or_default(),
                    },
                }
            })
            .collect()
    }

    /// Create tool context for execution (legacy compatibility)
    pub fn tool_context(&self, session_id: &str) -> forge_core::RuntimeContext {
        forge_core::RuntimeContext::new(
            session_id,
            self.working_dir.clone(),
            self.core_ctx_permissions(),
        )
    }

    /// Get permissions from core context
    fn core_ctx_permissions(&self) -> Arc<PermissionService> {
        // core_ctx에서 권한 서비스를 가져오거나 기본값 생성
        Arc::new(PermissionService::new())
    }

    /// Check if a tool exists
    pub async fn has_tool(&self, name: &str) -> bool {
        self.core_ctx.has_tool(name).await
    }

    /// List available tools
    pub async fn list_tools(&self) -> Vec<(String, String)> {
        self.core_ctx.list_tools().await
    }

    // ========================================================================
    // MCP Integration (위임 to Layer2-core)
    // ========================================================================

    /// Connect to an MCP server
    pub async fn connect_mcp_server(
        &self,
        name: &str,
        config: forge_core::McpTransportConfig,
    ) -> Result<()> {
        self.core_ctx.connect_mcp_server(name, config).await
    }

    /// Disconnect from an MCP server
    pub async fn disconnect_mcp_server(&self, name: &str) -> Result<()> {
        self.core_ctx.disconnect_mcp_server(name).await
    }

    /// List connected MCP servers
    pub async fn list_mcp_servers(&self) -> Vec<String> {
        self.core_ctx.list_mcp_servers().await
    }

    /// Refresh MCP tools
    pub async fn refresh_mcp_tools(&self) -> Result<()> {
        self.core_ctx.refresh_mcp_tools().await
    }

    // ========================================================================
    // Statistics (위임 to Layer2-core)
    // ========================================================================

    /// Get execution statistics
    pub async fn stats(&self) -> forge_core::ExecutionStats {
        self.core_ctx.stats().await
    }

    /// Reset statistics
    pub async fn reset_stats(&self) {
        self.core_ctx.reset_stats().await
    }

    // ========================================================================
    // Runtime LLM Management
    // ========================================================================

    /// Switch to a different provider at runtime
    pub async fn switch_provider(&self, name: &str) -> Result<()> {
        self.gateway.set_default(name).await
    }

    /// Get current provider name
    pub async fn current_provider(&self) -> String {
        self.gateway.default_provider_name().await
    }

    /// Get current model info
    pub async fn current_model(&self) -> Option<String> {
        if let Ok(provider) = self.gateway.default_provider().await {
            Some(provider.model().id.clone())
        } else {
            None
        }
    }

    /// List available providers
    pub fn list_providers(&self) -> Vec<&str> {
        self.gateway.list_providers()
    }

    /// List available models for current provider
    pub async fn list_models(&self) -> Vec<String> {
        if let Ok(provider) = self.gateway.default_provider().await {
            provider
                .list_models()
                .iter()
                .map(|m| m.id.clone())
                .collect()
        } else {
            vec![]
        }
    }

    /// Get provider info (name, model, available status)
    pub fn provider_info(&self) -> Vec<ProviderInfo> {
        self.gateway
            .list_provider_info()
            .into_iter()
            .map(|(name, meta)| ProviderInfo {
                name: name.to_string(),
                display_name: meta.display_name.clone(),
                default_model: meta.default_model.clone(),
                available: self.gateway.is_provider_available(name),
            })
            .collect()
    }

    // ========================================================================
    // Context Access
    // ========================================================================

    /// Get the core context (Layer2-core)
    pub fn core_context(&self) -> &Arc<CoreAgentContext> {
        &self.core_ctx
    }

    /// Get session ID from core context
    pub fn session_id(&self) -> &str {
        self.core_ctx.session_id()
    }
}

// ============================================================================
// Builder
// ============================================================================

/// Builder for AgentContext
pub struct AgentContextBuilder {
    gateway: Option<Arc<Gateway>>,
    core_ctx: Option<Arc<CoreAgentContext>>,
    task_manager: Option<Arc<TaskManager>>,
    working_dir: PathBuf,
    system_prompt: Option<String>,
    permissions: Option<Arc<PermissionService>>,
}

impl AgentContextBuilder {
    pub fn new() -> Self {
        Self {
            gateway: None,
            core_ctx: None,
            task_manager: None,
            working_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            system_prompt: None,
            permissions: None,
        }
    }

    /// Set the LLM gateway
    pub fn gateway(mut self, gateway: Arc<Gateway>) -> Self {
        self.gateway = Some(gateway);
        self
    }

    /// Set the core context directly
    pub fn core_context(mut self, ctx: Arc<CoreAgentContext>) -> Self {
        self.core_ctx = Some(ctx);
        self
    }

    /// Set working directory
    pub fn working_directory(mut self, path: PathBuf) -> Self {
        self.working_dir = path;
        self
    }

    /// Set system prompt
    pub fn system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Set permission service
    pub fn permissions(mut self, permissions: Arc<PermissionService>) -> Self {
        self.permissions = Some(permissions);
        self
    }

    /// Set task manager for Task/PTY execution
    pub fn task_manager(mut self, task_manager: Arc<TaskManager>) -> Self {
        self.task_manager = Some(task_manager);
        self
    }

    /// Build the AgentContext
    pub fn build(self) -> Result<AgentContext> {
        let gateway = self.gateway.ok_or_else(|| {
            forge_foundation::Error::Config("Gateway is required".to_string())
        })?;

        // core_ctx가 없으면 새로 생성
        let core_ctx = match self.core_ctx {
            Some(ctx) => ctx,
            None => {
                let mut builder = CoreAgentContext::builder()
                    .working_directory(self.working_dir.clone());

                if let Some(perms) = self.permissions {
                    builder = builder.with_permission_service(perms);
                }

                Arc::new(builder.build())
            }
        };

        Ok(AgentContext {
            gateway,
            core_ctx,
            task_manager: self.task_manager,
            tool_classifier: ToolClassifier::new(),
            working_dir: self.working_dir,
            system_prompt: self.system_prompt.unwrap_or_else(default_system_prompt),
        })
    }
}

impl Default for AgentContextBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Supporting Types
// ============================================================================

/// Information about a provider
#[derive(Debug, Clone)]
pub struct ProviderInfo {
    pub name: String,
    pub display_name: String,
    pub default_model: String,
    pub available: bool,
}

/// Default system prompt for the agent
fn default_system_prompt() -> String {
    // 환경 정보 감지
    let env = Environment::detect();
    let env_info = env.to_system_info();
    
    format!(r#"You are ForgeCode, an AI coding assistant running in the terminal.

{}

You help users with software engineering tasks including:
- Writing and editing code
- Debugging and fixing bugs
- Explaining code
- Running commands and tests
- Searching and navigating codebases

## Guidelines

- Be concise and direct in your responses
- Use tools to gather information before making changes
- Always read files before editing them
- Make minimal, focused changes
- Explain what you're doing and why
- Ask for clarification if requirements are unclear

## Task Management (IMPORTANT)

You have access to Task tools for managing long-running processes and parallel execution.
**Use these tools proactively without waiting for explicit user commands.**

### When to use Task tools automatically:

1. **Running servers**: When you need to start a backend or frontend server for testing, use `task_spawn` with mode="pty"
2. **Parallel processes**: When testing requires multiple concurrent processes (e.g., backend + frontend)
3. **Waiting for readiness**: After spawning a server, use `task_wait` with condition="output_contains" to wait for it to be ready
4. **Checking results**: Use `task_logs` to check output and errors after running tests or commands
5. **Cleanup**: Use `task_stop` to stop servers after testing is complete

### Typical workflow example:

```
1. task_spawn: Start backend server (mode="pty", name="backend")
2. task_wait: Wait for "Listening on" in backend output
3. task_spawn: Start frontend or run tests
4. task_wait: Wait for test completion
5. task_logs: Check results and errors
6. task_stop: Stop all servers
```

### Available Task tools:

- `task_spawn` - Start a new process (local, pty for servers, container for isolation)
- `task_wait` - Wait for a condition (output_contains, complete, regex match)
- `task_logs` - Get logs from a task (filter by tail, errors_only, search)
- `task_status` - Check task state (running, completed, failed)
- `task_list` - List all active tasks
- `task_stop` - Stop a running task
- `task_send` - Send input to an interactive task (PTY stdin)

**Key principle**: When implementing and testing features that require running processes,
always use Task tools automatically. Don't ask the user for permission - just execute
the necessary workflow: spawn → wait → verify → fix → repeat until tests pass.

## Tool Selection (CRITICAL)

### `bash` vs `task_spawn` Decision:

| Use `bash` for: | Use `task_spawn` for: |
|-----------------|----------------------|
| ls, cat, grep, find | cargo run (servers) |
| git status, git diff | npm start, npm run dev |
| cargo build, cargo test | python -m http.server |
| npm install, pip install | docker run |
| --version checks | watch modes |
| One-shot commands (< 30s) | Processes needing monitoring |

### Quick Decision:
1. **Completes in < 30 seconds?** → `bash`
2. **Server or daemon?** → `task_spawn` (mode: pty)
3. **Needs background execution?** → `task_spawn`
4. **Need to send input later?** → `task_spawn` (mode: pty)
5. **Otherwise** → `bash`

### After task_spawn:
- `task_wait` - Wait for "Listening on", "ready", etc.
- `task_logs` - Check output
- `task_send` - Send input (PTY)
- `task_stop` - Terminate

## Planning Mode (Complex Tasks)

For complex tasks involving multiple files or steps:

1. **Analyze first**: Read relevant files before making changes
2. **Create a plan**: List the steps you'll take
3. **Execute sequentially**: Complete each step before moving to the next
4. **Verify after changes**: Run appropriate checks

### Plan Format:
```
PLAN: [Title]
1. [READ] Analyze current structure
2. [READ] Check existing patterns
3. [WRITE/EDIT] Make changes
4. [BASH] Verify (build/test)
```

## Verification (After Changes)

Choose verification level based on impact:

| Change Type | Verification |
|-------------|--------------|
| Documentation only | None |
| Single file, minor | Quick (cargo check / tsc --noEmit) |
| Multiple files | Standard (build + related tests) |
| Architecture changes | Thorough (full test suite + lint) |

### Rust Project:
- Quick: `cargo check`
- Standard: `cargo check && cargo test`
- Thorough: `cargo check && cargo build && cargo test && cargo clippy`

### Node Project:
- Quick: `npx tsc --noEmit`
- Standard: `npm test`
- Thorough: `npm run lint && npm test && npm run build`

**Always verify after making changes. If verification fails, fix the issue before proceeding.**

You have access to various tools to help accomplish tasks. Use them effectively."#, env_info)
}
