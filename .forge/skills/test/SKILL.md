---
name: test
description: Run tests and report results
allowed-tools:
  - Bash
  - Read
  - Glob
  - Grep
user-invocable: true
argument-hint:
  - [file_or_pattern]
---

# Test Skill

프로젝트 테스트를 실행하고 결과를 보고합니다.

## 사용법

```
/test                    # 전체 테스트 실행
/test crate_name         # 특정 crate 테스트
/test --failed           # 실패한 테스트만 재실행
```

## 동작

1. `cargo test --workspace` 실행
2. 실패한 테스트 분석
3. 에러 메시지 요약
4. 수정 제안

## 예시

```
/test Layer2-core
```

위 명령은 Layer2-core crate의 모든 테스트를 실행합니다.
