# ForgeCode - Claude Code 호환성 설계

## 1. 개요

ForgeCode는 Claude Code의 Plugin/Skill 시스템과 **호환**되도록 설계합니다.
이를 통해:
- Claude Code 사용자가 기존 Plugin/Skill을 그대로 사용 가능
- ForgeCode 전용 확장 기능도 지원
- 생태계 공유로 개발자 진입 장벽 낮춤

---

## 2. Claude Code vs ForgeCode 아키텍처 비교

### 2.1 전체 구조

| 구분 | Claude Code | ForgeCode |
|------|-------------|-----------|
| **언어** | TypeScript/Node.js | Rust |
| **구조** | Monolithic (단일 패키지) | Layered (4-tier) |
| **설정 형식** | JSON/YAML | Rust traits + 파일 호환 |
| **Plugin 로딩** | 파일 기반 (런타임) | Native + 파일 호환 |

### 2.2 레이어 매핑

```
Claude Code                          ForgeCode
===========                          =========

┌─────────────────┐                  ┌─────────────────┐
│  CLI Interface  │  ←────────────→  │  Layer4: CLI    │
│  (Node.js)      │                  │  (TUI/CLI)      │
└────────┬────────┘                  └────────┬────────┘
         │                                    │
┌────────▼────────┐                  ┌────────▼────────┐
│  Agent Runtime  │  ←────────────→  │  Layer3: Agent  │
│  (Skills, Hooks)│                  │  (AgentContext) │
└────────┬────────┘                  └────────┬────────┘
         │                                    │
┌────────▼────────┐                  ┌────────▼────────┐
│  Core Services  │  ←────────────→  │  Layer2: Core   │
│  (MCP, Tools)   │                  │  (Registry,MCP) │
└────────┬────────┘                  └────────┬────────┘
         │                                    │
┌────────▼────────┐                  ┌────────▼────────┐
│  Filesystem     │  ←────────────→  │  Layer1: Found. │
│  (settings.json)│                  │  (Config,Perm)  │
└─────────────────┘                  └─────────────────┘
```

---

## 3. 호환성 목표

### 3.1 완전 호환 (읽기/쓰기)

| 항목 | Claude Code 형식 | ForgeCode 지원 |
|------|-----------------|----------------|
| **settings.json** | JSON | 읽기/쓰기 |
| **SKILL.md** | YAML frontmatter + Markdown | 읽기/쓰기 |
| **hooks.json** | JSON | 읽기/쓰기 |
| **plugin.json** | JSON manifest | 읽기/쓰기 |
| **.mcp.json** | JSON | 읽기/쓰기 |
| **CLAUDE.md** | Markdown | 읽기/쓰기 |

### 3.2 부분 호환

| 항목 | 호환 수준 | 비고 |
|------|---------|------|
| **Wasm Plugins** | 향후 지원 | wasmtime 통합 예정 |
| **Remote MCP** | 지원 | HTTP/SSE/stdio |
| **Script Plugins** | 제한적 | Node.js 스크립트는 서브프로세스로 |

---

## 4. 파일 기반 Skill 로더 설계

### 4.1 Claude Code SKILL.md 형식

```yaml
---
name: commit
description: Git 커밋 자동화
allowed-tools: Read, Bash, Grep, Glob
context: fork
agent: Explore
argument-hint: [-m message]
hooks:
  PreToolUse:
    - matcher: "Bash"
      hooks:
        - type: command
          command: "./scripts/validate.sh"
---

커밋 작성 시 다음을 수행하세요:
1. `git status`로 변경사항 확인
2. 변경된 파일들 분석
3. 커밋 메시지 작성

$ARGUMENTS 파라미터가 있으면 사용합니다.
```

### 4.2 ForgeCode SkillLoader

```rust
/// SKILL.md 파일을 파싱하여 Skill로 변환
pub struct SkillLoader {
    /// Skill 검색 경로
    paths: Vec<PathBuf>,
}

/// 파일 기반 Skill
pub struct FileBasedSkill {
    /// 정의
    definition: SkillDefinition,

    /// 시스템 프롬프트 (Markdown body)
    system_prompt: String,

    /// 설정 (YAML frontmatter)
    config: SkillConfig,

    /// 스킬별 Hooks
    hooks: Vec<SkillHook>,

    /// 소스 파일 경로
    source_path: PathBuf,
}

/// YAML frontmatter에서 파싱된 설정
#[derive(Debug, Deserialize)]
pub struct SkillConfig {
    pub name: String,
    pub description: Option<String>,

    #[serde(rename = "allowed-tools")]
    pub allowed_tools: Option<Vec<String>>,

    #[serde(rename = "disable-model-invocation")]
    pub disable_model_invocation: Option<bool>,

    #[serde(rename = "user-invocable")]
    pub user_invocable: Option<bool>,

    pub context: Option<String>,  // "fork" for subagent
    pub agent: Option<String>,    // "Explore", "Plan", custom
    pub model: Option<String>,    // model override

    #[serde(rename = "argument-hint")]
    pub argument_hint: Option<Vec<String>>,

    pub hooks: Option<SkillHooks>,
}
```

### 4.3 Skill 검색 경로 (Claude Code 호환)

```rust
impl SkillLoader {
    /// Claude Code와 동일한 우선순위로 Skill 검색
    fn search_paths(&self, working_dir: &Path) -> Vec<PathBuf> {
        vec![
            // 1. Managed (Enterprise) - 최우선
            PathBuf::from("/etc/forgecode/skills"),

            // 2. User-level
            dirs::home_dir().unwrap().join(".claude/skills"),
            dirs::home_dir().unwrap().join(".forgecode/skills"),

            // 3. Project-level
            working_dir.join(".claude/skills"),
            working_dir.join(".forgecode/skills"),

            // 4. Plugin-provided (동적)
            // plugins/<plugin>/skills/ 에서 로드
        ]
    }
}
```

---

## 5. Hook 시스템 설계

### 5.1 Claude Code hooks.json 형식

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          {
            "type": "command",
            "command": ".claude/hooks/validate-bash.sh",
            "timeout": 600
          }
        ]
      }
    ],
    "PostToolUse": [
      {
        "matcher": "*",
        "hooks": [
          {
            "type": "prompt",
            "prompt": "Was this tool call successful?"
          }
        ]
      }
    ]
  }
}
```

### 5.2 ForgeCode Hook 시스템

```rust
/// Hook 이벤트 타입 (Claude Code 호환)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HookEvent {
    SessionStart,
    SessionEnd,
    UserPromptSubmit,
    PreToolUse,
    PostToolUse,
    PostToolUseFailure,
    PermissionRequest,
    Notification,
    SubagentStart,
    SubagentStop,
    Stop,
    PreCompact,
}

/// Hook 정의
#[derive(Debug, Clone, Deserialize)]
pub struct HookDefinition {
    /// 매칭 패턴 (tool 이름 또는 "*")
    pub matcher: String,

    /// 실행할 Hook 액션들
    pub hooks: Vec<HookAction>,
}

/// Hook 액션 타입
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum HookAction {
    /// 셸 명령어 실행
    #[serde(rename = "command")]
    Command {
        command: String,
        #[serde(default = "default_timeout")]
        timeout: u64,
        #[serde(default)]
        async_exec: bool,
    },

    /// LLM 프롬프트 (단일 턴)
    #[serde(rename = "prompt")]
    Prompt {
        prompt: String,
    },

    /// Agent 실행 (멀티 턴)
    #[serde(rename = "agent")]
    Agent {
        prompt: String,
        #[serde(default)]
        allowed_tools: Vec<String>,
    },
}

/// Hook 실행 결과 (Claude Code 호환)
#[derive(Debug, Serialize)]
pub struct HookResult {
    /// 계속 진행 여부
    pub continue_execution: bool,

    /// PreToolUse 전용: 권한 결정
    pub permission_decision: Option<PermissionDecision>,

    /// 입력 수정 (PreToolUse)
    pub updated_input: Option<serde_json::Value>,

    /// Claude에게 전달할 추가 컨텍스트
    pub additional_context: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionDecision {
    Allow,
    Deny,
    Ask,
}
```

### 5.3 Hook 입출력 (Claude Code 호환)

Hook 프로세스는 stdin으로 JSON을 받고, stdout으로 JSON을 출력:

```rust
/// Hook에 전달되는 입력
#[derive(Debug, Serialize)]
pub struct HookInput {
    pub hook_event: String,
    pub session_id: String,

    // PreToolUse/PostToolUse
    pub tool_name: Option<String>,
    pub tool_input: Option<serde_json::Value>,

    // PostToolUse
    pub tool_output: Option<String>,
    pub tool_success: Option<bool>,

    // SubagentStart/Stop
    pub subagent_type: Option<String>,
    pub subagent_prompt: Option<String>,
}

/// Hook 환경 변수
pub fn hook_env_vars(ctx: &HookContext) -> HashMap<String, String> {
    let mut env = HashMap::new();
    env.insert("CLAUDE_PROJECT_DIR".into(), ctx.project_dir.display().to_string());
    env.insert("FORGE_PROJECT_DIR".into(), ctx.project_dir.display().to_string());

    if let Some(plugin_root) = &ctx.plugin_root {
        env.insert("CLAUDE_PLUGIN_ROOT".into(), plugin_root.display().to_string());
        env.insert("FORGE_PLUGIN_ROOT".into(), plugin_root.display().to_string());
    }

    env
}
```

---

## 6. Plugin 매니페스트 호환성

### 6.1 Claude Code plugin.json

```json
{
  "name": "my-plugin",
  "version": "1.0.0",
  "description": "My awesome plugin",
  "author": {
    "name": "Author",
    "email": "author@example.com"
  },
  "skills": "./skills/",
  "hooks": "./hooks/hooks.json",
  "mcpServers": "./.mcp.json"
}
```

### 6.2 ForgeCode 호환 파서

```rust
/// Claude Code plugin.json 파서
#[derive(Debug, Deserialize)]
pub struct ClaudePluginManifest {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub author: Option<PluginAuthor>,
    pub homepage: Option<String>,
    pub repository: Option<String>,
    pub license: Option<String>,
    pub keywords: Option<Vec<String>>,

    // 경로 (상대 경로)
    pub skills: Option<String>,
    pub hooks: Option<String>,
    #[serde(rename = "mcpServers")]
    pub mcp_servers: Option<String>,
    #[serde(rename = "lspServers")]
    pub lsp_servers: Option<String>,
}

impl From<ClaudePluginManifest> for PluginManifest {
    fn from(claude: ClaudePluginManifest) -> Self {
        PluginManifest {
            id: format!("claude.{}", claude.name),
            name: claude.name,
            version: PluginVersion::parse(&claude.version).unwrap_or_default(),
            description: claude.description.unwrap_or_default(),
            author: claude.author.map(|a| a.name),
            // ...
        }
    }
}
```

---

## 7. 설정 파일 통합

### 7.1 통합 설정 로더

```rust
/// 여러 설정 파일을 통합하여 로드
pub struct ConfigLoader {
    working_dir: PathBuf,
}

impl ConfigLoader {
    /// Claude Code와 ForgeCode 설정을 모두 로드
    pub fn load(&self) -> Result<IntegratedConfig> {
        let mut config = IntegratedConfig::default();

        // 1. User-level (낮은 우선순위)
        self.load_if_exists(&dirs::home_dir()?.join(".claude/settings.json"), &mut config)?;
        self.load_if_exists(&dirs::home_dir()?.join(".forgecode/settings.json"), &mut config)?;

        // 2. Project-level (버전 관리)
        self.load_if_exists(&self.working_dir.join(".claude/settings.json"), &mut config)?;
        self.load_if_exists(&self.working_dir.join(".forgecode/settings.json"), &mut config)?;

        // 3. Local (gitignored, 최고 우선순위)
        self.load_if_exists(&self.working_dir.join(".claude/settings.local.json"), &mut config)?;
        self.load_if_exists(&self.working_dir.join(".forgecode/settings.local.json"), &mut config)?;

        Ok(config)
    }
}
```

---

## 8. MCP 서버 호환성

### 8.1 .mcp.json 형식 (공통)

```json
{
  "mcpServers": {
    "database": {
      "command": "node",
      "args": ["./servers/db-server.js"],
      "env": {
        "DB_PATH": "./data/db.sqlite"
      }
    },
    "remote-api": {
      "type": "http",
      "url": "https://api.example.com/mcp"
    }
  }
}
```

### 8.2 ForgeCode MCP 통합

```rust
/// Claude Code .mcp.json 호환 파서
#[derive(Debug, Deserialize)]
pub struct McpConfigFile {
    #[serde(rename = "mcpServers")]
    pub servers: HashMap<String, McpServerDef>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum McpServerDef {
    /// stdio 기반 (command + args)
    Stdio {
        command: String,
        args: Option<Vec<String>>,
        env: Option<HashMap<String, String>>,
    },
    /// HTTP 기반
    Http {
        #[serde(rename = "type")]
        transport_type: String,  // "http" or "sse"
        url: String,
    },
}

impl From<McpServerDef> for McpServerConfig {
    fn from(def: McpServerDef) -> Self {
        match def {
            McpServerDef::Stdio { command, args, env } => {
                McpServerConfig {
                    transport: McpTransportConfig::Stdio {
                        command,
                        args: args.unwrap_or_default(),
                        env: env.unwrap_or_default(),
                    },
                    ..Default::default()
                }
            }
            McpServerDef::Http { url, .. } => {
                McpServerConfig {
                    transport: McpTransportConfig::Http { url },
                    ..Default::default()
                }
            }
        }
    }
}
```

---

## 9. 구현 우선순위

### Phase 1: 기본 호환성 (필수)
- [x] DynamicRegistry 시스템
- [ ] SKILL.md 파서 및 FileBasedSkill
- [ ] settings.json 로더
- [ ] hooks.json 파서

### Phase 2: Plugin 시스템
- [ ] plugin.json 파서
- [ ] Plugin 디렉토리 구조 지원
- [ ] 스킬/훅 경로 해석

### Phase 3: MCP 통합
- [ ] .mcp.json 파서
- [ ] Plugin 내 MCP 서버 자동 시작
- [ ] 환경변수 치환 (${CLAUDE_PLUGIN_ROOT})

### Phase 4: 고급 기능
- [ ] Wasm Plugin 지원
- [ ] Plugin 마켓플레이스
- [ ] Remote Plugin 설치

---

## 10. 디렉토리 구조 예시

```
project/
├── .claude/                    # Claude Code 호환
│   ├── settings.json           # 프로젝트 설정
│   ├── settings.local.json     # 로컬 설정 (gitignored)
│   ├── CLAUDE.md               # 프로젝트 컨텍스트
│   ├── skills/                 # 프로젝트 스킬
│   │   └── deploy/
│   │       └── SKILL.md
│   └── hooks/
│       └── hooks.json
│
├── .forgecode/                 # ForgeCode 전용 (선택)
│   ├── settings.json
│   └── skills/
│       └── custom/
│           └── SKILL.md
│
└── plugins/                    # 로컬 플러그인
    └── my-plugin/
        ├── .claude-plugin/
        │   └── plugin.json
        ├── skills/
        │   └── special/
        │       └── SKILL.md
        ├── hooks/
        │   └── hooks.json
        └── .mcp.json
```

---

## 11. 테스트 전략

### 11.1 호환성 테스트
- Claude Code의 공식 예제 Plugin/Skill로 테스트
- 양방향 설정 파일 읽기/쓰기 검증

### 11.2 경로 우선순위 테스트
- 같은 이름의 Skill이 여러 경로에 있을 때 올바른 것이 로드되는지

### 11.3 Hook 동작 테스트
- PreToolUse에서 차단/허용/수정이 제대로 동작하는지
- Hook 타임아웃 처리

