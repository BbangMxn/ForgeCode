# Task Module - 독립 실행 컨테이너

Agent 대화와 분리된 장시간/서버 프로세스 실행 시스템

## 1. 설계 목표

1. **독립 실행**: 사용자 대화와 분리되어 취소 없이 실행
2. **다중 프로세스**: 여러 서버/프로세스 동시 관리
3. **실시간 모니터링**: 출력 캡처 및 상태 추적
4. **크로스 플랫폼**: Windows/Unix PTY 지원

---

## 2. 아키텍처

```
┌─────────────────────────────────────────────────────────────────┐
│                       Agent (Layer3)                             │
│                            │                                     │
│                      TaskTool 호출                               │
│                            │                                     │
├────────────────────────────┼────────────────────────────────────┤
│                            ▼                                     │
│                      TaskExecutor                                │
│                            │                                     │
│              ┌─────────────┴─────────────┐                      │
│              │                           │                      │
│              ▼                           ▼                      │
│        TaskTracker                TaskContainer[]                │
│        (상태 관리)                    (독립 PTY)                 │
│                                         │                       │
│                            ┌────────────┴────────────┐          │
│                            ▼                         ▼          │
│                       Container 1              Container 2       │
│                       ┌─────────┐              ┌─────────┐      │
│                       │npm start│              │cargo run│      │
│                       │(서버)   │              │(서버)   │      │
│                       └─────────┘              └─────────┘      │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

---

## 3. Task vs Bash 사용 기준

### 3.1 Task 사용 (독립 컨테이너)

```
✅ Task 사용 시나리오:
├── npm start / npm run dev     # 개발 서버 (계속 실행)
├── python -m http.server       # HTTP 서버
├── cargo run (서버)            # 백엔드 서버
├── docker-compose up           # 컨테이너 실행
├── npm run watch               # 파일 감시 빌드
├── pytest -w                   # 테스트 감시 모드
└── jupyter notebook            # 노트북 서버
```

### 3.2 Bash 사용 (일반 명령)

```
✅ Bash 사용 시나리오:
├── ls, pwd, cat                # 즉시 완료
├── npm install                 # 완료 후 종료
├── cargo build                 # 빌드 후 종료
├── git status, commit, push    # 즉시 완료
├── cargo test                  # 테스트 완료 후 종료
└── rustfmt, prettier           # 포맷 후 종료
```

### 3.3 판단 기준

| 조건 | 도구 |
|------|------|
| 명령이 자동 종료됨 | Bash |
| 서버/데몬 프로세스 | Task |
| 사용자 입력 대기 | Task |
| 계속 실행되며 출력 생성 | Task |
| 백그라운드 감시 필요 | Task |

---

## 4. 핵심 타입

### 4.1 TaskContainerId

```rust
/// 컨테이너 고유 ID (UUID)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TaskContainerId(pub String);

impl TaskContainerId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}
```

### 4.2 TaskStatus

```rust
pub enum TaskStatus {
    Starting,    // 시작 중
    Running,     // 실행 중
    Stopped,     // 정상 종료
    Killed,      // 강제 종료
    Error,       // 오류 발생
}
```

### 4.3 ContainerConfig

```rust
pub struct ContainerConfig {
    /// 작업 디렉토리
    pub working_dir: PathBuf,

    /// 환경 변수
    pub env: Vec<(String, String)>,

    /// 쉘 명령어 (bash, powershell)
    pub shell: String,

    /// 초기 실행 명령어
    pub initial_command: Option<String>,
}
```

---

## 5. 핵심 컴포넌트

### 5.1 TaskContainer

독립 PTY 세션을 관리하는 컨테이너.

```rust
pub struct TaskContainer {
    id: TaskContainerId,
    config: ContainerConfig,
    output_buffer: Arc<Mutex<Vec<String>>>,
    input_tx: Option<mpsc::Sender<String>>,
    // PTY 핸들 (portable-pty)
}

impl TaskContainer {
    /// 컨테이너 시작
    pub async fn start(&mut self) -> Result<()>;

    /// 입력 전송
    pub async fn send_input(&self, input: &str) -> Result<()>;

    /// 출력 읽기
    pub async fn read_output(&self) -> Vec<String>;

    /// 최근 N줄 출력
    pub async fn read_recent_output(&self, lines: usize) -> Vec<String>;

    /// 정상 종료 (SIGTERM)
    pub async fn stop(&mut self) -> Result<()>;

    /// 강제 종료 (SIGKILL)
    pub async fn kill(&mut self) -> Result<()>;
}
```

### 5.2 TaskExecutor

컨테이너 생성 및 관리.

```rust
pub struct TaskExecutor {
    containers: Arc<RwLock<HashMap<TaskContainerId, TaskContainer>>>,
    tracker: TaskTracker,
    max_concurrent: usize,
}

impl TaskExecutor {
    /// 새 Task 시작
    pub async fn spawn(&self, config: ContainerConfig) -> Result<TaskContainerId>;

    /// Task에 입력 전송
    pub async fn send_input(&self, id: &TaskContainerId, input: &str) -> Result<()>;

    /// Task 출력 읽기
    pub async fn read_output(&self, id: &TaskContainerId) -> Result<Vec<String>>;

    /// Task 종료
    pub async fn stop(&self, id: &TaskContainerId) -> Result<()>;

    /// 모든 Task 종료
    pub async fn stop_all(&self) -> Result<()>;

    /// Task 목록
    pub fn list(&self) -> &TaskTracker;
}
```

### 5.3 TaskTracker

모든 Task의 상태와 이력 추적.

```rust
pub struct TaskTracker {
    tasks: RwLock<HashMap<String, TaskInfo>>,
}

pub struct TaskInfo {
    pub id: String,
    pub command: Option<String>,
    pub status: TaskStatus,
    pub started_at: SystemTime,
    pub stopped_at: Option<SystemTime>,
    pub error: Option<String>,
}
```

---

## 6. Agent Tool 인터페이스

### 6.1 TaskTool 액션

```rust
#[serde(tag = "action", rename_all = "snake_case")]
pub enum TaskInput {
    /// 새 Task 시작
    Start {
        command: String,
        cwd: Option<String>,
        env: Vec<(String, String)>,
    },

    /// Task에 입력 전송
    Input {
        task_id: String,
        input: String,
    },

    /// Task 출력 읽기
    Output {
        task_id: String,
        lines: Option<usize>,
    },

    /// Task 종료
    Stop { task_id: String },

    /// Task 강제 종료
    Kill { task_id: String },

    /// Task 목록
    List,
}
```

### 6.2 사용 예시 (Agent 관점)

```json
// 1. 개발 서버 시작
{
    "action": "start",
    "command": "npm run dev",
    "cwd": "/project/frontend"
}
// 응답: { "task_id": "abc-123", "status": "started" }

// 2. 서버 출력 확인
{
    "action": "output",
    "task_id": "abc-123",
    "lines": 50
}
// 응답: 최근 50줄 출력

// 3. 서버에 입력 (필요시)
{
    "action": "input",
    "task_id": "abc-123",
    "input": "rs"  // nodemon restart
}

// 4. 모든 Task 목록
{
    "action": "list"
}
// 응답: 실행 중인 모든 Task 정보

// 5. Task 종료
{
    "action": "stop",
    "task_id": "abc-123"
}
```

---

## 7. PTY 구현

### 7.1 portable-pty 사용

```rust
use portable_pty::{native_pty_system, CommandBuilder, PtySize};

pub async fn start(&mut self) -> Result<()> {
    let pty_system = native_pty_system();

    // PTY 쌍 생성
    let pair = pty_system.openpty(PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    })?;

    // 명령 빌드
    let mut cmd = CommandBuilder::new(&self.config.shell);
    cmd.cwd(&self.config.working_dir);
    for (key, value) in &self.config.env {
        cmd.env(key, value);
    }

    // 프로세스 시작
    let child = pair.slave.spawn_command(cmd)?;

    // reader/writer 설정
    let reader = pair.master.try_clone_reader()?;
    let writer = pair.master.take_writer()?;

    // 출력 스트리밍 시작
    self.start_output_reader(reader);

    // 초기 명령 실행
    if let Some(cmd) = &self.config.initial_command {
        self.send_input(cmd).await?;
    }

    Ok(())
}
```

### 7.2 출력 버퍼링

```rust
async fn start_output_reader(&self, reader: Box<dyn Read + Send>) {
    let buffer = self.output_buffer.clone();

    tokio::spawn(async move {
        let mut buf_reader = BufReader::new(reader);
        let mut line = String::new();

        loop {
            match buf_reader.read_line(&mut line) {
                Ok(0) => break, // EOF
                Ok(_) => {
                    let mut buf = buffer.lock().await;
                    buf.push(line.clone());

                    // 버퍼 크기 제한 (최근 10000줄)
                    if buf.len() > 10000 {
                        buf.drain(0..1000);
                    }

                    line.clear();
                }
                Err(_) => break,
            }
        }
    });
}
```

---

## 8. 크로스 플랫폼 지원

### 8.1 Windows

```rust
#[cfg(windows)]
fn default_shell() -> String {
    "powershell.exe".to_string()
}

// Windows ConPTY (Windows 10 1809+)
// portable-pty가 자동 처리
```

### 8.2 Unix (Linux/macOS)

```rust
#[cfg(unix)]
fn default_shell() -> String {
    std::env::var("SHELL")
        .unwrap_or_else(|_| "/bin/bash".to_string())
}

// Unix PTY
// portable-pty가 자동 처리
```

---

## 9. 구현 계획

### Phase 1: 기본 기능

1. **`container.rs`**: PTY 기반 컨테이너
   - portable-pty 연동
   - 프로세스 시작/종료
   - 입출력 스트리밍

2. **`executor.rs`**: Task 관리
   - 컨테이너 생성/관리
   - 동시 실행 제한
   - 전체 종료

3. **`tracker.rs`**: 상태 추적
   - Task 정보 저장
   - 상태 변경 알림

4. **`tool.rs`**: Agent Tool
   - TaskInput 처리
   - JSON 응답 생성

### Phase 2: 고급 기능

5. 출력 필터링 (ANSI escape 제거)
6. 타임아웃 설정
7. 리소스 제한 (메모리, CPU)
8. 프로세스 그룹 관리

### Phase 3: 최적화

9. 연결 풀링
10. 출력 압축
11. 성능 모니터링

---

## 10. 의존성

```toml
[dependencies]
# PTY 지원
portable-pty = "0.8"

# 비동기 런타임
tokio = { version = "1.0", features = ["sync", "time", "io-util", "macros"] }

# UUID 생성
uuid = { version = "1.0", features = ["v4"] }

# 동기화
parking_lot = "0.12"
```

---

## 11. 에러 처리

```rust
#[derive(Debug, thiserror::Error)]
pub enum TaskError {
    #[error("Task not found: {0}")]
    NotFound(String),

    #[error("Maximum concurrent tasks ({0}) reached")]
    MaxConcurrent(usize),

    #[error("Task already stopped")]
    AlreadyStopped,

    #[error("PTY error: {0}")]
    Pty(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
```

---

## 12. 테스트 전략

### 12.1 단위 테스트

```rust
#[test]
fn test_container_id() {
    let id1 = TaskContainerId::new();
    let id2 = TaskContainerId::new();
    assert_ne!(id1, id2);
}

#[test]
fn test_task_info() {
    let id = TaskContainerId::new();
    let info = TaskInfo::new(&id, Some("npm start"));
    assert!(info.is_running());
}
```

### 12.2 통합 테스트

```rust
#[tokio::test]
async fn test_spawn_and_stop() {
    let executor = TaskExecutor::new();

    let config = ContainerConfig {
        working_dir: PathBuf::from("."),
        initial_command: Some("echo hello".to_string()),
        ..Default::default()
    };

    let id = executor.spawn(config).await.unwrap();
    assert!(executor.exists(&id).await);

    // 잠시 대기
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 출력 확인
    let output = executor.read_output(&id).await.unwrap();
    assert!(output.iter().any(|l| l.contains("hello")));

    // 종료
    executor.stop(&id).await.unwrap();
}
```

---

## 13. 보안 고려사항

### 13.1 명령 검증

```rust
impl TaskTool {
    fn validate_command(command: &str) -> Result<()> {
        // Layer1 CommandAnalyzer 사용
        let analysis = forge_foundation::command_analyzer().analyze(command);

        if matches!(analysis.risk, CommandRisk::Forbidden) {
            return Err(TaskError::ForbiddenCommand(command.to_string()));
        }

        Ok(())
    }
}
```

### 13.2 리소스 제한

```rust
pub struct ResourceLimits {
    /// 최대 메모리 (bytes)
    pub max_memory: Option<u64>,

    /// 최대 CPU 시간 (초)
    pub max_cpu_time: Option<u64>,

    /// 최대 출력 크기 (bytes)
    pub max_output_size: Option<u64>,
}
```

---

## 14. 파일 구조

```
task/
├── mod.rs          # 모듈 진입점, exports
├── container.rs    # PTY 컨테이너 구현
├── executor.rs     # Task 실행 관리
├── tracker.rs      # 상태 추적
├── tool.rs         # Agent Tool 구현
└── CLAUDE.md       # 이 문서
```

---

## 15. Layer1 연동

### 15.1 Permission 연동

```rust
impl Tool for TaskTool {
    fn required_permission(&self, input: &Value) -> Option<PermissionAction> {
        let action = input.get("action").and_then(|v| v.as_str())?;

        if action == "start" {
            let command = input.get("command")?.as_str()?;
            Some(PermissionAction::Execute {
                command: command.to_string(),
            })
        } else {
            None
        }
    }
}
```

### 15.2 ShellConfig 연동

```rust
use forge_foundation::ShellConfig;

impl ContainerConfig {
    pub fn with_shell_config(shell_config: &ShellConfig) -> Self {
        Self {
            shell: shell_config.default_shell().to_string(),
            env: shell_config.environment_vars().clone(),
            ..Default::default()
        }
    }
}
```

---

## 16. 참고 자료

### 라이브러리
- [portable-pty](https://docs.rs/portable-pty/0.8/portable_pty/) - 크로스 플랫폼 PTY
- [tokio::process](https://docs.rs/tokio/latest/tokio/process/) - 비동기 프로세스
- [wezterm-pty](https://github.com/wez/wezterm) - PTY 참고 구현

### 시스템 API
- [Windows ConPTY](https://docs.microsoft.com/en-us/windows/console/creating-a-pseudoconsole-session)
- [Unix PTY](https://man7.org/linux/man-pages/man7/pty.7.html)
