//! Task Tools - LLM이 여러 Task/PTY를 관리하고 상호작용하는 도구들
//!
//! ## 제공 도구
//!
//! - `task_spawn` - 새 Task 시작 (Local, PTY, Container)
//! - `task_wait` - Task 조건 대기 (출력 패턴, 완료 등)
//! - `task_logs` - Task 로그 조회/분석
//! - `task_stop` - Task 정지
//! - `task_send` - Task에 입력 전송 (PTY stdin)
//! - `task_list` - 실행 중인 Task 목록
//! - `task_status` - Task 상태 조회
//!
//! ## 사용 예시
//!
//! ```ignore
//! // 백엔드 서버 시작
//! let result = task_spawn.execute(json!({
//!     "command": "cargo run --bin server",
//!     "mode": "pty",
//!     "name": "backend"
//! })).await;
//!
//! // 서버 준비 대기
//! task_wait.execute(json!({
//!     "task_id": "abc123",
//!     "condition": "output_contains",
//!     "pattern": "Listening on"
//! })).await;
//!
//! // 로그 확인
//! task_logs.execute(json!({
//!     "task_id": "abc123",
//!     "tail": 50
//! })).await;
//! ```

use async_trait::async_trait;
use forge_foundation::{
    PermissionAction, Result, Tool, ToolContext, ToolMeta, ToolResult,
};
use forge_task::{
    ExecutionMode, OrchestratorConfig, Task, TaskId, TaskOrchestrator, WaitCondition, WaitResult,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::info;

// ============================================================================
// Global Orchestrator (싱글톤)
// ============================================================================

lazy_static::lazy_static! {
    static ref GLOBAL_ORCHESTRATOR: Arc<RwLock<Option<TaskOrchestrator>>> = Arc::new(RwLock::new(None));
    static ref TASK_NAME_MAP: Arc<RwLock<HashMap<String, TaskId>>> = Arc::new(RwLock::new(HashMap::new()));
}

/// Orchestrator 초기화 (처음 사용 시 자동 호출)
async fn get_orchestrator() -> Arc<TaskOrchestrator> {
    let mut guard = GLOBAL_ORCHESTRATOR.write().await;
    if guard.is_none() {
        let orchestrator = TaskOrchestrator::new(OrchestratorConfig::default()).await;
        *guard = Some(orchestrator);
    }
    // TaskOrchestrator를 Arc로 감싸서 반환 (실제로는 내부 참조)
    // 여기서는 임시로 새로 생성 - 실제 구현에서는 싱글톤 패턴 개선 필요
    Arc::new(TaskOrchestrator::new(OrchestratorConfig::default()).await)
}

/// Task ID 조회 (이름 또는 ID)
async fn resolve_task_id(id_or_name: &str) -> Option<TaskId> {
    // UUID 형식이면 직접 파싱
    if let Ok(uuid) = uuid::Uuid::parse_str(id_or_name) {
        return Some(TaskId(uuid));
    }

    // 짧은 ID 형식 (8자)이면 이름 맵에서 검색
    let map = TASK_NAME_MAP.read().await;
    map.get(id_or_name).copied()
}

/// Task 이름 등록
async fn register_task_name(name: &str, task_id: TaskId) {
    let mut map = TASK_NAME_MAP.write().await;
    map.insert(name.to_string(), task_id);
    // 짧은 ID도 등록
    map.insert(task_id.to_string(), task_id);
}

// ============================================================================
// TaskSpawnTool - 새 Task 시작
// ============================================================================

/// Task 시작 도구
pub struct TaskSpawnTool;

impl TaskSpawnTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TaskSpawnTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for TaskSpawnTool {
    fn name(&self) -> &str {
        "task_spawn"
    }

    fn meta(&self) -> ToolMeta {
        ToolMeta::new("task_spawn")
            .display_name("Task Spawn")
            .description("Start a new task (command execution). Supports local, PTY (interactive), and container modes. Use PTY mode for long-running servers or interactive commands.")
            .category("task")
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Command to execute"
                },
                "mode": {
                    "type": "string",
                    "enum": ["local", "pty", "container"],
                    "description": "Execution mode: local (simple), pty (interactive/servers), container (isolated)",
                    "default": "local"
                },
                "name": {
                    "type": "string",
                    "description": "Optional name for the task (for easier reference)"
                },
                "working_dir": {
                    "type": "string",
                    "description": "Working directory for the command"
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Timeout in seconds (default: 300)",
                    "default": 300
                },
                "env": {
                    "type": "object",
                    "description": "Additional environment variables"
                }
            },
            "required": ["command"]
        })
    }

    fn required_permission(&self, input: &Value) -> Option<PermissionAction> {
        let command = input["command"].as_str().unwrap_or("");
        Some(PermissionAction::Execute { command: command.to_string() })
    }

    async fn execute(
        &self,
        input: Value,
        ctx: &dyn ToolContext,
    ) -> Result<ToolResult> {
        let command = input["command"]
            .as_str()
            .ok_or_else(|| forge_foundation::Error::InvalidInput("command is required".to_string()))?;

        let mode = input["mode"].as_str().unwrap_or("local");
        let name = input["name"].as_str();
        let timeout_secs = input["timeout_secs"].as_u64().unwrap_or(300);

        // Execution mode 결정
        let execution_mode = match mode {
            "pty" => ExecutionMode::Pty,
            "container" => {
                let image = input["image"].as_str().unwrap_or("ubuntu:latest");
                ExecutionMode::Container {
                    image: image.to_string(),
                    workdir: input["container_workdir"].as_str().map(String::from),
                    env: vec![],
                    volumes: vec![],
                }
            }
            _ => ExecutionMode::Local,
        };

        // Task 생성
        let task = Task::new(
            ctx.session_id(),
            "task_spawn",
            command,
            input.clone(),
        )
        .with_execution_mode(execution_mode)
        .with_timeout(Duration::from_secs(timeout_secs));

        // Orchestrator에서 실행
        let orchestrator = get_orchestrator().await;
        let task_id = orchestrator.spawn(task).await.map_err(|e| {
            forge_foundation::Error::Task(format!("Failed to spawn task: {}", e))
        })?;

        // 이름 등록
        if let Some(task_name) = name {
            register_task_name(task_name, task_id).await;
        }
        register_task_name(&task_id.to_string(), task_id).await;

        info!("Spawned task {} with command: {}", task_id, command);

        Ok(ToolResult::success(json!({
            "task_id": task_id.to_string(),
            "name": name,
            "command": command,
            "mode": mode,
            "status": "running"
        }).to_string()))
    }
}

// ============================================================================
// TaskWaitTool - Task 조건 대기
// ============================================================================

/// Task 대기 도구
pub struct TaskWaitTool;

impl TaskWaitTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TaskWaitTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for TaskWaitTool {
    fn name(&self) -> &str {
        "task_wait"
    }

    fn meta(&self) -> ToolMeta {
        ToolMeta::new("task_wait")
            .display_name("Task Wait")
            .description("Wait for a task to meet a condition. Use 'output_contains' to wait for specific output (e.g., 'Listening on'), 'complete' to wait for task finish, or 'regex' for pattern matching.")
            .category("task")
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "Task ID or name to wait for"
                },
                "condition": {
                    "type": "string",
                    "enum": ["complete", "output_contains", "output_matches", "ready"],
                    "description": "Wait condition type"
                },
                "pattern": {
                    "type": "string",
                    "description": "Pattern to match (for output_contains/output_matches)"
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Timeout in seconds (default: 60)",
                    "default": 60
                }
            },
            "required": ["task_id", "condition"]
        })
    }

    fn required_permission(&self, _input: &Value) -> Option<PermissionAction> {
        None // Read-only operation
    }

    async fn execute(
        &self,
        input: Value,
        _ctx: &dyn ToolContext,
    ) -> Result<ToolResult> {
        let task_id_str = input["task_id"]
            .as_str()
            .ok_or_else(|| forge_foundation::Error::InvalidInput("task_id is required".to_string()))?;

        let condition_type = input["condition"]
            .as_str()
            .ok_or_else(|| forge_foundation::Error::InvalidInput("condition is required".to_string()))?;

        let timeout_secs = input["timeout_secs"].as_u64().unwrap_or(60);
        let pattern = input["pattern"].as_str();

        // Task ID 해석
        let task_id = resolve_task_id(task_id_str).await.ok_or_else(|| {
            forge_foundation::Error::NotFound(format!("Task not found: {}", task_id_str))
        })?;

        // 조건 생성
        let condition = match condition_type {
            "complete" => WaitCondition::Complete,
            "output_contains" => {
                let pat = pattern.ok_or_else(|| {
                    forge_foundation::Error::InvalidInput("pattern is required for output_contains".to_string())
                })?;
                WaitCondition::OutputContains(pat.to_string())
            }
            "output_matches" => {
                let pat = pattern.ok_or_else(|| {
                    forge_foundation::Error::InvalidInput("pattern is required for output_matches".to_string())
                })?;
                WaitCondition::OutputMatches(pat.to_string())
            }
            "ready" => WaitCondition::Signal(forge_task::TaskSignal::Ready),
            _ => {
                return Err(forge_foundation::Error::InvalidInput(format!(
                    "Unknown condition: {}",
                    condition_type
                )))
            }
        };

        // 대기 실행
        let orchestrator = get_orchestrator().await;
        let result = orchestrator
            .wait_for(task_id, condition, Some(Duration::from_secs(timeout_secs)))
            .await
            .map_err(|e| forge_foundation::Error::Task(format!("Wait failed: {}", e)))?;

        match result {
            WaitResult::Satisfied { condition, data } => {
                Ok(ToolResult::success(json!({
                    "success": true,
                    "condition": condition,
                    "matched_data": data
                }).to_string()))
            }
            WaitResult::Timeout => {
                Ok(ToolResult::error("Timeout waiting for condition"))
            }
            WaitResult::Error(msg) => {
                Ok(ToolResult::error(msg))
            }
            WaitResult::Cancelled => {
                Ok(ToolResult::error("Wait was cancelled"))
            }
        }
    }
}

// ============================================================================
// TaskLogsTool - Task 로그 조회
// ============================================================================

/// Task 로그 조회 도구
pub struct TaskLogsTool;

impl TaskLogsTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TaskLogsTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for TaskLogsTool {
    fn name(&self) -> &str {
        "task_logs"
    }

    fn meta(&self) -> ToolMeta {
        ToolMeta::new("task_logs")
            .display_name("Task Logs")
            .description("Get logs from a running or completed task. Use 'tail' to get last N lines, 'errors_only' to filter errors, or 'analyze' for LLM-friendly analysis.")
            .category("task")
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "Task ID or name"
                },
                "tail": {
                    "type": "integer",
                    "description": "Get last N lines (default: all)"
                },
                "errors_only": {
                    "type": "boolean",
                    "description": "Show only error lines",
                    "default": false
                },
                "search": {
                    "type": "string",
                    "description": "Search for specific text in logs"
                },
                "analyze": {
                    "type": "boolean",
                    "description": "Return analysis report instead of raw logs",
                    "default": false
                }
            },
            "required": ["task_id"]
        })
    }

    fn required_permission(&self, _input: &Value) -> Option<PermissionAction> {
        None // Read-only
    }

    async fn execute(
        &self,
        input: Value,
        _ctx: &dyn ToolContext,
    ) -> Result<ToolResult> {
        let task_id_str = input["task_id"]
            .as_str()
            .ok_or_else(|| forge_foundation::Error::InvalidInput("task_id is required".to_string()))?;

        let tail = input["tail"].as_u64();
        let errors_only = input["errors_only"].as_bool().unwrap_or(false);
        let search = input["search"].as_str();
        let analyze = input["analyze"].as_bool().unwrap_or(false);

        // Task ID 해석
        let task_id = resolve_task_id(task_id_str).await.ok_or_else(|| {
            forge_foundation::Error::NotFound(format!("Task not found: {}", task_id_str))
        })?;

        let orchestrator = get_orchestrator().await;

        if analyze {
            // 분석 리포트 반환
            let log_manager = orchestrator.log_manager();
            if let Some(report) = log_manager.get_analysis(&task_id.to_string()).await {
                return Ok(ToolResult::success(report.format_for_llm()));
            } else {
                return Ok(ToolResult::error("No logs found for task"));
            }
        }

        // 로그 조회
        let logs = orchestrator.get_task_logs(task_id).await;

        if let Some(entries) = logs {
            let mut filtered: Vec<_> = entries.into_iter().collect();

            // 에러만 필터링
            if errors_only {
                filtered.retain(|e| e.level.is_error());
            }

            // 검색 필터링
            if let Some(search_term) = search {
                filtered.retain(|e| e.content.contains(search_term));
            }

            // tail 적용
            if let Some(n) = tail {
                let n = n as usize;
                if filtered.len() > n {
                    filtered = filtered.into_iter().rev().take(n).rev().collect();
                }
            }

            // 포맷팅
            let output: Vec<String> = filtered
                .iter()
                .map(|e| e.format_for_analysis())
                .collect();

            Ok(ToolResult::success(json!({
                "task_id": task_id_str,
                "line_count": output.len(),
                "logs": output.join("\n")
            }).to_string()))
        } else {
            Ok(ToolResult::error("No logs found for task"))
        }
    }
}

// ============================================================================
// TaskStopTool - Task 정지
// ============================================================================

/// Task 정지 도구
pub struct TaskStopTool;

impl TaskStopTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TaskStopTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for TaskStopTool {
    fn name(&self) -> &str {
        "task_stop"
    }

    fn meta(&self) -> ToolMeta {
        ToolMeta::new("task_stop")
            .display_name("Task Stop")
            .description("Stop a running task. Use this to terminate servers, long-running processes, or cancel pending tasks.")
            .category("task")
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "Task ID or name to stop"
                },
                "force": {
                    "type": "boolean",
                    "description": "Force kill (SIGKILL instead of SIGTERM)",
                    "default": false
                }
            },
            "required": ["task_id"]
        })
    }

    fn required_permission(&self, input: &Value) -> Option<PermissionAction> {
        let task_id = input["task_id"].as_str().unwrap_or("unknown");
        Some(PermissionAction::Execute { command: format!("stop task {}", task_id) })
    }

    async fn execute(
        &self,
        input: Value,
        _ctx: &dyn ToolContext,
    ) -> Result<ToolResult> {
        let task_id_str = input["task_id"]
            .as_str()
            .ok_or_else(|| forge_foundation::Error::InvalidInput("task_id is required".to_string()))?;

        // Task ID 해석
        let task_id = resolve_task_id(task_id_str).await.ok_or_else(|| {
            forge_foundation::Error::NotFound(format!("Task not found: {}", task_id_str))
        })?;

        let orchestrator = get_orchestrator().await;
        orchestrator.stop(task_id).await.map_err(|e| {
            forge_foundation::Error::Task(format!("Failed to stop task: {}", e))
        })?;

        info!("Stopped task {}", task_id);

        Ok(ToolResult::success(json!({
            "task_id": task_id_str,
            "status": "stopped"
        }).to_string()))
    }
}

// ============================================================================
// TaskSendTool - Task에 입력 전송
// ============================================================================

/// Task 입력 전송 도구
pub struct TaskSendTool;

impl TaskSendTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TaskSendTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for TaskSendTool {
    fn name(&self) -> &str {
        "task_send"
    }

    fn meta(&self) -> ToolMeta {
        ToolMeta::new("task_send")
            .display_name("Task Send")
            .description("Send input to a running PTY task. Use this to interact with interactive commands, answer prompts, or send keyboard input.")
            .category("task")
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "Task ID or name"
                },
                "input": {
                    "type": "string",
                    "description": "Input to send to the task"
                },
                "newline": {
                    "type": "boolean",
                    "description": "Append newline to input (default: true)",
                    "default": true
                }
            },
            "required": ["task_id", "input"]
        })
    }

    fn required_permission(&self, input: &Value) -> Option<PermissionAction> {
        let task_id = input["task_id"].as_str().unwrap_or("unknown");
        Some(PermissionAction::Execute { command: format!("send input to task {}", task_id) })
    }

    async fn execute(
        &self,
        input: Value,
        _ctx: &dyn ToolContext,
    ) -> Result<ToolResult> {
        let task_id_str = input["task_id"]
            .as_str()
            .ok_or_else(|| forge_foundation::Error::InvalidInput("task_id is required".to_string()))?;

        let text = input["input"]
            .as_str()
            .ok_or_else(|| forge_foundation::Error::InvalidInput("input is required".to_string()))?;

        let newline = input["newline"].as_bool().unwrap_or(true);

        // Task ID 해석
        let task_id = resolve_task_id(task_id_str).await.ok_or_else(|| {
            forge_foundation::Error::NotFound(format!("Task not found: {}", task_id_str))
        })?;

        // TODO: PTY stdin 전송 구현 필요
        // 현재는 TaskOrchestrator를 통한 메시지 전송만 지원
        let orchestrator = get_orchestrator().await;

        let message = if newline {
            forge_task::TaskMessage::Text(format!("{}\n", text))
        } else {
            forge_task::TaskMessage::Text(text.to_string())
        };

        // 자기 자신에게 브로드캐스트 (실제로는 PTY stdin으로 보내야 함)
        orchestrator.broadcast(task_id, message).await.map_err(|e| {
            forge_foundation::Error::Task(format!("Failed to send input: {}", e))
        })?;

        Ok(ToolResult::success(json!({
            "task_id": task_id_str,
            "sent": text,
            "newline": newline
        }).to_string()))
    }
}

// ============================================================================
// TaskListTool - Task 목록 조회
// ============================================================================

/// Task 목록 조회 도구
pub struct TaskListTool;

impl TaskListTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TaskListTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for TaskListTool {
    fn name(&self) -> &str {
        "task_list"
    }

    fn meta(&self) -> ToolMeta {
        ToolMeta::new("task_list")
            .display_name("Task List")
            .description("List all tasks (running, pending, completed). Shows task IDs, names, status, and commands.")
            .category("task")
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "status": {
                    "type": "string",
                    "enum": ["all", "running", "completed", "failed"],
                    "description": "Filter by status",
                    "default": "all"
                }
            }
        })
    }

    fn required_permission(&self, _input: &Value) -> Option<PermissionAction> {
        None // Read-only
    }

    async fn execute(
        &self,
        input: Value,
        _ctx: &dyn ToolContext,
    ) -> Result<ToolResult> {
        let _status_filter = input["status"].as_str().unwrap_or("all");

        // 이름 맵에서 Task 목록 가져오기
        let map = TASK_NAME_MAP.read().await;

        let mut tasks: Vec<Value> = vec![];
        let orchestrator = get_orchestrator().await;

        for (name, task_id) in map.iter() {
            // 짧은 ID와 이름 구분
            if name.len() == 8 {
                continue; // 짧은 ID는 스킵 (중복 방지)
            }

            if let Some(status) = orchestrator.status(*task_id).await {
                tasks.push(json!({
                    "task_id": task_id.to_string(),
                    "name": name,
                    "command": status.command,
                    "is_running": status.is_running,
                    "has_errors": status.has_errors,
                    "log_lines": status.log_line_count
                }));
            }
        }

        Ok(ToolResult::success(json!({
            "count": tasks.len(),
            "tasks": tasks
        }).to_string()))
    }
}

// ============================================================================
// TaskStatusTool - Task 상태 조회
// ============================================================================

/// Task 상태 조회 도구
pub struct TaskStatusTool;

impl TaskStatusTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TaskStatusTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for TaskStatusTool {
    fn name(&self) -> &str {
        "task_status"
    }

    fn meta(&self) -> ToolMeta {
        ToolMeta::new("task_status")
            .display_name("Task Status")
            .description("Get detailed status of a specific task including state, logs summary, and errors.")
            .category("task")
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "Task ID or name"
                }
            },
            "required": ["task_id"]
        })
    }

    fn required_permission(&self, _input: &Value) -> Option<PermissionAction> {
        None // Read-only
    }

    async fn execute(
        &self,
        input: Value,
        _ctx: &dyn ToolContext,
    ) -> Result<ToolResult> {
        let task_id_str = input["task_id"]
            .as_str()
            .ok_or_else(|| forge_foundation::Error::InvalidInput("task_id is required".to_string()))?;

        // Task ID 해석
        let task_id = resolve_task_id(task_id_str).await.ok_or_else(|| {
            forge_foundation::Error::NotFound(format!("Task not found: {}", task_id_str))
        })?;

        let orchestrator = get_orchestrator().await;

        if let Some(status) = orchestrator.status(task_id).await {
            Ok(ToolResult::success(json!({
                "task_id": task_id.to_string(),
                "command": status.command,
                "state": format!("{:?}", status.state),
                "is_running": status.is_running,
                "has_errors": status.has_errors,
                "log_line_count": status.log_line_count
            }).to_string()))
        } else {
            Ok(ToolResult::error(
                format!("Task not found: {}", task_id_str),
            ))
        }
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// 모든 Task 도구 반환
pub fn task_tools() -> Vec<Arc<dyn Tool>> {
    vec![
        Arc::new(TaskSpawnTool::new()) as Arc<dyn Tool>,
        Arc::new(TaskWaitTool::new()),
        Arc::new(TaskLogsTool::new()),
        Arc::new(TaskStopTool::new()),
        Arc::new(TaskSendTool::new()),
        Arc::new(TaskListTool::new()),
        Arc::new(TaskStatusTool::new()),
    ]
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_spawn_schema() {
        let tool = TaskSpawnTool::new();
        let schema = tool.schema();
        assert!(schema["properties"]["command"].is_object());
        assert!(schema["properties"]["mode"].is_object());
    }

    #[test]
    fn test_task_wait_schema() {
        let tool = TaskWaitTool::new();
        let schema = tool.schema();
        assert!(schema["properties"]["task_id"].is_object());
        assert!(schema["properties"]["condition"].is_object());
    }

    #[test]
    fn test_task_tools_count() {
        let tools = task_tools();
        assert_eq!(tools.len(), 7);
    }

    #[test]
    fn test_tool_names() {
        let tools = task_tools();
        let names: Vec<_> = tools.iter().map(|t| t.name()).collect();
        assert!(names.contains(&"task_spawn"));
        assert!(names.contains(&"task_wait"));
        assert!(names.contains(&"task_logs"));
        assert!(names.contains(&"task_stop"));
        assert!(names.contains(&"task_send"));
        assert!(names.contains(&"task_list"));
        assert!(names.contains(&"task_status"));
    }
}
