# ForgeCode 설계 철학

## 핵심 원칙

### 1. Provider Agnostic (프로바이더 독립성)

ForgeCode는 특정 AI 프로바이더에 종속되지 않습니다.

```
┌─────────────────────────────────────────────────────────────┐
│                    ForgeCode Agent                          │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │            Unified Agent Interface                   │   │
│  │  - query(prompt, options)                           │   │
│  │  - Tool execution                                   │   │
│  │  - Session management                               │   │
│  └─────────────────────────────────────────────────────┘   │
│                          │                                  │
│         ┌────────────────┼────────────────┐                │
│         ▼                ▼                ▼                │
│  ┌────────────┐  ┌────────────┐  ┌────────────┐           │
│  │  Claude    │  │  OpenAI    │  │  Local     │           │
│  │  Agent SDK │  │  Codex     │  │  Models    │           │
│  └────────────┘  └────────────┘  └────────────┘           │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### 2. Simple Agent Loop (단순한 에이전트 루프)

Claude Code, OpenCode, Gemini CLI 분석 결과, 복잡한 4단계 루프보다 단순한 루프가 더 효과적입니다.

```rust
// ForgeCode의 핵심 루프
loop {
    // 1. 컨텍스트 관리
    if context_usage > 0.92 { compress(); }
    
    // 2. 외부 제어 확인
    if steering.should_stop() { break; }
    
    // 3. LLM 호출
    let response = provider.query(history, tools);
    
    // 4. Tool 없으면 완료
    if response.tool_calls.is_empty() { break; }
    
    // 5. Tool 실행
    for tool in response.tool_calls {
        hooks.before_tool(&tool);
        let result = execute(tool);
        hooks.after_tool(&tool, &result);
    }
}
```

### 3. Hook-based Extensibility (훅 기반 확장성)

복잡한 Strategy 패턴 대신 간단한 Hook 시스템으로 확장합니다.

```rust
trait AgentHook {
    async fn before_agent(&self, history: &History) -> HookResult;
    async fn after_agent(&self, history: &History, response: &str) -> HookResult;
    async fn before_tool(&self, tool: &ToolCall) -> HookResult;
    async fn after_tool(&self, tool: &ToolCall, result: &ToolResult) -> HookResult;
}
```

### 4. MCP First (MCP 우선)

Model Context Protocol을 통해 외부 도구와 연결합니다.

```
┌──────────────────┐     ┌──────────────────┐
│   ForgeCode      │────▶│   MCP Server     │
│   Agent          │     │   (GitHub, Slack │
└──────────────────┘     │    Linear, etc)  │
                         └──────────────────┘
```

### 5. Unified Tool Interface (통합 도구 인터페이스)

모든 프로바이더가 동일한 도구 인터페이스를 사용합니다.

| Tool | Claude SDK | Codex | ForgeCode |
|------|------------|-------|-----------|
| Read | Read | read_file | Read |
| Write | Write | write_file | Write |
| Edit | Edit | edit_file | Edit |
| Bash | Bash | shell | Bash |
| Grep | Grep | search | Grep |
| Glob | Glob | list_files | Glob |
| WebSearch | WebSearch | web_search | WebSearch |

## 아키텍처 레이어

```
┌─────────────────────────────────────────────────────────────┐
│ Layer 4: Interface (CLI/TUI/IDE)                            │
│ - forge-cli: Terminal interface                             │
│ - IDE plugins: VS Code, JetBrains, Xcode                   │
├─────────────────────────────────────────────────────────────┤
│ Layer 3: Agent (Orchestration)                              │
│ - Agent Loop: Simple while(tool_call) loop                 │
│ - Hooks: Before/After Agent, Tool                          │
│ - Steering: Pause, Resume, Stop, Redirect                  │
│ - Compressor: Auto context compression                     │
├─────────────────────────────────────────────────────────────┤
│ Layer 2: Services                                           │
│ - Provider Gateway: Multi-provider abstraction             │
│ - Tool Registry: Unified tool interface                    │
│ - Task Manager: Background execution                       │
│ - MCP Bridge: External tool integration                    │
├─────────────────────────────────────────────────────────────┤
│ Layer 1: Foundation                                         │
│ - Config: Settings, API keys                               │
│ - Permission: Security, approval workflow                  │
│ - Storage: Session persistence                             │
│ - Tokenizer: Token counting                                │
└─────────────────────────────────────────────────────────────┘
```

## 프로바이더 연동 전략

### Claude Agent SDK 연동

```rust
// Claude SDK를 ForgeCode Provider로 래핑
pub struct ClaudeAgentProvider {
    sdk: ClaudeAgentSDK,
}

impl AgentProvider for ClaudeAgentProvider {
    async fn query(&self, prompt: &str, options: QueryOptions) -> AgentStream {
        // Claude SDK의 query()를 ForgeCode 형식으로 변환
        let claude_options = ClaudeAgentOptions {
            allowed_tools: options.tools.into_iter()
                .map(|t| t.to_claude_name())
                .collect(),
            permission_mode: options.permission_mode.into(),
            hooks: options.hooks.into_claude_hooks(),
        };
        
        self.sdk.query(prompt, claude_options).into()
    }
}
```

### OpenAI Codex 연동

```rust
// Codex Responses API를 ForgeCode Provider로 래핑
pub struct CodexProvider {
    client: CodexClient,
}

impl AgentProvider for CodexProvider {
    async fn query(&self, prompt: &str, options: QueryOptions) -> AgentStream {
        // Codex API를 ForgeCode 형식으로 변환
        let codex_request = CodexRequest {
            prompt,
            tools: options.tools.into_iter()
                .map(|t| t.to_codex_schema())
                .collect(),
            approval_mode: options.approval_mode.into(),
        };
        
        self.client.execute(codex_request).into()
    }
}
```

### Local Models 연동

```rust
// Ollama/vLLM 등 로컬 모델 지원
pub struct LocalModelProvider {
    endpoint: String,
    model: String,
}

impl AgentProvider for LocalModelProvider {
    async fn query(&self, prompt: &str, options: QueryOptions) -> AgentStream {
        // OpenAI 호환 API로 요청
        // Tool 실행은 ForgeCode가 직접 처리
    }
}
```

## 도구 매핑 전략

### ForgeCode Tool → Claude SDK Tool

```rust
impl From<ForgeTool> for ClaudeTool {
    fn from(tool: ForgeTool) -> Self {
        match tool {
            ForgeTool::Read { path } => ClaudeTool::Read { file_path: path },
            ForgeTool::Write { path, content } => ClaudeTool::Write { file_path: path, content },
            ForgeTool::Edit { path, old, new } => ClaudeTool::Edit { 
                file_path: path, 
                old_string: old, 
                new_string: new 
            },
            ForgeTool::Bash { command } => ClaudeTool::Bash { command },
            ForgeTool::Grep { pattern, path } => ClaudeTool::Grep { pattern, path },
            ForgeTool::Glob { pattern } => ClaudeTool::Glob { pattern },
        }
    }
}
```

### ForgeCode Tool → Codex Tool

```rust
impl From<ForgeTool> for CodexTool {
    fn from(tool: ForgeTool) -> Self {
        match tool {
            ForgeTool::Read { path } => CodexTool::ReadFile { path },
            ForgeTool::Write { path, content } => CodexTool::WriteFile { path, content },
            ForgeTool::Edit { path, old, new } => CodexTool::EditFile { path, old, new },
            ForgeTool::Bash { command } => CodexTool::Shell { command },
            ForgeTool::Grep { pattern, path } => CodexTool::Search { query: pattern, path },
            ForgeTool::Glob { pattern } => CodexTool::ListFiles { pattern },
        }
    }
}
```

## 세션 관리

### Unified Session Format

```rust
pub struct ForgeSession {
    /// 세션 ID
    pub id: String,
    
    /// 프로바이더 타입
    pub provider: ProviderType,
    
    /// 프로바이더별 세션 ID (있으면)
    pub provider_session_id: Option<String>,
    
    /// 메시지 히스토리
    pub messages: Vec<Message>,
    
    /// 사용된 도구들
    pub tools_used: Vec<String>,
    
    /// 토큰 사용량
    pub token_usage: TokenUsage,
    
    /// 메타데이터
    pub metadata: SessionMetadata,
}
```

### Session Portability

```rust
// 세션을 다른 프로바이더로 이전 가능
impl ForgeSession {
    /// Claude SDK 세션으로 변환
    pub fn to_claude_session(&self) -> ClaudeSession { ... }
    
    /// Codex 세션으로 변환
    pub fn to_codex_session(&self) -> CodexSession { ... }
    
    /// 로컬 히스토리로 변환
    pub fn to_local_history(&self) -> MessageHistory { ... }
}
```

## 권한 모델

### Unified Permission System

```rust
pub enum PermissionLevel {
    /// 모든 작업 자동 승인
    BypassAll,
    
    /// 읽기만 자동 승인
    ReadOnly,
    
    /// 쓰기는 확인 필요
    ConfirmWrites,
    
    /// 모든 작업 확인 필요
    ConfirmAll,
    
    /// 커스텀 규칙
    Custom(PermissionRules),
}

// Claude SDK의 permissionMode와 매핑
impl From<PermissionLevel> for ClaudePermissionMode {
    fn from(level: PermissionLevel) -> Self {
        match level {
            PermissionLevel::BypassAll => "bypassPermissions",
            PermissionLevel::ReadOnly => "bypassPermissions", // + tool restriction
            PermissionLevel::ConfirmWrites => "acceptEdits",
            PermissionLevel::ConfirmAll => "default",
            PermissionLevel::Custom(_) => "default", // + custom hooks
        }
    }
}
```

## MCP 통합

### ForgeCode as MCP Client

```rust
// ForgeCode가 MCP 서버에 연결
pub struct McpBridge {
    servers: HashMap<String, McpServer>,
}

impl McpBridge {
    /// MCP 서버의 도구를 ForgeCode 도구로 등록
    pub async fn register_server(&mut self, name: &str, config: McpServerConfig) {
        let server = McpServer::connect(config).await?;
        let tools = server.list_tools().await?;
        
        for tool in tools {
            self.tool_registry.register(McpTool::new(name, tool));
        }
        
        self.servers.insert(name.to_string(), server);
    }
}
```

### ForgeCode as MCP Server

```rust
// ForgeCode 자체를 MCP 서버로 노출 (Codex 스타일)
pub struct ForgeMcpServer {
    agent: Agent,
}

impl McpServer for ForgeMcpServer {
    fn list_tools(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: "forge_query",
                description: "Run ForgeCode agent with a prompt",
                schema: json!({
                    "type": "object",
                    "properties": {
                        "prompt": { "type": "string" },
                        "tools": { "type": "array", "items": { "type": "string" } }
                    }
                }),
            }
        ]
    }
    
    async fn call_tool(&self, name: &str, args: Value) -> Value {
        // 다른 에이전트가 ForgeCode를 도구로 사용 가능
    }
}
```

## 결론

ForgeCode의 설계 철학:

1. **단순함** - 복잡한 추상화보다 직관적인 구조
2. **호환성** - Claude SDK, Codex, 로컬 모델 모두 지원
3. **확장성** - Hook과 MCP로 무한 확장 가능
4. **투명성** - 모든 동작이 추적 가능하고 디버깅 가능
5. **안전성** - 권한 시스템으로 위험한 작업 제어

이 철학은 Claude Code와 OpenAI Codex의 장점을 결합하면서도 ForgeCode만의 유연성을 제공합니다.
