# Layer2-core (forge-core)

Agent가 사용하는 도구(Tool) 구현 레이어

## 설계 목표

1. **Agent 도구 제공**: LLM Agent가 호출하는 모든 도구 구현
2. **Task 컨테이너**: 장시간/서버 프로세스의 독립 실행
3. **Layer1 완전 활용**: Permission, ShellConfig, Storage 연동
4. **확장성**: MCP 브릿지, LSP 통합

---

## 1. 아키텍처

```
┌─────────────────────────────────────────────────────────────────┐
│                        Layer3 (Agent)                            │
│                             │                                    │
│                             ▼                                    │
├─────────────────────────────────────────────────────────────────┤
│                     Layer2-core (도구)                           │
│                                                                  │
│  ┌─────────────────────────────────────────────────────────────┐│
│  │                      Tool Registry                           ││
│  │                                                              ││
│  │  [핵심 도구]                                                 ││
│  │  ┌──────────┬──────────┬──────────┬──────────┬──────────┐  ││
│  │  │   Bash   │   Read   │  Write   │   Edit   │   Glob   │  ││
│  │  └──────────┴──────────┴──────────┴──────────┴──────────┘  ││
│  │  ┌──────────┐                                               ││
│  │  │   Grep   │                                               ││
│  │  └──────────┘                                               ││
│  │                                                              ││
│  │  [웹 도구]                                                   ││
│  │  ┌──────────┬──────────┐                                    ││
│  │  │ WebFetch │WebSearch │                                    ││
│  │  └──────────┴──────────┘                                    ││
│  │                                                              ││
│  │  [상호작용]                                                  ││
│  │  ┌──────────┬──────────┐                                    ││
│  │  │   Todo   │ Question │                                    ││
│  │  └──────────┴──────────┘                                    ││
│  └─────────────────────────────────────────────────────────────┘│
│                                                                  │
│  ┌─────────────────────────────────────────────────────────────┐│
│  │                    Task (독립 컨테이너)                       ││
│  │                                                              ││
│  │   장시간/서버 프로세스 독립 실행                              ││
│  │   • npm start, cargo run (서버)                             ││
│  │   • docker-compose up                                        ││
│  │   • API 테스트 서버                                          ││
│  │                                                              ││
│  │   특징: 사용자 대화와 분리, 취소 없이 실행                    ││
│  └─────────────────────────────────────────────────────────────┘│
│                                                                  │
│  ┌─────────────────────────────────────────────────────────────┐│
│  │                    확장 시스템                                ││
│  │                                                              ││
│  │  ┌──────────┬──────────┐                                    ││
│  │  │   MCP    │   LSP    │                                    ││
│  │  │ (Bridge) │ (연구중) │                                    ││
│  │  └──────────┴──────────┘                                    ││
│  └─────────────────────────────────────────────────────────────┘│
│                                                                  │
├─────────────────────────────────────────────────────────────────┤
│                    Layer1 (Foundation)                           │
│         Permission │ ShellConfig │ Storage │ Config             │
└─────────────────────────────────────────────────────────────────┘
```

---

## 2. 모듈 구조

```
Layer2-core/src/
│
├── lib.rs                    # 공개 API
│
├── tool/                     # 도구 시스템
│   ├── mod.rs               # Tool Registry, exports
│   ├── registry.rs          # 도구 등록/관리
│   ├── context.rs           # ToolContext 구현
│   │
│   └── builtin/             # 내장 도구들
│       ├── mod.rs           # builtin exports, all_tools()
│       ├── bash.rs          # ✅ Shell 명령 실행
│       ├── read.rs          # ✅ 파일 읽기
│       ├── write.rs         # ✅ 파일 쓰기
│       ├── edit.rs          # ✅ 파일 편집 (문자열 교체)
│       ├── glob.rs          # ✅ 파일 패턴 검색
│       ├── grep.rs          # ✅ 내용 검색 (ripgrep)
│       ├── webfetch.rs      # ✅ 웹 페이지 가져오기
│       ├── websearch.rs     # ✅ 웹 검색
│       ├── todo.rs          # ✅ 작업 관리
│       └── question.rs      # ✅ 사용자에게 질문
│
├── task/                     # Task 컨테이너 (독립 실행)
│   ├── mod.rs               # ✅ Task exports
│   ├── executor.rs          # ✅ Task 생성/관리
│   ├── container.rs         # ✅ 독립 Shell 세션
│   ├── tracker.rs           # ✅ 상태 추적
│   └── tool.rs              # ✅ Task Tool (Agent용)
│
├── mcp/                      # MCP 브릿지
│   ├── mod.rs               # ✅ MCP exports
│   ├── types.rs             # ✅ MCP 타입 정의
│   ├── client.rs            # ✅ MCP 클라이언트
│   └── bridge.rs            # ✅ MCP→Tool 변환
│
└── lsp/                      # LSP 통합
    ├── mod.rs               # ✅ LSP exports
    ├── types.rs             # ✅ LSP 타입 정의
    ├── client.rs            # ✅ LSP 클라이언트
    └── manager.rs           # ✅ 언어별 서버 관리

✅ = 구조 생성됨 (TODO 구현 필요)
```

---

## 3. 도구 목록

### 3.1 핵심 도구 (Core)

| 도구 | 설명 | Layer1 연동 | 상태 |
|------|------|-------------|------|
| **Bash** | Shell 명령 실행 | PermissionService, ShellConfig | ✅ 구조 완료 |
| **Read** | 파일 읽기 | PermissionService | ✅ 구조 완료 |
| **Write** | 파일 쓰기 | PermissionService | ✅ 구조 완료 |
| **Edit** | 파일 편집 (정확한 문자열 교체) | PermissionService | ✅ 구조 완료 |
| **Glob** | 파일 패턴 검색 | - | ✅ 구조 완료 |
| **Grep** | 내용 검색 (ripgrep 기반) | - | ✅ 구조 완료 |

### 3.2 Task 시스템

| 도구 | 설명 | 특징 | 상태 |
|------|------|------|------|
| **Task** | 독립 컨테이너 실행 | 장시간/서버 프로세스, 사용자 대화와 분리 | ✅ 구조 완료 |

```
Task vs Bash 사용 기준:

Task 사용 (독립 컨테이너):
├── npm start, npm run dev     # 개발 서버
├── python -m http.server      # HTTP 서버
├── cargo run (서버)           # 백엔드 서버
├── docker-compose up          # 컨테이너
└── API 테스트 서버            # 장시간 실행

Bash 사용 (일반 명령):
├── ls, cat, pwd               # 즉시 완료
├── npm install                # 완료 후 종료
├── cargo build                # 빌드 후 종료
└── git status, git commit     # 즉시 완료
```

### 3.3 웹 도구

| 도구 | 설명 | 의존성 | 상태 |
|------|------|--------|------|
| **WebFetch** | 웹 페이지 가져오기 | reqwest | ✅ 구조 완료 |
| **WebSearch** | 웹 검색 | 검색 API 필요 | ✅ 구조 완료 |

### 3.4 사용자 상호작용

| 도구 | 설명 | 용도 | 상태 |
|------|------|------|------|
| **Todo** | 작업 목록 관리 | 복잡한 작업 추적 | ✅ 구조 완료 |
| **Question** | 사용자에게 질문 | 명확화 필요 시 | ✅ 구조 완료 |

### 3.5 확장 도구

| 도구 | 설명 | 복잡도 | 상태 |
|------|------|--------|------|
| **MCP Bridge** | 외부 MCP 서버 연동 | 중간 | ✅ 구조 완료 |
| **LSP** | Language Server Protocol | 높음 | ✅ 구조 완료 |

### 3.6 설정으로 처리

| 기능 | 설명 | 위치 |
|------|------|------|
| **Git 자동 커밋** | 변경 시 자동 커밋 | Layer1 ForgeConfig |

---

## 4. 사용 예시

### 4.1 ToolRegistry 사용

```rust
use forge_core::{create_tool_registry, ToolRegistry};

// Builtin 도구가 포함된 레지스트리 생성
let registry = create_tool_registry();

// 도구 조회
let bash = registry.get("bash").unwrap();
let schema = bash.schema();

// 모든 도구 스키마 (LLM에 전달용)
let all_schemas = registry.schemas();
```

### 4.2 Task 시스템 사용

```rust
use forge_core::{create_task_executor, task::ContainerConfig};
use std::path::PathBuf;

// Task Executor 생성
let executor = create_task_executor();

// 서버 시작
let config = ContainerConfig {
    working_dir: PathBuf::from("/project"),
    initial_command: Some("npm run dev".to_string()),
    ..Default::default()
};

let task_id = executor.spawn(config).await?;

// 출력 읽기
let output = executor.read_recent_output(&task_id, 50).await?;

// 종료
executor.stop(&task_id).await?;
```

### 4.3 MCP Bridge 사용

```rust
use forge_core::{create_mcp_bridge, mcp::{McpClient, McpTransportConfig}};

// MCP Bridge 생성
let bridge = create_mcp_bridge();

// MCP 서버 연결
let mut client = McpClient::new("notion");
client.connect(&McpTransportConfig::Stdio {
    command: "npx".to_string(),
    args: vec!["@notionhq/client".to_string()],
    env: Default::default(),
}).await?;

bridge.add_server(client).await;

// MCP 도구들을 Layer2 Tool로 변환
let mcp_tools = bridge.get_all_tools().await;
registry.register_all(mcp_tools);
```

---

## 5. Layer1 연동

### 5.1 Permission 연동

```rust
// 도구 실행 시 권한 확인 흐름
impl Tool for BashTool {
    fn required_permission(&self, input: &Value) -> Option<PermissionAction> {
        let command = input["command"].as_str().unwrap_or("");
        Some(PermissionAction::Execute {
            command: command.to_string(),
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolResult> {
        // RuntimeContext에서 권한 확인 후 실행
        ...
    }
}
```

### 5.2 RuntimeContext

```rust
use forge_core::RuntimeContext;
use forge_foundation::PermissionService;
use std::sync::Arc;

// RuntimeContext 생성
let permissions = Arc::new(PermissionService::new());
let ctx = RuntimeContext::new(
    "session-123",
    PathBuf::from("/project"),
    permissions,
);

// 권한 확인
let status = ctx.check_permission("bash", &action);
```

---

## 6. 구현 우선순위

### Phase 1: 핵심 도구 구현 ⬜
1. ⬜ `bash.rs` - 실제 명령 실행 (tokio::process)
2. ⬜ `read.rs` - 파일 읽기 구현
3. ⬜ `write.rs` - 파일 쓰기 구현
4. ⬜ `edit.rs` - 문자열 치환 구현
5. ⬜ `glob.rs` - glob 패턴 검색
6. ⬜ `grep.rs` - ripgrep 또는 regex 구현

### Phase 2: Task 시스템 구현 ⬜
1. ⬜ `container.rs` - PTY 세션 (portable-pty)
2. ⬜ `executor.rs` - 프로세스 관리
3. ⬜ `tracker.rs` - 상태 추적

### Phase 3: 웹 + 상호작용 ⬜
1. ⬜ `webfetch.rs` - HTTP 요청 (reqwest)
2. ⬜ `websearch.rs` - 검색 API 연동
3. ⬜ `todo.rs` - 세션 스토리지 연동
4. ⬜ `question.rs` - UI 연동

### Phase 4: 확장 ⬜
1. ⬜ `mcp/client.rs` - MCP 프로토콜 구현
2. ⬜ `mcp/bridge.rs` - 도구 변환
3. ⬜ `lsp/client.rs` - LSP 프로토콜 구현
4. ⬜ `lsp/manager.rs` - 서버 관리

---

## 7. 연구 필요 사항

### 7.1 Task 시스템
- [ ] PTY 세션 관리 (portable-pty vs tokio-pty-process)
- [ ] Windows/Unix 크로스 플랫폼
- [ ] 프로세스 그룹 관리
- [ ] 리소스 제한 (memory, CPU)

### 7.2 WebSearch
- [ ] 검색 API 선택 (Google, Bing, Tavily, SerpAPI)
- [ ] API 키 관리 (Layer1 Config)
- [ ] Rate limiting

### 7.3 MCP
- [ ] JSON-RPC 2.0 구현
- [ ] stdio/SSE 전송
- [ ] 도구 스키마 변환
- [ ] 에러 핸들링

### 7.4 LSP
- [ ] LSP 프로토콜 이해
- [ ] 언어별 서버 자동 감지
- [ ] 지원 기능 범위 결정
  - textDocument/definition
  - textDocument/references
  - textDocument/hover
  - workspace/symbol
- [ ] 성능 최적화 (캐싱)

---

## 8. 테스트 전략

```rust
#[cfg(test)]
mod tests {
    // 1. 단위 테스트 - Tool trait 구현 확인
    #[test]
    fn test_bash_tool_meta() {
        let tool = BashTool::new();
        assert_eq!(tool.meta().name, "bash");
    }

    // 2. 통합 테스트 - Layer1 연동
    #[tokio::test]
    async fn test_bash_permission_check() {
        let ctx = create_test_context();
        let tool = BashTool::new();
        // ...
    }

    // 3. Registry 테스트
    #[test]
    fn test_registry_with_builtins() {
        let registry = ToolRegistry::with_builtins();
        assert!(registry.contains("bash"));
        assert!(registry.contains("read"));
    }
}
```

---

## 9. 참고 자료

### CLI 도구 비교
- [Claude Code Tools](https://www.vtrivedy.com/posts/claudecode-tools-reference)
- [OpenCode Tools](https://opencode.ai/docs/tools/)
- [Aider](https://aider.chat/)
- [Gemini CLI](https://github.com/google-gemini/gemini-cli)
- [Amazon Q Developer CLI](https://github.com/aws/amazon-q-developer-cli)

### 프로토콜
- [MCP (Model Context Protocol)](https://modelcontextprotocol.io/)
- [LSP (Language Server Protocol)](https://microsoft.github.io/language-server-protocol/)

### 라이브러리
- [portable-pty](https://docs.rs/portable-pty/) - PTY 세션
- [reqwest](https://docs.rs/reqwest/) - HTTP 클라이언트
- [glob](https://docs.rs/glob/) - 패턴 매칭
- [regex](https://docs.rs/regex/) - 정규식
