# MCP Module - Model Context Protocol 브릿지

외부 MCP 서버를 Layer2 Tool로 통합하는 브릿지

## 1. MCP란?

MCP (Model Context Protocol)는 AI 애플리케이션을 외부 시스템에 연결하는 오픈 소스 표준입니다.

```
MCP = AI 애플리케이션의 USB-C 포트
```

### 1.1 MCP가 제공하는 것

| 원시 타입 | 설명 | 예시 |
|-----------|------|------|
| **Tools** | AI가 호출할 수 있는 실행 함수 | 파일 작업, API 호출, DB 쿼리 |
| **Resources** | AI에 컨텍스트 제공하는 데이터 소스 | 파일 내용, DB 레코드 |
| **Prompts** | 재사용 가능한 상호작용 템플릿 | 시스템 프롬프트, few-shot |

### 1.2 ForgeCode에서의 활용

ForgeCode Agent → MCP Bridge → MCP Servers (Notion, GitHub, Slack, ...)

---

## 2. 아키텍처

```
┌─────────────────────────────────────────────────────────────────┐
│                       ForgeCode Agent                            │
│                            │                                     │
│                      도구 호출 요청                              │
│                            │                                     │
├────────────────────────────┼────────────────────────────────────┤
│                            ▼                                     │
│                       McpBridge                                  │
│                            │                                     │
│     ┌──────────────────────┼──────────────────────┐             │
│     │                      │                      │             │
│     ▼                      ▼                      ▼             │
│  ┌──────────┐       ┌──────────┐           ┌──────────┐         │
│  │McpClient │       │McpClient │           │McpClient │         │
│  │(Notion)  │       │(GitHub)  │           │(Slack)   │         │
│  └────┬─────┘       └────┬─────┘           └────┬─────┘         │
│       │ stdio            │ stdio                │ sse           │
├───────┼──────────────────┼──────────────────────┼───────────────┤
│       ▼                  ▼                      ▼               │
│  ┌──────────┐       ┌──────────┐           ┌──────────┐         │
│  │  Notion  │       │  GitHub  │           │  Slack   │         │
│  │  Server  │       │  Server  │           │  Server  │         │
│  └──────────┘       └──────────┘           └──────────┘         │
│                    (External Processes)                         │
└─────────────────────────────────────────────────────────────────┘
```

---

## 3. JSON-RPC 2.0 프로토콜

### 3.1 메시지 포맷

```rust
// 요청
{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "tools/list",
    "params": {}
}

// 응답
{
    "jsonrpc": "2.0",
    "id": 1,
    "result": {
        "tools": [...]
    }
}

// 알림 (응답 없음)
{
    "jsonrpc": "2.0",
    "method": "notifications/initialized"
}
```

### 3.2 통신 방식

#### stdio 전송 (로컬)

```rust
// 헤더 + 바디 포맷
Content-Length: <length>\r\n
\r\n
<JSON body>
```

#### SSE 전송 (원격)

```rust
// HTTP POST로 요청
// Server-Sent Events로 스트리밍 응답
```

---

## 4. MCP 수명주기

### 4.1 초기화 시퀀스

```
Client                              Server
   │                                   │
   │  ──── initialize ────────────▶   │
   │       (capabilities 협상)         │
   │                                   │
   │  ◀─── initialize result ─────   │
   │       (서버 capabilities)         │
   │                                   │
   │  ──── notifications/initialized ▶ │
   │                                   │
   │        (서버 사용 가능)            │
```

### 4.2 도구 호출 시퀀스

```
Client                              Server
   │                                   │
   │  ──── tools/list ────────────▶   │
   │                                   │
   │  ◀─── tools 목록 ────────────   │
   │                                   │
   │  ──── tools/call ────────────▶   │
   │       (name, arguments)           │
   │                                   │
   │  ◀─── result ────────────────   │
   │       (content[])                 │
```

---

## 5. 핵심 타입

### 5.1 MCP 도구 정보

```rust
/// MCP 서버에서 제공하는 도구
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    /// 도구 이름
    pub name: String,

    /// 도구 설명
    pub description: Option<String>,

    /// 입력 스키마 (JSON Schema)
    pub input_schema: Value,
}
```

### 5.2 도구 호출/결과

```rust
/// MCP 도구 호출
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolCall {
    pub name: String,
    pub arguments: Value,
}

/// MCP 도구 결과
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolResult {
    pub is_error: bool,
    pub content: Vec<McpContent>,
}

/// 결과 콘텐츠
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum McpContent {
    Text { text: String },
    Image { data: String, mime_type: String },
    Resource { uri: String, mime_type: Option<String> },
}
```

### 5.3 서버 설정

```rust
/// MCP 서버 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    pub transport: McpTransportConfig,
    pub auto_connect: bool,
}

/// 전송 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum McpTransportConfig {
    /// stdio 전송 (로컬 프로세스)
    Stdio {
        command: String,
        args: Vec<String>,
        env: HashMap<String, String>,
    },
    /// SSE 전송 (HTTP)
    Sse {
        url: String,
    },
}
```

---

## 6. 핵심 컴포넌트

### 6.1 McpClient

개별 MCP 서버와의 통신 담당.

```rust
pub struct McpClient {
    name: String,
    transport: Option<McpTransport>,
    tools: Vec<McpTool>,
    connected: bool,
}

impl McpClient {
    /// 서버에 연결
    pub async fn connect(&mut self, config: &McpTransportConfig) -> Result<()>;

    /// 연결 종료
    pub async fn disconnect(&mut self) -> Result<()>;

    /// 도구 목록 갱신
    pub async fn refresh_tools(&mut self) -> Result<()>;

    /// 사용 가능한 도구들
    pub fn tools(&self) -> &[McpTool];

    /// 도구 호출
    pub async fn call_tool(&self, call: &McpToolCall) -> Result<McpToolResult>;
}
```

### 6.2 McpBridge

여러 MCP 서버를 관리하고 Layer2 Tool로 변환.

```rust
pub struct McpBridge {
    clients: RwLock<HashMap<String, Arc<RwLock<McpClient>>>>,
}

impl McpBridge {
    /// MCP 서버 추가
    pub async fn add_server(&self, client: McpClient);

    /// MCP 서버 제거
    pub async fn remove_server(&self, name: &str) -> Option<...>;

    /// 모든 MCP 도구를 Layer2 Tool로 변환
    pub async fn get_all_tools(&self) -> Vec<Arc<dyn Tool>>;

    /// 특정 서버의 도구들
    pub async fn get_server_tools(&self, server_name: &str) -> Vec<Arc<dyn Tool>>;
}
```

### 6.3 McpToolAdapter

MCP 도구를 Layer2 Tool trait으로 변환.

```rust
pub struct McpToolAdapter {
    server_name: String,
    mcp_tool: McpTool,
    client: Arc<RwLock<McpClient>>,
}

impl Tool for McpToolAdapter {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            name: format!("mcp_{}_{}", self.server_name, self.mcp_tool.name),
            description: self.mcp_tool.description.clone().unwrap_or_default(),
            version: "1.0.0".to_string(),
        }
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            input_schema: self.mcp_tool.input_schema.clone(),
        }
    }

    fn required_permission(&self, _input: &Value) -> Option<PermissionAction> {
        Some(PermissionAction::Mcp {
            server: self.server_name.clone(),
            tool: self.mcp_tool.name.clone(),
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolResult> {
        let call = McpToolCall {
            name: self.mcp_tool.name.clone(),
            arguments: input,
        };

        let client = self.client.read().await;
        let result = client.call_tool(&call).await?;

        // McpToolResult → ToolResult 변환
        ...
    }
}
```

---

## 7. 사용 예시

### 7.1 MCP 서버 연결

```rust
use forge_core::{create_mcp_bridge, mcp::{McpClient, McpTransportConfig}};

// 브릿지 생성
let bridge = create_mcp_bridge();

// Notion MCP 서버 연결
let mut client = McpClient::new("notion");
client.connect(&McpTransportConfig::Stdio {
    command: "npx".to_string(),
    args: vec!["-y".to_string(), "@notionhq/mcp-server".to_string()],
    env: HashMap::from([("NOTION_API_KEY".to_string(), "secret_xxx".to_string())]),
}).await?;

bridge.add_server(client).await;
```

### 7.2 도구 목록 가져오기

```rust
// 모든 MCP 도구를 Layer2 Tool로
let mcp_tools = bridge.get_all_tools().await;

// ToolRegistry에 등록
for tool in mcp_tools {
    registry.register(tool);
}
```

### 7.3 도구 호출

```rust
// Agent가 호출하면 자동으로 McpToolAdapter를 통해 MCP 서버로 전달
let tool = registry.get("mcp_notion_search_pages").unwrap();
let result = tool.execute(json!({
    "query": "meeting notes"
})).await?;
```

---

## 8. 인기 MCP 서버

### 8.1 공식 서버

| 서버 | 설명 | 설치 |
|------|------|------|
| **filesystem** | 로컬 파일 시스템 접근 | `@modelcontextprotocol/server-filesystem` |
| **github** | GitHub API | `@modelcontextprotocol/server-github` |
| **postgres** | PostgreSQL 쿼리 | `@modelcontextprotocol/server-postgres` |

### 8.2 커뮤니티 서버

| 서버 | 설명 |
|------|------|
| **notion** | Notion 페이지/DB 접근 |
| **slack** | Slack 메시지 전송 |
| **linear** | Linear 이슈 관리 |
| **sentry** | Sentry 오류 추적 |

---

## 9. 구현 계획

### Phase 1: 기본 기능

1. **`types.rs`**: MCP 타입 정의
   - McpTool, McpToolCall, McpToolResult
   - McpServerConfig, McpTransportConfig
   - McpContent (Text, Image, Resource)

2. **`client.rs`**: MCP 클라이언트
   - JSON-RPC 통신 (stdio)
   - initialize/shutdown 수명주기
   - tools/list, tools/call

3. **`bridge.rs`**: 브릿지
   - 다중 클라이언트 관리
   - Tool trait 어댑터

### Phase 2: 전송 확장

4. SSE 전송 지원
5. OAuth 인증

### Phase 3: 고급 기능

6. 리소스 프리미티브 지원
7. 프롬프트 프리미티브 지원
8. 알림 처리 (도구 변경 감지)

---

## 10. 의존성

### 10.1 권장 크레이트

```toml
[dependencies]
# 공식 MCP Rust SDK (있으면 사용)
rmcp = "0.8"

# 또는 직접 구현 시
serde_json = "1.0"
tokio = { version = "1.0", features = ["process", "io-util", "sync"] }

# HTTP (SSE 지원 시)
reqwest = { version = "0.12", features = ["json"] }
eventsource-stream = "0.2"
```

### 10.2 rmcp SDK 사용 시

```rust
use rmcp::{ServiceExt, transport::TokioChildProcess};
use tokio::process::Command;

let client = ().serve(TokioChildProcess::new(
    Command::new("npx")
        .arg("-y")
        .arg("@modelcontextprotocol/server-everything")
)?).await?;

// 도구 목록
let tools = client.list_tools().await?;

// 도구 호출
let result = client.call_tool("tool_name", json!({})).await?;
```

---

## 11. 에러 처리

```rust
#[derive(Debug, thiserror::Error)]
pub enum McpError {
    #[error("Not connected to server")]
    NotConnected,

    #[error("Server not found: {0}")]
    ServerNotFound(String),

    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    #[error("JSON-RPC error: {code} - {message}")]
    JsonRpc { code: i32, message: String },

    #[error("Transport error: {0}")]
    Transport(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}
```

---

## 12. Layer1 연동

### 12.1 Permission 연동

```rust
impl Tool for McpToolAdapter {
    fn required_permission(&self, _input: &Value) -> Option<PermissionAction> {
        Some(PermissionAction::Mcp {
            server: self.server_name.clone(),
            tool: self.mcp_tool.name.clone(),
        })
    }
}

// Layer1에서 MCP 권한 정의
pub enum PermissionAction {
    // ...
    Mcp { server: String, tool: String },
}
```

### 12.2 설정 연동

```rust
// Layer1 ForgeConfig에서 MCP 서버 설정 로드
let mcp_servers = forge_config.mcp_servers();

for config in mcp_servers {
    let mut client = McpClient::new(&config.name);
    client.connect(&config.transport).await?;
    bridge.add_server(client).await;
}
```

---

## 13. 테스트 전략

### 13.1 단위 테스트

```rust
#[test]
fn test_mcp_tool_result() {
    let result = McpToolResult::success("Hello");
    assert!(!result.is_error);
    assert_eq!(result.text(), Some("Hello"));
}

#[test]
fn test_transport_config() {
    let config = McpTransportConfig::Stdio {
        command: "npx".to_string(),
        args: vec!["-y".to_string(), "server".to_string()],
        env: HashMap::new(),
    };
    // ...
}
```

### 13.2 통합 테스트

```rust
#[tokio::test]
#[ignore] // MCP 서버 필요
async fn test_mcp_client_connection() {
    let mut client = McpClient::new("test");
    client.connect(&McpTransportConfig::Stdio {
        command: "npx".to_string(),
        args: vec!["-y".to_string(), "@modelcontextprotocol/server-everything".to_string()],
        env: HashMap::new(),
    }).await.unwrap();

    assert!(client.is_connected());

    let tools = client.tools();
    assert!(!tools.is_empty());
}
```

---

## 14. 파일 구조

```
mcp/
├── mod.rs          # 모듈 진입점, exports
├── types.rs        # MCP 타입 정의
├── client.rs       # MCP 클라이언트
├── bridge.rs       # MCP→Tool 브릿지
└── CLAUDE.md       # 이 문서
```

---

## 15. 참고 자료

### 공식 문서
- [MCP Introduction](https://modelcontextprotocol.io/introduction)
- [MCP Architecture](https://modelcontextprotocol.io/docs/learn/architecture)
- [MCP Specification](https://modelcontextprotocol.io/specification/latest)

### SDK
- [Rust SDK (rmcp)](https://github.com/modelcontextprotocol/rust-sdk)
- [TypeScript SDK](https://github.com/modelcontextprotocol/typescript-sdk)
- [Python SDK](https://github.com/modelcontextprotocol/python-sdk)

### MCP 서버
- [공식 서버 목록](https://github.com/modelcontextprotocol/servers)
- [MCP Directory](https://mcp.run/)
