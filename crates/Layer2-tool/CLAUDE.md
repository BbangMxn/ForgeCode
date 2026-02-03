# Layer2-tool (forge-tool)

> LLM 에이전트를 위한 도구 시스템

## 개요

forge-tool은 ForgeCode의 도구(Tool) 시스템을 제공합니다. LLM 에이전트가 파일 시스템 조작, 명령 실행, 코드 검색 등의 작업을 수행할 수 있게 합니다.

## 모듈 구조

```
forge-tool/
├── CLAUDE.md           # 이 문서
├── Cargo.toml          # 의존성 정의
└── src/
    ├── lib.rs          # 공개 API
    ├── trait.rs        # Tool, ToolDef, ToolContext, ToolResult
    ├── registry.rs     # ToolRegistry - 도구 관리
    ├── builtin/        # 내장 도구들
    │   ├── mod.rs
    │   ├── bash.rs         # BashTool - 기본 명령 실행
    │   ├── read.rs         # ReadTool - 파일 읽기
    │   ├── write.rs        # WriteTool - 파일 쓰기
    │   ├── edit.rs         # EditTool - 파일 편집
    │   ├── grep.rs         # GrepTool - 내용 검색
    │   ├── glob.rs         # GlobTool - 파일 검색
    │   └── forgecmd_tool.rs # ForgeCmdTool - PTY 래퍼
    └── forgecmd/       # PTY 기반 쉘 환경
        ├── mod.rs          # ForgeCmd 메인 진입점
        ├── config.rs       # ForgeCmdConfig 설정
        ├── error.rs        # ForgeCmdError, CommandResult
        ├── shell.rs        # PtySession - PTY 세션 관리
        ├── filter.rs       # CommandFilter - 위험 명령 필터링
        ├── permission.rs   # PermissionChecker - Layer1 연동
        └── tracker.rs      # CommandTracker - 히스토리 추적
```

## 핵심 타입

### Tool Trait

모든 도구가 구현해야 하는 인터페이스:

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    /// 도구 정의 (이름, 설명, 파라미터 스키마)
    fn definition(&self) -> ToolDef;

    /// 도구 실행
    async fn execute(&self, ctx: &ToolContext, params: Value) -> ToolResult;

    /// 도구 이름 (convenience)
    fn name(&self) -> String { self.definition().name }
}
```

### ToolDef

LLM에게 전달되는 도구 정의:

```rust
pub struct ToolDef {
    pub name: String,           // 도구 식별자
    pub description: String,    // 설명
    pub parameters: ToolParameters, // JSON Schema
}

// Builder 패턴 사용
let def = ToolDef::builder("bash", "Execute shell command")
    .string_param("command", "Command to run", true)
    .integer_param("timeout", "Timeout in seconds", false)
    .build();
```

### ToolContext

도구 실행 컨텍스트:

```rust
pub struct ToolContext {
    pub session_id: String,
    pub working_dir: PathBuf,
    pub permissions: Arc<PermissionService>,
    pub auto_approve: bool,  // 위험! 개발 전용
}
```

### ToolResult

도구 실행 결과:

```rust
pub struct ToolResult {
    pub success: bool,
    pub content: String,
    pub metadata: Option<Value>,
    pub error: Option<String>,
}

// 편의 생성자
ToolResult::success("Output text")
ToolResult::success_with_metadata("Output", json!({ "lines": 10 }))
ToolResult::error("Something went wrong")
ToolResult::permission_denied("File write not allowed")
```

### ToolRegistry

도구 관리 레지스트리:

```rust
let registry = ToolRegistry::with_builtins();

// 도구 등록/해제
registry.register(Arc::new(MyTool::new()));
registry.unregister("old_tool");

// 도구 비활성화/활성화
registry.disable("dangerous_tool");
registry.enable("dangerous_tool");

// 도구 실행
let result = registry.execute("bash", &ctx, params).await;

// LLM에게 전달할 정의들
let definitions = registry.definitions();
```

## 내장 도구 (Builtin Tools)

### BashTool
기본 명령 실행 (stateless, non-PTY):
- 파라미터: `command`, `timeout`
- 안전 명령 자동 승인 (ls, pwd, git status 등)
- 차단 명령: curl, wget, nc, telnet
- 최대 출력: 30,000자

### ReadTool
파일 읽기:
- 파라미터: `path`, `offset`, `limit`
- 최대 파일 크기: 50KB
- 기본 라인 제한: 2,000
- 바이너리 파일 감지

### WriteTool
파일 생성/덮어쓰기:
- 파라미터: `path`, `content`
- 권한 확인 필요
- 부모 디렉토리 자동 생성

### EditTool
파일 부분 편집:
- 파라미터: `path`, `old_string`, `new_string`, `replace_all`
- 고유성 검사 (replace_all=false일 때)
- 빈 old_string으로 새 파일 생성

### GrepTool
내용 검색 (ripgrep 우선, grep 폴백):
- 파라미터: `pattern`, `path`, `case_insensitive`, `file_type`, `context`
- 최대 결과: 100개

### GlobTool
파일 패턴 검색:
- 파라미터: `pattern`, `path`
- 최대 결과: 200개

### ForgeCmdTool
PTY 기반 명령 실행 (forgecmd 래퍼):
- 파라미터: `command`, `timeout`, `working_dir`
- 인터랙티브 프로그램 지원
- 5단계 위험도 분석
- spawn_blocking으로 PTY 작업 처리

## forgecmd 서브시스템

### ForgeCmd

PTY 기반 쉘 환경의 메인 진입점:

```rust
// 생성
let forge_cmd = ForgeCmd::new(permission_service)?;

// 또는 Builder 사용
let forge_cmd = ForgeCmdBuilder::new()
    .permission_service(service)
    .shell("bash")
    .timeout(60)
    .pty_size(24, 80)
    .build()?;

// 명령 실행
let result = forge_cmd.execute("ls -la").await?;

// 권한 확인 후 실행
let result = forge_cmd.execute_with_confirmation(
    "rm file.txt",
    PermissionScope::Session
).await?;

// 위험 분석
let analysis = forge_cmd.analyze("git reset --hard");
println!("Category: {:?}, Risk: {}", analysis.category, analysis.risk_score);
```

### ForgeCmdConfig

설정 옵션:

```rust
pub struct ForgeCmdConfig {
    pub shell: String,              // "bash" | "cmd.exe"
    pub shell_args: Vec<String>,    // ["-i"]
    pub pty_size: PtySize,          // { rows: 24, cols: 80 }
    pub timeout: u64,               // 60초
    pub max_output_size: usize,     // 100KB
    pub rules: PermissionRules,     // allow/deny/ask 규칙
    pub risk_thresholds: RiskThresholds,
    pub blocked_env_vars: HashSet<String>, // AWS_*, *_TOKEN 등
    pub allowed_paths: Vec<String>,
    pub track_history: bool,
    pub strip_ansi: bool,
}

// 프리셋
let dev_config = ForgeCmdConfig::development();   // 더 관대
let prod_config = ForgeCmdConfig::production();   // 더 엄격
```

### CommandCategory & 위험도

| 카테고리 | 위험도 | 처리 | 예시 |
|----------|--------|------|------|
| ReadOnly | 0-2 | 자동 승인 | ls, cat, pwd, git status |
| SafeWrite | 2-3 | 세션 승인 | mkdir, touch, git add |
| Caution | 4-5 | 확인 필요 | rm, mv, cp, git push |
| Interactive | 5 | 특별 처리 | vim, htop, python REPL |
| Dangerous | 7-8 | 매번 확인 | rm -rf, git reset --hard |
| Forbidden | 10 | 항상 차단 | rm -rf /, fork bomb |

### RiskThresholds

```rust
pub struct RiskThresholds {
    pub auto_approve: u8,     // 2 - 이하면 자동 승인
    pub session_approve: u8,  // 5 - 이하면 세션 승인
    pub always_ask: u8,       // 7 - 이하면 매번 확인
    pub block: u8,            // 8 - 이상이면 차단
}
```

### PtySession

portable-pty를 래핑한 PTY 세션:

```rust
let mut session = PtySession::new(config, working_dir)?;

// 동기 실행
let result = session.execute("ls -la")?;

// 타임아웃 지정
let result = session.execute_with_timeout("long_cmd", Duration::from_secs(300))?;

// 비동기 실행
let mut spawned = session.spawn("long_running_process")?;
spawned.send("input\n")?;
let output = spawned.read_output()?;
let result = spawned.wait()?;

// 크기 조절
session.resize(30, 100)?;

// 환경 변수
session.set_env("MY_VAR", "value");
session.remove_env("MY_VAR");
```

### CommandFilter

위험 명령 필터링:

```rust
let filter = CommandFilter::new();

// 금지 명령 확인
if let Some(reason) = filter.is_forbidden("rm -rf /") {
    println!("Blocked: {}", reason);
}

// 위험 분석
let analysis = filter.analyze("git push --force", &config);
```

### PermissionChecker

Layer1 PermissionService 연동:

```rust
let checker = PermissionChecker::new(permission_service, config);

// 권한 확인
match checker.check_permission("rm file.txt")? {
    CheckResult::Allowed { scope } => { /* 실행 */ }
    CheckResult::NeedsConfirmation { analysis } => { /* 사용자 확인 */ }
    CheckResult::Denied { reason } => { /* 거부 */ }
}

// 권한 부여
checker.grant("npm install", PermissionScope::Session);
checker.grant_pattern("git *", PermissionScope::Session);

// 분석
let analysis = checker.analyze("dangerous_command");
```

### CommandTracker

명령 히스토리 추적:

```rust
let tracker = CommandTracker::new("session-123");

// 시작
let record_id = tracker.start("ls -la", "/home/user", &analysis);

// 완료
tracker.complete_success(&record_id, 0, "output", "");
tracker.complete_failed(&record_id, 1, "", "error");
tracker.complete_timeout(&record_id, "partial output", "");

// 조회
let recent = tracker.get_recent(10);
let failed = tracker.get_failed();
let stats = tracker.stats();

// Layer1 Storage 저장
tracker.save_to_storage(&storage, &record)?;
tracker.complete_and_save(&storage, &record_id, 0, "output", "")?;
```

## Layer1 연동

### PermissionService 통합

```rust
use forge_foundation::permission::{
    PermissionService, PermissionAction, PermissionScope, PermissionStatus
};

// 권한 확인
let action = PermissionAction::Execute { command: cmd.to_string() };
match permission_service.check("forgecmd", &action) {
    PermissionStatus::Granted => { /* OK */ }
    PermissionStatus::AutoApproved => { /* OK */ }
    PermissionStatus::Denied => { /* 거부 */ }
    PermissionStatus::Unknown => { /* 사용자 확인 필요 */ }
}

// 권한 부여
permission_service.grant_session("forgecmd", action);
```

### Storage 통합

```rust
use forge_foundation::{Storage, ToolExecutionRecord};

// CommandRecord를 ToolExecutionRecord로 변환
let tool_record = record.to_tool_execution_record();

// Storage에 저장
let id = storage.start_tool_execution(&tool_record)?;
storage.complete_tool_execution(id, Some("output"), "success", None, duration)?;
```

### Permission 등록

```rust
use forge_tool::register_permissions;

// 애플리케이션 초기화 시 호출 (멱등성)
register_permissions();

// 등록되는 권한들:
// - forgecmd.execute     (risk: 5) - 기본 실행
// - forgecmd.readonly    (risk: 1) - 읽기 전용
// - forgecmd.write       (risk: 4) - 안전한 쓰기
// - forgecmd.caution     (risk: 6) - 주의 필요, 확인 필요
// - forgecmd.interactive (risk: 7) - 인터랙티브, 확인 필요
// - forgecmd.dangerous   (risk: 9) - 위험, 확인 필요
// - forgecmd.forbidden   (risk: 10) - 금지
```

## 사용 예시

### 기본 사용

```rust
use forge_tool::{ToolRegistry, ToolContext, Tool};
use forge_foundation::permission::PermissionService;
use std::sync::Arc;

// Registry 생성 (내장 도구 포함)
let registry = ToolRegistry::with_builtins();

// Context 생성
let permissions = Arc::new(PermissionService::new());
let ctx = ToolContext::new(
    "session-001",
    PathBuf::from("/project"),
    permissions,
);

// 도구 실행
let params = serde_json::json!({ "command": "ls -la" });
let result = registry.execute("bash", &ctx, params).await;

if result.success {
    println!("Output: {}", result.content);
} else {
    println!("Error: {:?}", result.error);
}
```

### ForgeCmd 직접 사용

```rust
use forge_tool::{ForgeCmd, ForgeCmdBuilder, PermissionScope};

let permissions = Arc::new(PermissionService::new());

let mut cmd = ForgeCmdBuilder::new()
    .permission_service(permissions)
    .working_dir(PathBuf::from("/project"))
    .timeout(120)
    .build()?;

// 위험 분석
let analysis = cmd.analyze("git push origin main");
println!("Risk: {}/10, Category: {:?}", analysis.risk_score, analysis.category);

// 실행
match cmd.execute("git push origin main").await {
    Ok(result) => println!("{}", result.stdout),
    Err(ForgeCmdError::PermissionRequired { description, .. }) => {
        println!("Need approval: {}", description);
        // 사용자 확인 후...
        cmd.execute_with_confirmation("git push origin main", PermissionScope::Session).await?;
    }
    Err(e) => eprintln!("Error: {}", e),
}
```

### 커스텀 도구 생성

```rust
use forge_tool::{Tool, ToolDef, ToolContext, ToolResult};
use async_trait::async_trait;
use serde_json::Value;

struct MyTool;

#[async_trait]
impl Tool for MyTool {
    fn definition(&self) -> ToolDef {
        ToolDef::builder("my_tool", "Does something useful")
            .string_param("input", "Input data", true)
            .boolean_param("verbose", "Enable verbose output", false)
            .build()
    }

    async fn execute(&self, ctx: &ToolContext, params: Value) -> ToolResult {
        #[derive(Deserialize)]
        struct Params {
            input: String,
            #[serde(default)]
            verbose: bool,
        }

        let params: Params = serde_json::from_value(params)
            .map_err(|e| return ToolResult::error(format!("Invalid params: {}", e)))?;

        // 권한 확인
        if !ctx.auto_approve {
            let action = PermissionAction::Custom {
                description: format!("my_tool: {}", params.input),
            };
            // ... permission check
        }

        // 작업 수행
        let result = do_something(&params.input);

        if params.verbose {
            ToolResult::success_with_metadata(
                result,
                serde_json::json!({ "input_len": params.input.len() })
            )
        } else {
            ToolResult::success(result)
        }
    }
}

// 등록
let mut registry = ToolRegistry::with_builtins();
registry.register(Arc::new(MyTool));
```

## 테스트

```bash
# 전체 테스트
cargo test -p forge-tool

# forgecmd 테스트
cargo test -p forge-tool forgecmd

# 특정 테스트
cargo test -p forge-tool test_forbidden_commands

# PTY 테스트 (Unix only)
cargo test -p forge-tool test_pty_session
```

## 개선 계획

### 단기
1. [ ] ForgeCmdConfig 파일 로딩 구현
2. [ ] 비동기 PTY 작업 최적화
3. [ ] 도구 캐싱 메커니즘

### 중기
1. [ ] MCP (Model Context Protocol) 클라이언트
2. [ ] 플러그인 시스템
3. [ ] 스트리밍 출력 지원

### 장기
1. [ ] WebAssembly 도구 샌드박스
2. [ ] 분산 도구 실행
3. [ ] 도구 체인 (파이프라인)

## 의존성

```toml
[dependencies]
forge-foundation = { workspace = true }
tokio = { workspace = true }
async-trait = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
glob = { workspace = true }
which = { workspace = true }
regex = { workspace = true }
chrono = { workspace = true }
portable-pty = { workspace = true }
strip-ansi-escapes = { workspace = true }
shlex = { workspace = true }
```
