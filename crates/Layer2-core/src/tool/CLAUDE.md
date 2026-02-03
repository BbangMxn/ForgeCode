# Tool System Design (Layer2-core)

## 아키텍처 개요

```
┌─────────────────────────────────────────────────────────────────────┐
│ Layer4-CLI                                                           │
│ └── PermissionDelegate 구현 (UI 프롬프트)                             │
├─────────────────────────────────────────────────────────────────────┤
│ Layer3-Agent                                                         │
│ └── TaskContext 구현 (도구 오케스트레이션)                             │
├─────────────────────────────────────────────────────────────────────┤
│ Layer2-Core (이 레이어)                                              │
│ ├── ToolRegistry - 도구 등록/조회/관리                                │
│ ├── RuntimeContext - ToolContext 구현                                │
│ └── Builtin Tools                                                    │
│     ├── read - 파일 읽기                                             │
│     ├── write - 파일 쓰기                                            │
│     ├── edit - 파일 편집 (string replace)                            │
│     ├── glob - 파일 패턴 검색                                        │
│     ├── grep - 내용 검색 (regex)                                     │
│     └── bash - Shell 명령 실행                                       │
├─────────────────────────────────────────────────────────────────────┤
│ Layer1-Foundation                                                    │
│ ├── Tool trait, ToolMeta, ToolResult                                 │
│ ├── ToolContext trait                                                │
│ ├── PermissionService, PermissionAction                              │
│ └── CommandAnalyzer, PathAnalyzer (보안)                             │
└─────────────────────────────────────────────────────────────────────┘
```

## Layer1 핵심 타입

### Tool Trait
```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn meta(&self) -> ToolMeta;
    fn name(&self) -> &str;
    fn schema(&self) -> Value;  // MCP 호환 JSON Schema
    fn required_permission(&self, input: &Value) -> Option<PermissionAction>;
    async fn execute(&self, input: Value, context: &dyn ToolContext) -> Result<ToolResult>;
}
```

### PermissionAction 종류
```rust
pub enum PermissionAction {
    Execute { command: String },           // Bash 명령 실행
    FileWrite { path: String },            // 파일 쓰기
    FileDelete { path: String },           // 파일 삭제
    FileReadSensitive { path: String },    // 민감 파일 읽기
    Network { url: String },               // 네트워크 요청
    Custom { name: String, details: String },
}
```

### 권한 카테고리
- `filesystem` - 파일 읽기/쓰기/삭제
- `execute` - Shell 명령 실행
- `network` - 네트워크 요청
- `mcp` - MCP 도구 호출
- `system` - 시스템 설정

## 도구 설계 (모두 구현 완료 ✓)

### 1. ReadTool ✓
- **목적**: 파일 내용 읽기
- **권한**: 민감 파일만 `FileReadSensitive`
- **특징**:
  - 줄 번호 포함 (cat -n 스타일)
  - offset/limit으로 대용량 파일 처리
  - 바이너리 파일 감지

### 2. WriteTool ✓
- **목적**: 파일 쓰기 (전체 덮어쓰기)
- **권한**: 항상 `FileWrite`
- **특징**:
  - 새 파일 생성 또는 덮어쓰기
  - 디렉토리 자동 생성 옵션
  - 민감/시스템 파일 차단

### 3. EditTool ✓
- **목적**: 파일 부분 편집 (string replace)
- **권한**: 항상 `FileWrite`
- **특징**:
  - old_string → new_string 치환
  - unique match 검증
  - replace_all 옵션

### 4. GlobTool ✓
- **목적**: 파일 패턴 검색
- **권한**: 없음 (읽기 전용)
- **특징**:
  - gitignore 존중 (ignore 라이브러리)
  - 수정 시간 정렬
  - 결과 제한

### 5. GrepTool ✓
- **목적**: 내용 검색 (정규식)
- **권한**: 없음 (읽기 전용)
- **특징**:
  - ripgrep 스타일
  - 컨텍스트 라인 (-A, -B, -C)
  - 파일 타입/글로브 필터
  - 3가지 출력 모드: content, files_with_matches, count

### 6. BashTool ✓
- **목적**: Shell 명령 실행
- **권한**: 안전 명령어 외 `Execute`
- **특징**:
  - Layer1 CommandAnalyzer로 위험도 분석
  - 금지 명령어 자동 차단 (rm -rf /, fork bomb 등)
  - 대화형 명령어 차단
  - 타임아웃 지원 (기본 2분, 최대 10분)
  - 안전 명령어는 권한 불필요 (ls, pwd, git status 등)

## 권한 흐름

```
1. Tool.required_permission(input) 호출
   → PermissionAction 반환 (또는 None)

2. Context.check_permission(tool, action) 호출
   → PermissionStatus 반환

3. Status에 따라:
   - Granted/AutoApproved → 실행
   - Denied → 에러 반환
   - Unknown → Context.request_permission() 호출
     → UI에서 사용자 확인
     → 결과에 따라 grant_session/grant_permanent
```

## 보안 원칙

1. **최소 권한**: 필요한 경우에만 권한 요청
2. **명시적 승인**: 위험 작업은 항상 사용자 확인
3. **금지 목록**: rm -rf / 등 위험 명령어 차단
4. **민감 경로 보호**: .env, .ssh 등 자동 탐지
5. **명령어 분석**: Layer1 CommandAnalyzer 활용

## 파일 구조

```
tool/
├── mod.rs           - 모듈 진입점, re-exports
├── context.rs       - RuntimeContext, DefaultShellConfig
├── registry.rs      - ToolRegistry
├── CLAUDE.md        - 이 문서
└── builtin/
    ├── mod.rs       - builtin 도구 모음, all_tools(), core_tools()
    ├── read.rs      - ReadTool ✓
    ├── write.rs     - WriteTool ✓
    ├── edit.rs      - EditTool ✓
    ├── glob.rs      - GlobTool ✓
    ├── grep.rs      - GrepTool ✓
    └── bash.rs      - BashTool ✓
```

## 테스트 현황

- **총 57개 테스트 통과**
- LSP 모듈: 12개
- Tool 모듈: 45개
  - read: 6개
  - write: 5개
  - edit: 4개
  - glob: 4개
  - grep: 5개
  - bash: 6개
  - builtin: 5개
  - context: 3개
  - registry: 5개
  - lib: 4개

## 구현 체크리스트

- [x] ReadTool - 파일 읽기
- [x] WriteTool - 파일 쓰기
- [x] EditTool - 파일 편집
- [x] GlobTool - 패턴 검색
- [x] GrepTool - 내용 검색
- [x] BashTool - Shell 실행
- [x] ToolRegistry - 도구 관리
- [x] RuntimeContext - 실행 컨텍스트
- [x] DefaultShellConfig - Shell 설정
