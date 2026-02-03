//! Local executor - runs tasks on the host system

use crate::executor::Executor;
use crate::task::{ExecutionMode, Task, TaskResult};
use async_trait::async_trait;
use forge_foundation::{Error, Result};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio::time::timeout;

/// Local executor that runs tasks directly on the host
pub struct LocalExecutor {
    /// Running processes by task ID
    processes: Arc<Mutex<HashMap<String, Child>>>,
}

impl LocalExecutor {
    /// Create a new local executor
    pub fn new() -> Self {
        Self {
            processes: Arc::new(Mutex::new(HashMap::new())),
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
            .stderr(Stdio::piped());

        // Spawn process
        let child = cmd
            .spawn()
            .map_err(|e| Error::Task(format!("Failed to spawn process: {}", e)))?;

        // Store child process
        {
            let mut processes = self.processes.lock().await;
            processes.insert(task.id.to_string(), child);
        }

        // Wait for completion with timeout
        let result = {
            let mut processes = self.processes.lock().await;
            if let Some(mut child) = processes.remove(&task.id.to_string()) {
                match timeout(task.timeout, child.wait_with_output()).await {
                    Ok(Ok(output)) => {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        let stderr = String::from_utf8_lossy(&output.stderr);

                        let mut content = stdout.to_string();
                        if !stderr.is_empty() {
                            if !content.is_empty() {
                                content.push_str("\n--- stderr ---\n");
                            }
                            content.push_str(&stderr);
                        }

                        Ok(TaskResult::with_exit_code(
                            content,
                            output.status.code().unwrap_or(-1),
                        ))
                    }
                    Ok(Err(e)) => Err(Error::Task(format!("Process error: {}", e))),
                    Err(_) => Err(Error::Timeout("Task execution timed out".to_string())),
                }
            } else {
                Err(Error::Task("Process not found".to_string()))
            }
        };

        result
    }

    async fn cancel(&self, task: &Task) -> Result<()> {
        let mut processes = self.processes.lock().await;
        if let Some(mut child) = processes.remove(&task.id.to_string()) {
            child
                .kill()
                .await
                .map_err(|e| Error::Task(format!("Failed to kill process: {}", e)))?;
        }
        Ok(())
    }

    fn is_available(&self) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "local"
    }
}
