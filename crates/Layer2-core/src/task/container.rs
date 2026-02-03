//! Task Container - 독립 실행 컨테이너
//!
//! PTY 기반 쉘 세션을 관리하는 컨테이너

use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

/// 컨테이너 고유 ID
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TaskContainerId(pub String);

impl TaskContainerId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for TaskContainerId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TaskContainerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// 컨테이너 설정
#[derive(Debug, Clone)]
pub struct ContainerConfig {
    /// 작업 디렉토리
    pub working_dir: PathBuf,

    /// 환경 변수
    pub env: Vec<(String, String)>,

    /// 쉘 명령어
    pub shell: String,

    /// 초기 명령어 (선택)
    pub initial_command: Option<String>,
}

impl Default for ContainerConfig {
    fn default() -> Self {
        Self {
            working_dir: std::env::current_dir().unwrap_or_default(),
            env: Vec::new(),
            #[cfg(windows)]
            shell: "powershell.exe".to_string(),
            #[cfg(not(windows))]
            shell: "/bin/bash".to_string(),
            initial_command: None,
        }
    }
}

/// Task 컨테이너 - 독립 PTY 쉘 세션
pub struct TaskContainer {
    id: TaskContainerId,
    config: ContainerConfig,
    // TODO: PTY 핸들
    // pty: Option<PtyHandle>,
    output_buffer: Arc<Mutex<Vec<String>>>,
    input_tx: Option<mpsc::Sender<String>>,
}

impl TaskContainer {
    /// 새 컨테이너 생성
    pub fn new(config: ContainerConfig) -> Self {
        Self {
            id: TaskContainerId::new(),
            config,
            output_buffer: Arc::new(Mutex::new(Vec::new())),
            input_tx: None,
        }
    }

    /// 컨테이너 ID
    pub fn id(&self) -> &TaskContainerId {
        &self.id
    }

    /// 컨테이너 시작
    pub async fn start(&mut self) -> forge_foundation::Result<()> {
        // TODO: PTY 세션 시작
        // 1. portable_pty 또는 tokio-pty-process 사용
        // 2. 쉘 프로세스 시작
        // 3. 초기 명령어 실행 (있으면)
        // 4. 출력 스트리밍 시작

        if let Some(cmd) = &self.config.initial_command {
            self.send_input(cmd).await?;
        }

        Ok(())
    }

    /// 컨테이너에 입력 전송
    pub async fn send_input(&self, input: &str) -> forge_foundation::Result<()> {
        if let Some(tx) = &self.input_tx {
            tx.send(format!("{}\n", input)).await.map_err(|e| {
                forge_foundation::Error::internal(format!("Failed to send input: {}", e))
            })?;
        }
        Ok(())
    }

    /// 출력 버퍼 읽기
    pub async fn read_output(&self) -> Vec<String> {
        self.output_buffer.lock().await.clone()
    }

    /// 최근 출력 읽기
    pub async fn read_recent_output(&self, lines: usize) -> Vec<String> {
        let buffer = self.output_buffer.lock().await;
        let start = buffer.len().saturating_sub(lines);
        buffer[start..].to_vec()
    }

    /// 컨테이너 종료
    pub async fn stop(&mut self) -> forge_foundation::Result<()> {
        // TODO: PTY 세션 종료
        // 1. SIGTERM 전송
        // 2. 타임아웃 후 SIGKILL
        // 3. 리소스 정리

        Ok(())
    }

    /// 컨테이너 강제 종료
    pub async fn kill(&mut self) -> forge_foundation::Result<()> {
        // TODO: SIGKILL 전송
        Ok(())
    }

    /// 컨테이너 실행 중 여부
    pub fn is_running(&self) -> bool {
        // TODO: 프로세스 상태 확인
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_container_id() {
        let id1 = TaskContainerId::new();
        let id2 = TaskContainerId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_default_config() {
        let config = ContainerConfig::default();
        #[cfg(windows)]
        assert!(config.shell.contains("powershell"));
        #[cfg(not(windows))]
        assert!(config.shell.contains("bash"));
    }
}
