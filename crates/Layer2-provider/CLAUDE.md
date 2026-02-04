# forge-provider (Layer2-provider)

LLM Provider 추상화 계층 - 다양한 LLM 프로바이더와 AI 에이전트 프로바이더를 통합 인터페이스로 제공

---

## 아키텍처

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Layer2-provider 구조                                 │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │                       LLM Provider Layer                               │  │
│  │                                                                        │  │
│  │   ┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐    │  │
│  │   │  Anthropic  │ │   OpenAI    │ │   Gemini    │ │    Groq     │    │  │
│  │   │   Claude    │ │   GPT-4o    │ │   2.0 Flash │ │   LPU       │    │  │
│  │   └──────┬──────┘ └──────┬──────┘ └──────┬──────┘ └──────┬──────┘    │  │
│  │          │               │               │               │            │  │
│  │          └───────────────┴───────────────┴───────────────┘            │  │
│  │                              │                                         │  │
│  │                              ▼                                         │  │
│  │                    ┌─────────────────┐                                 │  │
│  │                    │  Provider Trait │                                 │  │
│  │                    │  (통합 인터페이스)│                                 │  │
│  │                    └────────┬────────┘                                 │  │
│  │                             │                                          │  │
│  │                             ▼                                          │  │
│  │                    ┌─────────────────┐                                 │  │
│  │                    │    Gateway      │                                 │  │
│  │                    │  (라우팅/폴백)   │                                 │  │
│  │                    └─────────────────┘                                 │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                                                                              │
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │                     Agent Provider Layer                               │  │
│  │                                                                        │  │
│  │   ┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐    │  │
│  │   │   Native    │ │ Claude SDK  │ │ OpenAI Codex│ │  MCP Server │    │  │
│  │   │  (Layer3)   │ │  (External) │ │  (External) │ │  (Custom)   │    │  │
│  │   └──────┬──────┘ └──────┬──────┘ └──────┬──────┘ └──────┬──────┘    │  │
│  │          │               │               │               │            │  │
│  │          └───────────────┴───────────────┴───────────────┘            │  │
│  │                              │                                         │  │
│  │                              ▼                                         │  │
│  │                  ┌─────────────────────┐                               │  │
│  │                  │  AgentProvider Trait │                               │  │
│  │                  │  (통합 인터페이스)    │                               │  │
│  │                  └──────────┬──────────┘                               │  │
│  │                             │                                          │  │
│  │                             ▼                                          │  │
│  │                  ┌─────────────────────┐                               │  │
│  │                  │ AgentProviderRegistry│                               │  │
│  │                  │  (프로바이더 관리)    │                               │  │
│  │                  └─────────────────────┘                               │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 모듈 구조

```
forge-provider/src/
├── lib.rs                    # 메인 export
├── trait.rs                  # Provider trait, 공통 타입
├── gateway.rs                # Gateway (프로바이더 라우팅)
├── message.rs                # Message, ToolCall, ToolResult
├── tool_def.rs               # ToolDef (도구 정의)
├── error.rs                  # ProviderError
├── retry.rs                  # RetryConfig, with_retry
├── agent_provider.rs         # AgentProvider 추상화 (NEW)
└── providers/
    ├── mod.rs
    ├── anthropic.rs          # Anthropic Claude
    ├── openai.rs             # OpenAI GPT
    ├── gemini.rs             # Google Gemini
    ├── groq.rs               # Groq (LPU)
    └── ollama.rs             # Ollama (로컬)
```

---

## Part 1: LLM Provider Layer

### Provider Trait

```rust
#[async_trait]
pub trait Provider: Send + Sync {
    fn metadata(&self) -> &ProviderMetadata;
    fn model(&self) -> &ModelInfo;
    
    // 스트리밍
    fn stream(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolDef>,
        system_prompt: Option<String>,
    ) -> Pin<Box<dyn Stream<Item = StreamEvent> + Send + '_>>;
    
    // 논스트리밍
    async fn complete(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolDef>,
        system_prompt: Option<String>,
    ) -> Result<ProviderResponse, ProviderError>;
    
    fn is_available(&self) -> bool;
    fn set_model(&mut self, model_id: &str) -> Result<(), ProviderError>;
}
```

### Message

```rust
// 메시지 생성
let user = Message::user("Hello!");
let assistant = Message::assistant("Hi there!");
let system = Message::system("You are helpful.");

// 도구 호출 포함 메시지
let with_tools = Message::assistant_with_tools(
    "Let me read that file.",
    vec![ToolCall::new("call_1", "read_file", json!({"path": "/test.txt"}))]
);

// 도구 결과
let result = Message::tool_result("call_1", "file contents...", false);
```

### StreamEvent

```rust
match event {
    StreamEvent::Text(text) => print!("{}", text),
    StreamEvent::Thinking(thought) => { /* 사고 과정 */ },
    StreamEvent::ToolCall(tc) => { /* 도구 호출 처리 */ },
    StreamEvent::Usage(usage) => { /* 토큰 사용량 */ },
    StreamEvent::Done => { /* 완료 */ },
    StreamEvent::Error(e) => { /* 에러 */ },
}
```

### Gateway

```rust
// forge-foundation 설정에서 로드
let gateway = Gateway::load()?;

// 기본 프로바이더로 완료
let response = gateway.complete(messages, tools, system_prompt).await?;

// 특정 프로바이더 사용
let response = gateway.complete_with_provider("anthropic", messages, tools, None).await?;

// 자동 fallback
let response = gateway.complete_with_fallback(messages, tools, None).await?;

// 스트리밍
let provider = gateway.get_default_provider_for_stream().await?;
let stream = provider.stream(messages, tools, system_prompt);
```

### 지원 LLM 프로바이더

| 프로바이더 | 모델 예시 | 특징 |
|-----------|----------|------|
| Anthropic | claude-sonnet-4-20250514, claude-opus-4-20250514 | Tool use, Vision, Extended thinking |
| OpenAI | gpt-4o, gpt-4o-mini, o1 | Function calling, Vision |
| Gemini | gemini-2.0-flash, gemini-1.5-pro | 대용량 컨텍스트 (1M+) |
| Groq | llama-3.3-70b-versatile, mixtral-8x7b | 초고속 추론 (LPU) |
| Ollama | llama3, codellama, mistral | 로컬 실행, API key 불필요 |

---

## Part 2: Agent Provider Layer (NEW)

다양한 AI 에이전트 프로바이더(Claude Agent SDK, OpenAI Codex, 자체 Agent)를 통합하는 추상화 레이어입니다.

### 설계 원칙

1. **Provider Agnostic** - 어떤 프로바이더든 동일한 인터페이스로 사용
2. **Tool Mapping** - 각 프로바이더의 도구 형식을 통합 형식으로 변환
3. **Session Portability** - 세션을 프로바이더 간 이전 가능
4. **Graceful Fallback** - 한 프로바이더 실패 시 다른 프로바이더로 폴백

### AgentProviderType

```rust
pub enum AgentProviderType {
    Native,          // ForgeCode 자체 Agent (Layer3)
    ClaudeAgentSdk,  // Claude Agent SDK
    OpenAiCodex,     // OpenAI Codex CLI/API
    GeminiCli,       // Google Gemini CLI
    McpServer,       // 커스텀 MCP 서버
}
```

### AgentProvider Trait

```rust
#[async_trait]
pub trait AgentProvider: Send + Sync {
    fn provider_type(&self) -> AgentProviderType;
    fn name(&self) -> &str;
    fn supported_tools(&self) -> Vec<String>;
    async fn is_available(&self) -> bool;
    
    /// Agent 쿼리 실행
    async fn query(
        &self,
        prompt: &str,
        options: AgentQueryOptions,
    ) -> Result<AgentStream, AgentProviderError>;
    
    /// 세션 재개
    async fn resume_session(
        &self,
        session_id: &str,
        prompt: &str,
        options: AgentQueryOptions,
    ) -> Result<AgentStream, AgentProviderError>;
    
    /// 세션 정보 조회
    async fn get_session_info(&self, session_id: &str) -> Result<SessionInfo, AgentProviderError>;
    
    async fn list_models(&self) -> Vec<String>;
    fn current_model(&self) -> &str;
}
```

### AgentQueryOptions

```rust
pub struct AgentQueryOptions {
    pub allowed_tools: Vec<String>,       // 허용된 도구
    pub permission_mode: PermissionMode,  // 권한 모드
    pub max_turns: Option<u32>,           // 최대 턴 수
    pub working_dir: Option<String>,      // 작업 디렉토리
    pub system_prompt: Option<String>,    // 시스템 프롬프트
    pub resume_session: Option<String>,   // 세션 재개 ID
    pub mcp_servers: HashMap<String, McpServerConfig>,  // MCP 서버
    pub subagents: HashMap<String, SubagentDefinition>, // 서브에이전트
}
```

### AgentStreamEvent

```rust
pub enum AgentStreamEvent {
    SessionStart { session_id, provider },
    Text(String),
    Thinking(String),
    ToolCallStart { tool_use_id, tool_name, arguments },
    ToolCallComplete { tool_use_id, tool_name, result, success, duration_ms },
    SubagentStart { agent_name, parent_tool_use_id },
    SubagentComplete { agent_name, result },
    Usage { input_tokens, output_tokens },
    Done { result, total_turns },
    Error(String),
}
```

### Agent Provider Registry

```rust
let mut registry = AgentProviderRegistry::new();

// 프로바이더 등록
registry.register("native", Box::new(NativeAgentProvider::new(model, config)));
registry.register("claude-sdk", Box::new(ClaudeAgentSdkProvider::from_env()?));
registry.register("codex", Box::new(CodexProvider::from_env()?));

// 기본 프로바이더 설정
registry.set_default("native");

// 프로바이더 사용
let provider = registry.default_provider().unwrap();
let stream = provider.query("Hello", AgentQueryOptions::default()).await?;

// 사용 가능한 프로바이더 확인
let available = registry.available_providers().await;
```

### Tool Name Mapping

프로바이더 간 도구 이름 변환:

| ForgeCode | Claude SDK | OpenAI Codex |
|-----------|------------|--------------|
| `Read` | `Read` | `read_file` |
| `Write` | `Write` | `write_file` |
| `Edit` | `Edit` | `edit_file` |
| `Bash` | `Bash` | `shell` |
| `Glob` | `Glob` | `list_files` |
| `Grep` | `Grep` | `search` |
| `WebSearch` | `WebSearch` | `web_search` |
| `WebFetch` | `WebFetch` | `fetch_url` |
| `Task` | `Task` | `spawn_agent` |

```rust
// ForgeCode → Codex
let codex_name = map_tool_name("Read", AgentProviderType::OpenAiCodex);
// => "read_file"

// Codex → ForgeCode
let forge_name = normalize_tool_name("shell", AgentProviderType::OpenAiCodex);
// => "Bash"
```

---

## 에러 처리

### ProviderError (LLM)

```rust
match error {
    ProviderError::Authentication(_) => { /* API key 문제 */ },
    ProviderError::RateLimited { retry_after_ms } => { /* 재시도 대기 */ },
    ProviderError::ContextLengthExceeded(_) => { /* 컨텍스트 초과 */ },
    ProviderError::ContentFiltered(_) => { /* 콘텐츠 필터링 */ },
    ProviderError::ServerError(_) => { /* 서버 에러 (재시도 가능) */ },
    ProviderError::Network(_) => { /* 네트워크 에러 (재시도 가능) */ },
    ProviderError::ModelNotFound(_) => { /* 모델 없음 */ },
}
```

### AgentProviderError

```rust
match error {
    AgentProviderError::NotAvailable(_) => { /* 프로바이더 사용 불가 */ },
    AgentProviderError::AuthenticationFailed(_) => { /* 인증 실패 */ },
    AgentProviderError::SessionNotFound(_) => { /* 세션 없음 */ },
    AgentProviderError::ToolNotSupported(_) => { /* 도구 미지원 */ },
    AgentProviderError::RateLimitExceeded => { /* Rate limit */ },
    AgentProviderError::NetworkError(_) => { /* 네트워크 에러 */ },
    AgentProviderError::ProviderError(_) => { /* 프로바이더 에러 */ },
}
```

---

## 재시도 설정

```rust
let config = RetryConfig {
    max_retries: 3,
    initial_delay_ms: 1000,
    backoff_multiplier: 2.0,
    max_delay_ms: 30000,
    jitter: true,
};

let gateway = Gateway::load()?.with_retry_config(config);
```

---

## 설정 파일

### providers.json

```json
{
  "default": "anthropic",
  "providers": {
    "anthropic": {
      "type": "anthropic",
      "model": "claude-sonnet-4-20250514",
      "maxTokens": 8192
    },
    "openai": {
      "type": "openai",
      "model": "gpt-4o"
    },
    "groq": {
      "type": "groq",
      "model": "llama-3.3-70b-versatile"
    }
  }
}
```

### 환경 변수

```bash
ANTHROPIC_API_KEY=sk-ant-...
OPENAI_API_KEY=sk-...
GEMINI_API_KEY=...
GROQ_API_KEY=gsk_...
```

---

## 의존성

```
Layer1-foundation
      │
      ▼
Layer2-provider
      │
      ├──► Layer2-core (tools)
      └──► Layer3-agent (via provider_bridge)
```

### 직접 의존

- `forge-foundation` (Layer1): ProviderConfig, 설정 로드

### 역방향 의존 (구현)

- `Layer3-agent`: `ForgeNativeProvider`가 `AgentProvider` trait 구현

---

## 사용 예시

### LLM Provider 사용

```rust
use forge_provider::{Gateway, Message, ToolDef};

let gateway = Gateway::load()?;
let messages = vec![Message::user("Hello!")];
let response = gateway.complete(messages, vec![], None).await?;
```

### Agent Provider 사용

```rust
use forge_provider::{AgentProviderRegistry, NativeAgentProvider, AgentQueryOptions};

let mut registry = AgentProviderRegistry::new();
let native = NativeAgentProvider::new("claude-sonnet-4", config);
registry.register("native", Box::new(native));

let provider = registry.default_provider().unwrap();
let stream = provider.query("Build a web server", AgentQueryOptions {
    allowed_tools: vec!["Read", "Write", "Bash"].into_iter().map(String::from).collect(),
    max_turns: Some(20),
    ..Default::default()
}).await?;

// 스트림 처리
while let Some(event) = stream.next().await {
    match event {
        AgentStreamEvent::Text(t) => print!("{}", t),
        AgentStreamEvent::ToolCallStart { tool_name, .. } => println!("→ {}", tool_name),
        AgentStreamEvent::Done { result, .. } => break,
        _ => {}
    }
}
```
