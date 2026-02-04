# ForgeCode Layer Architecture

ForgeCode는 4개의 Layer로 구성된 모듈식 아키텍처를 사용합니다.

## Layer 다이어그램

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              Layer4: CLI/TUI                                 │
│                            (forge-cli)                                       │
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │  • main.rs (엔트리포인트)                                              │  │
│  │  • TUI 인터페이스 (ratatui)                                           │  │
│  │  • 키보드 이벤트 처리                                                  │  │
│  │  • PermissionDelegate 구현                                            │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│         │                    │                    │                          │
│         ▼                    ▼                    ▼                          │
└─────────┼────────────────────┼────────────────────┼──────────────────────────┘
          │                    │                    │
          │ AgentEvent         │ ToolResult         │ Permission
          │                    │                    │
┌─────────┼────────────────────┼────────────────────┼──────────────────────────┐
│         ▼                    │                    │    Layer3: Agent          │
│  ┌───────────────────────────┼────────────────────┼───────────────────────┐  │
│  │                           │                    │                        │  │
│  │      ┌────────────────────┴────────────────────┴────────────────┐     │  │
│  │      │                    Agent Loop                             │     │  │
│  │      │  ┌─────────────┐ ┌─────────────┐ ┌─────────────────┐     │     │  │
│  │      │  │    Hook     │ │ Compressor  │ │    Steering     │     │     │  │
│  │      │  │   System    │ │  (92%)      │ │   (h2A style)   │     │     │  │
│  │      │  └─────────────┘ └─────────────┘ └─────────────────┘     │     │  │
│  │      └──────────────────────────────────────────────────────────┘     │  │
│  │                           │                                            │  │
│  │                           │ Provider Bridge                            │  │
│  │                           ▼                                            │  │
│  │      ┌──────────────────────────────────────────────────────────┐     │  │
│  │      │              ForgeNativeProvider                          │     │  │
│  │      │         (AgentProvider implementation)                    │     │  │
│  │      └──────────────────────────────────────────────────────────┘     │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│         │                    │                    │                          │
│         ▼                    ▼                    ▼                          │
└─────────┼────────────────────┼────────────────────┼──────────────────────────┘
          │                    │                    │
          │ Stream             │ Tools              │ Tasks
          │                    │                    │
┌─────────┴────────────────────┴────────────────────┴──────────────────────────┐
│                              Layer2: Core Services                            │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐              │
│  │  forge-provider │  │   forge-core    │  │   forge-task    │              │
│  │                 │  │                 │  │                 │              │
│  │  • Gateway      │  │  • ToolRegistry │  │  • TaskManager  │              │
│  │  • Anthropic    │  │  • 8 Builtin    │  │  • Executors    │              │
│  │  • OpenAI       │  │  • MCP Bridge   │  │    - Local      │              │
│  │  • Gemini       │  │  • LSP Client   │  │    - PTY        │              │
│  │  • Groq         │  │  • Git Ops      │  │    - Container  │              │
│  │  • Ollama       │  │  • Skills       │  │    - Sandbox    │              │
│  │                 │  │  • Hooks        │  │  • SubAgents    │              │
│  │  • AgentProvider│  │  • Plugins      │  │                 │              │
│  │    Abstraction  │  │  • RepoMap      │  │                 │              │
│  └─────────────────┘  └─────────────────┘  └─────────────────┘              │
│         │                    │                    │                          │
│         └────────────────────┴────────────────────┘                          │
│                              │                                               │
│                              ▼                                               │
└──────────────────────────────┼───────────────────────────────────────────────┘
                               │
                               │ Foundation APIs
                               │
┌──────────────────────────────┴───────────────────────────────────────────────┐
│                              Layer1: Foundation                               │
│  ┌─────────────────────────────────────────────────────────────────────────┐ │
│  │                                                                          │ │
│  │  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐ ┌──────────────┐   │ │
│  │  │    Core      │ │  Permission  │ │   Registry   │ │    Config    │   │ │
│  │  │   Traits     │ │   System     │ │   (MCP/LLM)  │ │   (Forge)    │   │ │
│  │  └──────────────┘ └──────────────┘ └──────────────┘ └──────────────┘   │ │
│  │                                                                          │ │
│  │  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐ ┌──────────────┐   │ │
│  │  │   Storage    │ │    Event     │ │    Audit     │ │    Cache     │   │ │
│  │  │  (SQLite)    │ │    Bus       │ │   Logger     │ │   Manager    │   │ │
│  │  └──────────────┘ └──────────────┘ └──────────────┘ └──────────────┘   │ │
│  │                                                                          │ │
│  │  ┌──────────────┐ ┌──────────────┐                                      │ │
│  │  │  Tokenizer   │ │   Security   │                                      │ │
│  │  │  (Multi-LLM) │ │  (Analyzer)  │                                      │ │
│  │  └──────────────┘ └──────────────┘                                      │ │
│  │                                                                          │ │
│  └─────────────────────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────────────────┘
```

## Layer 의존성 매트릭스

| From \ To | L1-Foundation | L2-Core | L2-Provider | L2-Task | L3-Agent | L4-CLI |
|-----------|--------------|---------|-------------|---------|----------|--------|
| L1-Foundation | - | - | - | - | - | - |
| L2-Core | **Yes** | - | - | - | - | - |
| L2-Provider | **Yes** | - | - | - | - | - |
| L2-Task | **Yes** | - | - | - | - | - |
| L3-Agent | **Yes** | **Yes** | **Yes** | **Yes** | - | - |
| L4-CLI | **Yes** | **Yes** | **Yes** | **Yes** | **Yes** | - |

## Layer 상세

### Layer1: Foundation (`forge-foundation`)

**역할**: 모든 상위 Layer가 의존하는 핵심 인프라

**모듈**:
- `core/` - Tool, Provider, Task 등 핵심 Trait 정의
- `permission/` - 권한 관리 시스템 (macOS TCC 스타일)
- `registry/` - MCP, Provider, Model, Shell 레지스트리
- `config/` - ForgeConfig, LimitsConfig
- `storage/` - SQLite (세션), JsonStore (설정)
- `event/` - EventBus (전역 이벤트)
- `audit/` - AuditLogger (감사 로그)
- `cache/` - CacheManager, LRU, Context Compaction
- `tokenizer/` - 모델별 토큰 계산 (Claude, OpenAI, Gemini, Llama)

**Export 예시**:
```rust
pub use core::{Tool, ToolContext, Provider, Task, PermissionDelegate};
pub use permission::{PermissionService, CommandAnalyzer, PathAnalyzer};
pub use storage::{Storage, JsonStore};
pub use tokenizer::{Tokenizer, TokenizerFactory};
```

---

### Layer2: Core Services

#### 2.1 forge-core

**역할**: AI Agent 도구 시스템

**모듈**:
- `tool/` - ToolRegistry, 8개 내장 도구
  - bash, read, write, edit, glob, grep, web_search, web_fetch
- `git/` - GitOps, CheckpointManager, CommitGenerator
- `mcp/` - MCP 브릿지 (stdio/sse transport)
- `lsp/` - LSP 클라이언트
- `skill/` - 슬래시 명령어 (/commit, /review-pr)
- `plugin/` - 플러그인 시스템
- `hook/` - Claude Code 호환 훅
- `repomap/` - 코드베이스 분석

#### 2.2 forge-provider

**역할**: LLM 프로바이더 추상화

**모듈**:
- `trait.rs` - Provider trait
- `gateway.rs` - 프로바이더 라우팅/폴백
- `providers/` - Anthropic, OpenAI, Gemini, Groq, Ollama
- `agent_provider.rs` - AgentProvider 추상화 (NEW)
  - NativeAgentProvider, ClaudeAgentSdkProvider, CodexProvider

#### 2.3 forge-task

**역할**: 작업 관리 및 실행

**모듈**:
- `executor/` - Local, PTY, Container, Sandbox
- `subagent/` - Sub-agent 오케스트레이션
- `log.rs` - 실시간 로그 스트리밍

---

### Layer3: Agent (`forge-agent`)

**역할**: Claude Code 스타일 Agent Loop

**핵심 컴포넌트**:
- `agent.rs` - 메인 루프 (`while(tool_call)`)
- `hook.rs` - 확장 가능한 훅 시스템
- `compressor.rs` - 92% threshold 자동 압축
- `steering.rs` - h2A 스타일 실시간 제어
- `provider_bridge.rs` - Layer2 AgentProvider 연결

**특징**:
- Single-threaded flat loop
- Hook 기반 확장성
- 자동 컨텍스트 압축
- 실시간 pause/resume/redirect

---

### Layer4: CLI (`forge-cli`)

**역할**: 사용자 인터페이스

**모듈**:
- `main.rs` - 엔트리포인트
- `tui/` - ratatui 기반 TUI
- `pages/` - 채팅, 설정 페이지
- `components/` - 입력, 메시지, 권한 모달

**책임**:
- 키보드/마우스 이벤트 처리
- AgentEvent → UI 렌더링
- PermissionDelegate 구현

---

## 데이터 흐름

### 사용자 요청 → 응답

```
User Input
    │
    ▼
┌────────────────┐
│   Layer4-CLI   │  Parse input
└───────┬────────┘
        │
        ▼
┌────────────────┐
│  Layer3-Agent  │  Agent Loop
│                │  ├─ Check context
│                │  ├─ Check steering
│                │  └─ Stream to provider
└───────┬────────┘
        │
        ├──────────────────┐
        ▼                  ▼
┌────────────────┐  ┌────────────────┐
│ Layer2-Provider│  │  Layer2-Core   │
│                │  │                │
│  LLM Request   │  │  Tool Execute  │
└───────┬────────┘  └───────┬────────┘
        │                   │
        ▼                   ▼
┌────────────────┐  ┌────────────────┐
│ Anthropic API  │  │ Layer1 Perm.   │
│ OpenAI API     │  │ Check & Log    │
│ etc.           │  │                │
└────────────────┘  └────────────────┘
```

### 권한 요청 흐름

```
Tool Execution Request
    │
    ▼
┌─────────────────────────────────────────────┐
│ Layer2-Core: Tool                           │
│   tool.required_permission(&input)          │
└───────────────────┬─────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────┐
│ Layer1: PermissionService                   │
│   1. Check deny rules                       │
│   2. Check allow rules                      │
│   3. Security analyzer (CommandRisk)        │
│   4. Session grants                         │
│   5. → Delegate to UI                       │
└───────────────────┬─────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────┐
│ Layer4: PermissionDelegate                  │
│   Show modal → User decision                │
│   AllowOnce | AllowSession | Deny           │
└───────────────────┬─────────────────────────┘
                    │
                    ▼
                 Execute or Reject
```

---

## 빌드 및 테스트

### 전체 빌드
```bash
cargo build
```

### Layer별 테스트
```bash
cargo test -p forge-foundation  # Layer1
cargo test -p forge-core        # Layer2-Core
cargo test -p forge-provider    # Layer2-Provider
cargo test -p forge-task        # Layer2-Task
cargo test -p forge-agent       # Layer3
cargo test -p forge-cli         # Layer4
```

### 전체 테스트
```bash
cargo test --workspace
```

---

## 확장 가이드

### 새 Tool 추가 (Layer2-Core)

```rust
// crates/Layer2-core/src/tool/builtin/my_tool.rs
pub struct MyTool;

impl Tool for MyTool {
    fn meta(&self) -> ToolMeta {
        ToolMeta::new("my_tool")
            .description("My custom tool")
            .category("custom")
    }

    async fn execute(&self, input: Value, ctx: &dyn ToolContext) -> Result<ToolResult> {
        // Implementation
    }
}

// Register in registry.rs
registry.register(Arc::new(MyTool));
```

### 새 Provider 추가 (Layer2-Provider)

```rust
// crates/Layer2-provider/src/providers/my_provider.rs
pub struct MyProvider { /* ... */ }

#[async_trait]
impl Provider for MyProvider {
    fn metadata(&self) -> &ProviderMetadata { /* ... */ }
    
    fn stream(&self, messages: Vec<Message>, tools: Vec<ToolDef>, ...) -> AgentStream {
        // Implementation
    }
}
```

### 새 Hook 추가 (Layer3-Agent)

```rust
// Custom hook
pub struct MyHook;

#[async_trait]
impl AgentHook for MyHook {
    fn name(&self) -> &str { "my-hook" }
    
    async fn before_tool(&self, tool_call: &ToolCall, _: &MessageHistory) -> Result<HookResult> {
        // Validation logic
        Ok(HookResult::Continue)
    }
}

// Register
let agent = Agent::new(ctx).with_hook(MyHook);
```

---

## 참고 문서

- [GAP_ANALYSIS.md](./GAP_ANALYSIS.md) - 오픈소스 비교 분석
- [PHILOSOPHY.md](./PHILOSOPHY.md) - 설계 철학
- [CLAUDE_CODE_COMPATIBILITY.md](./CLAUDE_CODE_COMPATIBILITY.md) - Claude Code 호환성
