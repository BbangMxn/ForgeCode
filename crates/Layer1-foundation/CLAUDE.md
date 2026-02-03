# forge-foundation

Foundation 계층 - 모든 상위 크레이트가 의존하는 핵심 인프라

## 설계 목표

1. **MCP + Builtin 통합**: MCP 도구와 내장 도구를 동일한 권한 시스템으로 관리
2. **전용 Shell 최적화**: 각 OS별 Shell(cmd, bash, powershell)을 통해 최적화된 실행
3. **macOS TCC 스타일**: 도구가 권한을 등록하고, 중앙에서 관리/UI 표시
4. **Task 독립 실행**: 병렬 프로그래밍을 위한 독립적인 Task 시스템

---

## 1. 핵심 아키텍처

### 1.1 전체 흐름

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          ForgeCode Architecture                              │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │                         도구 실행 흐름                                  │  │
│  │                                                                        │  │
│  │   [사용자 요청] → [Agent] → [Tool Registry] → [Permission] → [실행]   │  │
│  │                                                                        │  │
│  │   ┌──────────────────────────────────────────────────────────────┐    │  │
│  │   │                    Tool Registry (통합)                       │    │  │
│  │   │                                                               │    │  │
│  │   │  ┌─────────────────────┐    ┌─────────────────────┐          │    │  │
│  │   │  │   Builtin Tools     │    │    MCP Tools        │          │    │  │
│  │   │  │   (Layer2-tool)     │    │    (MCP Servers)    │          │    │  │
│  │   │  │                     │    │                     │          │    │  │
│  │   │  │  ├── Bash ──────────┼────┼─► 전용 Shell 실행   │          │    │  │
│  │   │  │  ├── Read           │    │  ├── Notion         │          │    │  │
│  │   │  │  ├── Write          │    │  ├── Chrome         │          │    │  │
│  │   │  │  ├── Edit           │    │  ├── GitHub         │          │    │  │
│  │   │  │  ├── Glob           │    │  ├── Slack          │          │    │  │
│  │   │  │  ├── Grep           │    │  └── Custom...      │          │    │  │
│  │   │  │  └── WebFetch       │    │                     │          │    │  │
│  │   │  └─────────────────────┘    └─────────────────────┘          │    │  │
│  │   │              │                        │                       │    │  │
│  │   │              └───────────┬────────────┘                       │    │  │
│  │   │                          │                                    │    │  │
│  │   │                          ▼                                    │    │  │
│  │   │          ┌───────────────────────────────┐                    │    │  │
│  │   │          │      Permission System        │                    │    │  │
│  │   │          │    (통합 권한 관리)            │                    │    │  │
│  │   │          │                               │                    │    │  │
│  │   │          │  1. Rule 매칭 (Deny/Allow/Ask)│                    │    │  │
│  │   │          │  2. Security 분석             │                    │    │  │
│  │   │          │  3. UI Delegate 호출          │                    │    │  │
│  │   │          └───────────────────────────────┘                    │    │  │
│  │   │                          │                                    │    │  │
│  │   │              ┌───────────┴───────────┐                        │    │  │
│  │   │              ▼                       ▼                        │    │  │
│  │   │  ┌─────────────────────┐  ┌─────────────────────┐            │    │  │
│  │   │  │   Shell Executor    │  │   MCP Transport     │            │    │  │
│  │   │  │   (전용 Shell)       │  │   (stdio/sse)       │            │    │  │
│  │   │  │                     │  │                     │            │    │  │
│  │   │  │  Windows:           │  │  ┌──► Notion API    │            │    │  │
│  │   │  │   ├── PowerShell    │  │  ├──► Chrome Ext    │            │    │  │
│  │   │  │   └── cmd.exe       │  │  ├──► GitHub API    │            │    │  │
│  │   │  │                     │  │  └──► Custom...     │            │    │  │
│  │   │  │  macOS/Linux:       │  │                     │            │    │  │
│  │   │  │   ├── bash          │  │                     │            │    │  │
│  │   │  │   ├── zsh           │  │                     │            │    │  │
│  │   │  │   └── fish          │  │                     │            │    │  │
│  │   │  └─────────────────────┘  └─────────────────────┘            │    │  │
│  │   └──────────────────────────────────────────────────────────────┘    │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                                                                              │
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │                      Task System (독립 실행)                           │  │
│  │                                                                        │  │
│  │    ┌─────────┐   ┌─────────┐   ┌─────────┐   ┌─────────┐              │  │
│  │    │ Task 1  │   │ Task 2  │   │ Task 3  │   │ Task 4  │              │  │
│  │    │ (Agent) │   │ (Build) │   │ (Test)  │   │ (Deploy)│              │  │
│  │    │   🔄    │   │   🔄    │   │   ✓     │   │   ⏳    │              │  │
│  │    └────┬────┘   └────┬────┘   └────┬────┘   └────┬────┘              │  │
│  │         │             │             │             │                    │  │
│  │         └─────────────┴─────────────┴─────────────┘                    │  │
│  │                              │                                         │  │
│  │                              ▼                                         │  │
│  │                ┌─────────────────────────┐                             │  │
│  │                │   Task Context (공유)   │                             │  │
│  │                │  - 권한 위임            │                             │  │
│  │                │  - Shell 설정 공유      │                             │  │
│  │                │  - 진행 상황 보고       │                             │  │
│  │                │  - 하위 Task 생성       │                             │  │
│  │                └─────────────────────────┘                             │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 1.2 전용 Shell 실행 최적화

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                     전용 Shell 권한 최적화 흐름                              │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  [명령어 요청: "npm install express"]                                        │
│         │                                                                    │
│         ▼                                                                    │
│  ┌─────────────────────────────────────────┐                                │
│  │ 1. Security Analyzer (security.rs)       │                                │
│  │    - CommandAnalyzer: 위험도 분석         │                                │
│  │    - PathAnalyzer: 민감 경로 확인         │                                │
│  │                                          │                                │
│  │    결과: Caution (risk_level: 3)         │                                │
│  └─────────────────────────────────────────┘                                │
│         │                                                                    │
│         ▼                                                                    │
│  ┌─────────────────────────────────────────┐                                │
│  │ 2. Permission Rules 확인                 │                                │
│  │                                          │                                │
│  │    규칙 매칭:                             │                                │
│  │    - "builtin:bash" + "npm *" → Allow    │ ← 매칭!                        │
│  │                                          │                                │
│  │    결과: 자동 허용                        │                                │
│  └─────────────────────────────────────────┘                                │
│         │                                                                    │
│         ▼                                                                    │
│  ┌─────────────────────────────────────────┐                                │
│  │ 3. Shell Executor (전용 Shell 선택)      │                                │
│  │                                          │                                │
│  │    OS 감지 → Windows                     │                                │
│  │    기본 Shell: PowerShell                │                                │
│  │                                          │                                │
│  │    실행:                                  │                                │
│  │    powershell.exe -NoProfile -Command    │                                │
│  │    "npm install express"                 │                                │
│  └─────────────────────────────────────────┘                                │
│         │                                                                    │
│         ▼                                                                    │
│  ┌─────────────────────────────────────────┐                                │
│  │ 4. 결과 반환                              │                                │
│  │    - stdout, stderr 캡처                 │                                │
│  │    - exit_code 확인                       │                                │
│  │    - 실행 시간 기록                       │                                │
│  └─────────────────────────────────────────┘                                │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 1.3 Permission 시스템 (Allow/Ask/Deny)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          Permission 흐름                                     │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  [도구 호출]                                                                 │
│       │                                                                      │
│       ▼                                                                      │
│  ┌────────────────────────────────────────────────────┐                     │
│  │                Permission Service                   │                     │
│  │                                                     │                     │
│  │  1. Deny 목록 확인 ─────────────────────────────────┼──► [거부] → Error   │
│  │     "builtin:bash" + "rm -rf /*" → Deny             │                     │
│  │                                                     │                     │
│  │  2. Security Analyzer ──────────────────────────────┼──► [Forbidden]      │
│  │     CommandRisk::Forbidden → 무조건 차단            │     → Error         │
│  │                                                     │                     │
│  │  3. Allow 목록 확인 ────────────────────────────────┼──► [허용] → 실행    │
│  │     "builtin:bash" + "ls *" → Allow                 │                     │
│  │                                                     │                     │
│  │  4. Auto-approve 확인 ──────────────────────────────┼──► [허용] → 실행    │
│  │     CommandRisk::Safe → 자동 허용                   │                     │
│  │                                                     │                     │
│  │  5. Session Grants 확인 ────────────────────────────┼──► [허용] → 실행    │
│  │     이미 세션에서 허용됨                             │                     │
│  │                                                     │                     │
│  │  6. 해당 없음 → UI Delegate ────────────────────────┼──► [질문]           │
│  │                                                     │                     │
│  └────────────────────────────────────────────────────┘                     │
│                     │                                                        │
│                     ▼                                                        │
│        ┌──────────────────────────────────┐                                 │
│        │      PermissionDelegate          │                                 │
│        │      (Layer4 TUI 구현)           │                                 │
│        │                                  │                                 │
│        │  ┌────────────────────────────┐  │                                 │
│        │  │ ⚠️ 권한 요청               │  │                                 │
│        │  │                            │  │                                 │
│        │  │ bash 도구가 다음을 실행:   │  │                                 │
│        │  │ npm install express        │  │                                 │
│        │  │                            │  │                                 │
│        │  │ 위험도: ⚡ Caution (3/10)  │  │                                 │
│        │  │                            │  │                                 │
│        │  │ [허용] [세션] [영구] [거부]│  │                                 │
│        │  └────────────────────────────┘  │                                 │
│        └──────────────────────────────────┘                                 │
│                     │                                                        │
│         ┌───────────┼───────────┬───────────┐                               │
│         ▼           ▼           ▼           ▼                               │
│    [AllowOnce] [AllowSession] [AllowPerm] [Deny]                            │
│         │           │           │           │                               │
│         ▼           ▼           ▼           ▼                               │
│      [실행]   [Session저장]  [JSON저장]  [거부]                              │
│                   +실행        +실행                                         │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. 도구 식별자 체계

### 2.1 ToolSource

```rust
// 도구의 출처를 명확히 구분
pub enum ToolSource {
    /// 내장 도구 (Layer2-tool)
    Builtin { name: String },

    /// MCP 서버 도구
    Mcp { server: String, tool: String },

    /// 사용자 정의 도구
    Custom { id: String },
}

// 식별자 예시
"builtin:bash"              // 내장 Bash 도구
"builtin:read"              // 내장 Read 도구
"builtin:write"             // 내장 Write 도구
"mcp:notion:create-page"    // Notion MCP의 페이지 생성
"mcp:chrome:navigate"       // Chrome MCP의 네비게이션
"mcp:*"                     // 모든 MCP 도구
"mcp:notion:*"              // Notion의 모든 도구
```

### 2.2 Permission Rule 매칭

```rust
pub struct PermissionRule {
    /// 도구 패턴 (glob 지원)
    pub tool_pattern: String,      // "builtin:bash", "mcp:*", "mcp:notion:*"

    /// 액션 패턴 (glob 지원)
    pub action_pattern: Option<String>,  // "rm *", "/home/user/**"

    /// 규칙 액션
    pub rule: PermissionRuleAction,      // Allow, Ask, Deny

    /// 설명
    pub reason: Option<String>,
}

// 매칭 우선순위
// 1. 더 구체적인 패턴이 우선
// 2. Deny > Allow > Ask
// 3. 먼저 정의된 규칙이 우선
```

---

## 3. 모듈 구조

```
Layer1-foundation/
│
├── core/                         🆕 핵심 인터페이스
│   ├── mod.rs
│   ├── traits.rs                 # Tool, Provider, Task, PermissionDelegate
│   └── types.rs                  # ToolSource, PermissionRule, SessionInfo
│
├── permission/                    📦 권한 시스템
│   ├── mod.rs
│   ├── types.rs                  # PermissionDef (동적 등록)
│   ├── service.rs                # PermissionService (런타임)
│   ├── settings.rs               # PermissionSettings (JSON 저장)
│   ├── security.rs               # CommandAnalyzer, PathAnalyzer
│   └── delegate.rs               🆕 PermissionDelegate (UI 연동)
│
├── registry/                      📦 레지스트리
│   ├── mod.rs
│   ├── mcp/                      # MCP 서버 설정
│   │   ├── mod.rs
│   │   └── server.rs             # McpConfig, McpServer
│   ├── provider/                 # LLM Provider 설정
│   │   ├── mod.rs
│   │   ├── provider.rs           # ProviderConfig, Provider
│   │   └── provider_type.rs      # ProviderType
│   ├── model/                    # 모델 정보
│   │   └── mod.rs                # ModelRegistry, ModelInfo
│   ├── shell/                    🆕 Shell 설정
│   │   ├── mod.rs
│   │   └── config.rs             # ShellConfig, ShellType, ShellSettings
│   └── tool/                     🆕 도구 메타데이터
│       └── mod.rs                # ToolRegistry
│
├── config/                        📦 통합 설정
│   ├── mod.rs
│   ├── forge.rs                  🆕 ForgeConfig (통합)
│   └── limits.rs                 # LimitsConfig (사용량 제한)
│
├── storage/                       📦 저장소
│   ├── mod.rs
│   ├── db.rs                     # SQLite (런타임 데이터)
│   └── json/
│       ├── mod.rs
│       └── store.rs              # JsonStore (설정 파일)
│
├── error/                         📦 에러
│   └── mod.rs                    # Error, Result
│
└── lib.rs                         📦 공개 API
```

---

## 4. 핵심 Trait

### 4.1 Tool

```rust
/// 도구 인터페이스 (Layer2에서 구현)
#[async_trait]
pub trait Tool: Send + Sync {
    /// 도구 메타데이터
    fn meta(&self) -> ToolMeta;

    /// JSON 스키마 (MCP 호환)
    fn schema(&self) -> Value;

    /// 도구 실행
    async fn execute(&self, input: Value, context: &dyn ToolContext) -> Result<ToolResult>;

    /// 필요한 권한 액션 생성
    fn required_permission(&self, input: &Value) -> Option<PermissionAction>;

    /// Layer1에 권한 등록
    fn register_permissions(&self) {
        for perm in self.meta().permissions {
            crate::permission::register(perm);
        }
    }
}

pub struct ToolMeta {
    pub name: String,           // "bash"
    pub display_name: String,   // "Bash Shell"
    pub description: String,    // "Execute shell commands"
    pub category: String,       // "execute", "filesystem", "network"
    pub permissions: Vec<PermissionDef>,
}
```

### 4.2 ToolContext

```rust
/// 도구 실행 컨텍스트 (Layer3에서 구현)
#[async_trait]
pub trait ToolContext: Send + Sync {
    /// 작업 디렉토리
    fn working_dir(&self) -> &Path;

    /// 세션 ID
    fn session_id(&self) -> &str;

    /// 환경 변수
    fn env(&self) -> &HashMap<String, String>;

    /// 권한 검사
    async fn check_permission(&self, tool: &str, action: &PermissionAction) -> PermissionStatus;

    /// 권한 요청 (UI 프롬프트)
    async fn request_permission(
        &self,
        tool: &str,
        description: &str,
        action: PermissionAction,
    ) -> Result<bool>;

    /// Shell 설정
    fn shell_config(&self) -> &dyn ShellConfig;
}
```

### 4.3 ShellConfig

```rust
/// Shell 타입
pub enum ShellType {
    Bash,       // Linux 기본
    Zsh,        // macOS 기본
    Fish,
    PowerShell, // Windows 기본
    Cmd,        // Windows 레거시
    Nushell,
}

/// Shell 설정 trait
pub trait ShellConfig: Send + Sync {
    fn shell_type(&self) -> ShellType;
    fn executable(&self) -> &str;
    fn exec_args(&self) -> Vec<String>;
    fn env_vars(&self) -> HashMap<String, String>;
    fn timeout_secs(&self) -> u64;
    fn working_dir(&self) -> Option<&Path>;
}

/// Shell 설정 (저장용)
pub struct ShellSettings {
    pub enabled: bool,
    pub executable: Option<String>,
    pub args: Option<Vec<String>>,
    pub env: HashMap<String, String>,
    pub timeout_secs: u64,
    pub working_dir: Option<PathBuf>,
}
```

### 4.4 PermissionDelegate

```rust
/// 권한 UI 델리게이트 (Layer4에서 구현)
#[async_trait]
pub trait PermissionDelegate: Send + Sync {
    async fn request_permission(
        &self,
        tool_name: &str,
        action: &PermissionAction,
        description: &str,
        risk_score: u8,
    ) -> PermissionResponse;

    fn notify(&self, message: &str);
    fn show_error(&self, error: &str);
}

pub enum PermissionResponse {
    AllowOnce,      // 이번만 허용
    AllowSession,   // 세션 동안 허용
    AllowPermanent, // 영구 허용 (저장)
    Deny,           // 거부
    DenyPermanent,  // 영구 거부 (저장)
}
```

### 4.5 Task

```rust
/// 독립 실행 태스크
#[async_trait]
pub trait Task: Send + Sync {
    fn meta(&self) -> TaskMeta;
    async fn run(&self, context: &dyn TaskContext) -> Result<TaskResult>;
    async fn cancel(&self) -> Result<()>;
    fn progress(&self) -> Option<f32>;
}

/// 태스크 컨텍스트
#[async_trait]
pub trait TaskContext: Send + Sync {
    fn session_id(&self) -> &str;
    async fn execute_tool(&self, tool: &str, input: Value) -> Result<ToolResult>;
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse>;
    fn report_progress(&self, progress: f32, message: &str);
    async fn spawn_subtask(&self, task: Box<dyn Task>) -> Result<String>;
}
```

---

## 5. 설정 파일

### 5.1 파일 위치

| 설정 | 글로벌 | 프로젝트 |
|------|--------|----------|
| 통합 설정 | `~/.forgecode/config.json` | `.forgecode/config.json` |
| MCP | `~/.forgecode/mcp.json` | `.forgecode/mcp.json` |
| Provider | `~/.forgecode/providers.json` | `.forgecode/providers.json` |
| Permission | `~/.forgecode/permissions.json` | `.forgecode/permissions.json` |
| Shell | `~/.forgecode/shell.json` | `.forgecode/shell.json` |
| Limits | `~/.forgecode/limits.json` | `.forgecode/limits.json` |
| SQLite | `~/.local/share/forgecode/forgecode.db` | - |

### 5.2 permissions.json

```json
{
  "version": 1,
  "rules": [
    // MCP 도구 규칙
    { "toolPattern": "mcp:notion:*", "rule": "allow" },
    { "toolPattern": "mcp:chrome:*", "rule": "ask" },
    { "toolPattern": "mcp:github:*", "rule": "ask" },

    // Builtin 도구 - 안전한 명령어
    { "toolPattern": "builtin:bash", "actionPattern": "ls *", "rule": "allow" },
    { "toolPattern": "builtin:bash", "actionPattern": "pwd", "rule": "allow" },
    { "toolPattern": "builtin:bash", "actionPattern": "cat *", "rule": "allow" },
    { "toolPattern": "builtin:bash", "actionPattern": "git status", "rule": "allow" },
    { "toolPattern": "builtin:bash", "actionPattern": "git log *", "rule": "allow" },
    { "toolPattern": "builtin:bash", "actionPattern": "git diff *", "rule": "allow" },

    // Builtin 도구 - 주의 필요
    { "toolPattern": "builtin:bash", "actionPattern": "npm *", "rule": "ask" },
    { "toolPattern": "builtin:bash", "actionPattern": "git push *", "rule": "ask" },
    { "toolPattern": "builtin:bash", "actionPattern": "git commit *", "rule": "ask" },

    // Builtin 도구 - 위험 (항상 차단)
    { "toolPattern": "builtin:bash", "actionPattern": "rm -rf /*", "rule": "deny" },
    { "toolPattern": "builtin:bash", "actionPattern": "rm -rf /", "rule": "deny" },

    // 파일 시스템
    { "toolPattern": "builtin:write", "actionPattern": "**/.env*", "rule": "deny" },
    { "toolPattern": "builtin:write", "actionPattern": "**/*.pem", "rule": "deny" },
    { "toolPattern": "builtin:write", "actionPattern": "**/*_rsa", "rule": "deny" }
  ],
  "autoApproveTools": [
    "builtin:read",
    "builtin:glob",
    "builtin:grep"
  ],
  "autoApprove": false
}
```

### 5.3 shell.json

```json
{
  "default": "powershell",
  "globalEnv": {
    "LANG": "en_US.UTF-8"
  },
  "globalTimeoutSecs": 120,
  "shells": {
    "powershell": {
      "enabled": true,
      "executable": "powershell.exe",
      "args": ["-NoProfile", "-NonInteractive", "-Command"],
      "env": {},
      "timeoutSecs": 120
    },
    "cmd": {
      "enabled": true,
      "executable": "cmd.exe",
      "args": ["/C"],
      "env": {},
      "timeoutSecs": 120
    },
    "bash": {
      "enabled": true,
      "executable": "bash",
      "args": ["-c"],
      "env": {},
      "timeoutSecs": 120
    }
  }
}
```

### 5.4 mcp.json

```json
{
  "mcpServers": {
    "notion": {
      "command": "npx",
      "args": ["-y", "@notionhq/notion-mcp-server"],
      "env": {
        "NOTION_API_KEY": "${NOTION_API_KEY}"
      }
    },
    "chrome": {
      "command": "npx",
      "args": ["-y", "@anthropic/claude-chrome-mcp"],
      "env": {}
    },
    "filesystem": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "/home/user/projects"],
      "env": {}
    }
  }
}
```

---

## 6. 사용 예시

### 6.1 Layer2에서 도구 구현

```rust
// Layer2-tool/src/builtin/bash.rs
use forge_foundation::{
    Tool, ToolMeta, ToolResult, ToolContext,
    PermissionDef, PermissionAction,
    permission_categories, command_analyzer,
};

pub struct BashTool;

impl Tool for BashTool {
    fn meta(&self) -> ToolMeta {
        ToolMeta::new("bash")
            .display_name("Bash Shell")
            .description("Execute shell commands")
            .category("execute")
            .permission(
                PermissionDef::new("bash.execute", permission_categories::EXECUTE)
                    .risk_level(7)
                    .description("Execute shell command")
            )
    }

    fn required_permission(&self, input: &Value) -> Option<PermissionAction> {
        let command = input.get("command")?.as_str()?;
        Some(PermissionAction::Execute { command: command.to_string() })
    }

    async fn execute(&self, input: Value, ctx: &dyn ToolContext) -> Result<ToolResult> {
        let command = input.get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::InvalidInput("command required".into()))?;

        // 1. 보안 분석
        let analysis = command_analyzer().analyze(command);
        if analysis.risk.is_blocked() {
            return Ok(ToolResult::error(format!(
                "Command blocked: {}",
                analysis.reason.unwrap_or_default()
            )));
        }

        // 2. 권한 확인
        if let Some(action) = self.required_permission(&input) {
            let permitted = ctx.request_permission(
                "builtin:bash",
                &format!("Execute: {}", command),
                action,
            ).await?;

            if !permitted {
                return Ok(ToolResult::error("Permission denied"));
            }
        }

        // 3. Shell 설정에 따라 실행
        let shell = ctx.shell_config();
        let (exe, args) = (shell.executable(), shell.exec_args());

        // ... 실제 실행 로직

        Ok(ToolResult::success(output))
    }
}
```

### 6.2 Layer4에서 PermissionDelegate 구현

```rust
// Layer4-cli/src/permission_ui.rs
use forge_foundation::{PermissionDelegate, PermissionAction, PermissionResponse};

pub struct TuiPermissionDelegate {
    tx: mpsc::Sender<PermissionRequest>,
    rx: mpsc::Receiver<PermissionResponse>,
}

#[async_trait]
impl PermissionDelegate for TuiPermissionDelegate {
    async fn request_permission(
        &self,
        tool_name: &str,
        action: &PermissionAction,
        description: &str,
        risk_score: u8,
    ) -> PermissionResponse {
        // TUI에 권한 요청 전송
        self.tx.send(PermissionRequest {
            tool: tool_name.to_string(),
            action: action.clone(),
            description: description.to_string(),
            risk_score,
        }).await.unwrap();

        // 사용자 응답 대기
        self.rx.recv().await.unwrap_or(PermissionResponse::Deny)
    }
}
```

---

## 7. 보안 (security.rs)

### 7.1 CommandAnalyzer

```rust
use forge_foundation::{command_analyzer, CommandRisk};

let analysis = command_analyzer().analyze("rm -rf /");

match analysis.risk {
    CommandRisk::Forbidden => {
        // 절대 실행 불가 (rm -rf /, fork bomb 등)
    }
    CommandRisk::Dangerous => {
        // 항상 확인 필요 (rm, mv, git push 등)
    }
    CommandRisk::Caution => {
        // 주의 필요 (mkdir, npm install 등)
    }
    CommandRisk::Safe => {
        // 자동 실행 가능 (ls, pwd, cat 등)
    }
    CommandRisk::Interactive => {
        // 대화형 명령 (vim, htop 등) - 특수 처리
    }
    CommandRisk::Unknown => {
        // 알 수 없음 - 확인 필요
    }
}
```

### 7.2 PathAnalyzer

```rust
use forge_foundation::path_analyzer;

if path_analyzer().is_sensitive("/home/user/.ssh/id_rsa") {
    // 민감한 파일!
    // - SSH 키, AWS 자격증명, .env 파일 등
}

let score = path_analyzer().sensitivity_score(path);
// 0: 일반 파일
// 5: 주의 필요
// 10: 매우 민감 (SSH 키 등)
```

---

## 8. 의존성 흐름

```
                           ┌─────────────────────────────────────────┐
                           │              lib.rs                     │
                           │         (공개 API 내보내기)              │
                           └─────────────────────────────────────────┘
                                              ▲
                                              │
        ┌─────────────────┬───────────────────┼───────────────────┬─────────────────┐
        │                 │                   │                   │                 │
        ▼                 ▼                   ▼                   ▼                 ▼
┌───────────────┐ ┌───────────────┐ ┌───────────────┐ ┌───────────────┐ ┌───────────────┐
│    core/      │ │  permission/  │ │   registry/   │ │    config/    │ │   storage/    │
│               │ │               │ │               │ │               │ │               │
│  traits.rs    │ │  types.rs     │ │  mcp/         │ │  forge.rs     │ │  db.rs        │
│  types.rs     │ │  service.rs   │ │  provider/    │ │  limits.rs    │ │  json/        │
│               │ │  settings.rs  │ │  shell/       │ │               │ │    store.rs   │
│               │ │  security.rs  │ │  model/       │ │               │ │               │
│               │ │  delegate.rs  │ │  tool/        │ │               │ │               │
└───────────────┘ └───────┬───────┘ └───────┬───────┘ └───────┬───────┘ └───────────────┘
                          │                 │                 │                 ▲
                          │                 │                 │                 │
                          └─────────────────┴─────────────────┴─────────────────┘
                                              │
                                              ▼
                                    ┌───────────────┐
                                    │    error/     │
                                    │    mod.rs     │
                                    └───────────────┘
```

---

## 9. 구현 상태

### 완료된 모듈

1. ✅ **core/traits.rs** - 핵심 Trait 정의
   - `Tool`, `ToolContext` - 도구 인터페이스
   - `Provider` - LLM Provider 인터페이스
   - `Task`, `TaskContext`, `TaskObserver` - 태스크 시스템
   - `ShellConfig`, `ShellType` - Shell 설정 인터페이스
   - `PermissionDelegate`, `PermissionResponse` - UI 연동
   - `Configurable` - 설정 관리 Trait

2. ✅ **core/types.rs** - 공용 타입 정의
   - `ToolSource` - 도구 출처 (Builtin/MCP/Custom)
   - `PermissionRule`, `PermissionRuleAction` - 권한 규칙
   - `ExecutionEnv` - 실행 환경
   - `SessionInfo` - 세션 정보
   - `ModelHint` - 모델 선택 힌트

3. ✅ **permission/** - 권한 시스템
   - `types.rs` - PermissionDef, PermissionRegistry (동적 등록)
   - `service.rs` - PermissionService (런타임 관리)
   - `settings.rs` - PermissionSettings (JSON 저장)
   - `security.rs` - CommandAnalyzer, PathAnalyzer (보안 분석)

4. ✅ **registry/** - 레지스트리
   - `mcp/server.rs` - McpConfig, McpServer, McpTransport
   - `provider/provider.rs` - ProviderConfig, Provider
   - `provider/provider_type.rs` - ProviderType
   - `model/mod.rs` - ModelRegistry, ModelInfo, ModelPricing
   - `shell/config.rs` - ShellConfig, ShellSettings, ShellRunner

5. ✅ **config/** - 통합 설정
   - `forge.rs` - ForgeConfig (통합 설정)
   - `limits.rs` - LimitsConfig, SessionLimits, DailyLimits, MonthlyLimits

6. ✅ **storage/** - 저장소
   - `db.rs` - SQLite Storage (세션, 메시지, 토큰 사용량)
   - `json/store.rs` - JsonStore (설정 파일)

7. ✅ **error/mod.rs** - 에러 타입

8. ✅ **lib.rs** - 공개 API Export
   - 모든 모듈 올바르게 export
   - 이름 충돌 해결 (`shell_store`, `provider_store` 서브모듈)

### 모듈 Export 구조

```rust
// Core (핵심 Trait 및 타입)
pub use core::{
    Tool, ToolContext, ToolMeta, ToolResult,
    Provider, ProviderMeta, ChatMessage, ChatRequest, ChatResponse,
    Task, TaskContext, TaskMeta, TaskResult, TaskState, TaskObserver,
    ShellConfig, ShellType,
    PermissionDelegate, PermissionResponse,
    Configurable,
    ToolSource, PermissionRule, ExecutionEnv, SessionInfo, ModelHint,
};

// Permission (권한 시스템)
pub use permission::{
    PermissionDef, PermissionRegistry, register_permission,
    PermissionService, Permission, PermissionAction, PermissionStatus,
    PermissionSettings, PermissionGrant, PermissionDeny,
    CommandAnalyzer, CommandRisk, PathAnalyzer, command_analyzer, path_analyzer,
};

// Registry (레지스트리)
pub use registry::{
    McpConfig, McpServer, McpTransport,
    ProviderConfig, ProviderType,
    ModelRegistry, ModelInfo, ModelCapabilities, ModelPricing,
    ShellSettings, ShellRunner,
};

// Config (설정)
pub use config::{
    ForgeConfig, ThemeConfig, EditorConfig, AutoSaveConfig, ExperimentalConfig,
    LimitsConfig, SessionLimits, DailyLimits, MonthlyLimits,
};

// Storage (저장소)
pub use storage::{
    Storage, SessionRecord, MessageRecord, TokenUsageRecord,
    JsonStore,
};

// 이름 충돌 해결용 서브모듈
pub mod shell_store { /* registry::shell types */ }
pub mod provider_store { /* registry::provider types */ }
```

### 다음 단계 (Layer2)

Layer1이 완성되었으므로 Layer2-tool에서:

1. **도구 구현**: `Tool` trait 구현
   - BashTool, ReadTool, WriteTool, EditTool, GlobTool, GrepTool

2. **권한 등록**: 각 도구가 `register_permissions()` 호출
   - Layer1의 `PermissionRegistry`에 권한 정의 등록

3. **Shell 연동**: `ToolContext.shell_config()` 사용
   - OS별 최적화된 Shell 실행
