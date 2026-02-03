# ProvidersConfig 설계

## 구현 목록

### 1. 공통 구조체

- [ ] `ProvidersConfig` - 프로바이더 전체 설정
  - default: String (기본 프로바이더)
  - anthropic: Option<AnthropicConfig>
  - openai: Option<OpenAiConfig>
  - gemini: Option<GeminiConfig>
  - ollama: Option<OllamaConfig>
  - groq: Option<GroqConfig>
  - custom: HashMap<String, CustomProviderConfig>

- [ ] `ProviderCommon` - 공통 설정 (trait 또는 struct)
  - api_key: Option<String>
  - base_url: Option<String>
  - model: String
  - max_tokens: u32
  - timeout_secs: u64
  - retry: RetryConfig
  - proxy: Option<ProxyConfig>

---

### 2. Anthropic (Claude)

```rust
pub struct AnthropicConfig {
    // 인증
    pub api_key: String,              // 필수, env: ANTHROPIC_API_KEY
    
    // 모델
    pub model: String,                // 기본: "claude-sonnet-4-20250514"
    pub max_tokens: u32,              // 기본: 8192
    
    // 요청 설정
    pub timeout_secs: u64,            // 기본: 300
    pub retry: RetryConfig,           // 재시도 설정
    
    // 기능 플래그
    pub enable_thinking: bool,        // 확장 사고 (Opus)
    pub enable_caching: bool,         // 프롬프트 캐싱
}
```

**엔드포인트:** `https://api.anthropic.com/v1/messages` (고정)

**헤더:**
- `x-api-key: {api_key}`
- `anthropic-version: 2023-06-01`
- `content-type: application/json`

**지원 모델:**
| 모델 ID | Context | Max Output | Vision | Thinking | 가격 (input/output) |
|---------|---------|------------|--------|----------|---------------------|
| claude-opus-4-20250514 | 200K | 32K | O | O | $15 / $75 |
| claude-sonnet-4-20250514 | 200K | 16K | O | X | $3 / $15 |
| claude-3-5-haiku-20241022 | 200K | 8K | X | X | $0.8 / $4 |

---

### 3. OpenAI

```rust
pub struct OpenAiConfig {
    // 인증
    pub api_key: String,              // 필수, env: OPENAI_API_KEY
    
    // 엔드포인트
    pub base_url: Option<String>,     // 기본: "https://api.openai.com/v1"
                                      // Azure, LocalAI, vLLM 호환
    
    // 모델
    pub model: String,                // 기본: "gpt-4o"
    pub max_tokens: u32,              // 기본: 4096
    
    // 요청 설정
    pub timeout_secs: u64,            // 기본: 300
    pub retry: RetryConfig,
    
    // 옵션
    pub organization: Option<String>, // Organization ID
    pub project: Option<String>,      // Project ID
}
```

**엔드포인트:** `{base_url}/chat/completions`

**헤더:**
- `Authorization: Bearer {api_key}`
- `OpenAI-Organization: {organization}` (선택)
- `OpenAI-Project: {project}` (선택)

**지원 모델:**
| 모델 ID | Context | Max Output | Vision | 가격 (input/output) |
|---------|---------|------------|--------|---------------------|
| gpt-4o | 128K | 4K | O | $2.50 / $10 |
| gpt-4o-mini | 128K | 4K | O | $0.15 / $0.60 |
| o1 | 200K | 100K | O | $15 / $60 |
| o1-mini | 128K | 65K | X | $3 / $12 |
| gpt-4-turbo | 128K | 4K | O | $10 / $30 |

---

### 4. Google Gemini

```rust
pub struct GeminiConfig {
    // 인증
    pub api_key: String,              // 필수, env: GEMINI_API_KEY
    
    // 모델
    pub model: String,                // 기본: "gemini-2.0-flash"
    pub max_tokens: u32,              // 기본: 8192
    
    // 요청 설정
    pub timeout_secs: u64,            // 기본: 300
    pub retry: RetryConfig,
    
    // 안전 설정
    pub safety_settings: Option<SafetySettings>,
}
```

**엔드포인트:** `https://generativelanguage.googleapis.com/v1beta/models/{model}:streamGenerateContent`

**지원 모델:**
| 모델 ID | Context | Max Output | Vision | 가격 (input/output) |
|---------|---------|------------|--------|---------------------|
| gemini-2.0-flash | 1M | 8K | O | $0.075 / $0.30 |
| gemini-1.5-pro | 2M | 8K | O | $1.25 / $5.00 |
| gemini-1.5-flash | 1M | 8K | O | $0.075 / $0.30 |

---

### 5. Ollama (로컬)

```rust
pub struct OllamaConfig {
    // 연결
    pub base_url: String,             // 기본: "http://localhost:11434"
                                      // env: OLLAMA_HOST
    
    // 모델
    pub model: String,                // 기본: "llama3"
    
    // 요청 설정
    pub timeout_secs: u64,            // 기본: 600 (로컬 모델은 느림)
    pub retry: RetryConfig,
    
    // 모델 옵션
    pub num_ctx: Option<u32>,         // 컨텍스트 윈도우
    pub num_gpu: Option<i32>,         // GPU 레이어 수 (-1: all)
    pub temperature: Option<f32>,
}
```

**엔드포인트:**
- Chat: `{base_url}/api/chat`
- List: `{base_url}/api/tags`
- Ping: `{base_url}/api/tags`

**인증:** 없음 (로컬)

---

### 6. Groq (추가)

```rust
pub struct GroqConfig {
    // 인증
    pub api_key: String,              // 필수, env: GROQ_API_KEY
    
    // 모델
    pub model: String,                // 기본: "llama-3.3-70b-versatile"
    pub max_tokens: u32,              // 기본: 8192
    
    // 요청 설정
    pub timeout_secs: u64,            // 기본: 60 (Groq는 빠름)
    pub retry: RetryConfig,
}
```

**엔드포인트:** `https://api.groq.com/openai/v1/chat/completions` (OpenAI 호환)

---

### 7. Custom Provider (OpenAI 호환)

```rust
pub struct CustomProviderConfig {
    // 연결
    pub base_url: String,             // 필수
    pub api_key: Option<String>,      // 선택
    
    // 인증 방식
    pub auth_type: AuthType,          // Bearer, ApiKey, None
    pub auth_header: Option<String>,  // 커스텀 헤더명
    
    // 모델
    pub model: String,
    pub max_tokens: u32,
    
    // 요청 설정
    pub timeout_secs: u64,
    pub retry: RetryConfig,
    
    // API 형식
    pub api_format: ApiFormat,        // OpenAI, Anthropic, Custom
    
    // 추가 헤더
    pub headers: HashMap<String, String>,
}

pub enum AuthType {
    Bearer,         // Authorization: Bearer {key}
    ApiKey,         // x-api-key: {key}
    Header(String), // {header}: {key}
    None,
}

pub enum ApiFormat {
    OpenAI,         // OpenAI 호환 API
    Anthropic,      // Anthropic 호환 API
}
```

**사용 예:**
- Azure OpenAI
- LocalAI
- vLLM
- LMStudio
- Together AI
- Fireworks AI

---

### 8. 공통 설정

#### RetryConfig
```rust
pub struct RetryConfig {
    pub max_retries: u32,             // 기본: 3
    pub initial_delay_ms: u64,        // 기본: 1000
    pub backoff_multiplier: f64,      // 기본: 2.0
    pub max_delay_ms: u64,            // 기본: 30000
    pub jitter: bool,                 // 기본: true
}
```

#### ProxyConfig
```rust
pub struct ProxyConfig {
    pub http: Option<String>,         // HTTP 프록시
    pub https: Option<String>,        // HTTPS 프록시
    pub no_proxy: Vec<String>,        // 프록시 제외 호스트
}
```

---

### 9. 환경변수 매핑

| Provider | 환경변수 | 설정 경로 |
|----------|----------|-----------|
| Anthropic | `ANTHROPIC_API_KEY` | `providers.anthropic.api_key` |
| OpenAI | `OPENAI_API_KEY` | `providers.openai.api_key` |
| OpenAI | `OPENAI_BASE_URL` | `providers.openai.base_url` |
| OpenAI | `OPENAI_ORG_ID` | `providers.openai.organization` |
| Gemini | `GEMINI_API_KEY` | `providers.gemini.api_key` |
| Ollama | `OLLAMA_HOST` | `providers.ollama.base_url` |
| Groq | `GROQ_API_KEY` | `providers.groq.api_key` |
| 공통 | `HTTP_PROXY` | `providers.*.proxy.http` |
| 공통 | `HTTPS_PROXY` | `providers.*.proxy.https` |

---

### 10. 설정 파일 예시

```toml
[providers]
default = "anthropic"

[providers.anthropic]
api_key = "${ANTHROPIC_API_KEY}"
model = "claude-sonnet-4-20250514"
max_tokens = 8192
timeout_secs = 300
enable_thinking = false
enable_caching = true

[providers.anthropic.retry]
max_retries = 3
initial_delay_ms = 1000
backoff_multiplier = 2.0

[providers.openai]
api_key = "${OPENAI_API_KEY}"
model = "gpt-4o"
max_tokens = 4096
# base_url = "https://api.openai.com/v1"  # 기본값
# organization = "org-xxx"  # 선택

[providers.ollama]
base_url = "http://localhost:11434"
model = "llama3"
timeout_secs = 600
num_ctx = 8192
num_gpu = -1

[providers.groq]
api_key = "${GROQ_API_KEY}"
model = "llama-3.3-70b-versatile"
timeout_secs = 60

# Azure OpenAI (커스텀)
[providers.custom.azure]
base_url = "https://myresource.openai.azure.com/openai/deployments/gpt-4"
api_key = "${AZURE_OPENAI_KEY}"
auth_type = "bearer"
api_format = "openai"
model = "gpt-4"
max_tokens = 4096

[providers.custom.azure.headers]
"api-version" = "2024-02-15-preview"

# LocalAI (커스텀)
[providers.custom.localai]
base_url = "http://localhost:8080/v1"
api_format = "openai"
auth_type = "none"
model = "mistral-7b"
max_tokens = 4096
timeout_secs = 600
```

---

## 아키텍처

```
ProvidersConfig
├── default: String
├── anthropic: Option<AnthropicConfig>
├── openai: Option<OpenAiConfig>
├── gemini: Option<GeminiConfig>
├── ollama: Option<OllamaConfig>
├── groq: Option<GroqConfig>
└── custom: HashMap<String, CustomProviderConfig>
         │
         ▼
    ┌─────────────────┐
    │ Provider Trait  │  (forge-provider)
    ├─────────────────┤
    │ stream()        │
    │ complete()      │
    │ is_available()  │
    └─────────────────┘
         │
         ▼
    ┌─────────────────┐
    │    Gateway      │
    ├─────────────────┤
    │ providers: Map  │
    │ default         │
    │ fallback chain  │
    └─────────────────┘
```

---

## 검증 규칙

- [ ] `api_key` 필수 체크 (클라우드 프로바이더)
- [ ] `base_url` 형식 검증 (URL)
- [ ] `model` 지원 여부 체크
- [ ] `max_tokens` 범위 체크 (모델별 한계)
- [ ] `timeout_secs` 최소값 체크 (> 0)
- [ ] 환경변수 참조 `${VAR}` 해석
