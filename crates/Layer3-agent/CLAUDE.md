# forge-agent (Layer3-agent)

Agent 시스템 - Claude Code / OpenCode / Gemini CLI 스타일의 단순하고 효율적인 Agent Loop 구현

## 설계 원칙

1. **Simple is Better** - 복잡한 4단계 loop 대신 단순한 `while(tool_call)` loop
2. **Single Thread** - Multi-agent swarm 대신 single loop + sub-agent dispatch
3. **Flat History** - 단순한 MessageHistory 관리
4. **Sequential Tools** - 병렬 Tool 실행은 sub-agent로 위임
5. **Hook System** - 확장성을 위한 훅 기반 아키텍처

---

## 아키텍처

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           Layer3-agent 구조                                  │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │                         Agent Loop                                     │  │
│  │                                                                        │  │
│  │   User Input → hooks.before_agent()                                    │  │
│  │       → Main Loop:                                                     │  │
│  │           1. Check context (>92%? → compress)                          │  │
│  │           2. Check steering (paused? stopped?)                         │  │
│  │           3. provider.stream(history, tools)                           │  │
│  │           4. Process stream events                                     │  │
│  │           5. No tool calls? → break                                    │  │
│  │           6. For each tool:                                            │  │
│  │              - hooks.before_tool()                                     │  │
│  │              - execute()                                               │  │
│  │              - hooks.after_tool()                                      │  │
│  │           7. Continue loop                                             │  │
│  │       → hooks.after_agent()                                            │  │
│  │   → Return Response                                                    │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                                                                              │
│  ┌─────────────────┐ ┌─────────────────┐ ┌─────────────────┐                │
│  │  Hook System    │ │   Compressor    │ │   Steering      │                │
│  │                 │ │                 │ │                 │                │
│  │  - before_agent │ │  - 92% threshold│ │  - pause/resume │                │
│  │  - after_agent  │ │  - auto-compress│ │  - stop         │                │
│  │  - before_tool  │ │  - LLM summary  │ │  - redirect     │                │
│  │  - after_tool   │ │  - token saving │ │  - inject       │                │
│  └─────────────────┘ └─────────────────┘ └─────────────────┘                │
│                                                                              │
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │                      Provider Bridge                                   │  │
│  │                                                                        │  │
│  │   ForgeNativeProvider: Layer3 Agent → AgentProvider interface          │  │
│  │   - 다른 프로바이더(Claude SDK, Codex)와 동일한 인터페이스              │  │
│  │   - AgentEvent → AgentStreamEvent 변환                                 │  │
│  │   - 세션 관리 및 토큰 추적                                              │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 모듈 구조

```
Layer3-agent/src/
├── lib.rs                    # 메인 export 및 prelude
├── agent.rs                  # 핵심 Agent Loop 구현
├── context.rs                # AgentContext (공유 상태)
├── history.rs                # MessageHistory (대화 기록)
├── session.rs                # Session, SessionManager
│
├── hook.rs                   # Hook 시스템
├── compressor.rs             # 컨텍스트 압축
├── steering.rs               # 실시간 제어 (h2A 스타일)
├── provider_bridge.rs        # AgentProvider 브릿지
├── recovery.rs               # 에러 복구
├── optimizer.rs              # 컨텍스트 최적화 (레거시)
│
├── bench/                    # 벤치마크
│   ├── mod.rs
│   ├── metrics.rs            # 성능 메트릭
│   ├── runner.rs             # 벤치 실행기
│   ├── report.rs             # 결과 리포트
│   └── scenario.rs           # 테스트 시나리오
│
├── runtime/                  # 레거시 런타임 (deprecated)
│   ├── mod.rs
│   ├── traits.rs
│   ├── context.rs
│   ├── output.rs
│   └── lifecycle.rs
│
├── strategy/                 # 레거시 전략 (deprecated)
│   ├── mod.rs
│   ├── reasoning.rs          # ReAct, CoT, ToT
│   ├── planning.rs           # Planning strategies
│   ├── execution.rs          # Execution strategies
│   └── memory.rs             # Memory strategies
│
└── variant/                  # 레거시 에이전트 변형 (deprecated)
    ├── mod.rs
    ├── registry.rs
    ├── builder.rs
    ├── classic.rs
    ├── react.rs
    ├── reflexion.rs
    └── tree_search.rs
```

---

## 핵심 컴포넌트

### 1. Agent (`agent.rs`)

핵심 에이전트 루프를 구현합니다.

```rust
use forge_agent::{Agent, AgentConfig, AgentEvent};

// Agent 생성
let agent = Agent::with_config(ctx, AgentConfig::default())
    .with_hook(LoggingHook::new())
    .with_max_iterations(50);

// Steering handle로 외부 제어
let handle = agent.steering_handle();

// 실행
let (tx, mut rx) = tokio::sync::mpsc::channel(100);
let response = agent.run(session_id, &mut history, "Hello", tx).await?;

// 이벤트 처리
while let Some(event) = rx.recv().await {
    match event {
        AgentEvent::Text(text) => print!("{}", text),
        AgentEvent::ToolStart { tool_name, .. } => println!("→ {}", tool_name),
        AgentEvent::Done { full_response } => break,
        _ => {}
    }
}
```

#### AgentConfig

| 설정 | 기본값 | 설명 |
|------|--------|------|
| `max_iterations` | 50 | 최대 반복 횟수 |
| `compressor_config` | Claude Code style | 압축 설정 |
| `auto_compress` | true | 자동 압축 활성화 |
| `streaming` | true | 스트리밍 활성화 |

#### 프리셋

- `AgentConfig::default()` - 일반 사용
- `AgentConfig::fast()` - 빠른 실행 (max 20, 공격적 압축)
- `AgentConfig::long_session()` - 긴 세션 (max 100, 보수적 압축)

---

### 2. Hook System (`hook.rs`)

확장 가능한 훅 시스템으로 Agent 동작을 커스터마이즈합니다.

```rust
use forge_agent::hook::{AgentHook, HookResult, ToolResult, TurnInfo};

#[async_trait]
pub trait AgentHook: Send + Sync {
    fn name(&self) -> &str;
    
    // 라이프사이클 훅
    async fn before_agent(&self, history: &MessageHistory) -> Result<HookResult>;
    async fn after_agent(&self, history: &MessageHistory, response: &str, turn_info: &TurnInfo) -> Result<HookResult>;
    
    // 턴 훅
    async fn before_turn(&self, history: &MessageHistory, turn: u32) -> Result<HookResult>;
    async fn after_turn(&self, history: &MessageHistory, turn: u32, response: &str) -> Result<HookResult>;
    
    // Tool 훅
    async fn before_tool(&self, tool_call: &ToolCall, history: &MessageHistory) -> Result<HookResult>;
    async fn after_tool(&self, tool_call: &ToolCall, result: &ToolResult, history: &MessageHistory) -> Result<HookResult>;
    
    // 압축 훅
    async fn before_compress(&self, history: &MessageHistory) -> Result<HookResult>;
    async fn after_compress(&self, history: &MessageHistory, tokens_saved: usize) -> Result<HookResult>;
    
    // 에러 훅
    async fn on_error(&self, error: &Error, history: &MessageHistory) -> Result<HookResult>;
}
```

#### HookResult

```rust
pub enum HookResult {
    Continue,                           // 계속 진행
    Stop { reason: String },            // 실행 중단
    Block { response: String },         // 차단 (합성 응답)
    ModifyAndContinue { modified: String }, // 수정 후 계속
}
```

#### 내장 Hook

| Hook | 설명 |
|------|------|
| `LoggingHook` | 모든 이벤트 로깅 |
| `TokenTrackingHook` | 토큰 사용량 추적 및 제한 |

---

### 3. Context Compressor (`compressor.rs`)

Claude Code 스타일의 자동 컨텍스트 압축 시스템입니다.

```rust
use forge_agent::compressor::{ContextCompressor, CompressorConfig};

// 기본 설정
let compressor = ContextCompressor::new(CompressorConfig::claude_code_style());

// 압축 필요 여부 확인
if compressor.needs_compression(&history) {
    let result = compressor.compress(&mut history)?;
    println!("Saved {} tokens", result.tokens_saved);
}
```

#### CompressorConfig

| 프리셋 | threshold | target | keep_recent |
|--------|-----------|--------|-------------|
| `claude_code_style()` | 92% | 50% | 10 |
| `aggressive()` | 80% | 30% | 5 |
| `conservative()` | 95% | 70% | 20 |

#### 압축 동작

1. 토큰 사용량이 threshold(92%) 초과 시 트리거
2. 오래된 메시지 요약 생성
3. Tool 결과 축약
4. 최근 N개 메시지 유지
5. 목표 사용률(50%)까지 압축

---

### 4. Steering System (`steering.rs`)

Claude Code의 h2A 스타일 실시간 스티어링 시스템입니다.

```rust
use forge_agent::steering::{SteeringHandle, SteeringQueue};

let queue = SteeringQueue::new();
let handle = queue.handle();  // 외부 제어용
let checker = queue.checker(); // Agent 내부용

// 외부에서 제어
handle.pause().await?;           // 일시 중단
handle.resume().await?;          // 재개
handle.stop("User canceled").await?;  // 중단
handle.redirect("Focus on tests").await?;  // 방향 전환
handle.inject_context("New info").await?;  // 컨텍스트 주입

// 상태 조회
let status = handle.query_status().await?;
println!("Turn: {}, State: {:?}", status.current_turn, status.state);
```

#### AgentState

```rust
pub enum AgentState {
    Idle,           // 대기 중
    Running,        // 실행 중
    Paused,         // 일시 중단
    ExecutingTool,  // Tool 실행 중
    WaitingForLlm,  // LLM 응답 대기
    Completed,      // 완료
    Error,          // 에러
}
```

---

### 5. Provider Bridge (`provider_bridge.rs`)

Layer3 Agent를 Layer2 AgentProvider 인터페이스로 래핑합니다.

```rust
use forge_agent::provider_bridge::{ForgeNativeProvider, build_provider_registry};

// Native provider 생성
let provider = ForgeNativeProvider::new(agent_ctx);

// AgentProvider 인터페이스로 사용
let stream = provider.query("Hello", AgentQueryOptions::default()).await?;

// Provider registry 구축
let registry = build_provider_registry(ctx);
```

#### 이벤트 변환

| AgentEvent | AgentStreamEvent |
|------------|------------------|
| `Text(s)` | `Text(s)` |
| `ToolStart{..}` | `ToolCallStart{..}` |
| `ToolComplete{..}` | `ToolCallComplete{..}` |
| `Usage{..}` | `Usage{..}` |
| `Done{..}` | `Done{..}` |
| `Error(e)` | `Error(e)` |

---

### 6. Error Recovery (`recovery.rs`)

Tool 실행 실패 시 자동 복구를 시도합니다.

```rust
use forge_agent::recovery::{ErrorRecovery, RecoveryStrategy, RecoveryAction};

let recovery = ErrorRecovery::new()
    .with_strategy(FileNotFoundRecovery)
    .with_strategy(RateLimitRecovery)
    .with_strategy(TimeoutRecovery);

let action = recovery.handle_error(tool, input, error, &mut ctx).await;

match action {
    RecoveryAction::Retry { modified_input, .. } => { /* 재시도 */ }
    RecoveryAction::UseFallback { tool, input, .. } => { /* 대체 도구 */ }
    RecoveryAction::WaitAndRetry { delay_ms, .. } => { /* 대기 후 재시도 */ }
    RecoveryAction::AskUser { question, .. } => { /* 사용자에게 질문 */ }
    RecoveryAction::GiveUp { suggestions, .. } => { /* 포기 */ }
}
```

#### 내장 Recovery Strategy

| Strategy | 처리 에러 |
|----------|----------|
| `FileNotFoundRecovery` | 파일 없음 |
| `PermissionDeniedRecovery` | 권한 거부 |
| `RateLimitRecovery` | Rate limit |
| `TimeoutRecovery` | 타임아웃 |
| `EditConflictRecovery` | 편집 충돌 |

---

## AgentEvent

Agent가 실행 중 방출하는 이벤트입니다.

```rust
pub enum AgentEvent {
    Thinking,                                    // LLM 처리 중
    Text(String),                                // 텍스트 응답
    ToolStart { tool_name, tool_call_id },       // Tool 시작
    ToolComplete { tool_name, tool_call_id, result, success, duration_ms },
    TurnStart { turn },                          // 턴 시작
    TurnComplete { turn },                       // 턴 완료
    Usage { input_tokens, output_tokens },       // 토큰 사용량
    Compressed { tokens_before, tokens_after, tokens_saved },
    Paused,                                      // 일시 중단
    Resumed,                                     // 재개
    Stopped { reason },                          // 중단
    Done { full_response },                      // 완료
    Error(String),                               // 에러
}
```

---

## 의존성

```
Layer1-foundation
      │
      ▼
Layer2-core ◄────────────┐
      │                  │
      ▼                  │
Layer2-provider          │
      │                  │
      ▼                  │
Layer3-agent ────────────┘
      │
      ▼
Layer4-cli (TUI)
```

### 직접 의존

- `forge-foundation` (Layer1): 설정, 권한, 에러
- `forge-core` (Layer2): Tool 레지스트리, ToolContext
- `forge-provider` (Layer2): LLM Gateway, AgentProvider

---

## 사용 예시

### 기본 사용

```rust
use forge_agent::prelude::*;

// Context 생성
let ctx = Arc::new(AgentContext::new(gateway, tools, system_prompt));

// Agent 실행
let agent = Agent::new(ctx);
let (tx, mut rx) = mpsc::channel(100);
let response = agent.run("session-1", &mut history, "Hello!", tx).await?;
```

### Hook과 함께 사용

```rust
let agent = Agent::with_config(ctx, AgentConfig::default())
    .with_hook(LoggingHook::new().with_tool_args())
    .with_hook(TokenTrackingHook::new().with_limits(100000, 10000));
```

### Steering과 함께 사용

```rust
let agent = Agent::new(ctx);
let handle = agent.steering_handle();

// 백그라운드에서 실행
tokio::spawn(async move {
    agent.run(session_id, &mut history, prompt, tx).await
});

// 사용자 입력 대기
if user_pressed_escape() {
    handle.stop("User canceled").await?;
}

if user_typed_redirect() {
    handle.redirect("Focus on the main function").await?;
}
```

---

## 레거시 모듈 (Deprecated)

다음 모듈들은 호환성을 위해 유지되지만, 새 시스템 사용을 권장합니다:

- `runtime/` - 복잡한 4단계 런타임 → `agent.rs` 사용
- `strategy/` - 전략 패턴 → Hook 시스템 사용
- `variant/` - 에이전트 변형 → 단일 Agent + Hook 조합

---

## 벤치마크

```bash
cargo bench -p forge-agent
```

### 메트릭

- `turns_per_task` - 작업당 평균 턴 수
- `tokens_per_task` - 작업당 토큰 사용량
- `tool_success_rate` - Tool 성공률
- `compression_efficiency` - 압축 효율
- `response_time_ms` - 응답 시간
