---
name: validate
description: ForgeCode 계층 아키텍처 규칙 검증
allowed-tools:
  - Read
  - Glob
  - Grep
  - Bash
user-invocable: true
argument-hint:
  - --layer <layer-name>
  - --fix
---

# ForgeCode 아키텍처 검증 Skill

ForgeCode의 4계층 아키텍처 규칙을 검증합니다.

## 계층 구조

```
Layer4: CLI (forge-cli)           - 사용자 인터페이스
Layer3: Agent (forge-agent)       - 에이전트 오케스트레이션  
Layer2: Core Services             - 핵심 서비스
  ├── forge-core (Tool/MCP/LSP/Skill)
  ├── forge-tool (내장 도구)
  ├── forge-task (작업/Sub-agent)
  └── forge-provider (LLM 통합)
Layer1: Foundation (forge-foundation) - 권한/보안/설정
```

## 검증 규칙

### 1. 의존성 방향 규칙
- 상위 계층 → 하위 계층만 의존 가능
- Layer4 → Layer3 → Layer2 → Layer1
- **위반**: Layer1이 Layer2를 import하면 안 됨

### 2. 계층 건너뛰기 금지
- Layer4가 Layer1을 직접 의존하면 안 됨 (Layer3를 통해야 함)
- 예외: 공통 타입, 에러 타입은 허용

### 3. 순환 의존성 금지
- 크레이트 간 순환 참조 금지

### 4. 모듈 네이밍 규칙
- Layer1: `forge-foundation` 또는 `Layer1-*`
- Layer2: `forge-core`, `forge-tool`, `forge-task`, `forge-provider` 또는 `Layer2-*`
- Layer3: `forge-agent` 또는 `Layer3-*`
- Layer4: `forge-cli` 또는 `Layer4-*`

## 검증 수행 단계

1. `crates/` 디렉토리의 모든 Cargo.toml 파일 읽기
2. 각 크레이트의 계층 식별 (이름 기반)
3. dependencies 섹션에서 의존성 추출
4. 의존성 방향 규칙 검증
5. 순환 의존성 검사
6. 결과 리포트 생성

## 파라미터

- `$ARGUMENTS`: 전체 인자
- `--layer <name>`: 특정 계층만 검증 (예: Layer2)
- `--fix`: 발견된 문제에 대한 수정 제안 포함

## 출력 형식

```
=== ForgeCode 아키텍처 검증 결과 ===

✅ 통과: 의존성 방향 규칙
✅ 통과: 순환 의존성 없음
⚠️ 경고: Layer2-core가 Layer1을 직접 참조 (허용됨)
❌ 위반: [위반 내용]

총 검사: N개 | 통과: N개 | 경고: N개 | 위반: N개
```
