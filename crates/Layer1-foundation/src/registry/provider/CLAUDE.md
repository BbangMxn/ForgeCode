# Config 모듈 설계

## 구현 목록

### 1. 핵심 구조체

- [ ] `Config` - 최상위 설정
- [ ] `PathsConfig` - 경로 설정 (data_dir, working_dir)
- [ ] `ProvidersConfig` - 프로바이더 설정
- [ ] `ModelsConfig` - 모델 정보 + 가격
- [ ] `ExecutionConfig` - 실행 환경
- [ ] `ToolsConfig` - 도구 설정
- [ ] `McpConfig` - MCP 서버 설정
- [ ] `TuiConfig` - UI 설정
- [ ] `LimitsConfig` - 사용량/예산 제한

---

### 2. 프로바이더 설정

- [ ] `ProviderConfig` - 공통 프로바이더 설정 트레이트
- [ ] `AnthropicConfig`
- [ ] `OpenAiConfig`
- [ ] `GeminiConfig`
- [ ] `OllamaConfig`
- [ ] `GroqConfig` (추가 고려)
- [ ] `CustomProviderConfig` - 커스텀 OpenAI 호환 API

---

### 3. 모델 정보 + 가격

- [ ] `ModelInfo` - 모델 메타데이터
  - id, display_name
  - context_window, max_output_tokens
  - supports_tools, supports_vision, supports_thinking
- [ ] `ModelPricing` - 가격 정보
  - input_price_per_1m
  - output_price_per_1m
  - cache_read_price_per_1m (선택)
  - cache_write_price_per_1m (선택)
- [ ] 기본 모델 가격 테이블 (하드코딩 → 설정으로 이동)

---

### 4. 실행 환경

- [ ] `ExecutionMode` - Local / Container / Ask
- [ ] `ContainerConfig`
  - default_image
  - memory_limit, cpu_limit
  - timeout_secs
  - network_enabled
- [ ] `LocalConfig`
  - allowed_paths
  - blocked_paths
  - blocked_commands
  - confirm_commands

---

### 5. 보안/제한

- [ ] `LimitsConfig`
  - max_tokens_per_request
  - max_tokens_per_session
  - max_cost_per_session (USD)
  - max_cost_per_day (USD)
  - max_cost_per_month (USD)
- [ ] `SecurityConfig` (LocalConfig에서 분리 고려)
  - blocked_paths
  - blocked_commands
  - require_permission_for

---

### 6. MCP 설정

- [ ] `McpConfig` - MCP 전체 설정
  - servers: HashMap<String, McpServerConfig>
  - default_timeout_secs
  - auto_start: bool

- [ ] `McpServerConfig` - 개별 서버 설정
  - server_type: Local / Remote / Stdio
  - enabled: bool
  - command: Vec<String> (Local/Stdio)
  - args: Vec<String> (선택)
  - url: String (Remote - SSE/WebSocket)
  - environment: HashMap<String, String>
  - timeout_secs: u64
  - retry_attempts: u32
  - working_dir: Option<PathBuf>

- [ ] `McpServerType` enum
  - `Local` - 로컬 프로세스 (stdio)
  - `Remote` - 원격 서버 (HTTP SSE)
  - `Stdio` - 표준 입출력 직접 통신

- [ ] MCP 서버 라이프사이클
  - 자동 시작/종료
  - 재연결 로직
  - 헬스체크

- [ ] MCP 도구 관리
  - 서버별 도구 목록 캐싱
  - 도구 활성화/비활성화
  - 도구 별칭 (alias)

- [ ] MCP 리소스 관리
  - 리소스 URI 패턴
  - 리소스 캐싱 정책

- [ ] MCP 프롬프트 관리
  - 서버별 프롬프트 템플릿
  - 프롬프트 인자 스키마

---

### 7. 설정 로드/저장

- [ ] `Config::load()` - 설정 로드
- [ ] `Config::load_from_file()` - 특정 파일에서 로드
- [ ] `Config::merge()` - 설정 병합 (부분 덮어쓰기)
- [ ] `Config::apply_env_overrides()` - 환경변수 적용
- [ ] `Config::save()` - 설정 저장 (선택)

---

### 8. 검증

- [ ] `Validate` 트레이트
- [ ] `Config::validate()` - 전체 검증
- [ ] API 키 형식 검증
- [ ] URL 형식 검증
- [ ] 경로 존재 여부 확인
- [ ] 숫자 범위 검증 (토큰, 비용 등)

---

### 9. 환경변수 매핑

| 환경변수 | 설정 경로 |
|----------|-----------|
| `FORGECODE_DATA_DIR` | `paths.data_dir` |
| `FORGECODE_PROVIDER` | `providers.default` |
| `ANTHROPIC_API_KEY` | `providers.anthropic.api_key` |
| `OPENAI_API_KEY` | `providers.openai.api_key` |
| `OPENAI_BASE_URL` | `providers.openai.base_url` |
| `GEMINI_API_KEY` | `providers.gemini.api_key` |
| `OLLAMA_HOST` | `providers.ollama.base_url` |
| `GITHUB_TOKEN` | `mcp.servers.github.environment.GITHUB_TOKEN` |
| `DATABASE_URL` | `mcp.servers.postgres.environment.DATABASE_URL` |

**환경변수 참조 문법:**
- 설정 파일에서 `${ENV_VAR}` 형식으로 환경변수 참조 가능
- 예: `api_key = "${ANTHROPIC_API_KEY}"`

---

### 10. 파일 구조

```
config/
├── mod.rs           # 모듈 정의 + re-export
├── types.rs         # Config 최상위 구조체
├── providers.rs     # 프로바이더 설정 (Anthropic, OpenAI, etc.)
├── models.rs        # 모델 정보 + 가격
├── execution.rs     # 실행 환경 설정 (Local, Container)
├── mcp.rs           # MCP 서버 설정
├── tools.rs         # 도구 설정
├── limits.rs        # 사용량/예산 제한
├── paths.rs         # 경로 설정
├── tui.rs           # TUI 설정
├── loader.rs        # 설정 로드/병합 로직
├── validation.rs    # 검증 로직
└── CLAUDE.md        # 이 문서
```

---

### 11. 설정 파일 예시 (config.toml)

```toml
[paths]
data_dir = "~/.forgecode"
working_dir = "."

[providers]
default = "anthropic"

[providers.anthropic]
api_key = "${ANTHROPIC_API_KEY}"
model = "claude-sonnet-4-20250514"
max_tokens = 8192

[providers.openai]
api_key = "${OPENAI_API_KEY}"
model = "gpt-4o"
base_url = "https://api.openai.com/v1"  # 선택

[providers.ollama]
base_url = "http://localhost:11434"
model = "llama3"

[models.pricing]
# 커스텀 가격 오버라이드 (선택)
"claude-sonnet-4-20250514" = { input = 3.0, output = 15.0 }

[execution]
default_mode = "local"
allow_local = true

[execution.container]
default_image = "forgecode/workspace:latest"
timeout_secs = 120
memory_limit = "2g"

[execution.local]
blocked_paths = ["~/.ssh", "~/.aws"]
blocked_commands = ["rm -rf /"]
confirm_commands = ["rm", "mv"]

[limits]
max_cost_per_day = 10.0      # USD
max_cost_per_month = 100.0   # USD
warn_at_percentage = 80      # 80%에서 경고

# MCP 전역 설정
[mcp]
auto_start = true
default_timeout_secs = 30

# GitHub MCP 서버 (로컬 프로세스)
[mcp.servers.github]
type = "local"
enabled = true
command = ["npx", "-y", "@modelcontextprotocol/server-github"]
timeout_secs = 60
retry_attempts = 3
[mcp.servers.github.environment]
GITHUB_TOKEN = "${GITHUB_TOKEN}"

# Filesystem MCP 서버
[mcp.servers.filesystem]
type = "local"
enabled = true
command = ["npx", "-y", "@modelcontextprotocol/server-filesystem"]
args = ["--root", "."]
working_dir = "."

# Postgres MCP 서버
[mcp.servers.postgres]
type = "local"
enabled = false
command = ["npx", "-y", "@modelcontextprotocol/server-postgres"]
[mcp.servers.postgres.environment]
DATABASE_URL = "${DATABASE_URL}"

# 원격 MCP 서버 (SSE)
[mcp.servers.remote-api]
type = "remote"
enabled = false
url = "https://mcp.example.com/sse"
timeout_secs = 30

# 커스텀 MCP 서버 (Stdio)
[mcp.servers.custom]
type = "stdio"
enabled = false
command = ["/path/to/custom-mcp-server"]
args = ["--config", "config.json"]

[tui]
theme = "default"
show_tokens = true
show_cost = true
```

---

## 우선순위

1. **Phase 1**: 핵심 구조체 (Config, Providers, Execution)
2. **Phase 2**: 모델 가격 분리 (Models, Pricing)
3. **Phase 3**: 검증 + 제한 (Validation, Limits)
4. **Phase 4**: 설정 병합 + Hot-reload

---

## 의존성

```toml
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
dirs = "5"
thiserror = "1"
```
