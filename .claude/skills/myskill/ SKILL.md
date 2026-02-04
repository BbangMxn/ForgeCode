---
name: architecture
description: ForgeCode 계층 구조 분석
allowed-tools:
  - Read
  - Glob
  - Grep
user-invocable: true
---

ForgeCode의 4계층 아키텍처를 분석합니다:
- Layer1-foundation: 권한/보안/설정
- Layer2-core/tool/task/provider: 핵심 서비스
- Layer3-agent: 에이전트 main 
- Layer4-cli: 사용자 인터페이스

$ARGUMENTS: 분석할 레이어 또는 모듈
