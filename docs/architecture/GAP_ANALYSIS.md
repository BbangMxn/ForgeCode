# ForgeCode Gap Analysis

> 오픈소스 AI 코딩 어시스턴트들과 비교한 ForgeCode 구현 상태 분석

## 분석 대상 오픈소스

| 프로젝트 | 특징 | 언어 |
|---------|------|------|
| Claude Code | Anthropic 공식 CLI, Master Agent Loop, h2A Steering | TypeScript |
| OpenAI Codex | Responses API, Sandbox, MCP Server Mode | Rust |
| Aider | Architect/Editor 패턴, Repo Map, Git 통합 | Python |
| Continue.dev | IDE 통합, Context Providers, Hub 생태계 | TypeScript |

---

## 1. 구현 완료 기능 (ForgeCode vs 오픈소스)

### Agent Loop ✅
| 기능 | Claude Code | Codex | ForgeCode |
|-----|-------------|-------|-----------|
| Single-threaded Master Loop | ✅ | ✅ | ✅ |
| while(tool_call) 패턴 | ✅ | ✅ | ✅ |
| Sequential Tool Execution | ✅ | ✅ | ✅ |
| Streaming Response | ✅ | ✅ | ✅ |

### Hook System ✅
| Hook 이벤트 | Claude Code | ForgeCode |
|------------|-------------|-----------|
| BeforeAgent | ✅ | ✅ |
| AfterAgent | ✅ | ✅ |
| BeforeTool | ✅ (PreToolUse) | ✅ |
| AfterTool | ✅ (PostToolUse) | ✅ |
| BeforeCompress | ✅ (PreCompact) | ✅ |
| AfterCompress | ✅ | ✅ |
| BeforeTurn | ✅ | ✅ |
| AfterTurn | ✅ | ✅ |

### Context Compression ✅
| 기능 | Claude Code | ForgeCode |
|-----|-------------|-----------|
| Auto-compress at threshold | ✅ (92%) | ✅ (92%) |
| Manual /compact | ✅ | ✅ |
| LLM-based summarization | ✅ | ✅ |
| Token tracking | ✅ | ✅ |

### Steering (실시간 제어) ✅
| 기능 | Claude Code | ForgeCode |
|-----|-------------|-----------|
| Pause/Resume | ✅ | ✅ |
| Stop with reason | ✅ | ✅ |
| Redirect/Inject context | ✅ | ✅ |
| Permission mode change | ✅ | ✅ |

### Tool System ✅
| 도구 | Claude Code | Codex | ForgeCode |
|-----|-------------|-------|-----------|
| Read | ✅ | ✅ (read_file) | ✅ |
| Write | ✅ | ✅ (write_file) | ✅ |
| Edit | ✅ | ✅ (edit_file) | ✅ |
| Bash | ✅ | ✅ (shell) | ✅ |
| Glob | ✅ | ✅ (list_files) | ✅ |
| Grep | ✅ | ✅ (search) | ✅ |
| WebSearch | ✅ | ❌ | ⚠️ (stub) |
| WebFetch | ✅ | ❌ | ⚠️ (stub) |
| Task (subagent) | ✅ | ✅ | ✅ |

### MCP Integration ✅
| 기능 | Claude Code | Codex | ForgeCode |
|-----|-------------|-------|-----------|
| MCP Client | ✅ | ✅ | ✅ |
| MCP Server Mode | ❌ | ✅ | ⚠️ (partial) |
| Tool Search (dynamic) | ✅ | ❌ | ❌ |
| stdio transport | ✅ | ✅ | ✅ |
| HTTP transport | ✅ | ✅ | ✅ |

### Error Recovery ✅
| 기능 | Claude Code | Codex | ForgeCode |
|-----|-------------|-------|-----------|
| Retry with backoff | ✅ | ✅ | ✅ |
| Recovery strategies | ✅ | ✅ | ✅ |
| Tool suggestion | ✅ | ❌ | ✅ |
| Permission escalation | ✅ | ✅ | ✅ |

---

## 2. 부족한 기능 (Gap)

### 🔴 Critical Gaps (핵심 기능 부재)

#### 2.1 WebSearch / WebFetch 도구
- **현재**: Stub 구현만 존재
- **필요**: 실제 웹 검색/페이지 가져오기 기능
- **참고**: Claude Code는 Brave Search API 사용

#### 2.2 MCP Tool Search (Dynamic Loading)
- **현재**: 모든 MCP 도구를 항상 로드
- **필요**: 컨텍스트 10% 이상 시 동적 로딩
- **이유**: 대규모 MCP 서버 환경에서 컨텍스트 효율성

#### 2.3 Session Forking
- **현재**: 세션 재개만 지원
- **필요**: 세션 분기 (branch) 기능
- **참고**: Claude Code의 `--fork` 옵션

### 🟡 Important Gaps (중요 기능 부재)

#### 2.4 Sandbox Execution
- **현재**: 직접 실행만 지원
- **필요**: 
  - macOS: Seatbelt sandbox
  - Linux: Landlock + seccomp
  - Container: Docker isolation
- **참고**: Codex의 3단계 샌드박스 시스템

#### 2.5 Repository Map (Aider 스타일)
- **현재**: RepoMap 모듈 존재하지만 기본 수준
- **필요**:
  - Tree-sitter AST 기반 분석
  - Graph-based 랭킹
  - 동적 토큰 예산 관리
- **참고**: Aider의 `--map-tokens` 옵션

#### 2.6 Git Integration
- **현재**: 기본 git 명령 실행 가능
- **필요**:
  - Auto-commit (Aider 스타일)
  - Ghost commit (Codex 스타일)
  - Checkpoint/Rollback
  - Diff 기반 커밋 메시지 생성
- **참고**: Aider의 `--auto-commits`

#### 2.7 Architect/Editor Mode
- **현재**: 단일 Agent 모드
- **필요**:
  - Architect: 고수준 계획 생성
  - Editor: 실제 코드 수정
  - 분리된 프롬프트와 책임
- **참고**: Aider의 architect 모드

### 🟢 Nice-to-have (부가 기능)

#### 2.8 Prompt Caching
- **현재**: Response 캐싱만 존재
- **필요**:
  - System prompt 캐싱 (Anthropic API)
  - Read-only 파일 캐싱
  - Keepalive ping
- **참고**: Aider의 `--cache-prompts`

#### 2.9 Voice Mode
- **현재**: 없음
- **필요**:
  - 음성 입력 (Whisper)
  - 실시간 transcription
- **참고**: Aider의 `/voice` 명령

#### 2.10 Linting Integration
- **현재**: 없음
- **필요**:
  - 자동 lint 실행
  - Tree-sitter 기반 에러 컨텍스트
  - Auto-fix 시도
- **참고**: Aider의 `--auto-lint`

#### 2.11 IDE Extension
- **현재**: CLI/TUI만 존재
- **필요**:
  - VS Code Extension
  - JetBrains Plugin
- **참고**: Continue.dev 아키텍처

#### 2.12 Embeddings & Semantic Search
- **현재**: Grep 기반 검색만
- **필요**:
  - 로컬 임베딩 생성
  - Vector DB 저장
  - Reranking
- **참고**: Continue.dev의 codebase indexing

---

## 3. 구현 우선순위

### Phase 1: Core Gaps (핵심)
1. **Sandbox Execution** - 보안 필수
2. **WebSearch/WebFetch** - 기본 도구
3. **Git Auto-commit** - 개발자 경험

### Phase 2: Enhancement (향상)
4. **Repository Map 고도화** - 컨텍스트 효율
5. **Session Forking** - 실험 지원
6. **MCP Tool Search** - 확장성

### Phase 3: Advanced (고급)
7. **Architect/Editor Mode** - SOTA 성능
8. **Prompt Caching** - 비용 절감
9. **Embeddings** - 의미 검색

### Phase 4: Ecosystem (생태계)
10. **IDE Extension** - 접근성
11. **Voice Mode** - 편의성
12. **Plugin Marketplace** - 커뮤니티

---

## 4. 기술적 권장사항

### Sandbox 구현
```rust
// Platform-specific sandbox
#[cfg(target_os = "macos")]
mod seatbelt {
    // sandbox-exec with profile
}

#[cfg(target_os = "linux")]
mod landlock {
    // Landlock LSM + seccomp BPF
}
```

### Git Integration
```rust
pub trait GitIntegration {
    fn auto_commit(&self, message: &str) -> Result<()>;
    fn create_checkpoint(&self) -> Result<CheckpointId>;
    fn rollback(&self, checkpoint: CheckpointId) -> Result<()>;
    fn generate_commit_message(&self, diff: &str) -> Result<String>;
}
```

### Architect/Editor Mode
```rust
pub enum AgentMode {
    /// 단일 에이전트 (현재)
    Unified,
    /// Architect가 계획, Editor가 실행
    ArchitectEditor {
        architect_model: String,
        editor_model: String,
    },
}
```

---

## 5. 결론

ForgeCode는 **85-90% 완성도**로 핵심 Agent Loop, Tool System, MCP 통합이 잘 구현되어 있습니다.

주요 Gap:
- **Sandbox**: 프로덕션 보안 필수
- **Git Integration**: 개발자 경험의 핵심
- **Repository Map 고도화**: 컨텍스트 효율성

Claude Code와 Codex 대비 장점:
- **Provider Agnostic**: 다양한 LLM 지원
- **Modular Architecture**: 확장 가능한 구조
- **Rust Performance**: 빠른 실행 속도

권장 액션:
1. Sandbox 시스템 구현 (보안)
2. Git 자동 커밋 추가 (UX)
3. RepoMap Tree-sitter 통합 (효율)
