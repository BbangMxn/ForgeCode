# ForgeCode 시스템 아키텍처

## 설계 철학

macOS의 계층화된 시스템 구조에서 영감을 받아, 각 기능을 독립적인 크레이트로 분리하고 명확한 의존성 방향을 유지합니다.

```
┌─────────────────────────────────────────────────────────┐
│                    forge-cli (Layer 4)                  │
│              사용자 인터페이스 (TUI/CLI)                 │
├─────────────────────────────────────────────────────────┤
│                   forge-agent (Layer 3)                 │
│          에이전트 루프, 세션 관리, 대화 히스토리          │
├──────────────────┬──────────────────┬───────────────────┤
│  forge-provider  │   forge-core     │    forge-task     │
│    (Layer 2)     │    (Layer 2)     │     (Layer 2)     │
│   LLM API 연결   │   도구 레지스트리  │    작업 실행/격리  │
├──────────────────┴──────────────────┴───────────────────┤
│                forge-foundation (Layer 1)               │
│        설정, 에러, 권한, 저장소, 이벤트 버스             │
└─────────────────────────────────────────────────────────┘
```

---

## forge-foundation (Layer 1)

**기반 인프라 계층** - 모든 크레이트가 의존하는 공통 기능

### 모듈 구성

| 파일 | 역할 |
|------|------|
| `config.rs` | TOML 기반 설정 관리, 환경변수 오버라이드, Provider/Execution/MCP 설정 |
| `error.rs` | 공통 에러 타입 (`Error` enum, `Result` type alias) |
| `event.rs` | Pub/Sub 이벤트 버스 (tokio broadcast channel 기반) |
| `permission.rs` | 권한 요청/응답 시스템, 도구 실행 전 사용자 승인 |
| `storage.rs` | SQLite 기반 영속화 (세션, 메시지, 권한, 토큰 사용량) |

### 주요 타입

```rust
// 설정 구조
Config {
    providers: ProvidersConfig,  // Anthropic, OpenAI, Ollama 설정
    execution: ExecutionConfig,  // 실행 모드 (Local/Container)
    mcp: McpConfig,              // MCP 서버 연결
}

// 저장소 레코드
SessionRecord    // 세션 정보 + 토큰 사용량/비용 추적
MessageRecord    // 메시지 + 토큰 수 + finish_reason
PermissionRecord // 도구 권한 (session/permanent 스코프)
ProviderRecord   // 프로바이더 설정 저장
TokenUsageRecord // 토큰 사용 기록
```

---

## forge-provider (Layer 2)

**LLM 프로바이더 추상화 계층** - 여러 LLM API를 통합 인터페이스로 제공

### 모듈 구성

| 파일 | 역할 |
|------|------|
| `trait.rs` | `Provider` 트레이트, `ModelInfo`, `StreamEvent`, `TokenUsage` 정의 |
| `error.rs` | 프로바이더별 에러 (`ProviderError`), 재시도 분류 |
| `retry.rs` | 지수 백오프 + 지터, `RetryConfig`, `with_retry()` |
| `gateway.rs` | 멀티 프로바이더 라우팅, 폴백 지원 |
| `message.rs` | `Message`, `ToolCall`, `ToolResult` 타입 |
| `tool_def.rs` | LLM에 전달할 도구 정의 스키마 |
| `providers/` | 개별 프로바이더 구현 |

### Provider 트레이트

```rust
#[async_trait]
pub trait Provider: Send + Sync {
    fn metadata(&self) -> &ProviderMetadata;
    fn model(&self) -> &ModelInfo;
    
    // SSE 스트리밍 응답
    fn stream(&self, messages, tools, system_prompt) 
        -> Pin<Box<dyn Stream<Item = StreamEvent> + Send + '_>>;
    
    // 완전한 응답 (비스트리밍)
    async fn complete(&self, messages, tools, system_prompt) 
        -> Result<ProviderResponse, ProviderError>;
    
    fn is_available(&self) -> bool;
    fn set_model(&mut self, model_id: &str) -> Result<(), ProviderError>;
}
```

### 지원 프로바이더

| 프로바이더 | 파일 | 특징 |
|-----------|------|------|
| **Anthropic** | `anthropic.rs` | Claude 모델, thinking 지원, 캐시 토큰 추적 |
| **OpenAI** | `openai.rs` | GPT-4o/o1 모델, vision 지원, 가격 정보 내장 |
| **Ollama** | `ollama.rs` | 로컬 모델, NDJSON 스트리밍, `ping()`/`list_models()` |

### StreamEvent 종류

```rust
pub enum StreamEvent {
    Text(String),           // 텍스트 청크
    Thinking(String),       // 추론 과정 (Claude)
    ToolCallStart { .. },   // 도구 호출 시작
    ToolCallDelta { .. },   // 도구 인자 청크
    ToolCall(ToolCall),     // 완성된 도구 호출
    Usage(TokenUsage),      // 토큰 사용량
    Done,                   // 스트림 종료
    Error(ProviderError),   // 에러 발생
}
```

---

## forge-tool (Layer 2)

**도구 레지스트리 계층** - LLM이 사용할 수 있는 도구 관리

### 모듈 구성

| 파일 | 역할 |
|------|------|
| `trait.rs` | `Tool` 트레이트, `ToolContext`, `ToolResult` |
| `registry.rs` | 도구 등록/조회, 빌트인 도구 초기화 |
| `builtin/` | 기본 제공 도구들 |

### Tool 트레이트

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> ToolParameters;
    
    async fn execute(&self, ctx: &ToolContext, args: Value) -> ToolResult;
    
    // 권한 필요 여부 (기본: true)
    fn requires_permission(&self) -> bool { true }
}
```

### 빌트인 도구

| 도구 | 파일 | 기능 |
|------|------|------|
| `bash` | `bash.rs` | 셸 명령 실행, 위험 명령 차단 (`rm -rf /` 등) |
| `read` | `read.rs` | 파일 읽기, 라인 범위 지정 가능 |
| `write` | `write.rs` | 파일 쓰기, 디렉토리 자동 생성 |
| `edit` | `edit.rs` | 파일 편집 (old_string → new_string 치환) |
| `glob` | `glob.rs` | 파일 패턴 매칭 |
| `grep` | `grep.rs` | 파일 내용 검색 |

---

## forge-task (Layer 2)

**작업 실행/격리 계층** - 도구 실행을 안전하게 관리

### 모듈 구성

| 파일 | 역할 |
|------|------|
| `task.rs` | `Task` 구조체, `TaskId`, `ExecutionMode` |
| `state.rs` | 작업 상태 머신 (`Pending` → `Running` → `Completed/Failed/Timeout`) |
| `manager.rs` | 작업 큐 관리, 동시 실행 제한 |
| `executor/` | 실행기 구현 |

### 실행 모드

```rust
pub enum ExecutionMode {
    Local,                    // 직접 실행 (권한 시스템으로 보호)
    Container {               // Docker 컨테이너 격리
        image: String,
        memory_limit: Option<String>,
        cpu_limit: Option<f64>,
        network: bool,
    },
}
```

### 실행기 (Executor)

| 실행기 | 파일 | 특징 |
|--------|------|------|
| `LocalExecutor` | `local.rs` | 직접 프로세스 실행, 타임아웃 지원 |
| `ContainerExecutor` | `container.rs` | Docker API (Bollard), 리소스 제한 |

### TaskManager

```rust
impl TaskManager {
    pub async fn submit(&self, task: Task) -> TaskId;  // 작업 제출
    pub async fn get(&self, task_id: TaskId) -> Option<Task>;
    pub async fn cancel(&self, task_id: TaskId) -> Result<()>;
    pub async fn list_by_session(&self, session_id: &str) -> Vec<Task>;
}
```

---

## forge-agent (Layer 3)

**에이전트 루프 계층** - LLM과 도구 실행의 반복적 상호작용

### 모듈 구성

| 파일 | 역할 |
|------|------|
| `agent.rs` | 핵심 에이전트 루프, 이벤트 발행 |
| `context.rs` | `AgentContext` - Gateway, ToolRegistry, TaskManager 통합 |
| `session.rs` | 세션 생성/관리, Storage 연동 |
| `history.rs` | 대화 히스토리, 토큰 추정, 컨텍스트 윈도우 관리 |

### 에이전트 루프

```
사용자 입력
    ↓
┌─────────────────────────────────────┐
│  1. 히스토리에 사용자 메시지 추가     │
│  2. LLM 호출 (스트리밍)              │
│  3. 응답 처리                        │
│     - 텍스트 → 사용자에게 출력       │
│     - 도구 호출 → 4번으로            │
│  4. 도구 실행                        │
│     - 권한 확인                      │
│     - TaskManager로 실행             │
│     - 결과를 히스토리에 추가          │
│  5. 도구 호출이 있었다면 2번으로      │
└─────────────────────────────────────┘
    ↓
최종 응답
```

### AgentEvent

```rust
pub enum AgentEvent {
    Thinking,                              // 처리 중
    Text(String),                          // 텍스트 청크
    ToolStart { tool_name, tool_call_id }, // 도구 실행 시작
    ToolComplete { tool_name, result, .. },// 도구 실행 완료
    Done { full_response },                // 완료
    Error(String),                         // 에러
    Usage { input_tokens, output_tokens }, // 토큰 사용량
}
```

---

## forge-cli (Layer 4)

**사용자 인터페이스 계층** - TUI와 CLI 명령어

### 모듈 구성

| 파일/디렉토리 | 역할 |
|--------------|------|
| `main.rs` | 진입점, Clap 기반 CLI 파싱 |
| `cli.rs` | 비대화형 CLI 모드 |
| `tui/` | Ratatui 기반 TUI |

### TUI 구조

```
tui/
├── app.rs           # 메인 TUI 애플리케이션
├── event.rs         # 키보드/마우스 이벤트 처리
├── theme.rs         # 색상 테마
├── components/      # 재사용 위젯
│   ├── input.rs     # 입력 박스
│   └── message_list.rs  # 메시지 목록
└── pages/           # 페이지
    └── chat.rs      # 채팅 페이지
```

---

## 데이터 흐름

### 1. 사용자 메시지 처리

```
[CLI/TUI] 
    → Agent.run(user_message)
        → MessageHistory.add_user(message)
        → Gateway.get_default_provider()
        → Provider.stream(messages, tools, system_prompt)
            ← StreamEvent::Text/ToolCall/...
        → (도구 호출 시) ToolRegistry.get(tool_name)
            → PermissionService.check()
            → TaskManager.submit(task)
            → Executor.execute(task)
            ← TaskResult
        → MessageHistory.add_tool_result()
        → (반복)
    ← AgentEvent::Done { full_response }
```

### 2. 설정 로드

```
Config::load()
    → 기본값 적용
    → ~/.forgecode/config.toml 로드 (있으면)
    → 환경변수 오버라이드
        - ANTHROPIC_API_KEY
        - OPENAI_API_KEY
        - OLLAMA_HOST
    → Config 반환
```

### 3. 저장소 스키마

```sql
sessions (id, title, provider, model, total_tokens, cost, ...)
    ↓ 1:N
messages (id, session_id, role, content, tool_calls, tokens, ...)
    ↓ 1:N  
tool_executions (id, session_id, message_id, tool_name, status, ...)

permissions (tool_name, pattern, scope, session_id, expires_at)
providers (id, provider_type, api_key_env, base_url, ...)
token_usage (provider, model, input_tokens, output_tokens, cost, ...)
```

---

## 확장 포인트

### 새 프로바이더 추가

1. `forge-provider/src/providers/`에 새 파일 생성
2. `Provider` 트레이트 구현
3. `providers/mod.rs`에 export 추가
4. `gateway.rs`에서 설정 기반 초기화 추가

### 새 도구 추가

1. `forge-tool/src/builtin/`에 새 파일 생성
2. `Tool` 트레이트 구현
3. `builtin/mod.rs`에 export 추가
4. `registry.rs`의 `ToolRegistry::with_builtins()`에 등록

### 새 실행기 추가

1. `forge-task/src/executor/`에 새 파일 생성
2. `Executor` 트레이트 구현
3. `ExecutionMode` enum에 새 변형 추가
4. `TaskManager`에서 선택 로직 추가
