---
name: commit
description: Create a git commit with AI-generated message
allowed-tools:
  - Bash
  - Read
user-invocable: true
argument-hint:
  - [message]
---

# Commit Skill

변경 사항을 분석하고 커밋 메시지를 자동 생성합니다.

## 사용법

```
/commit                  # 변경 사항 분석 후 커밋 메시지 제안
/commit "message"        # 지정된 메시지로 커밋
/commit --amend          # 마지막 커밋 수정
```

## 동작

1. `git diff --staged` 분석
2. 변경 유형 파악 (feat, fix, refactor, docs 등)
3. Conventional Commits 형식으로 메시지 생성
4. 사용자 확인 후 커밋

## 커밋 메시지 형식

```
<type>(<scope>): <description>

<body>

<footer>
```

### Type

- `feat`: 새 기능
- `fix`: 버그 수정
- `refactor`: 리팩토링
- `docs`: 문서
- `test`: 테스트
- `chore`: 기타 변경

## 예시

```
/commit
```

위 명령 실행 시:
1. staged 파일 분석
2. "feat(task): add task_spawn and task_wait tools" 같은 메시지 제안
3. 확인 후 `git commit` 실행
