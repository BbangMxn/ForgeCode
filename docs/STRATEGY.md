# ForgeCode 전략 문서

## 1. 프로젝트 현황 분석 (2025년 1월)

### 1.1 완성도 현황

| 레이어 | 크레이트 | 완성도 | 상태 |
|--------|----------|--------|------|
| Layer 1 | forge-foundation | 95% | ✅ 거의 완성 |
| Layer 2 | forge-provider | 90% | ✅ 안정 |
| Layer 2 | forge-tool | 85% | 🔄 리팩토링 필요 |
| Layer 2 | forge-core | 80% | 🔄 중복 제거 필요 |
| Layer 2 | forge-task | 50% | ⚠️ 스켈레톤 |
| Layer 3 | forge-agent | 70% | 🔄 진행 중 |
| Layer 4 | forge-cli | 60% | ⚠️ TUI 미완성 |

### 1.2 핵심 아키텍처 문제

**문제 1: Tool 시스템 중복**
- `Layer2-core/src/tool/` 과 `Layer2-tool/` 에서 동일한 도구 구현
- 두 개의 다른 `Tool` trait 정의
- Layer3-agent는 Layer2-tool 사용 중

**해결 방안**: Layer2-core의 tool 모듈을 Layer1 trait만 re-export하고,
실제 구현은 Layer2-tool에 통합

**문제 2: LSP 모듈 위치**
- `Layer2-core/src/lsp/` 에 LSP 클라이언트 구현됨
- Lazy Loading, 10분 유휴 종료, 5분 가용성 캐시 구현 완료
- 위치가 적절한지 검토 필요

---

## 2. 2025년 AI 코딩 어시스턴트 시장 분석

### 2.1 주요 경쟁 제품

| 제품 | 강점 | 컨텍스트 | 성능 |
|------|------|----------|------|
| Claude Code | 200K 컨텍스트, Agentic 워크플로우 | 200K 토큰 | SWE-bench 80.9% |
| Cursor | VSCode 포크, Composer 모드 | - | 빠른 반복 |
| GitHub Copilot | 광범위한 IDE 지원 | - | 안정적 |
| Windsurf | MCP 기반, 오픈소스 | - | - |

### 2.2 핵심 트렌드

1. **MCP (Model Context Protocol)**
   - Anthropic이 2024년 11월 발표
   - 2025년 초 1.0 정식 버전
   - Elicitation, Auth 등 새로운 기능 추가
   - 업계 표준으로 자리잡는 중

2. **Agentic 워크플로우**
   - 단순 자동완성에서 자율 에이전트로 진화
   - 멀티스텝 태스크 처리
   - 자체 루프를 통한 문제 해결

3. **Context Engineering**
   - AGENTS.md 파일로 프로젝트 컨텍스트 제공
   - 코드베이스 인덱싱 및 임베딩
   - 효율적인 컨텍스트 윈도우 활용

4. **로컬 모델 지원**
   - Ollama, LM Studio 등 로컬 LLM 통합
   - 프라이버시 중시 사용자 대상

---

## 3. ForgeCode 차별화 전략

### 3.1 핵심 차별화 요소

#### 1. 네이티브 MCP 통합 (최우선)
```
현재: MCP 서버 설정 구조 정의됨 (Layer1)
목표: 완전한 MCP 클라이언트 구현

차별점:
- MCP를 핵심 아키텍처로 채택
- Builtin 도구와 MCP 도구 통합 관리
- 동일한 권한 시스템 적용
```

#### 2. 계층화된 보안 모델 (TCC 스타일)
```
현재: PermissionService, CommandAnalyzer, PathAnalyzer 구현됨
목표: macOS TCC처럼 직관적인 권한 UI

차별점:
- 도구별 세밀한 권한 제어
- 패턴 기반 허용/거부 규칙
- 세션/영구 권한 분리
```

#### 3. OS 네이티브 Shell 최적화
```
현재: ShellConfig trait, DefaultShellConfig 구현됨
목표: 각 OS에 최적화된 Shell 실행

차별점:
- Windows: PowerShell 기본
- macOS: Zsh 기본
- Linux: Bash 기본
- Shell별 환경 변수 및 설정 관리
```

#### 4. 효율적인 LSP 통합
```
현재: Lazy Loading LSP Manager 구현됨
목표: 코드 이해력 향상을 위한 LSP 활용

차별점:
- 요청 시에만 서버 시작
- 10분 미사용 시 자동 종료
- 5분 가용성 캐시
- definition, references, hover 지원
```

#### 5. 독립 Task 시스템
```
현재: Task trait 정의됨, 구현 미완성
목표: 병렬 태스크 실행 및 관리

차별점:
- 에이전트 간 독립 실행
- 권한 위임 메커니즘
- 진행 상황 보고
- 하위 태스크 생성
```

### 3.2 구현 우선순위

```
Phase 1: 아키텍처 정리 (1-2주)
├── Tool 시스템 통합 (Layer2-tool로 일원화)
├── Layer2-core에서 tool 모듈 중복 제거
└── lib.rs export 정리

Phase 2: MCP 완성 (2-3주)
├── MCP 클라이언트 구현
├── stdio/SSE 트랜스포트
├── 도구 스키마 변환
└── MCP 서버 자동 시작/종료

Phase 3: TUI 완성 (2-3주)
├── Ratatui 기반 인터페이스
├── 권한 요청 다이얼로그
├── 메시지 스트리밍 표시
└── 세션 관리 UI

Phase 4: Task 시스템 (2-3주)
├── TaskManager 구현
├── LocalExecutor 완성
├── 병렬 실행 지원
└── 진행 상황 추적
```

---

## 4. 기술 로드맵

### 4.1 단기 (1개월)

1. **아키텍처 정리**
   - Layer2-tool을 표준 Tool 시스템으로 채택
   - Layer2-core에서 중복 tool 모듈 제거
   - LSP 모듈은 Layer2-core에 유지

2. **MCP 클라이언트 MVP**
   - stdio 트랜스포트 구현
   - 기본 도구 호출 연동
   - 에러 처리

3. **TUI 기본 완성**
   - 채팅 화면 구현
   - 도구 결과 표시
   - 권한 요청 프롬프트

### 4.2 중기 (2-3개월)

1. **MCP 고급 기능**
   - SSE 트랜스포트
   - 리소스 구독
   - 프롬프트 캐싱

2. **Task 시스템**
   - 병렬 실행
   - 컨테이너 격리 (선택적)
   - 진행 상황 UI

3. **LSP 확장**
   - 추가 언어 지원
   - 심볼 검색
   - 타입 정보 활용

### 4.3 장기 (3-6개월)

1. **IDE 통합**
   - VS Code 확장
   - Zed 플러그인

2. **팀 기능**
   - 설정 공유
   - 권한 정책 관리
   - 사용량 대시보드

3. **고급 에이전트**
   - 멀티 에이전트 협업
   - 학습된 선호도 적용
   - 코드베이스 인덱싱

---

## 5. 성공 지표

### 5.1 기술 지표
- [ ] 빌드 성공률 100%
- [ ] 테스트 커버리지 80% 이상
- [ ] LSP 응답 시간 < 100ms
- [ ] MCP 도구 호출 성공률 99%

### 5.2 사용성 지표
- [ ] 첫 실행까지 3분 이내
- [ ] 권한 프롬프트 이해도 높음
- [ ] 도움말 없이 기본 사용 가능

### 5.3 차별화 지표
- [ ] MCP 서버 5개 이상 기본 지원
- [ ] OS별 최적화된 Shell 실행
- [ ] 세밀한 권한 제어 UI

---

## 6. 결론

ForgeCode의 핵심 가치는:

1. **MCP 네이티브**: 업계 표준 프로토콜 채택으로 확장성 확보
2. **보안 우선**: TCC 스타일의 직관적인 권한 관리
3. **OS 최적화**: 각 플랫폼에 맞는 최적의 실행 환경
4. **계층화 설계**: 명확한 책임 분리와 재사용성

현재 아키텍처는 대부분 완성되어 있으며, 핵심은 중복 제거와
MCP/TUI 구현에 집중하는 것입니다.
