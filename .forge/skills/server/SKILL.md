---
name: server
description: Manage development servers using Task system
allowed-tools:
  - task_spawn
  - task_wait
  - task_logs
  - task_stop
  - task_list
  - task_status
  - Bash
user-invocable: true
argument-hint:
  - start <name> <command>
  - stop <name>
  - logs <name>
  - list
---

# Server Management Skill

Task 시스템을 사용하여 개발 서버를 관리합니다.

## 사용법

```
/server start backend "cargo run --bin server"
/server start frontend "npm run dev"
/server stop backend
/server logs backend --tail 50
/server list
```

## 동작

### start

1. `task_spawn`으로 PTY 모드에서 서버 시작
2. `task_wait`로 "Listening on" 같은 준비 메시지 대기
3. 성공/실패 보고

### stop

1. `task_stop`으로 서버 종료
2. 정상 종료 확인

### logs

1. `task_logs`로 로그 조회
2. 에러 하이라이팅
3. 분석 리포트 (선택)

### list

1. `task_list`로 실행 중인 서버 목록 표시
2. 상태, 메모리, CPU 사용량 표시

## 예시: 백엔드 + 프론트엔드 동시 실행

```
/server start api "cargo run --bin api" --wait "Server ready"
/server start web "npm run dev" --wait "Compiled successfully"
```

위 명령은:
1. API 서버를 PTY 모드로 시작
2. "Server ready" 출력 대기
3. 웹 서버 시작
4. "Compiled successfully" 출력 대기
5. 두 서버 모두 준비되면 완료 보고
