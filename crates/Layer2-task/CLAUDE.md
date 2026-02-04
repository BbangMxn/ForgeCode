# forge-task

> Task 관리 및 실행 시스템 - Sub-agent 오케스트레이션, Sandbox 실행 지원

## 1. 개요

forge-task는 ForgeCode의 작업 관리 시스템입니다:
- Task 생명주기 관리
- **실행 백엔드**: Local, PTY, Container, **Sandbox** (NEW)
- **Sub-agent 시스템**: 전문화된 에이전트 생성 및 관리
- **로그 시스템**: 실시간 로그 스트리밍 및 LLM 분석
- **태스크 제어**: 종료/강제 종료 지원
- 백그라운드 실행 및 출력 스트리밍
- 컨텍스트 격리 및 지식 공유

## 2. 모듈 구조

```
forge-task/
├── src/
│   ├── lib.rs           # 공개 API
│   ├── task.rs          # Task, TaskId, TaskResult, ExecutionMode
│   ├── state.rs         # TaskState (7개 상태)
│   ├── manager.rs       # TaskManager (로그/종료 기능 포함)
│   ├── log.rs           # 로그 시스템
│   │                    # LogEntry, TaskLogBuffer, TaskLogManager
│   │                    # LogAnalysisReport (LLM 분석용)
│   ├── executor/
│   │   ├── mod.rs
│   │   ├── trait.rs     # Executor trait
│   │   ├── local.rs     # ✅ LocalExecutor (로그 스트리밍)
│   │   ├── pty.rs       # ✅ PtyExecutor (대화형 명령)
│   │   ├── container.rs # ✅ ContainerExecutor (Docker)
│   │   └── sandbox.rs   # ✅ SandboxExecutor (NEW)
│   └── subagent/
│       ├── mod.rs
│       ├── types.rs     # SubAgent, SubAgentId, SubAgentType
│       ├── config.rs    # SubAgentConfig, ModelSelection
│       ├── context.rs   # SubAgentContext, ContextWindowConfig
│       └── manager.rs   # SubAgentManager
└── CLAUDE.md            # 이 문서
```

## 3. Sandbox 실행 시스템 (NEW)

### 3.1 개요

플랫폼별 네이티브 샌드박스로 명령어를 안전하게 실행합니다:
- **macOS**: Seatbelt (sandbox-exec)
- **Linux**: Landlock LSM + seccomp BPF
- **Fallback**: Docker container isolation

### 3.2 SandboxType

```rust
pub enum SandboxType {
    /// 샌드박스 없음 (전체 접근)
    None,

    /// 플랫폼 네이티브 (Seatbelt/Landlock)
    Native,  // 기본값

    /// Docker 컨테이너 격리
    Container,

    /// 가장 엄격한 모드
    Strict,
}
```

### 3.3 SandboxConfig

```rust
let config = SandboxConfig {
    sandbox_type: SandboxType::Native,
    allowed_read_paths: vec!["/usr/local".into()],
    allowed_write_paths: vec![],
    allow_network: false,
    allowed_hosts: vec![],
    allow_spawn: false,
    timeout_ms: 30_000,
    trusted_commands: HashSet::from(["git".to_string()]),
    env_passthrough: vec!["PATH", "HOME", "LANG"],
};

// 프리셋
let permissive = SandboxConfig::permissive();  // 신뢰할 수 있는 작업용
let strict = SandboxConfig::strict();          // 신뢰할 수 없는 작업용
```

### 3.4 SandboxExecutor

```rust
use forge_task::SandboxExecutor;

let executor = SandboxExecutor::new(SandboxConfig::default());

// 샌드박스 내 실행
let result = executor.execute("npm install", &working_dir).await?;

// 결과 확인
if result.success() {
    println!("Output: {}", result.stdout);
} else if result.is_sandbox_error() {
    // 샌드박스 제한으로 인한 실패
    println!("Sandbox blocked: {}", result.stderr);
    
    // 사용자 승인 후 샌드박스 없이 재실행
    let result = executor.execute_unsandboxed_trusted(cmd, &dir).await?;
}
```

### 3.5 macOS Seatbelt 프로필

```rust
// 자동 생성되는 Seatbelt 프로필 예시
(version 1)
(deny default)

; 시스템 라이브러리 읽기 허용
(allow file-read*
    (subpath "/usr")
    (subpath "/bin")
    (subpath "/System"))

; 작업 디렉토리 허용
(allow file-read* file-write* (subpath "/project"))

; 네트워크 차단 (설정에 따라)
; (allow network*)  ; allow_network: true일 때만
```

### 3.6 Linux Landlock

```rust
// Landlock LSM (Linux 5.13+)
// 파일시스템 접근 제한 + seccomp BPF 시스템 콜 제한

// 커널 버전 확인
if SandboxExecutor::is_landlock_available() {
    // Landlock 사용
} else {
    // seccomp만 사용 또는 unsandboxed fallback
}
```

### 3.7 Sandbox Escalation 패턴

```rust
// Codex 스타일 샌드박스 에스컬레이션
let result = executor.execute(command, &dir).await?;

if result.is_sandbox_error() {
    // 1. 사용자에게 권한 요청
    let approved = permission_service.request(
        "Sandbox blocked this command. Allow without sandbox?"
    ).await?;
    
    if approved {
        // 2. 신뢰 명령으로 표시하고 재실행
        let result = executor.execute_unsandboxed_trusted(command, &dir).await?;
    }
}
```

## 4. 실행기 (Executors)

### 4.1 비교

| Executor | 용도 | 격리 수준 | 플랫폼 |
|----------|------|----------|--------|
| `LocalExecutor` | 단순 명령 실행 | 없음 | 모든 플랫폼 |
| `PtyExecutor` | 대화형 명령 | 없음 | Unix/Windows |
| `ContainerExecutor` | Docker 격리 | 높음 | Docker 필요 |
| `SandboxExecutor` | 네이티브 샌드박스 | 중간 | macOS/Linux |

### 4.2 LocalExecutor

```rust
let executor = LocalExecutor::new(LocalExecutorConfig {
    timeout: TimeoutPolicy::Fixed(Duration::from_secs(60)),
    env: HashMap::new(),
    working_dir: Some(PathBuf::from("/project")),
});

let (result, logs) = executor.execute("cargo build").await?;
```

### 4.3 PtyExecutor

```rust
let executor = PtyExecutor::new(PtyExecutorConfig {
    size: PtySizeConfig { rows: 24, cols: 80 },
    env_security: PtyEnvSecurityConfig::default(),
    ..Default::default()
});

// 대화형 명령 실행
let session = executor.spawn("bash").await?;
session.write("ls -la\n").await?;
let output = session.read().await?;
```

### 4.4 ContainerExecutor

```rust
let executor = ContainerExecutor::new(ContainerConfig {
    image: "alpine:latest".to_string(),
    network_mode: NetworkMode::None,
    resource_limits: ResourceLimits {
        memory: Some("512m".to_string()),
        cpu: Some(1.0),
    },
    ..Default::default()
});
```

## 5. 로그 시스템

### 5.1 LogEntry

```rust
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub level: LogLevel,    // Stdout, Stderr, System, Debug, Error
    pub content: String,
    pub line_number: usize,
}

let entry = LogEntry::stdout("컴파일 중...", 1);
let entry = LogEntry::stderr("오류: 파일 없음", 2);
```

### 5.2 LLM 분석용 리포트

```rust
let report = task_manager.get_log_analysis(task_id).await?;

// LLM에 전달할 텍스트
let llm_text = report.format_for_llm();
// === Task Log Analysis: abc12345 ===
// Command: cargo build
// Status: Failed
// Duration: 5.2s
// Errors: 3
// 
// Error Patterns:
//   - error[E0001]: 2 occurrences
// 
// Recent Errors:
//   [14:23:45] [stderr] error[E0001]: ...
```

## 6. Sub-agent 시스템

### 6.1 SubAgentType

```rust
pub enum SubAgentType {
    Explore,  // Read, Grep, Glob만 허용
    Plan,     // 수정 불가, 설계만
    General,  // 모든 도구
    Bash,     // 명령 실행 전문
    Custom(String),
}
```

### 6.2 SubAgentConfig

```rust
let config = SubAgentConfig::new()
    .with_model(ModelSelection::Haiku)  // 빠른 모델
    .with_max_turns(30)
    .run_in_background()
    .allow_tool("read")
    .disallow_tool("bash");

// 프리셋
let quick = SubAgentConfig::quick_explore();       // Haiku, 15 turns
let thorough = SubAgentConfig::thorough_explore(); // Sonnet, 50 turns
```

### 6.3 SubAgentManager

```rust
let manager = SubAgentManager::new(config);

// 에이전트 생성
let agent_id = manager.spawn(
    "parent-session",
    SubAgentType::Explore,
    "Find all API endpoints",
    "API exploration",
).await?;

// 시작
manager.start(agent_id).await?;

// 완료
manager.complete(agent_id, "Found 5 endpoints").await?;

// 재개 (컨텍스트 유지)
let new_id = manager.resume(agent_id, "More details").await?;
```

### 6.4 컨텍스트 윈도우 관리

```rust
let ctx = SubAgentContext::for_model("claude-3-sonnet");

// 상태 확인
let status = ctx.window_status();
println!("Usage: {}%", status.usage_percent);
println!("Needs summarization: {}", status.needs_summarization);

// 토큰 리포트
let report = ctx.token_report();
```

## 7. Layer 연결

### 7.1 Layer1 연동

```rust
use forge_foundation::{Error, Result, PermissionService};

// 권한 확인 후 실행
let permitted = permission_service.check("bash", &action).await?;
```

### 7.2 Layer2-core 연동

```rust
// ToolContext에서 TaskManager 사용
let task = Task::new(session_id, "bash", "cargo test", json!({}));
let task_id = task_manager.submit(task).await;
```

### 7.3 Layer3 연동

```rust
// Agent에서 SubAgentManager 사용
let agent_id = subagent_manager.spawn(
    session_id,
    SubAgentType::Explore,
    prompt,
    description,
).await?;
```

## 8. API 요약

### 실행기
| API | 설명 |
|-----|------|
| `LocalExecutor::execute()` | 로컬 명령 실행 |
| `PtyExecutor::spawn()` | PTY 세션 시작 |
| `ContainerExecutor::run()` | 컨테이너 실행 |
| `SandboxExecutor::execute()` | 샌드박스 실행 (NEW) |

### 태스크 관리
| API | 설명 |
|-----|------|
| `TaskManager::submit()` | 작업 제출 |
| `TaskManager::cancel()` | 작업 취소 |
| `TaskManager::get_log_analysis()` | LLM 분석용 리포트 |

### Sub-agent
| API | 설명 |
|-----|------|
| `SubAgentManager::spawn()` | 에이전트 생성 |
| `SubAgentManager::resume()` | 에이전트 재개 |
| `SubAgentContext::window_status()` | 컨텍스트 상태 |

## 9. 테스트

```bash
# 전체 테스트
cargo test -p forge-task

# Sandbox 테스트
cargo test -p forge-task sandbox

# Sub-agent 테스트
cargo test -p forge-task subagent
```

## 10. 의존성

```toml
[dependencies]
forge-foundation = { workspace = true }
tokio = { workspace = true }
async-trait = { workspace = true }
bollard = { workspace = true }  # Docker
portable-pty = { workspace = true }
chrono = { workspace = true }
uuid = { workspace = true }

[target.'cfg(unix)'.dependencies]
libc = "0.2"  # Sandbox용
```
