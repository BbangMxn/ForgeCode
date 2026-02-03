# forge-foundation

Foundation 계층 - 모든 상위 크레이트가 의존하는 핵심 인프라

## 구조

```
forge-foundation/src/
├── lib.rs                    # 메인 export
├── error/
│   └── mod.rs                # Error, Result (중앙 에러 관리)
├── permission/               # 권한 시스템 (독립 모듈)
│   ├── mod.rs
│   ├── types.rs              # PermissionDef, Registry (동적 등록)
│   ├── service.rs            # PermissionService (런타임)
│   └── settings.rs           # PermissionSettings (JSON 저장)
├── registry/                 # 도구 등록 (독립 모듈)
│   ├── mod.rs
│   ├── mcp/                  # MCP 서버 (자체 load/save)
│   │   ├── mod.rs
│   │   └── server.rs         # McpConfig, McpServer, McpTransport
│   └── provider/             # LLM Provider (자체 load/save)
│       ├── mod.rs
│       ├── provider.rs       # ProviderConfig, Provider
│       └── provider_type.rs  # ProviderType enum
└── storage/                  # 저장소
    ├── mod.rs
    ├── db.rs                 # SQLite (런타임 데이터)
    └── json/
        ├── mod.rs
        └── store.rs          # JsonStore (범용 JSON 저장)
```

## 설계 원칙

1. **모듈 독립성** - 각 모듈이 자체 load/save 담당, Config 없이 독립 사용
2. **동적 등록** - Permission은 다른 모듈에서 동적으로 등록
3. **저장소 분리** - SQLite(런타임), JSON(설정)

## 모듈별 역할

### error/

중앙 에러 관리

```rust
use forge_foundation::{Error, Result};

// 에러 카테고리
Error::Config(String)           // 설정
Error::PermissionDenied(String) // 권한 거부
Error::PermissionNotFound(String)
Error::Storage(String)          // 저장소
Error::Provider(String)         // Provider
Error::ProviderNotFound(String)
Error::Api { provider, message } // API 호출
Error::RateLimited(String)
Error::Mcp(String)              // MCP
Error::McpServerNotFound(String)
Error::McpConnection(String)
Error::Tool(String)             // Tool
Error::ToolNotFound(String)
Error::ToolExecution { tool, message }
Error::Task(String)             // Task/Agent
Error::Agent(String)
Error::Timeout(String)          // 실행
Error::Cancelled
Error::NotFound(String)         // 일반
Error::InvalidInput(String)
Error::Validation(String)
Error::Io(std::io::Error)       // 외부 변환
Error::Json(serde_json::Error)
Error::Sqlite(rusqlite::Error)
Error::Http(String)
Error::Internal(String)

// 헬퍼 메서드
error.is_retryable()    // 재시도 가능 여부
error.is_user_facing()  // 사용자 표시 여부
Error::api("anthropic", "rate limit exceeded")
Error::tool_execution("bash", "command failed")
```

### permission/

권한 시스템 - 동적 등록 + 런타임 관리

**types.rs** - 권한 정의 동적 등록
```rust
use forge_foundation::{
    register_permission, PermissionDef, permission_categories,
};

// 다른 모듈(forge-tool 등)에서 권한 등록
register_permission(
    PermissionDef::new("bash.execute", permission_categories::EXECUTE)
        .risk_level(9)
        .description("Execute shell command")
);

// 등록된 권한 조회
let registry = permission_registry();
let all = registry.all();
let high_risk = registry.by_risk_level(7);
```

**service.rs** - 런타임 권한 관리
```rust
use forge_foundation::{PermissionService, PermissionAction, PermissionStatus};

let service = PermissionService::load()?;

// 권한 확인
let action = PermissionAction::Execute { command: "ls".to_string() };
match service.check("bash", &action) {
    PermissionStatus::Granted => { /* 실행 */ }
    PermissionStatus::Denied => { /* 거부 */ }
    PermissionStatus::Unknown => { /* 사용자에게 질문 */ }
    PermissionStatus::AutoApproved => { /* 자동 실행 */ }
}

// 권한 부여
service.grant_session("bash", action);      // 세션 동안
service.grant_permanent("bash", action)?;   // 영구 저장
```

**settings.rs** - JSON 설정 저장
```rust
use forge_foundation::PermissionSettings;

let settings = PermissionSettings::load()?;  // global + project 병합
settings.save_global()?;
settings.save_project()?;
```

파일: `~/.forgecode/permissions.json`, `.forgecode/permissions.json`

### registry/mcp/

MCP 서버 등록 - 자체 load/save

```rust
use forge_foundation::{McpConfig, McpServer, McpTransport};

// 로드
let mcp = McpConfig::load()?;  // global + project 병합

// 서버 추가
mcp.add("filesystem", McpServer::stdio("npx")
    .arg("-y")
    .arg("@modelcontextprotocol/server-filesystem")
    .arg("/home/user"));

mcp.add("remote", McpServer::sse("http://localhost:3000/sse"));

// 조회
for (name, server) in mcp.iter_enabled() {
    println!("{}: {:?}", name, server.transport);
}

// 저장
mcp.save_global()?;
mcp.save_project()?;
```

파일: `~/.forgecode/mcp.json`, `.forgecode/mcp.json`

Claude Code 호환 형식:
```json
{
  "mcpServers": {
    "filesystem": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "/path"]
    }
  }
}
```

### registry/provider/

LLM Provider 등록 - 자체 load/save + 환경변수

```rust
use forge_foundation::{ProviderConfig, Provider, ProviderType};

// 로드 (global + project + 환경변수 병합)
let providers = ProviderConfig::load()?;

// 프로바이더 추가
providers.add("anthropic", Provider::new(ProviderType::Anthropic)
    .api_key("sk-...")
    .model("claude-sonnet-4-20250514"));

// 기본 프로바이더
providers.set_default("anthropic");
let default = providers.get_default();

// 저장
providers.save_global()?;
```

환경변수 자동 감지:
- `ANTHROPIC_API_KEY`
- `OPENAI_API_KEY`
- `GEMINI_API_KEY`
- `GROQ_API_KEY`

파일: `~/.forgecode/providers.json`, `.forgecode/providers.json`

ProviderType:
- `Anthropic` - claude-sonnet-4-20250514
- `Openai` - gpt-4o
- `Gemini` - gemini-2.0-flash
- `Groq` - llama-3.3-70b-versatile
- `Ollama` - llama3 (로컬, API key 불필요)

### storage/db.rs

SQLite - 런타임 데이터

```rust
use forge_foundation::{Storage, SessionRecord, MessageRecord};

let storage = Storage::new(&data_dir)?;

// 세션 관리
storage.create_session(&session)?;
storage.get_session("session-id")?;
storage.get_sessions(Some(10))?;  // 최근 10개

// 메시지 관리
storage.save_message(&message)?;
storage.get_messages("session-id")?;
storage.get_recent_messages("session-id", 20)?;

// 토큰 사용량
storage.record_usage(&usage)?;
storage.get_usage_summary(Some("2024-01-01"))?;

// 도구 실행 로그
let id = storage.start_tool_execution(&execution)?;
storage.complete_tool_execution(id, Some("output"), "success", None, Some(150))?;
```

테이블:
- `sessions` - 세션 메타데이터
- `messages` - 대화 메시지
- `token_usage` - 토큰 사용량 추적
- `tool_executions` - 도구 실행 로그

파일: `~/.local/share/forgecode/forgecode.db`

### storage/json/

JsonStore - 범용 JSON 저장

```rust
use forge_foundation::JsonStore;

// 글로벌 (~/.forgecode/)
let global = JsonStore::global()?;

// 프로젝트 (.forgecode/)
let project = JsonStore::current_project()?;

// 로드/저장
let data: MyConfig = store.load("config.json")?;
let data: MyConfig = store.load_or_default("config.json");
let data: Option<MyConfig> = store.load_optional("config.json")?;
store.save("config.json", &data)?;
```

## 설정 파일 위치

| 파일 | 글로벌 | 프로젝트 |
|------|--------|----------|
| MCP | `~/.forgecode/mcp.json` | `.forgecode/mcp.json` |
| Provider | `~/.forgecode/providers.json` | `.forgecode/providers.json` |
| Permission | `~/.forgecode/permissions.json` | `.forgecode/permissions.json` |
| SQLite | `~/.local/share/forgecode/forgecode.db` | - |

## 의존성 흐름

```
error/ ─────────────────────────────────────┐
                                            │
permission/ ────────────────────────────────┼──▶ lib.rs
  ├── types.rs (동적 등록)                   │
  ├── service.rs (런타임)                   │
  └── settings.rs ──┬── storage/json        │
                    │                       │
registry/ ──────────┼───────────────────────┤
  ├── mcp/ ─────────┤                       │
  └── provider/ ────┘                       │
                                            │
storage/ ───────────────────────────────────┘
  ├── db.rs (SQLite)
  └── json/store.rs (범용)
```
