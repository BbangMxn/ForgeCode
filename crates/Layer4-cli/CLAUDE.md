# Layer4-cli (forge-cli)

ForgeCode의 CLI/TUI 인터페이스입니다.

## 아키텍처

```
Layer4-cli
├── main.rs          # 엔트리포인트, CLI 인자 파싱
├── cli.rs           # 비대화형 모드 (단일 프롬프트)
└── tui/
    ├── mod.rs       # TUI 모듈 export
    ├── app.rs       # 메인 애플리케이션 루프
    ├── event.rs     # 이벤트 핸들링 (키보드, 리사이즈)
    ├── theme.rs     # 테마 설정
    ├── pages/
    │   ├── mod.rs
    │   └── chat.rs  # 메인 채팅 페이지
    └── components/
        ├── mod.rs
        ├── input.rs         # 텍스트 입력 박스
        ├── message_list.rs  # 메시지 목록
        ├── permission.rs    # 권한 요청 모달
        ├── settings.rs      # 설정 페이지
        ├── model_switcher.rs # 모델 전환 UI
        └── progress.rs      # 진행 상태 위젯
```

## 실행 모드

### 1. 대화형 TUI 모드 (기본)
```bash
forge
```

### 2. 비대화형 모드
```bash
forge -p "프롬프트 내용"
forge --prompt "코드 분석해줘"
```

### 3. 세션 이어가기
```bash
forge -s <session-id>
forge --session abc123
```

## 키보드 단축키

| 단축키 | 기능 |
|--------|------|
| `Ctrl+C` | 종료 |
| `Ctrl+P` | 일시정지/재개 |
| `Ctrl+X` | Agent 중단 |
| `Ctrl+S` | 설정 열기 |
| `Ctrl+L` | 대화 지우기 |
| `Ctrl+H` / `F1` | 도움말 |
| `Esc` | 취소/닫기 |
| `PageUp/Down` | 스크롤 |

## 슬래시 명령어

| 명령어 | 설명 |
|--------|------|
| `/help` | 도움말 표시 |
| `/clear` | 대화 지우기 |
| `/new` | 새 세션 시작 |
| `/model` | 현재 모델 정보 |
| `/tokens` | 토큰 사용량 |

## Layer3 Agent 연동

### AgentEvent 처리

```rust
match event {
    AgentEvent::Thinking => { /* 생각 중 표시 */ }
    AgentEvent::Text(text) => { /* 응답 텍스트 */ }
    AgentEvent::ToolStart { tool_name, .. } => { /* 도구 실행 시작 */ }
    AgentEvent::ToolComplete { result, success, duration_ms, .. } => { /* 도구 완료 */ }
    AgentEvent::TurnStart { turn } => { /* 턴 시작 */ }
    AgentEvent::TurnComplete { turn } => { /* 턴 완료 */ }
    AgentEvent::Compressed { tokens_saved, .. } => { /* 컨텍스트 압축됨 */ }
    AgentEvent::Paused => { /* 일시정지됨 */ }
    AgentEvent::Resumed => { /* 재개됨 */ }
    AgentEvent::Stopped { reason } => { /* 중단됨 */ }
    AgentEvent::Done { full_response } => { /* 완료 */ }
    AgentEvent::Error(e) => { /* 에러 */ }
    AgentEvent::Usage { input_tokens, output_tokens } => { /* 토큰 사용량 */ }
}
```

### Steering 제어

```rust
// Steering handle 획득
let handle = agent.steering_handle();

// 일시정지
handle.pause().await?;

// 재개
handle.resume().await?;

// 중단
handle.stop("User requested").await?;

// 방향 전환
handle.redirect("Focus on tests").await?;
```

## 상태 표시줄

```
┌─────────────────────────────────────────────────────────────┐
│ Ready │ 1234↓ 567↑ │ ████░░░░ 45% │ abc12345 │
└─────────────────────────────────────────────────────────────┘
  ^        ^              ^              ^
  상태     토큰 사용량    컨텍스트 게이지  세션 ID
```

## 의존성

- `forge-foundation` (Layer1): 설정, 권한
- `forge-core` (Layer2): 도구 레지스트리
- `forge-provider` (Layer2): LLM Gateway
- `forge-task` (Layer2): 작업 관리
- `forge-agent` (Layer3): Agent 시스템
- `ratatui`: TUI 렌더링
- `crossterm`: 터미널 제어
- `clap`: CLI 인자 파싱
