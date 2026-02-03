# ForgeCode 구현 TODO 목록

## 즉시 해결 필요 (Phase 1: 아키텍처 정리)

### 1. Tool 시스템 통합 🔴 HIGH
현재 Layer2-core와 Layer2-tool에 동일한 도구가 중복 구현되어 있음.

```
문제:
- Layer2-core/src/tool/builtin/ - 6개 도구 구현
- Layer2-tool/src/builtin/ - 동일한 6개 도구 구현
- 두 개의 다른 Tool trait 존재

해결:
□ Layer2-tool을 표준으로 채택 (Layer3-agent가 이미 사용 중)
□ Layer2-core/src/tool/builtin/ 삭제
□ Layer2-core/src/tool/mod.rs에서 Layer2-tool 도구 re-export
□ Layer2-core lib.rs 정리
```

### 2. 크레이트 의존성 정리 🔴 HIGH
```
현재 의존성:
Layer3-agent → Layer2-tool (도구 사용)
Layer3-agent → Layer2-provider (LLM 연동)
Layer3-agent → Layer2-task (태스크 실행)
Layer3-agent → Layer2-core (???)

해결:
□ Layer2-core의 역할 재정의
  - LSP 모듈 유지
  - MCP 브릿지 구현 (예정)
  - tool 모듈은 re-export만
□ 불필요한 의존성 제거
```

---

## MCP 구현 (Phase 2)

### 3. MCP 클라이언트 구현 🔴 HIGH
```
위치: Layer2-core/src/mcp/

□ mcp/client.rs
  - McpClient 구조체
  - connect(), disconnect()
  - call_tool(), list_tools()

□ mcp/transport/
  - mod.rs - Transport trait
  - stdio.rs - StdioTransport
  - sse.rs - SseTransport (나중에)

□ mcp/protocol.rs
  - JSON-RPC 메시지 타입
  - 요청/응답 직렬화

□ mcp/manager.rs
  - McpManager (여러 서버 관리)
  - 서버 자동 시작/종료
  - 도구 통합 (ToolRegistry와 연동)
```

### 4. MCP-Tool 통합 🟡 MEDIUM
```
□ MCP 도구를 Layer1 Tool trait으로 래핑
□ 통합 ToolRegistry
  - builtin 도구
  - MCP 도구
  - 같은 권한 시스템 적용
```

---

## TUI 구현 (Phase 3)

### 5. Ratatui 채팅 인터페이스 🔴 HIGH
```
위치: Layer4-cli/src/tui/

□ tui/app.rs
  - App 구조체 완성
  - 상태 관리

□ tui/pages/chat.rs
  - 메시지 목록 렌더링
  - 스트리밍 텍스트 표시
  - 도구 호출 결과 표시

□ tui/components/input.rs
  - 멀티라인 입력
  - 히스토리 네비게이션
  - 자동완성 (선택적)

□ tui/components/permission.rs
  - 권한 요청 모달
  - Allow/Deny/Session/Permanent 버튼
  - 위험도 표시
```

### 6. 키바인딩 및 네비게이션 🟡 MEDIUM
```
□ Vim 스타일 키바인딩 (선택적)
□ 세션 전환
□ 검색 기능
□ 복사/붙여넣기
```

---

## Task 시스템 (Phase 4)

### 7. Task 시스템 구현 🟡 MEDIUM
```
위치: Layer2-task/

□ task/manager.rs
  - TaskManager 완성
  - 태스크 큐 관리
  - 동시 실행 제한

□ task/executor/local.rs
  - LocalExecutor 완성
  - 타임아웃 처리
  - 출력 캡처

□ task/context.rs
  - TaskContext 구현
  - 권한 위임
  - 도구 실행 연동
```

### 8. 병렬 실행 🟢 LOW
```
□ 여러 태스크 동시 실행
□ 진행 상황 추적
□ 취소 처리
□ 결과 집계
```

---

## 테스트 및 문서화

### 9. 통합 테스트 🟡 MEDIUM
```
□ Layer1 ↔ Layer2 연동 테스트
□ Tool 실행 E2E 테스트
□ MCP 클라이언트 테스트 (mock 서버)
□ TUI 스냅샷 테스트
```

### 10. 문서화 🟢 LOW
```
□ README.md 업데이트
□ 사용자 가이드
□ API 문서 (rustdoc)
□ 예제 코드
```

---

## 우선순위 요약

| 우선순위 | 작업 | 예상 시간 |
|----------|------|-----------|
| 🔴 1 | Tool 시스템 통합 | 1일 |
| 🔴 2 | 크레이트 의존성 정리 | 0.5일 |
| 🔴 3 | MCP 클라이언트 기본 구현 | 3-5일 |
| 🔴 4 | TUI 채팅 인터페이스 | 3-5일 |
| 🟡 5 | MCP-Tool 통합 | 2일 |
| 🟡 6 | 키바인딩/네비게이션 | 2일 |
| 🟡 7 | Task 시스템 | 3-5일 |
| 🟡 8 | 통합 테스트 | 2-3일 |
| 🟢 9 | 병렬 실행 | 2일 |
| 🟢 10 | 문서화 | 지속적 |

---

## 완료된 항목 ✅

### Layer1-foundation
- [x] Tool trait 정의
- [x] ToolContext trait 정의
- [x] PermissionService 구현
- [x] CommandAnalyzer 구현
- [x] PathAnalyzer 구현
- [x] ShellConfig trait 정의
- [x] McpConfig 정의
- [x] ModelRegistry 구현
- [x] LimitsConfig 구현
- [x] JsonStore 구현

### Layer2-core
- [x] LSP 클라이언트 구현
  - Lazy Loading
  - 10분 유휴 종료
  - 5분 가용성 캐시
- [x] 6개 Builtin 도구 구현 (중복 - 정리 필요)

### Layer2-provider
- [x] Anthropic 프로바이더
- [x] OpenAI 프로바이더
- [x] Ollama 프로바이더
- [x] 스트리밍 응답
- [x] 재시도 로직

### Layer2-tool
- [x] 6개 Builtin 도구 구현 (표준)
- [x] ToolRegistry
