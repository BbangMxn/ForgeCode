# forge-provider

LLM Provider 추상화 계층 - 다양한 LLM 프로바이더를 통합 인터페이스로 제공
forge-foundation 위에서 작동을 하는 부분
## 구조

```
forge-provider/src/
├── lib.rs                    # 메인 export
├── trait.rs                  # Provider trait, 공통 타입
├── gateway.rs                # Gateway (프로바이더 라우팅)
├── message.rs                # Message, ToolCall, ToolResult
├── tool_def.rs               # ToolDef (도구 정의)
├── error.rs                  # ProviderError
├── retry.rs                  # RetryConfig, with_retry
└── providers/
    ├── mod.rs
    ├── anthropic.rs          # Anthropic Claude
    ├── openai.rs             # OpenAI GPT
    ├── gemini.rs             # Google Gemini
    ├── groq.rs               # Groq (LPU)
    └── ollama.rs             # Ollama (로컬)
```

## 설계 원칙

1. **통합 인터페이스** - 모든 프로바이더가 동일한 `Provider` trait 구현
2. **SSE 스트리밍** - 실시간 응답을 위한 스트리밍 지원
3. **재시도 로직** - 자동 재시도 (rate limit, 네트워크 오류 등)
4. **도구 호출** - Function calling / Tool use 지원

## 핵심 타입

### Provider Trait

```rust
use forge_provider::{Provider, Message, ToolDef, ProviderResponse, StreamEvent};

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
use forge_provider::{Message, MessageRole, ToolCall, ToolResult};

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

### ToolDef

```rust
use forge_provider::ToolDef;

let tool = ToolDef::new("read_file", "Read contents of a file")
    .with_string_param("path", "File path to read", true)
    .with_boolean_param("binary", "Read as binary", false);
```

### StreamEvent

```rust
use forge_provider::StreamEvent;

match event {
    StreamEvent::Text(text) => print!("{}", text),
    StreamEvent::Thinking(thought) => { /* 사고 과정 */ },
    StreamEvent::ToolCall(tc) => { /* 도구 호출 처리 */ },
    StreamEvent::Usage(usage) => { /* 토큰 사용량 */ },
    StreamEvent::Done => { /* 완료 */ },
    StreamEvent::Error(e) => { /* 에러 */ },
}
```

## Gateway 사용

Gateway는 여러 프로바이더를 관리하고 라우팅합니다.

```rust
use forge_provider::Gateway;

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
tokio::pin!(stream);
while let Some(event) = stream.next().await {
    // 이벤트 처리
}
```

## 개별 프로바이더 사용

```rust
use forge_provider::{AnthropicProvider, OpenAiProvider, GeminiProvider, GroqProvider, OllamaProvider};

// Anthropic
let anthropic = AnthropicProvider::new("sk-ant-...", "claude-sonnet-4-20250514", 8192);

// OpenAI
let openai = OpenAiProvider::new("sk-...", "gpt-4o", 4096);

// Gemini
let gemini = GeminiProvider::new("...", "gemini-2.0-flash", 8192);

// Groq
let groq = GroqProvider::new("gsk_...", "llama-3.3-70b-versatile", 8192);

// Ollama (로컬)
let ollama = OllamaProvider::new("http://localhost:11434", "llama3");
```

## 지원 프로바이더

| 프로바이더 | 모델 예시 | 특징 |
|-----------|----------|------|
| Anthropic | claude-sonnet-4-20250514, claude-opus-4-20250514 | Tool use, Vision, Extended thinking |
| OpenAI | gpt-4o, gpt-4o-mini, o1 | Function calling, Vision |
| Gemini | gemini-2.0-flash, gemini-1.5-pro | 대용량 컨텍스트 (1M+) |
| Groq | llama-3.3-70b-versatile, mixtral-8x7b | 초고속 추론 (LPU) |
| Ollama | llama3, codellama, mistral | 로컬 실행, API key 불필요 |

## 에러 처리

```rust
use forge_provider::ProviderError;

match error {
    ProviderError::Authentication(_) => { /* API key 문제 */ },
    ProviderError::RateLimited { retry_after_ms } => { /* 재시도 대기 */ },
    ProviderError::ContextLengthExceeded(_) => { /* 컨텍스트 초과 */ },
    ProviderError::ContentFiltered(_) => { /* 콘텐츠 필터링 */ },
    ProviderError::ServerError(_) => { /* 서버 에러 (재시도 가능) */ },
    ProviderError::Network(_) => { /* 네트워크 에러 (재시도 가능) */ },
    ProviderError::ModelNotFound(_) => { /* 모델 없음 */ },
    _ => { /* 기타 */ },
}
```

## 재시도 설정

```rust
use forge_provider::RetryConfig;

let config = RetryConfig {
    max_retries: 3,
    initial_delay_ms: 1000,
    backoff_multiplier: 2.0,
    max_delay_ms: 30000,
    jitter: true,
};

let gateway = Gateway::load()?.with_retry_config(config);
```

## forge-foundation 연동

`Gateway::load()`는 `forge-foundation`의 `ProviderConfig`를 사용합니다.

```rust
// 환경변수에서 자동 감지
// ANTHROPIC_API_KEY, OPENAI_API_KEY, GEMINI_API_KEY, GROQ_API_KEY

// 또는 설정 파일
// ~/.forgecode/providers.json
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
    }
  }
}
```

## 의존성

```
forge-foundation ──▶ forge-provider
                          │
                          ├── trait.rs (Provider trait)
                          ├── gateway.rs (라우팅)
                          └── providers/ (구현체)
```
