# forgecmd 연구 자료

> LLM이 안전하게 사용할 수 있는 격리된 명령 쉘 환경을 위한 연구 자료
> 
> 최종 업데이트: 2026-02-03

---

## 목차

1. [개요](#1-개요)
2. [PTY 라이브러리 비교](#2-pty-라이브러리-비교)
3. [기존 CLI 도구 분석](#3-기존-cli-도구-분석)
4. [명령어 추적/로깅](#4-명령어-추적로깅)
5. [샌드박싱/격리](#5-샌드박싱격리)
6. [위험 명령 탐지](#6-위험-명령-탐지)
7. [권한 시스템 설계](#7-권한-시스템-설계)
8. [Layer1 연동](#8-layer1-forge-foundation-연동)
9. [의존성 라이브러리](#9-의존성-라이브러리)
10. [아키텍처 설계](#10-아키텍처-설계)
11. [구현 계획](#11-구현-계획)
12. [참고 링크](#12-참고-링크)

---

## 1. 개요

### 1.1 목적

forgecmd는 ForgeCode의 LLM 에이전트가 사용하는 전용 명령 쉘 환경을 제공합니다.

### 1.2 핵심 요구사항

| 요구사항 | 설명 | 우선순위 |
|----------|------|----------|
| PTY 지원 | 대화형 명령 실행 (vim, htop 등) | 높음 |
| 권한 제어 | 위험 명령 차단, 사용자 승인 | 높음 |
| 명령 추적 | 실행 히스토리, 출력 로깅 | 중간 |
| 환경 격리 | 환경변수 필터링, 경로 제한 | 중간 |
| 크로스 플랫폼 | Windows/Linux/macOS 지원 | 높음 |

### 1.3 Layer 구조

```
Layer 4: forge-cli (TUI)
Layer 3: forge-agent (에이전트 로직)
Layer 2: forge-tool (forgecmd 포함) ← 여기
Layer 1: forge-foundation (권한, 저장소)
```

---

## 2. PTY 라이브러리 비교

### 2.1 비교 표

| 크레이트 | 버전 | 월간 다운로드 | Async | 플랫폼 | 라이선스 |
|----------|------|--------------|-------|--------|----------|
| **portable-pty** | 0.9.0 | 703,789 | smol | Win/Mac/Linux | MIT |
| **pty-process** | 0.5.3 | 97,980 | Tokio | Unix 전용 | MIT |
| **rust-pty** | 0.1.0 | 신규 | Tokio | Win/Mac/Linux | MIT/Apache |

### 2.2 portable-pty (권장)

- **저장소**: https://github.com/wez/wezterm/tree/main/pty
- **wezterm 프로젝트의 일부** - 안정성 검증됨
- **월간 703K 다운로드** - 가장 널리 사용됨

#### 장점
- 크로스 플랫폼 (Windows ConPTY 지원)
- 성숙한 라이브러리 (wezterm에서 사용)
- 풍부한 API

#### 단점
- 동기 API (spawn_blocking 필요)
- smol 런타임 내장 (Tokio와 별도)

#### 사용 예제
```rust
use portable_pty::{native_pty_system, PtySize, CommandBuilder};

let pty_system = native_pty_system();
let pair = pty_system.openpty(PtySize {
    rows: 24,
    cols: 80,
    pixel_width: 0,
    pixel_height: 0,
})?;

let cmd = CommandBuilder::new("bash");
let child = pair.slave.spawn_command(cmd)?;

// 별도 스레드에서 읽기 (deadlock 방지)
let reader = pair.master.try_clone_reader()?;
let writer = pair.master.take_writer()?;
```

#### Tokio 통합
```rust
// spawn_blocking으로 동기 API 래핑
let output = tokio::task::spawn_blocking(move || {
    let mut reader = pair.master.try_clone_reader()?;
    let mut buf = vec![0u8; 4096];
    reader.read(&mut buf)
}).await??;
```

### 2.3 pty-process (Unix 전용 대안)

- **저장소**: https://crates.io/crates/pty-process
- **네이티브 Tokio 지원**

#### 장점
- AsyncRead/AsyncWrite 구현
- 간단한 API
- Tokio 네이티브

#### 단점
- **Unix 전용 (Windows 미지원)**

#### 사용 예제
```rust
// Cargo.toml: pty-process = { version = "0.5", features = ["async"] }

let (mut pty, pts) = pty_process::open()?;
pty.resize(pty_process::Size::new(24, 80))?;

let mut cmd = pty_process::Command::new("bash");
let child = cmd.spawn(pts)?;

// pty는 AsyncRead + AsyncWrite 구현
```

### 2.4 rust-pty (신규)

- **버전**: 0.1.0 (2026-01-05)
- **크로스 플랫폼 + Tokio 네이티브**
- 아직 초기 단계, 커뮤니티 작음

### 2.5 선정 결론

**권장: portable-pty**

| 기준 | 선택 이유 |
|------|-----------|
| 안정성 | wezterm에서 검증됨 |
| 플랫폼 | Windows 지원 필수 |
| 다운로드 | 월 70만+ |
| 유지보수 | 활발한 개발 |

---

## 3. 기존 CLI 도구 분석

### 3.1 Claude Code

| 항목 | 내용 |
|------|------|
| **방식** | Stateless (매번 새 프로세스) |
| **구현** | subprocess.Popen + stdin/stdout 파이프 |
| **PTY** | 미지원 |
| **한계** | 대화형 명령 불가 (vim, htop hang) |
| **이슈** | [#9881](https://github.com/anthropics/claude-code/issues/9881) |

```
Windows: cmd.exe /C "명령어"
Linux/Mac: sh -c "명령어"
```

#### 권한 패턴 문법
```
규칙 평가 순서: deny → ask → allow (첫 매칭 승)

Bash              # 모든 bash 명령 허용
Bash(npm run *)   # npm run으로 시작하는 명령
Bash(* --help)    # --help로 끝나는 명령
```

### 3.2 Gemini CLI

| 항목 | 내용 |
|------|------|
| **방식** | PTY 지원 (v0.9.0+) |
| **구현** | node-pty (optionalDependencies) |
| **설정** | `tools.shell.enableInteractiveShell: true` |
| **fallback** | child_process |

#### 기능
- 양방향 통신
- 터미널 리사이즈
- 컬러/커서 지원
- vim, htop, git rebase -i 가능
- Ctrl+F로 포커스 전환

### 3.3 비교 요약

| 기능 | Claude Code | Gemini CLI | ForgeCode (목표) |
|------|-------------|------------|------------------|
| PTY | ❌ | ✅ | ✅ |
| 대화형 | ❌ | ✅ | ✅ |
| 권한 제어 | 기본 | 기본 | **강화** |
| 명령 필터링 | 간단 | 간단 | **세밀** |
| 환경 격리 | ❌ | ❌ | ✅ |

---

## 4. 명령어 추적/로깅

### 4.1 Atuin (참고)

- **저장소**: https://github.com/atuinsh/atuin
- **용도**: 쉘 히스토리 대체 (SQLite)
- **언어**: Rust

#### 저장 데이터
```sql
- command (명령어)
- cwd (작업 디렉토리)
- duration (실행 시간)
- exit_code (종료 코드)
- hostname
- session
- timestamp
```

### 4.2 ai-session (참고)

- **저장소**: https://crates.io/crates/ai-session
- **용도**: AI 에이전트 전용 세션 관리

#### 주요 기능
- 93% 토큰 압축
- zstd 압축 저장
- 출력 파싱 (빌드 결과, 에러 등)
- 명령 히스토리 감사 추적

### 4.3 forgecmd 히스토리 스키마

```sql
CREATE TABLE IF NOT EXISTS forgecmd_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    pty_session_id TEXT NOT NULL,
    command TEXT NOT NULL,
    cwd TEXT NOT NULL,
    
    -- 실행 정보
    started_at TEXT NOT NULL,
    ended_at TEXT,
    duration_ms INTEGER,
    exit_code INTEGER,
    
    -- 출력
    stdout TEXT,
    stderr TEXT,
    output_truncated BOOLEAN DEFAULT FALSE,
    
    -- 권한
    permission_status TEXT,  -- granted/denied/auto_approved
    permission_scope TEXT,   -- once/session/permanent
    risk_score INTEGER,
    
    -- 메타데이터
    env_snapshot TEXT,  -- JSON
    
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
);

CREATE INDEX idx_forgecmd_session ON forgecmd_history(session_id, started_at);
CREATE INDEX idx_forgecmd_command ON forgecmd_history(command);
```

---

## 5. 샌드박싱/격리

### 5.1 Linux 기술

| 기술 | 설명 | 격리 수준 |
|------|------|-----------|
| Namespace | PID, MNT, NET 격리 | 중간 |
| seccomp | syscall 필터링 | 높음 |
| Landlock | 파일시스템 제한 (5.13+) | 중간 |
| cgroups | 리소스 제한 | 낮음 |

### 5.2 Rust 샌드박스 라이브러리

| 크레이트 | 기능 | 플랫폼 |
|----------|------|--------|
| hakoniwa | namespace, seccomp, landlock | Linux |
| sandbox-rs | 프로세스 격리, syscall 필터링 | Linux |
| landlock | Landlock LSM 바인딩 | Linux 5.13+ |

### 5.3 forgecmd 격리 전략

**Phase 1 (애플리케이션 레벨)**:
- 환경변수 필터링
- 작업 디렉토리 제한
- 위험 명령 차단

**Phase 2 (선택적, 고급)**:
- Landlock 파일시스템 제한 (Linux)
- Windows: 별도 사용자 컨텍스트

---

## 6. 위험 명령 탐지

### 6.1 위험도 분류 체계

학술 연구 기반 5단계 분류 (R0-R4):

| 레벨 | risk_score | 예시 | 처리 |
|------|------------|------|------|
| R0 | 0-2 | ls, cat, pwd | 자동 승인 |
| R1 | 3-4 | mkdir, touch | 세션 승인 |
| R2 | 5 | cp, mv, chmod | 세션 승인 |
| R3 | 6-7 | rm, git push | 매번 확인 |
| R4 | 8-10 | rm -rf /, dd | 차단 |

### 6.2 DCG 참고

- **저장소**: https://github.com/Dicklesworthstone/destructive_command_guard
- **용도**: Claude Code 훅

#### 차단 목록
```
Git:
- git reset --hard
- git push --force (--force-with-lease 제외)
- git clean -f
- git stash drop/clear

파일시스템:
- rm -rf (단, /tmp 제외)
- dd if=
- mkfs.*

쉘:
- :(){ :|:& };:  (fork bomb)
- > /dev/sda
```

#### 허용 목록
```
- git checkout -b <branch>
- rm -rf /tmp/*
- git push --force-with-lease
```

### 6.3 forgecmd 명령 카테고리

```rust
pub enum CommandCategory {
    /// 읽기 전용 - 자동 승인
    ReadOnly,       // ls, cat, pwd, echo, git status
    
    /// 안전한 쓰기 - 세션 승인
    SafeWrite,      // mkdir, touch, git add, git commit
    
    /// 주의 필요 - 매번 확인
    Caution,        // rm, mv, git push
    
    /// 위험 - 기본 차단
    Dangerous,      // rm -rf, git reset --hard
    
    /// 금지 - 항상 차단
    Forbidden,      // rm -rf /, fork bomb
    
    /// 대화형 - 특별 처리
    Interactive,    // vim, htop, python REPL
}
```

---

## 7. 권한 시스템 설계

### 7.1 권한 체크 흐름

```
┌─────────────────────────────────────────────────────┐
│                 forgecmd 권한 체크                   │
├─────────────────────────────────────────────────────┤
│                                                     │
│  명령 입력                                          │
│      │                                              │
│      ▼                                              │
│  ┌─────────────┐                                   │
│  │ 1. 금지 체크 │ → 즉시 차단 (rm -rf /)           │
│  └─────────────┘                                   │
│      │                                              │
│      ▼                                              │
│  ┌─────────────┐                                   │
│  │ 2. deny 규칙│ → 패턴 매칭 → 차단                │
│  └─────────────┘                                   │
│      │                                              │
│      ▼                                              │
│  ┌──────────────┐                                  │
│  │ 3. allow 규칙│ → 패턴 매칭 → 허용               │
│  └──────────────┘                                  │
│      │                                              │
│      ▼                                              │
│  ┌──────────────┐                                  │
│  │ 4. 위험도 분석│ → risk_score 계산               │
│  └──────────────┘                                  │
│      │                                              │
│      ▼                                              │
│  ┌──────────────────────────────┐                  │
│  │ 5. Layer1 PermissionService  │                  │
│  └──────────────────────────────┘                  │
│      │                                              │
│      ▼                                              │
│  실행 / 차단 / 사용자 확인 요청                     │
│                                                     │
└─────────────────────────────────────────────────────┘
```

### 7.2 권한 결정 타입

```rust
pub enum PermissionDecision {
    /// 즉시 실행
    Allow,
    /// 세션 동안 허용
    AllowSession,
    /// 사용자 확인 필요
    AskUser,
    /// 차단 (이유 포함)
    Deny(String),
}
```

### 7.3 설정 파일 구조

```json
{
  "forgecmd": {
    "rules": {
      "allow": [
        {"pattern": "git status", "scope": "always"},
        {"pattern": "cargo *", "scope": "session"},
        {"pattern": "npm run *", "scope": "session"}
      ],
      "deny": [
        {"pattern": "rm -rf /**", "reason": "Recursive delete"},
        {"pattern": "sudo *", "reason": "Elevated privileges"},
        {"pattern": "curl * | sh", "reason": "Remote code execution"}
      ],
      "ask": [
        {"pattern": "rm *", "risk": 6},
        {"pattern": "git push *", "risk": 5}
      ]
    },
    "riskThresholds": {
      "autoApprove": 2,
      "sessionApprove": 5,
      "alwaysAsk": 7,
      "block": 8
    }
  }
}
```

### 7.4 패턴 매칭

```rust
pub struct PatternMatcher {
    exact: HashSet<String>,      // 정확한 매칭
    prefix: Vec<String>,         // "git *" 
    suffix: Vec<String>,         // "* --help"
    regex: Vec<Regex>,           // 고급 패턴
}

impl PatternMatcher {
    pub fn matches(&self, command: &str) -> bool {
        // 1. 정확한 매칭
        if self.exact.contains(command) {
            return true;
        }
        
        // 2. 접두사 매칭
        for prefix in &self.prefix {
            if command.starts_with(prefix) {
                return true;
            }
        }
        
        // 3. 접미사/정규식...
        // ...
    }
}
```

---

## 8. Layer1 (forge-foundation) 연동

### 8.1 사용할 컴포넌트

```rust
use forge_foundation::{
    // 권한
    PermissionService,
    PermissionAction,
    PermissionStatus,
    PermissionDef,
    register_permission,
    permission_categories,
    
    // 저장소
    Storage,
    JsonStore,
    
    // 에러
    Error, Result,
};
```

### 8.2 권한 등록

```rust
pub fn register_forgecmd_permissions() {
    register_permission(
        PermissionDef::new("forgecmd.execute", permission_categories::EXECUTE)
            .risk_level(7)
            .description("Execute command in PTY session")
            .requires_confirmation(true)
    );
    
    register_permission(
        PermissionDef::new("forgecmd.interactive", permission_categories::EXECUTE)
            .risk_level(8)
            .description("Run interactive programs")
            .requires_confirmation(true)
    );
}
```

### 8.3 Layer1 연동 흐름

```rust
impl ForgeCmd {
    async fn execute(&self, command: &str) -> Result<Output> {
        // 1. 자체 위험 명령 필터
        if self.is_forbidden(command) {
            return Err(Error::DangerousCommand);
        }
        
        // 2. Layer1 권한 체크
        let action = PermissionAction::Execute {
            command: command.to_string(),
        };
        
        match self.permissions.check("forgecmd", &action) {
            PermissionStatus::Granted => self.pty_execute(command).await,
            PermissionStatus::Denied => Err(Error::PermissionDenied),
            PermissionStatus::Unknown => Err(Error::NeedsUserApproval(action)),
            // ...
        }
    }
}
```

### 8.4 Layer1 확장 필요 여부

| 항목 | Layer1 제공 | forgecmd 대응 |
|------|------------|---------------|
| 권한 체크 | ✅ PermissionService | 그대로 사용 |
| 권한 저장 | ✅ PermissionSettings | 그대로 사용 |
| 도구 실행 로그 | ⚠️ ToolExecutionRecord | 별도 테이블 |
| 위험 명령 목록 | ✅ dangerous_commands() | 확장 사용 |

**결론**: Layer1 수정 없이 구현 가능

---

## 9. 의존성 라이브러리

### 9.1 핵심 의존성

| 크레이트 | 버전 | 용도 | 다운로드/월 | 라이선스 |
|----------|------|------|-------------|----------|
| **portable-pty** | 0.9.0 | PTY 세션 관리 | 703K | MIT |
| **shlex** | 1.3.0 | 명령어 파싱 | 21.5M | MIT/Apache |
| **strip-ansi-escapes** | 0.2.1 | ANSI 코드 제거 | 2.3M | MIT/Apache |
| **regex** | 1.12 | 패턴 매칭 | - | MIT/Apache |

### 9.2 forge-foundation 제공

- **rusqlite** - SQLite
- **serde/serde_json** - 직렬화
- **tokio** - 비동기 런타임
- **thiserror/anyhow** - 에러 처리
- **tracing** - 로깅

### 9.3 Cargo.toml (예상)

```toml
[package]
name = "forge-tool"
# ...

[dependencies]
# 기존
forge-foundation = { workspace = true }
tokio = { workspace = true }
async-trait = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
tracing = { workspace = true }

# forgecmd 추가
portable-pty = "0.9"
shlex = "1.3"
strip-ansi-escapes = "0.2"
regex = "1.12"

[features]
forgecmd = ["portable-pty", "shlex", "strip-ansi-escapes", "regex"]
```

### 9.4 의존성 검증

| 크레이트 | 보안 | 유지보수 | 안정성 |
|----------|------|----------|--------|
| portable-pty | ✅ | ✅ 활발 | ✅ wezterm |
| shlex | ⚠️ 1.2.1 보안패치 | ✅ | ✅ |
| strip-ansi-escapes | ✅ | ✅ | ✅ |
| regex | ✅ | ✅ rust-lang | ✅ |

---

## 10. 아키텍처 설계

### 10.1 모듈 구조

```
forge-tool/src/forgecmd/
├── mod.rs          # 공개 API
├── shell.rs        # PTY 세션 관리
├── permission.rs   # 권한 체크 (Layer1 연동)
├── tracker.rs      # 명령 히스토리
├── filter.rs       # 위험 명령 필터
└── config.rs       # 설정 로드
```

### 10.2 핵심 타입

```rust
// mod.rs
pub struct ForgeCmd {
    session: PtySession,
    permissions: Arc<PermissionService>,
    tracker: CommandTracker,
    filter: CommandFilter,
    config: ForgeCmdConfig,
}

// shell.rs
pub struct PtySession {
    master: Box<dyn MasterPty>,
    child: Box<dyn Child>,
    size: PtySize,
}

// tracker.rs
pub struct CommandTracker {
    storage: Arc<Storage>,
    session_id: String,
}

// filter.rs
pub struct CommandFilter {
    forbidden: Vec<String>,
    allow_rules: Vec<PermissionRule>,
    deny_rules: Vec<PermissionRule>,
}
```

### 10.3 시퀀스 다이어그램

```
LLM          ForgeCmd       Filter       Permission      PTY
 │              │              │              │            │
 │─execute(cmd)→│              │              │            │
 │              │─is_forbidden?→              │            │
 │              │←─────────────│              │            │
 │              │              │              │            │
 │              │─────────────check(cmd)─────→│            │
 │              │←────────────status──────────│            │
 │              │              │              │            │
 │              │ [if Granted]                │            │
 │              │─────────────────────────────────execute──→│
 │              │←────────────────────────────────output────│
 │              │              │              │            │
 │←───result────│              │              │            │
```

---

## 11. 구현 계획

### Phase 1: 기본 PTY 세션 (1-2주)
- [ ] portable-pty 통합
- [ ] PtySession 구현
- [ ] 기본 명령 실행/출력

### Phase 2: 권한 시스템 (1주)
- [ ] CommandFilter 구현
- [ ] 위험도 분석 로직
- [ ] Layer1 PermissionService 연동

### Phase 3: 추적/로깅 (1주)
- [ ] SQLite 스키마 생성
- [ ] CommandTracker 구현
- [ ] 출력 저장 (truncation 처리)

### Phase 4: 환경 격리 (1주)
- [ ] 환경변수 필터링
- [ ] 작업 디렉토리 제한
- [ ] ForgeCmdConfig 구현

### Phase 5: 통합 테스트 (1주)
- [ ] 단위 테스트
- [ ] 통합 테스트
- [ ] forge-agent 연동 테스트

---

## 12. 참고 링크

### PTY
- [portable-pty](https://github.com/wez/wezterm/tree/main/pty) - 권장
- [pty-process](https://crates.io/crates/pty-process) - Unix 대안
- [rust-pty](https://lib.rs/crates/rust-pty) - 신규

### 명령어 파싱
- [shlex](https://crates.io/crates/shlex) - 월 2100만 다운로드
- [shell-words](https://crates.io/crates/shell-words) - 대안

### 히스토리/추적
- [Atuin](https://github.com/atuinsh/atuin) - 쉘 히스토리
- [ai-session](https://crates.io/crates/ai-session) - AI 세션 관리

### 보안
- [DCG](https://github.com/Dicklesworthstone/destructive_command_guard)
- [agentsh](https://www.agentsh.org/)

### ANSI
- [strip-ansi-escapes](https://docs.rs/strip-ansi-escapes)
- [vte](https://docs.rs/vte) - 터미널 파서

### 기존 CLI
- [Gemini CLI](https://github.com/google-gemini/gemini-cli)
- [Claude Code #9881](https://github.com/anthropics/claude-code/issues/9881)

---

## 13. 구현 완료 현황

### 13.1 완료된 파일

| 파일 | 설명 | 상태 |
|------|------|------|
| `mod.rs` | ForgeCmd, ForgeCmdBuilder, 공개 API | ✅ |
| `error.rs` | ForgeCmdError, CommandResult | ✅ |
| `config.rs` | ForgeCmdConfig, PtySize, PermissionRule | ✅ |
| `filter.rs` | CommandFilter, RiskAnalysis, CommandCategory | ✅ |
| `permission.rs` | PermissionChecker, CheckResult (Layer1 연동) | ✅ |
| `tracker.rs` | CommandTracker, CommandRecord, TrackerStats | ✅ |
| `shell.rs` | PtySession, SpawnedCommand (portable-pty) | ✅ |
| `CLAUDE.md` | 모듈 설계 문서 | ✅ |

### 13.2 테스트 현황

```
32개 테스트 통과
- config: 3개
- error: 3개
- filter: 4개
- permission: 6개
- tracker: 6개
- shell: 3개
- mod: 7개
```

---

## 14. Layer1 통합 개선점 분석

### 14.1 현재 통합 상태

```
forgecmd/permission.rs
    │
    ├── Arc<PermissionService> 사용 ✅
    ├── PermissionAction::Execute 사용 ✅
    ├── PermissionStatus 체크 ✅
    └── grant_session/grant 호출 ✅
```

**문제점**:
1. forgecmd만의 권한 정의(PermissionDef)가 등록되지 않음
2. 명령어 히스토리가 Layer1 Storage에 통합되지 않음
3. AI 에이전트가 forgecmd를 Tool로 사용할 수 없음

### 14.2 필요한 개선 사항

#### A. 권한 정의 등록 (Layer1 연동)

```rust
// forgecmd 시작 시 권한 등록
pub fn register_forgecmd_permissions() {
    use forge_foundation::permission::{register, PermissionDef, categories};
    
    register(
        PermissionDef::new("forgecmd.execute", categories::EXECUTE)
            .risk_level(7)
            .description("Execute command in PTY session")
            .requires_confirmation(true)
    );
    
    register(
        PermissionDef::new("forgecmd.interactive", categories::EXECUTE)
            .risk_level(8)
            .description("Run interactive program (vim, htop)")
            .requires_confirmation(true)
    );
    
    register(
        PermissionDef::new("forgecmd.dangerous", categories::EXECUTE)
            .risk_level(10)
            .description("Potentially destructive command")
            .requires_confirmation(true)
    );
}
```

#### B. ForgeCmdTool 구현 (forge-tool 통합)

```rust
// forge-tool/src/builtin/forgecmd_tool.rs
pub struct ForgeCmdTool {
    forge_cmd: Arc<Mutex<ForgeCmd>>,
}

#[async_trait]
impl Tool for ForgeCmdTool {
    fn definition(&self) -> ToolDef {
        ToolDef::builder("forgecmd", "Execute shell commands with PTY support")
            .string_param("command", "Command to execute", true)
            .boolean_param("interactive", "Run in interactive mode", false)
            .integer_param("timeout", "Timeout in seconds", false)
            .build()
    }

    async fn execute(&self, ctx: &ToolContext, params: Value) -> ToolResult {
        let command = params["command"].as_str().unwrap_or("");
        
        // 1. 권한 확인 (Layer1 PermissionService 사용)
        if !ctx.auto_approve {
            let action = PermissionAction::Execute {
                command: command.to_string(),
            };
            
            match ctx.permissions.check("forgecmd", &action) {
                PermissionStatus::Denied => {
                    return ToolResult::permission_denied("Command denied");
                }
                PermissionStatus::Unknown => {
                    // 사용자 확인 필요
                    return ToolResult::error("Permission required");
                }
                _ => {}
            }
        }
        
        // 2. ForgeCmd 실행
        let mut cmd = self.forge_cmd.lock().await;
        match cmd.execute(command).await {
            Ok(result) => ToolResult::success(result.combined_output()),
            Err(e) => ToolResult::error(e.to_string()),
        }
    }
}
```

#### C. ToolContext 확장 (forgecmd 포함)

```rust
// forge-tool/src/trait.rs 확장
pub struct ToolContext {
    pub session_id: String,
    pub working_dir: PathBuf,
    pub permissions: Arc<PermissionService>,
    pub auto_approve: bool,
    
    // 새로 추가
    pub forge_cmd: Option<Arc<Mutex<ForgeCmd>>>,
}
```

#### D. 히스토리 Layer1 Storage 연동

```rust
// tracker.rs 확장
impl CommandTracker {
    /// Layer1 Storage에 실행 기록 저장
    pub fn save_to_storage(&self, storage: &Storage) -> Result<()> {
        for record in self.get_all() {
            storage.start_tool_execution(&ToolExecutionRecord {
                session_id: Some(record.session_id.clone()),
                message_id: None,
                tool_name: "forgecmd".to_string(),
                tool_call_id: record.id.clone(),
                input_json: serde_json::to_string(&serde_json::json!({
                    "command": record.command,
                    "working_dir": record.working_dir,
                }))?,
                output_text: Some(record.stdout.clone().unwrap_or_default()),
                status: match record.status {
                    ExecutionStatus::Success => "success",
                    ExecutionStatus::Failed => "error",
                    ExecutionStatus::Timeout => "timeout",
                    ExecutionStatus::Cancelled => "cancelled",
                    _ => "pending",
                }.to_string(),
                error_message: record.stderr.clone(),
                duration_ms: record.duration_ms.map(|d| d as i64),
                created_at: Some(record.started_at.to_rfc3339()),
                completed_at: record.completed_at.map(|t| t.to_rfc3339()),
            })?;
        }
        Ok(())
    }
}
```

### 14.3 AI 에이전트 통합 흐름

```
┌─────────────────────────────────────────────────────────────────┐
│                    AI Agent 통합 흐름                            │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  LLM Request                                                    │
│      │                                                          │
│      ▼                                                          │
│  forge-agent (AgentContext)                                     │
│      │                                                          │
│      ├─→ ToolRegistry.get("forgecmd")                          │
│      │       │                                                  │
│      │       ▼                                                  │
│      │   ForgeCmdTool.execute(ctx, params)                     │
│      │       │                                                  │
│      │       ├─→ ctx.permissions.check()  ← Layer1             │
│      │       │                                                  │
│      │       ├─→ [Unknown] → CLI/TUI에서 사용자 확인           │
│      │       │       │                                          │
│      │       │       ├─→ 승인 → grant_session()                │
│      │       │       └─→ 거부 → return Denied                  │
│      │       │                                                  │
│      │       └─→ ForgeCmd.execute()                            │
│      │               │                                          │
│      │               ├─→ PermissionChecker (forgecmd 내부)      │
│      │               ├─→ PtySession.execute()                  │
│      │               └─→ CommandTracker.record()               │
│      │                                                          │
│      └─→ ToolResult 반환                                        │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 14.4 구현 우선순위

| 우선순위 | 작업 | 파일 | 난이도 |
|----------|------|------|--------|
| 1 | 권한 정의 등록 함수 | forgecmd/mod.rs | 낮음 |
| 2 | ForgeCmdTool 구현 | builtin/forgecmd_tool.rs | 중간 |
| 3 | ToolRegistry에 등록 | registry.rs | 낮음 |
| 4 | ToolContext 확장 | trait.rs | 낮음 |
| 5 | Storage 연동 | tracker.rs | 중간 |
| 6 | CLI/TUI 권한 확인 UI | forge-cli | 높음 |

---

## 변경 이력

| 날짜 | 변경 내용 |
|------|-----------|
| 2026-02-04 | 구현 완료 현황, Layer1 통합 개선점 분석 추가 |
| 2026-02-03 | 문서 전면 개편, 의존성 검증, 아키텍처 정리 |
| 2026-02-03 | Layer1 연동 분석, 권한 시스템 설계 |
| 2026-02-03 | PTY 라이브러리 비교, 초기 연구 |
