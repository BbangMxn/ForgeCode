//! Container executor - runs tasks in Docker containers

use crate::executor::Executor;
use crate::task::{ExecutionMode, Task, TaskResult};
use async_trait::async_trait;
use bollard::container::{
    Config, CreateContainerOptions, RemoveContainerOptions, StartContainerOptions,
    WaitContainerOptions,
};
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::Docker;
use forge_foundation::{Error, Result};
use futures::StreamExt;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::timeout;

/// Default container image
const DEFAULT_IMAGE: &str = "ubuntu:22.04";

/// Container executor that runs tasks in Docker containers
pub struct ContainerExecutor {
    /// Docker client
    docker: Arc<Docker>,

    /// Running container IDs by task ID
    containers: Arc<Mutex<HashMap<String, String>>>,

    /// Whether Docker is available
    available: bool,
}

impl ContainerExecutor {
    /// Create a new container executor
    pub async fn new() -> Self {
        let docker = Docker::connect_with_local_defaults();

        let (docker, available) = match docker {
            Ok(d) => {
                // Test connection
                let available = d.ping().await.is_ok();
                (Arc::new(d), available)
            }
            Err(_) => (
                Arc::new(Docker::connect_with_local_defaults().unwrap_or_else(|_| {
                    // Create a dummy client (will fail on use)
                    Docker::connect_with_local_defaults().unwrap()
                })),
                false,
            ),
        };

        Self {
            docker,
            containers: Arc::new(Mutex::new(HashMap::new())),
            available,
        }
    }

    /// Create a container for the task
    async fn create_container(&self, task: &Task) -> Result<String> {
        let (image, workdir, env, volumes) = match &task.execution_mode {
            ExecutionMode::Container {
                image,
                workdir,
                env,
                volumes,
            } => (
                image.clone(),
                workdir.clone(),
                env.clone(),
                volumes.clone(),
            ),
            _ => (DEFAULT_IMAGE.to_string(), None, vec![], vec![]),
        };

        // Convert env to Docker format
        let env_vec: Vec<String> = env.iter().map(|(k, v)| format!("{}={}", k, v)).collect();

        // Convert volumes to Docker format
        let binds: Vec<String> = volumes.iter().map(|(h, c)| format!("{}:{}", h, c)).collect();

        // Create container config
        let config = Config {
            image: Some(image.clone()),
            working_dir: workdir,
            env: Some(env_vec),
            host_config: Some(bollard::models::HostConfig {
                binds: if binds.is_empty() { None } else { Some(binds) },
                auto_remove: Some(false),
                ..Default::default()
            }),
            // Keep container running with a sleep command
            cmd: Some(vec!["sleep".to_string(), "infinity".to_string()]),
            ..Default::default()
        };

        let options = CreateContainerOptions {
            name: format!("forgecode-{}", task.id),
            ..Default::default()
        };

        // Create container
        let response = self
            .docker
            .create_container(Some(options), config)
            .await
            .map_err(|e| Error::Task(format!("Failed to create container: {}", e)))?;

        // Start container
        self.docker
            .start_container(&response.id, None::<StartContainerOptions<String>>)
            .await
            .map_err(|e| Error::Task(format!("Failed to start container: {}", e)))?;

        Ok(response.id)
    }

    /// Execute command in container
    async fn exec_in_container(&self, container_id: &str, command: &str) -> Result<TaskResult> {
        let exec_options = CreateExecOptions {
            cmd: Some(vec!["sh", "-c", command]),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            ..Default::default()
        };

        let exec = self
            .docker
            .create_exec(container_id, exec_options)
            .await
            .map_err(|e| Error::Task(format!("Failed to create exec: {}", e)))?;

        let output = self
            .docker
            .start_exec(&exec.id, None)
            .await
            .map_err(|e| Error::Task(format!("Failed to start exec: {}", e)))?;

        let mut stdout = String::new();
        let mut stderr = String::new();

        if let StartExecResults::Attached { mut output, .. } = output {
            while let Some(Ok(msg)) = output.next().await {
                match msg {
                    bollard::container::LogOutput::StdOut { message } => {
                        stdout.push_str(&String::from_utf8_lossy(&message));
                    }
                    bollard::container::LogOutput::StdErr { message } => {
                        stderr.push_str(&String::from_utf8_lossy(&message));
                    }
                    _ => {}
                }
            }
        }

        // Get exit code
        let inspect = self
            .docker
            .inspect_exec(&exec.id)
            .await
            .map_err(|e| Error::Task(format!("Failed to inspect exec: {}", e)))?;

        let exit_code = inspect.exit_code.unwrap_or(-1) as i32;

        let mut content = stdout;
        if !stderr.is_empty() {
            if !content.is_empty() {
                content.push_str("\n--- stderr ---\n");
            }
            content.push_str(&stderr);
        }

        Ok(TaskResult::with_exit_code(content, exit_code))
    }

    /// Remove a container
    async fn remove_container(&self, container_id: &str) -> Result<()> {
        let options = RemoveContainerOptions {
            force: true,
            ..Default::default()
        };

        self.docker
            .remove_container(container_id, Some(options))
            .await
            .map_err(|e| Error::Task(format!("Failed to remove container: {}", e)))?;

        Ok(())
    }
}

#[async_trait]
impl Executor for ContainerExecutor {
    async fn execute(&self, task: &Task) -> Result<TaskResult> {
        if !self.available {
            return Err(Error::Task("Docker is not available".to_string()));
        }

        // Ensure task is for container execution
        if !matches!(task.execution_mode, ExecutionMode::Container { .. }) {
            return Err(Error::Task(
                "ContainerExecutor can only execute Container tasks".to_string(),
            ));
        }

        // Create container
        let container_id = self.create_container(task).await?;

        // Store container ID
        {
            let mut containers = self.containers.lock().await;
            containers.insert(task.id.to_string(), container_id.clone());
        }

        // Execute command with timeout
        let result = timeout(
            task.timeout,
            self.exec_in_container(&container_id, &task.command),
        )
        .await;

        // Cleanup container
        let _ = self.remove_container(&container_id).await;

        // Remove from tracking
        {
            let mut containers = self.containers.lock().await;
            containers.remove(&task.id.to_string());
        }

        match result {
            Ok(r) => r,
            Err(_) => Err(Error::Timeout("Task execution timed out".to_string())),
        }
    }

    async fn cancel(&self, task: &Task) -> Result<()> {
        let container_id = {
            let containers = self.containers.lock().await;
            containers.get(&task.id.to_string()).cloned()
        };

        if let Some(container_id) = container_id {
            self.remove_container(&container_id).await?;
        }

        Ok(())
    }

    fn is_available(&self) -> bool {
        self.available
    }

    fn name(&self) -> &'static str {
        "container"
    }
}
