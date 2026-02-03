//! Task Tool - Agent가 호출하는 Task 도구
//!
//! 독립 컨테이너에서 명령을 실행하는 도구

use super::{ContainerConfig, TaskExecutor};
use async_trait::async_trait;
use forge_foundation::{
    PermissionAction, Result, Tool, ToolMeta, ToolResult, ToolSchema,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;

/// Task 도구 입력
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum TaskInput {
    /// 새 Task 시작
    Start {
        /// 실행할 명령어
        command: String,

        /// 작업 디렉토리 (선택)
        #[serde(skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,

        /// 환경 변수 (선택)
        #[serde(default)]
        env: Vec<(String, String)>,
    },

    /// Task에 입력 전송
    Input {
        /// Task ID
        task_id: String,

        /// 입력 내용
        input: String,
    },

    /// Task 출력 읽기
    Output {
        /// Task ID
        task_id: String,

        /// 최근 N 라인 (선택)
        #[serde(skip_serializing_if = "Option::is_none")]
        lines: Option<usize>,
    },

    /// Task 종료
    Stop {
        /// Task ID
        task_id: String,
    },

    /// Task 강제 종료
    Kill {
        /// Task ID
        task_id: String,
    },

    /// Task 목록 조회
    List,
}

/// Task 도구
pub struct TaskTool {
    executor: Arc<TaskExecutor>,
}

impl TaskTool {
    pub fn new(executor: Arc<TaskExecutor>) -> Self {
        Self { executor }
    }
}

#[async_trait]
impl Tool for TaskTool {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            name: "task".to_string(),
            description: "Manage independent task containers for long-running processes"
                .to_string(),
            version: "1.0.0".to_string(),
        }
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            input_schema: serde_json::json!({
                "type": "object",
                "oneOf": [
                    {
                        "properties": {
                            "action": { "const": "start" },
                            "command": { "type": "string", "description": "Command to execute" },
                            "cwd": { "type": "string", "description": "Working directory" },
                            "env": {
                                "type": "array",
                                "items": {
                                    "type": "array",
                                    "items": [
                                        { "type": "string" },
                                        { "type": "string" }
                                    ]
                                }
                            }
                        },
                        "required": ["action", "command"]
                    },
                    {
                        "properties": {
                            "action": { "const": "input" },
                            "task_id": { "type": "string" },
                            "input": { "type": "string" }
                        },
                        "required": ["action", "task_id", "input"]
                    },
                    {
                        "properties": {
                            "action": { "const": "output" },
                            "task_id": { "type": "string" },
                            "lines": { "type": "integer" }
                        },
                        "required": ["action", "task_id"]
                    },
                    {
                        "properties": {
                            "action": { "const": "stop" },
                            "task_id": { "type": "string" }
                        },
                        "required": ["action", "task_id"]
                    },
                    {
                        "properties": {
                            "action": { "const": "kill" },
                            "task_id": { "type": "string" }
                        },
                        "required": ["action", "task_id"]
                    },
                    {
                        "properties": {
                            "action": { "const": "list" }
                        },
                        "required": ["action"]
                    }
                ]
            }),
        }
    }

    fn required_permission(&self, input: &Value) -> Option<PermissionAction> {
        let action = input.get("action").and_then(|v| v.as_str()).unwrap_or("");

        match action {
            "start" => {
                let command = input
                    .get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                Some(PermissionAction::Execute {
                    command: command.to_string(),
                })
            }
            _ => None, // 다른 액션은 권한 불필요
        }
    }

    async fn execute(&self, input: Value) -> Result<ToolResult> {
        let input: TaskInput = serde_json::from_value(input)?;

        match input {
            TaskInput::Start { command, cwd, env } => {
                let config = ContainerConfig {
                    working_dir: cwd.map(PathBuf::from).unwrap_or_else(|| {
                        std::env::current_dir().unwrap_or_default()
                    }),
                    env,
                    initial_command: Some(command.clone()),
                    ..Default::default()
                };

                let id = self.executor.spawn(config).await?;

                Ok(ToolResult::success(serde_json::json!({
                    "task_id": id.to_string(),
                    "command": command,
                    "status": "started"
                }).to_string()))
            }

            TaskInput::Input { task_id, input } => {
                let id = super::TaskContainerId(task_id.clone());
                self.executor.send_input(&id, &input).await?;

                Ok(ToolResult::success(format!(
                    "Input sent to task {}",
                    task_id
                )))
            }

            TaskInput::Output { task_id, lines } => {
                let id = super::TaskContainerId(task_id.clone());
                let output = if let Some(n) = lines {
                    self.executor.read_recent_output(&id, n).await?
                } else {
                    self.executor.read_output(&id).await?
                };

                Ok(ToolResult::success(output.join("\n")))
            }

            TaskInput::Stop { task_id } => {
                let id = super::TaskContainerId(task_id.clone());
                self.executor.stop(&id).await?;

                Ok(ToolResult::success(format!("Task {} stopped", task_id)))
            }

            TaskInput::Kill { task_id } => {
                let id = super::TaskContainerId(task_id.clone());
                self.executor.kill(&id).await?;

                Ok(ToolResult::success(format!("Task {} killed", task_id)))
            }

            TaskInput::List => {
                let tasks = self.executor.list().list_all();
                let result: Vec<_> = tasks
                    .iter()
                    .map(|t| {
                        serde_json::json!({
                            "id": t.id,
                            "command": t.command,
                            "status": format!("{:?}", t.status),
                            "duration_secs": t.duration().as_secs()
                        })
                    })
                    .collect();

                Ok(ToolResult::success(
                    serde_json::to_string_pretty(&result).unwrap_or_default(),
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_tool_meta() {
        let executor = Arc::new(TaskExecutor::new());
        let tool = TaskTool::new(executor);
        assert_eq!(tool.meta().name, "task");
    }
}
