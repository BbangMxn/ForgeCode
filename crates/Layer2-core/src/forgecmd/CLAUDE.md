# forgecmd 모듈

> LLM 에이전트를 위한 격리된 PTY 기반 명령 쉘 환경

## 개요

forgecmd는 ForgeCode의 LLM 에이전트가 안전하게 쉘 명령을 실행할 수 있는 환경을 제공합니다.
기존 stateless bash 도구와 달리, PTY 세션을 통해 대화형 명령도 지원합니다.

## 모듈 구조

```
forgecmd/
├── CLAUDE.md       # 이 문서
├── mod.rs          # 공개 API, ForgeCmd 구조체
├── error.rs        # 에러 타입 정의
├── config.rs       # ForgeCmdConfig 설정
├── shell.rs        # PtySession - PTY 세션 관리
├── filter.rs       # CommandFilter - 위험 명령 필터링
├── permission.rs   # PermissionChecker - Layer1 권한 연동
└── tracker.rs      # CommandTracker - 히스토리 추적
```

## 핵심 타입

### ForgeCmd

메인 진입점. 모든 하위 컴포넌트를 조율합니다.

```rust
pub struct ForgeCmd {
    config: ForgeCmdConfig,
    session: Option<PtySession>,
    filter: CommandFilter,
    permission: PermissionChecker,
    tracker: CommandTracker,
}
```

### PtySession (shell.rs)

portable-pty를 래핑한 PTY 세션 관리자.

```rust
pub struct PtySession {
    pair: PtyPair,
    child: Box<dyn Child + Send>,
    reader: Box<dyn Read + Send>,
    writer: Box<dyn Write + Send>,
}
```

### CommandFilter (filter.rs)

위험 명령 필터링 및 카테고리 분류.

```rust
pub struct CommandFilter {
    forbidden_patterns: Vec<Pattern>,
    allow_rules: Vec<PermissionRule>,
    deny_rules: Vec<PermissionRule>,
}
```

### PermissionChecker (permission.rs)

forge-foundation PermissionService 연동.

```rust
pub struct PermissionChecker {
    service: Arc<PermissionService>,
    session_grants: HashSet<String>,
}
```

### CommandTracker (tracker.rs)

명령 실행 히스토리 기록.

```rust
pub struct CommandTracker {
    storage: Arc<Storage>,
    session_id: String,
    pty_session_id: String,
}
```

## 권한 체크 흐름

```
1. is_forbidden()     → 금지 명령 즉시 차단
2. check_deny_rules() → deny 패턴 매칭 → 차단
3. check_allow_rules() → allow 패턴 매칭 → 허용
4. analyze_risk()     → 위험도 점수 계산
5. permission.check() → Layer1 권한 서비스 확인
6. → Allow / AllowSession / AskUser / Deny
```

## 위험도 분류

| 레벨 | risk_score | 처리 | 예시 |
|------|------------|------|------|
| R0 | 0-2 | 자동 승인 | ls, cat, pwd |
| R1 | 3-4 | 세션 승인 | mkdir, touch |
| R2 | 5 | 세션 승인 | cp, mv |
| R3 | 6-7 | 매번 확인 | rm, git push |
| R4 | 8-10 | 차단 | rm -rf /, dd |

## 의존성

```toml
portable-pty = "0.9"       # PTY 세션
shlex = "1.3"              # 명령어 파싱
strip-ansi-escapes = "0.2" # ANSI 제거
regex = "1.12"             # 패턴 매칭
```

## Layer1 연동

forge-foundation에서 가져오는 것:
- `PermissionService` - 권한 체크
- `PermissionAction::Execute` - 실행 권한
- `Storage` - SQLite 히스토리 저장
- `dangerous_commands()` - 기본 위험 명령 목록

## 사용 예시

```rust
use forge_tool::forgecmd::{ForgeCmd, ForgeCmdConfig};

// 생성
let config = ForgeCmdConfig::default();
let mut cmd = ForgeCmd::new(config, permissions, storage)?;

// 세션 시작
cmd.start_session("/path/to/workdir")?;

// 명령 실행
let result = cmd.execute("ls -la").await?;

// 세션 종료
cmd.close_session()?;
```

## 설정

```json
{
  "forgecmd": {
    "shell": "bash",
    "ptySize": { "rows": 24, "cols": 80 },
    "timeout": 60,
    "maxOutputSize": 100000,
    "rules": {
      "allow": ["git status", "cargo *"],
      "deny": ["rm -rf /**", "sudo *"],
      "ask": ["rm *", "git push *"]
    },
    "blockedEnvVars": ["AWS_*", "*_SECRET", "*_TOKEN"]
  }
}
```

## 테스트

```bash
cargo test -p forge-tool --features forgecmd
```
