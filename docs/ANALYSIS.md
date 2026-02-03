# ForgeCode 아키텍처 분석 및 개선 방안

## 1. 현재 아키텍처 분석

### 1.1 크레이트 간 의존성 현황

```
Layer4-cli (forge-cli)
    ├── forge-agent (Layer3)
    ├── forge-tool (Layer2)      ← 직접 의존
    ├── forge-task (Layer2)
    ├── forge-provider (Layer2)
    └── forge-foundation (Layer1)

Layer3-agent (forge-agent)
    ├── forge-tool (Layer2)      ← 핵심 의존
    ├── forge-task (Layer2)      ← 의존하지만 미사용!
    ├── forge-provider (Layer2)
    └── forge-foundation (Layer1)

Layer2-core (forge-core)
    ├── forge-foundation (Layer1)
    └── (자체 tool 모듈 중복 구현)
```

### 1.2 Tool 시스템 비교

| 항목 | Layer2-tool | Layer2-core/tool |
|------|-------------|------------------|
| **Trait 정의** | 자체 `Tool` trait | Layer1 `forge_foundation::Tool` trait |
| **도구 수** | 7개 (forgecmd 포함) | 6개 |
| **PTY 지원** | ✅ ForgeCmdTool | ❌ |
| **사용처** | Layer3-agent, Layer4-cli | 미사용 |
| **권한 연동** | Layer1 PermissionService | Layer1 PermissionService |
| **컨텍스트** | ToolContext (자체) | RuntimeContext (Layer1 ToolContext 구현) |

### 1.3 Task 시스템 상태

**Layer2-task 완성도: ~90%**

| 기능 | 상태 | 비고 |
|------|------|------|
| Task/TaskId/TaskResult | ✅ 완성 | |
| TaskState 상태 머신 | ✅ 완성 | 7개 상태 |
| LocalExecutor | ✅ 완성 | 타임아웃 지원 |
| ContainerExecutor | ✅ 완성 | Docker Bollard |
| TaskManager | ✅ 완성 | 동시성 제어 (max=4) |
| **Layer3 연동** | ❌ 미사용 | 핵심 문제 |

**문제점**: Layer3-agent가 forge-task를 의존하지만, 도구들이 직접 `tokio::process::Command`로 실행하여 TaskManager를 우회함.

---

## 2. 최신 Agent 아키텍처 연구 결과

### 2.1 Claude Code Task Tool 아키텍처

**핵심 개념**: Task tool로 전문화된 Sub-agent를 생성하여 복잡한 작업을 위임

**Sub-agent 유형**:
- **General-Purpose**: 모든 도구 접근, 복잡한 다단계 작업
- **Explore**: 읽기 전용, 코드베이스 탐색/검색 최적화
- **Plan**: 아키텍처 설계, 수정 불가
- **Bash**: 명령 실행 전문
- **사용자 정의**: 커스텀 프롬프트, 도구 제한

**백그라운드 실행**:
- 30초 이상 작업을 비동기 실행
- 메인 세션 계속 진행 가능
- 완료 시 알림, 결과 파일 저장
- `/tasks` 명령으로 상태 조회

**컨텍스트 격리**:
- 각 sub-agent는 독립적 컨텍스트
- 메인 대화 이력에 자동 접근 불가
- 프롬프트에 필요한 정보 명시 필요

### 2.2 Deep Agent Architecture (3-Agent 패턴)

**역할 분담**:

```
┌─────────────────────────────────────────────────────────────┐
│                     Orchestrator                             │
│  - 직접 코드 접근 불가 (강제 위임)                            │
│  - 전략적 작업 분해 및 조율                                   │
│  - Context Store 관리                                        │
└───────────────────────┬─────────────────────────────────────┘
                        │
        ┌───────────────┴───────────────┐
        ▼                               ▼
┌───────────────────┐         ┌───────────────────┐
│    Explorer       │         │      Coder        │
│  - 읽기 전용      │         │  - 읽기/쓰기     │
│  - 코드베이스 탐색 │         │  - 구현 실행     │
│  - 테스트 실행 가능│         │  - 컨텍스트 수신 │
│  - 발견사항 보고   │         │  - 결과 보고     │
└───────────────────┘         └───────────────────┘
```

**Context Store 혁신**:
- 지식 축적: 모든 발견이 영구적
- 중복 작업 제거
- 각 에이전트는 필요한 컨텍스트만 수신
- 정제된 결과만 반환 (컨텍스트 폭발 방지)

**통신 프로토콜**:
```xml
<task_create>
  agent_name: explorer
  title: 파일 구조 분석
  context_refs:
    - project_structure
</task_create>
```

### 2.3 Multi-Agent Orchestration 트렌드

**2025년 주요 패턴**:

1. **Conductor → Orchestrator 진화**
   - 단일 에이전트 지휘 → 다중 자율 에이전트 조율
   - 개발자 역할: 구현자 → 관리자/조율자

2. **전문화된 에이전트 팀**
   - 각 에이전트가 특정 도메인/기능 담당
   - 코드/프롬프트 복잡도 감소

3. **컨텍스트 관리 전략**
   - 지식 축적 + 컨텍스트 경량화
   - 정제된 결과만 전달
   - 중복 탐색 방지

---

## 3. ForgeCode 문제점 및 개선 방안

### 3.1 해결해야 할 핵심 문제

#### 문제 1: Tool 시스템 중복
**현황**: Layer2-tool과 Layer2-core에 동일한 도구 구현
**결정**: Layer2-tool 유지, Layer2-core의 tool 모듈 제거

**이유**:
- Layer2-tool이 Layer3-agent에서 실제 사용 중
- ForgeCmdTool (PTY 지원)은 Layer2-tool에만 있음
- Layer2-core의 도구는 아무도 사용하지 않음

#### 문제 2: Task 시스템 미사용
**현황**: TaskManager가 구현되었지만 도구들이 직접 Command 실행
**해결**: ToolContext에 TaskManager 통합

**개선 후 흐름**:
```
Tool.execute(ctx, params)
    └── ctx.task_manager.submit(task)
        └── TaskManager.execute()
            ├── LocalExecutor (기본)
            └── ContainerExecutor (격리 필요시)
```

#### 문제 3: Sub-agent 시스템 부재
**현황**: 단일 Agent 루프만 존재
**해결**: Task Tool 패턴 도입

**구현 방향**:
```rust
pub enum SubAgentType {
    Explore,    // 읽기 전용, 탐색 최적화
    Plan,       // 계획 수립, 수정 불가
    General,    // 모든 도구 접근
    Bash,       // 명령 실행만
    Custom(String),  // 사용자 정의
}

pub struct SubAgent {
    agent_type: SubAgentType,
    prompt: String,
    allowed_tools: Vec<String>,
    context: SubAgentContext,
}
```

### 3.2 권장 아키텍처 변경

#### 변경 1: Layer2-core 정리

```
Layer2-core (변경 후)
├── lsp/          ← 유지 (LSP 클라이언트)
├── mcp/          ← 구현 필요 (MCP 클라이언트)
└── (tool 모듈 제거)
```

#### 변경 2: Task 시스템 연동

```rust
// Layer2-tool의 ToolContext 확장
pub struct ToolContext {
    pub session_id: String,
    pub working_dir: PathBuf,
    pub permissions: Arc<PermissionService>,
    pub task_manager: Arc<TaskManager>,  // 추가
    pub auto_approve: bool,
}
```

#### 변경 3: Sub-agent 시스템 추가

```
Layer3-agent (변경 후)
├── agent.rs      ← 메인 에이전트
├── subagent/     ← 새로 추가
│   ├── mod.rs
│   ├── types.rs      # SubAgentType, SubAgentContext
│   ├── registry.rs   # SubAgentRegistry
│   ├── explore.rs    # ExploreAgent
│   ├── plan.rs       # PlanAgent
│   └── general.rs    # GeneralAgent
├── context.rs
├── session.rs
└── history.rs
```

### 3.3 구현 우선순위

| 우선순위 | 작업 | 난이도 | 효과 |
|----------|------|--------|------|
| 🔴 1 | Layer2-core tool 모듈 제거 | 쉬움 | 중복 해소 |
| 🔴 2 | ToolContext에 TaskManager 통합 | 중간 | 작업 관리 개선 |
| 🟡 3 | MCP 클라이언트 구현 | 어려움 | 확장성 |
| 🟡 4 | Sub-agent 기본 구조 | 중간 | 병렬 작업 |
| 🟢 5 | Context Store 구현 | 어려움 | 지식 축적 |

---

## 4. 삭제/통합 권장 사항

### 4.1 Layer2-tool 유지 권장

**이유**:
1. Layer3-agent, Layer4-cli가 직접 사용 중
2. ForgeCmdTool (PTY 지원)은 고유 기능
3. 7개 도구가 완전히 구현됨
4. Layer1 PermissionService와 연동됨

**조치**:
- Layer2-tool 그대로 유지
- Layer2-core의 tool 모듈만 삭제
- Layer2-core는 LSP, MCP에 집중

### 4.2 Layer2-core tool 모듈 삭제 권장

**삭제 대상**:
```
Layer2-core/src/tool/
├── mod.rs
├── context.rs
├── registry.rs
└── builtin/
    ├── mod.rs
    ├── bash.rs
    ├── read.rs
    ├── write.rs
    ├── edit.rs
    ├── glob.rs
    └── grep.rs
```

**lib.rs 수정**:
```rust
// 삭제: pub mod tool;
// 삭제: pub use tool::*;

// 유지
pub mod lsp;
pub mod mcp;  // 구현 필요
```

### 4.3 Layer2-task 활용 권장

**현재**: 구현 완료되었지만 미사용
**권장**: Layer2-tool의 도구들이 TaskManager를 통해 실행하도록 변경

---

## 5. 결론

ForgeCode는 기본적인 구조가 잘 설계되어 있지만, 몇 가지 핵심 문제가 있습니다:

1. **Tool 중복**: Layer2-core의 tool 모듈 삭제로 해결
2. **Task 미사용**: ToolContext에 TaskManager 통합으로 해결
3. **Sub-agent 부재**: 점진적으로 구현

최신 트렌드(Claude Code Task Tool, Deep Agent Architecture)를 참고하여
Sub-agent 시스템과 Context Store를 도입하면 경쟁력 있는 제품이 될 수 있습니다.

---

## 참고 자료

- [Claude Code Task Tool](https://dev.to/bhaidar/the-task-tool-claude-codes-agent-orchestration-system-4bf2)
- [Deep Agent Architecture](https://dev.to/apssouza22/a-deep-dive-into-deep-agent-architecture-for-ai-coding-assistants-3c8b)
- [Claude Code Sub-agents](https://code.claude.com/docs/en/sub-agents)
- [Anthropic Agent SDK](https://www.anthropic.com/engineering/building-agents-with-the-claude-agent-sdk)
- [AI Agent Orchestration Patterns](https://learn.microsoft.com/en-us/azure/architecture/ai-ml/guide/ai-agent-design-patterns)
