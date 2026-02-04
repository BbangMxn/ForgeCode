---
name: scaffold
description: ForgeCode 계층 구조에 맞는 새 모듈/크레이트 생성
allowed-tools:
  - Read
  - Write
  - Bash
  - Glob
  - Grep
user-invocable: true
argument-hint:
  - <layer> <name>
  - --type <crate|module>
  - --with-tests
---

# ForgeCode 스캐폴딩 Skill

ForgeCode의 4계층 아키텍처에 맞는 새 모듈이나 크레이트를 생성합니다.

## 사용법

```
/scaffold Layer2 mymodule              # Layer2에 새 모듈 생성
/scaffold Layer1 security --type crate # Layer1에 새 크레이트 생성
/scaffold Layer3 executor --with-tests # 테스트 포함
```

## 계층별 템플릿

### Layer1 (Foundation)
- 위치: `crates/Layer1-foundation/src/` 또는 `crates/Layer1-<name>/`
- 특징: 의존성 없음, 기본 타입/트레이트 정의
- 파일 구조:
  ```
  mod.rs
  types.rs
  error.rs (선택)
  ```

### Layer2 (Core Services)
- 위치: `crates/Layer2-<name>/`
- 특징: Layer1만 의존, 핵심 기능 구현
- 파일 구조:
  ```
  Cargo.toml
  src/
    lib.rs
    mod.rs
    types.rs
    error.rs
  CLAUDE.md (문서)
  ```

### Layer3 (Agent)
- 위치: `crates/Layer3-agent/src/` 또는 `crates/Layer3-<name>/`
- 특징: Layer1, Layer2 의존, 오케스트레이션 로직
- 파일 구조:
  ```
  mod.rs
  executor.rs
  context.rs
  ```

### Layer4 (CLI)
- 위치: `crates/Layer4-cli/src/`
- 특징: 모든 하위 계층 의존, UI 로직

## 생성 단계

1. 파라미터 파싱 (계층, 이름, 타입)
2. 기존 구조 확인 (중복 검사)
3. 계층에 맞는 템플릿 선택
4. 파일 생성:
   - Cargo.toml (크레이트인 경우)
   - lib.rs 또는 mod.rs
   - 기본 타입/에러 정의
   - CLAUDE.md 문서
5. 워크스페이스 Cargo.toml 업데이트 (크레이트인 경우)
6. 상위 mod.rs에 모듈 등록 (모듈인 경우)

## 파라미터

- `$1`: 계층 이름 (Layer1, Layer2, Layer3, Layer4)
- `$2`: 모듈/크레이트 이름
- `--type`: crate 또는 module (기본: module)
- `--with-tests`: tests/ 디렉토리 포함

## Cargo.toml 템플릿 (Layer2 크레이트)

```toml
[package]
name = "forge-{name}"
version = "0.1.0"
edition = "2021"

[dependencies]
forge-foundation = { path = "../Layer1-foundation" }
async-trait = "0.1"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "1"
tracing = "0.1"
```

## 주의사항

- 계층 규칙을 준수하는 의존성만 추가
- 기존 네이밍 컨벤션 따르기
- CLAUDE.md에 모듈 목적 문서화
