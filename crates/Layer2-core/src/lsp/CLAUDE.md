# LSP Module - 경량 Language Server Protocol 통합

AI 코딩 어시스턴트를 위한 효율적인 LSP 클라이언트 구현

## 1. 설계 철학

### 1.1 효율성 우선

이 모듈은 **과도하게 무거워지는 것을 방지**하기 위해 다음 원칙을 따릅니다:

1. **Lazy Loading**: LSP 서버는 Agent가 실제로 요청할 때만 시작
2. **최소 기능**: 핵심 3개 메서드만 구현 (definition, references, hover)
3. **외부 의존성 최소화**: `lsp-types` 크레이트 없이 직접 구현
4. **자동 정리**: 10분 미사용 시 서버 자동 종료
5. **가용성 캐싱**: 서버 설치 여부 5분 캐싱

### 1.2 왜 경량화가 중요한가?

| 문제 | 해결 방법 |
|------|----------|
| LSP 서버 메모리 (수백 MB) | 필요시에만 시작, 유휴 시 종료 |
| 실시간 Diagnostics 스트림 | 구현하지 않음 (Agent에 불필요) |
| 모든 LSP 기능 지원 | 핵심 3개만 구현 |
| lsp-types 크레이트 | 필요한 타입만 직접 정의 |

---

## 2. 아키텍처

```
┌─────────────────────────────────────────────────────────────────┐
│                       Agent (Layer3)                             │
│                            │                                     │
│    "파일 main.rs:42 심볼의 정의 위치를 찾아줘"                  │
│                            │                                     │
├────────────────────────────▼────────────────────────────────────┤
│                      LspManager                                  │
│                            │                                     │
│  ┌─────────────────────────┤                                    │
│  │                         │                                    │
│  │  1. 언어 감지 (.rs → rust)                                   │
│  │  2. 서버 가용성 확인 (캐시)                                  │
│  │  3. Lazy Start (필요시에만)                                  │
│  │  4. Idle Cleanup (10분 미사용 시)                            │
│  │                         │                                    │
│  └─────────────────────────┤                                    │
│                            ▼                                     │
│     ┌──────────────────────────────────────────────┐            │
│     │              LspClient (rust)                 │            │
│     │                                               │            │
│     │  - goto_definition()                         │            │
│     │  - find_references()                         │            │
│     │  - hover()                                   │            │
│     │                                               │            │
│     │  [JSON-RPC 2.0 over stdio]                   │            │
│     └──────────────────────┬───────────────────────┘            │
├────────────────────────────┼────────────────────────────────────┤
│                            ▼                                     │
│                    ┌──────────────┐                             │
│                    │rust-analyzer │  (외부 프로세스)             │
│                    └──────────────┘                             │
└─────────────────────────────────────────────────────────────────┘
```

---

## 3. 핵심 타입

### 3.1 Position & Range

```rust
/// 텍스트 위치 (0-based, UTF-16 offset)
pub struct Position {
    pub line: u32,      // 라인 번호 (0부터)
    pub character: u32, // 컬럼 (UTF-16 코드 유닛 기준)
}

/// 텍스트 범위
pub struct Range {
    pub start: Position,
    pub end: Position,
}

/// 파일 내 위치
pub struct Location {
    pub uri: String,    // file:///path/to/file.rs
    pub range: Range,
}
```

### 3.2 Hover

```rust
/// 호버 정보 (타입, 문서 등)
pub struct Hover {
    pub contents: HoverContents,  // Markdown 또는 plain text
    pub range: Option<Range>,
}
```

---

## 4. 지원 LSP 메서드

### 4.1 구현됨 (Phase 1)

| 메서드 | 용도 | 상태 |
|--------|------|------|
| `textDocument/definition` | 정의로 이동 | ✅ 구현됨 |
| `textDocument/references` | 참조 찾기 | ✅ 구현됨 |
| `textDocument/hover` | 심볼 정보 | ✅ 구현됨 |

### 4.2 의도적으로 미구현

| 메서드 | 이유 |
|--------|------|
| `textDocument/completion` | Agent가 직접 제안하므로 불필요 |
| `textDocument/publishDiagnostics` | 실시간 스트림 불필요 |
| `textDocument/semanticTokens` | 하이라이팅 불필요 |
| `textDocument/inlayHint` | UI 렌더링 불필요 |
| `textDocument/formatting` | Agent가 직접 포맷팅 |

---

## 5. 효율성 전략 상세

### 5.1 Lazy Loading

```rust
// 잘못된 방식 ❌
let manager = LspManager::new();
manager.start_all();  // 모든 언어 서버 즉시 시작

// 올바른 방식 ✅
let manager = LspManager::new();
// 아무것도 시작하지 않음

// Agent가 실제로 요청할 때만 시작
let client = manager.get_for_file(Path::new("src/main.rs")).await?;
// → rust-analyzer가 이 시점에 시작됨
```

### 5.2 Idle Timeout

```rust
// 10분 미사용 시 자동 종료
const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(600);

// 백그라운드 태스크에서 주기적 호출
manager.cleanup_idle().await;
// → 마지막 사용 후 10분 경과한 서버들 종료
```

### 5.3 Availability Cache

```rust
// 서버 설치 여부 5분 캐싱
const AVAILABILITY_CACHE_TTL: Duration = Duration::from_secs(300);

// 첫 요청: which rust-analyzer 실행
manager.get_or_start("rust", root).await?;

// 5분 내 재요청: 캐시된 결과 사용 (which 호출 없음)
manager.get_or_start("rust", root).await?;
```

---

## 6. 사용 예시

### 6.1 기본 사용

```rust
use forge_core::{LspManager, Position};

let manager = LspManager::new();

// 파일에 대한 클라이언트 가져오기 (자동으로 서버 시작)
let client = manager.get_for_file(Path::new("src/main.rs")).await?;

// 정의로 이동
let locations = client.goto_definition(
    "file:///path/to/file.rs",
    Position::new(10, 5)
).await?;

for loc in locations {
    println!("Definition at {}:{}", loc.file_path().unwrap(), loc.range.start.line);
}
```

### 6.2 성능 모드 (LSP 비활성화)

```rust
// LSP 완전 비활성화
let manager = LspManager::disabled();

// 또는 런타임에 비활성화
manager.set_enabled(false).await;
```

---

## 7. 지원 언어

### 7.1 기본 설정

| 언어 | 서버 | 루트 패턴 |
|------|------|----------|
| Rust | rust-analyzer | `Cargo.toml` |
| TypeScript | typescript-language-server | `tsconfig.json`, `package.json` |
| Python | pylsp | `pyproject.toml`, `setup.py` |
| Go | gopls | `go.mod` |

### 7.2 확장자 매핑

```rust
match extension {
    "rs" => "rust",
    "ts" | "tsx" | "mts" | "cts" => "typescript",
    "js" | "jsx" | "mjs" | "cjs" => "javascript",
    "py" | "pyw" | "pyi" => "python",
    "go" => "go",
    // ...
}
```

---

## 8. 파일 구조

```
lsp/
├── mod.rs          # 모듈 진입점, 팩토리 함수
├── types.rs        # 경량 LSP 타입 (lsp-types 없이)
├── client.rs       # JSON-RPC 클라이언트
├── manager.rs      # Lazy Loading 매니저
└── CLAUDE.md       # 이 문서
```

---

## 9. 주의사항

### 9.1 UTF-16 오프셋

LSP는 UTF-16 코드 유닛 기반 오프셋을 사용합니다. Rust 문자열(UTF-8)과 변환이 필요할 수 있습니다.

```rust
fn utf8_to_utf16_offset(text: &str, utf8_offset: usize) -> usize {
    text[..utf8_offset].encode_utf16().count()
}
```

### 9.2 파일 URI (Windows)

Windows에서 파일 경로 변환 시 주의:

```rust
// Windows: C:\path\to\file -> file:///C:/path/to/file
// Unix: /path/to/file -> file:///path/to/file
```

### 9.3 서버 미설치

LSP 서버가 설치되어 있지 않으면 `Error::NotFound` 반환. Agent는 이를 처리하고 LSP 없이 작업을 계속해야 합니다.

---

## 10. 향후 계획

### Phase 2 (선택적)

- `workspace/symbol` - 전체 프로젝트 심볼 검색
- `textDocument/documentSymbol` - 파일 내 심볼 목록

### 구현하지 않을 기능

- 실시간 Diagnostics
- Completion (Agent가 직접)
- Formatting (Agent가 직접)
- Semantic Highlighting

---

## 11. 테스트

```bash
# 모듈 테스트 실행
cargo test -p forge-core

# 결과: 12 tests passed
```
