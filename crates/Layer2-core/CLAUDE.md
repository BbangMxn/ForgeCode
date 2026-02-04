# Layer2-core (forge-core)

> AI Agent 도구 시스템 - Tool, MCP, LSP, Git, Skill, Plugin, Hook 통합 레이어

## 1. 개요

forge-core는 ForgeCode의 핵심 도구 레이어입니다:
- **8개 핵심 도구** (bash, read, write, edit, glob, grep, web_search, web_fetch)
- **MCP (Model Context Protocol)** 브릿지
- **LSP (Language Server Protocol)** 통합
- **Git Integration** (auto-commit, checkpoint, rollback)
- **Skill 시스템** (슬래시 명령어)
- **Plugin 시스템** (확장 가능)
- **Hook 시스템** (Claude Code 호환)
- Layer1 Permission/Config 연동

## 2. 모듈 구조

```
Layer2-core/
├── src/
│   ├── lib.rs              # 공개 API
│   ├── context.rs          # AgentContext 통합 인터페이스
│   │
│   ├── tool/               # 도구 시스템
│   │   ├── mod.rs          # exports
│   │   ├── registry.rs     # ToolRegistry
│   │   ├── context.rs      # RuntimeContext
│   │   ├── security.rs     # PathValidator, 보안 검사
│   │   └── builtin/        # 내장 도구들
│   │       ├── mod.rs
│   │       ├── bash.rs     # ✅ Shell 명령
│   │       ├── read.rs     # ✅ 파일 읽기
│   │       ├── write.rs    # ✅ 파일 쓰기
│   │       ├── edit.rs     # ✅ 문자열 교체
│   │       ├── glob.rs     # ✅ 패턴 검색
│   │       ├── grep.rs     # ✅ 내용 검색
│   │       ├── web_search.rs # ✅ 웹 검색 (NEW)
│   │       └── web_fetch.rs  # ✅ URL 가져오기 (NEW)
│   │
│   ├── git/                # Git 통합 (NEW)
│   │   ├── mod.rs          # exports
│   │   ├── ops.rs          # ✅ GitOps (status, diff, commit, reset)
│   │   ├── checkpoint.rs   # ✅ CheckpointManager (rollback)
│   │   └── commit.rs       # ✅ CommitGenerator (auto-commit)
│   │
│   ├── mcp/                # MCP 브릿지 ✅
│   │   ├── mod.rs          # exports
│   │   ├── types.rs        # McpTool, McpToolCall, McpContent
│   │   ├── transport.rs    # ✅ StdioTransport, SseTransport
│   │   ├── client.rs       # ✅ McpClient (JSON-RPC 2.0)
│   │   └── bridge.rs       # ✅ McpToolAdapter
│   │
│   ├── lsp/                # LSP 통합 ✅
│   │   ├── mod.rs          # exports
│   │   ├── types.rs        # Position, Location, Hover
│   │   ├── client.rs       # ✅ LspClient (tokio async)
│   │   └── manager.rs      # ✅ LspManager
│   │
│   ├── skill/              # Skill 시스템 ✅
│   │   ├── mod.rs
│   │   ├── traits.rs       # Skill trait
│   │   ├── registry.rs     # SkillRegistry
│   │   ├── loader.rs       # 파일 기반 로더
│   │   └── builtin/        # 내장 스킬
│   │       ├── commit.rs   # /commit
│   │       ├── review_pr.rs # /review-pr
│   │       └── explain.rs  # /explain
│   │
│   ├── plugin/             # Plugin 시스템 ✅
│   │   ├── mod.rs
│   │   ├── traits.rs       # Plugin trait
│   │   ├── registry.rs     # PluginRegistry
│   │   ├── manager.rs      # PluginManager
│   │   └── installer.rs    # 설치 관리
│   │
│   ├── hook/               # Hook 시스템 ✅
│   │   ├── mod.rs
│   │   ├── types.rs        # HookEvent, HookAction
│   │   ├── executor.rs     # HookExecutor
│   │   └── loader.rs       # 설정 파일 로더
│   │
│   ├── config/             # 설정 시스템 ✅
│   │   ├── mod.rs
│   │   ├── loader.rs       # ConfigLoader
│   │   ├── types.rs        # ForgeConfig
│   │   └── workflow.rs     # 워크플로우 설정
│   │
│   ├── repomap/            # 코드베이스 분석 ✅
│   │   ├── mod.rs
│   │   ├── analyzer.rs     # RepoAnalyzer
│   │   ├── graph.rs        # DependencyGraph
│   │   ├── ranker.rs       # FileRanker
│   │   └── types.rs        # SymbolDef, SymbolRef
│   │
│   ├── registry/           # 동적 레지스트리 ✅
│   │   ├── mod.rs
│   │   ├── dynamic.rs      # DynamicRegistry
│   │   ├── entry.rs        # RegistryEntry
│   │   └── snapshot.rs     # SnapshotManager
│   │
│   └── forgecmd/           # PTY 기반 Shell ✅
│       ├── mod.rs
│       ├── config.rs       # ForgeCmdConfig
│       ├── shell.rs        # PtySession
│       ├── filter.rs       # CommandFilter
│       ├── permission.rs   # PermissionChecker
│       └── tracker.rs      # CommandTracker
│
└── CLAUDE.md               # 이 문서
```

## 3. Tool 시스템

### 3.1 내장 도구 (8개)

| 도구 | 설명 | 권한 | 상태 |
|------|------|------|------|
| `bash` | Shell 명령 실행 | Execute | ✅ |
| `read` | 파일 읽기 (offset/limit) | Read | ✅ |
| `write` | 파일 쓰기 | Write | ✅ |
| `edit` | 문자열 교체 (fuzzy matching) | Write | ✅ |
| `glob` | 파일 패턴 검색 | - | ✅ |
| `grep` | 내용 검색 (regex) | - | ✅ |
| `web_search` | 웹 검색 | Network | ✅ NEW |
| `web_fetch` | URL 콘텐츠 가져오기 | Network | ✅ NEW |

### 3.2 WebSearch Tool (NEW)

```rust
use forge_core::tool::builtin::WebSearchTool;

// 지원 프로바이더
pub enum SearchProvider {
    Brave,      // Brave Search API (기본)
    DuckDuckGo, // API 키 불필요
    Google,     // Google Custom Search
    Tavily,     // Tavily AI Search
    SerpApi,    // SerpAPI (aggregator)
}

// 설정
let config = WebSearchConfig {
    provider: SearchProvider::Brave,
    api_key: Some("BRAVE_API_KEY".to_string()),
    max_results: 10,
    timeout: Duration::from_secs(30),
    include_snippets: true,
    safe_search: true,
};

// 실행
let result = tool.execute(json!({
    "query": "Rust async programming",
    "max_results": 5
}), &ctx).await?;
```

### 3.3 WebFetch Tool (NEW)

```rust
use forge_core::tool::builtin::WebFetchTool;

// HTML → Markdown 자동 변환
let result = tool.execute(json!({
    "url": "https://docs.rs/tokio",
    "prompt": "Find async runtime information"
}), &ctx).await?;

// 결과 예시:
// URL: https://docs.rs/tokio
// Status: 200
// Title: tokio - Rust
// Description: An event-driven, non-blocking I/O platform
// 
// ---
// 
// # Tokio
// **A runtime for writing reliable, asynchronous applications**
// ...
```

### 3.4 ToolRegistry

```rust
let registry = ToolRegistry::with_builtins();

// 8개 도구 확인
assert_eq!(registry.len(), 8);
assert!(registry.contains("web_search"));
assert!(registry.contains("web_fetch"));

// LLM에 전달할 스키마
let definitions = registry.definitions();
```

## 4. Git 통합 (NEW)

### 4.1 GitOps

```rust
use forge_core::GitOps;

let git = GitOps::new("/path/to/repo")?;

// 상태 확인
let status = git.status()?;
println!("Branch: {:?}", status.branch);
println!("Changed files: {:?}", status.files);

// Diff 가져오기
let diff = git.diff_staged()?;
let diff_all = git.diff_all()?;

// 커밋
git.add_all()?;
let hash = git.commit("feat: add new feature")?;

// Reset
git.reset("HEAD~1", true)?;  // hard reset

// 로그
let entries = git.log(10)?;
for entry in entries {
    println!("{}: {}", entry.short_hash, entry.message);
}
```

### 4.2 CheckpointManager (Rollback)

```rust
use forge_core::CheckpointManager;

let mut checkpoints = CheckpointManager::new("/path/to/repo")?
    .with_auto_checkpoint(true)  // 매 턴 자동 체크포인트
    .with_max_checkpoints(50);

// 수동 체크포인트 생성
let checkpoint_id = checkpoints.create("Before refactoring")?;

// 자동 체크포인트 (턴마다)
checkpoints.create_auto()?;

// 롤백
checkpoints.rollback(&checkpoint_id)?;

// N턴 전으로 롤백
checkpoints.rollback_turns(3)?;

// 마지막 체크포인트로 롤백
checkpoints.rollback_last()?;

// Diff 확인
let diff = checkpoints.diff_from(&checkpoint_id)?;
```

### 4.3 CommitGenerator (Auto-commit)

```rust
use forge_core::{CommitGenerator, AutoCommitConfig, CommitStyle};

// Aider 스타일 설정
let config = AutoCommitConfig::aider_style();

// 또는 커스텀 설정
let config = AutoCommitConfig {
    enabled: true,
    style: CommitStyle::Conventional,
    include_attribution: true,
    attribution: "(forge)".to_string(),
    commit_dirty_first: true,
    ..Default::default()
};

let generator = CommitGenerator::new("/path/to/repo", config)?;

// 자동 커밋 (diff 분석 → 메시지 생성 → 커밋)
let hash = generator.auto_commit(Some("User asked to add feature"))?;
// 결과: "feat: add user authentication (forge)"

// Dirty 파일 먼저 커밋
generator.commit_dirty()?;

// LLM 기반 커밋 메시지 생성 (TODO)
let message = generator.generate_message_with_llm(&diff, &chat_context).await?;
```

## 5. MCP 시스템

### 5.1 아키텍처

```
┌─────────────────────────────────────────────────┐
│                   McpBridge                      │
│    ┌─────────────────────────────────────────┐  │
│    │              McpClient                   │  │
│    │   ┌─────────────┐  ┌─────────────────┐  │  │
│    │   │ McpTransport│  │  JSON-RPC 2.0   │  │  │
│    │   │ ┌─────────┐ │  │  • initialize   │  │  │
│    │   │ │  Stdio  │ │  │  • tools/list   │  │  │
│    │   │ └─────────┘ │  │  • tools/call   │  │  │
│    │   │ ┌─────────┐ │  │  • resources/*  │  │  │
│    │   │ │   SSE   │ │  │  • prompts/*    │  │  │
│    │   │ └─────────┘ │  └─────────────────┘  │  │
│    │   └─────────────┘                       │  │
│    └─────────────────────────────────────────┘  │
│                        ↓                         │
│    ┌─────────────────────────────────────────┐  │
│    │            McpToolAdapter               │  │
│    │       MCP Tool → Layer2 Tool            │  │
│    └─────────────────────────────────────────┘  │
└─────────────────────────────────────────────────┘
```

### 5.2 사용법

```rust
let bridge = McpBridge::new();

// 서버 추가
let mut client = McpClient::new("notion");
client.connect(&McpTransportConfig::Stdio {
    command: "npx".to_string(),
    args: vec!["@notionhq/mcp-server"],
    env: Default::default(),
}).await?;

bridge.add_server(client).await;

// 도구 호출
let result = bridge.call_tool("notion", "search", json!({
    "query": "meeting notes"
})).await?;
```

## 6. Skill 시스템

```rust
use forge_core::{SkillRegistry, CommitSkill, ReviewPrSkill};

let mut registry = SkillRegistry::new();
registry.register(Arc::new(CommitSkill::new()));
registry.register(Arc::new(ReviewPrSkill::new()));

// 슬래시 명령어 처리
if let Some(skill) = registry.find_for_input("/commit -m 'fix bug'") {
    let result = skill.execute(input, &ctx).await?;
}
```

## 7. Hook 시스템 (Claude Code 호환)

```rust
use forge_core::{HookExecutor, HookEvent, load_hooks_from_dir};

// .claude/hooks/ 에서 훅 로드
let hooks = load_hooks_from_dir(".claude/hooks")?;
let executor = HookExecutor::new(hooks);

// 훅 실행
let result = executor.run(HookEvent::BeforeToolUse {
    tool_name: "bash".to_string(),
    arguments: json!({"command": "npm install"}),
}).await?;

match result {
    HookOutcome::Allow => { /* 계속 진행 */ }
    HookOutcome::Block { reason } => { /* 차단됨 */ }
    HookOutcome::Modify { new_args } => { /* 인자 수정 */ }
}
```

## 8. RepoMap (코드베이스 분석)

```rust
use forge_core::{RepoAnalyzer, RepoMapConfig};

let config = RepoMapConfig {
    max_tokens: 1000,
    include_imports: true,
    ..Default::default()
};

let analyzer = RepoAnalyzer::new(config);
let repo_map = analyzer.analyze("/path/to/repo")?;

// 그래프 기반 랭킹
let ranker = FileRanker::new(&repo_map);
let top_files = ranker.rank_for_context("authentication", 10);
```

## 9. Layer 연결

### 9.1 Layer1 연동 (forge-foundation)

```rust
// Permission 연동
impl Tool for BashTool {
    fn required_permission(&self, input: &Value) -> Option<PermissionRequest> {
        // Layer1 PermissionService와 연동
    }
}

// Error 타입 사용
use forge_foundation::{Error, Result};
```

### 9.2 Layer3 연동 (forge-agent)

```rust
// AgentContext에서 도구 사용
let ctx = AgentContext::new();
let result = ctx.execute_tool("web_search", json!({
    "query": "Rust async"
})).await?;
```

## 10. API 요약

### Tool 시스템
| API | 설명 |
|-----|------|
| `ToolRegistry::with_builtins()` | 8개 도구 포함 레지스트리 |
| `all_tools()` | 모든 도구 인스턴스 |
| `core_tools()` | 핵심 3개 (read, write, bash) |
| `filesystem_tools()` | 파일시스템 5개 |

### Git 시스템
| API | 설명 |
|-----|------|
| `GitOps::new()` | Git 연산 핸들러 |
| `CheckpointManager::new()` | 체크포인트 관리자 |
| `CommitGenerator::new()` | 자동 커밋 생성기 |

### MCP 시스템
| API | 설명 |
|-----|------|
| `McpBridge::new()` | MCP 브릿지 생성 |
| `McpClient::connect()` | MCP 서버 연결 |
| `McpClient::call_tool()` | 도구 호출 |

## 11. 테스트

```bash
# 전체 테스트
cargo test -p forge-core

# Tool 테스트
cargo test -p forge-core tool

# Git 테스트
cargo test -p forge-core git

# MCP 테스트
cargo test -p forge-core mcp
```

## 12. 의존성

```toml
[dependencies]
forge-foundation = { workspace = true }
tokio = { workspace = true }
async-trait = { workspace = true }
reqwest = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
glob = { workspace = true }
regex = { workspace = true }
chrono = { workspace = true }
url = "2.5"
urlencoding = "2.1"
```
