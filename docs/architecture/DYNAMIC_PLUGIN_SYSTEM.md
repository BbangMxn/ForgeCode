# ForgeCode Dynamic Plugin/Skill System Architecture

## 1. 개요

ForgeCode의 Plugin과 Skill 시스템은 Claude Code와 유사하게 런타임에 동적으로 교체/변경이 가능해야 합니다.

### 1.1 목표
- **동적 교체**: 런타임에 Tool/Skill/Plugin 추가/제거/교체
- **Hot-reload**: 재시작 없이 변경사항 적용
- **확장성**: 외부 개발자가 쉽게 플러그인 개발
- **안전성**: 잘못된 플러그인이 전체 시스템에 영향 주지 않음

### 1.2 핵심 원칙
1. **Interior Mutability**: `Arc<RwLock<T>>` 패턴으로 thread-safe 변경
2. **Event-driven**: 변경 시 이벤트 발행으로 리스너에게 통보
3. **Version Control**: 변경 이력 추적 및 롤백 지원
4. **Isolation**: 플러그인 간 격리, 오류 전파 방지

---

## 2. 전체 아키텍처

```
┌─────────────────────────────────────────────────────────────────────┐
│                         Layer4: CLI                                  │
│  ┌─────────────────────────────────────────────────────────────────┐│
│  │  User Input → Skill Detection → Agent Loop → Output             ││
│  └─────────────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────────┘
                                   │
                                   ▼
┌─────────────────────────────────────────────────────────────────────┐
│                       Layer3: Agent                                  │
│  ┌─────────────────────────────────────────────────────────────────┐│
│  │  AgentContext                                                    ││
│  │  ├── SkillExecutor (Skill 실행 관리)                            ││
│  │  ├── SubAgentManager (하위 Agent 관리)                          ││
│  │  └── EventBus (이벤트 구독/발행)                                ││
│  └─────────────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────────┘
                                   │
                                   ▼
┌─────────────────────────────────────────────────────────────────────┐
│                       Layer2: Core                                   │
│  ┌────────────────────────────────────────────────────────────────┐ │
│  │                   PluginManager                                 │ │
│  │  ┌──────────────────────────────────────────────────────────┐  │ │
│  │  │  PluginRegistry                                           │  │ │
│  │  │  ├── Plugin A (tools: [...], skills: [...])              │  │ │
│  │  │  ├── Plugin B (tools: [...], skills: [...])              │  │ │
│  │  │  └── Plugin C (MCP bridge)                               │  │ │
│  │  └──────────────────────────────────────────────────────────┘  │ │
│  │                          │                                      │ │
│  │  ┌──────────────────────┴───────────────────────────────────┐  │ │
│  │  │              DynamicRegistry System                       │  │ │
│  │  │  ┌─────────────────┐  ┌─────────────────┐                │  │ │
│  │  │  │ DynamicTool     │  │ DynamicSkill    │                │  │ │
│  │  │  │ Registry        │  │ Registry        │                │  │ │
│  │  │  │ (RwLock based)  │  │ (RwLock based)  │                │  │ │
│  │  │  └─────────────────┘  └─────────────────┘                │  │ │
│  │  │           │                    │                          │  │ │
│  │  │           └────────────────────┘                          │  │ │
│  │  │                    │                                      │  │ │
│  │  │              EventBus                                     │  │ │
│  │  │  (Registry 변경 이벤트 발행)                              │  │ │
│  │  └──────────────────────────────────────────────────────────┘  │ │
│  └────────────────────────────────────────────────────────────────┘ │
│                                                                      │
│  ┌────────────────────────────────────────────────────────────────┐ │
│  │                   MCP Bridge                                    │ │
│  │  (외부 MCP 서버 Tool을 DynamicToolRegistry에 동기화)           │ │
│  └────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────┘
                                   │
                                   ▼
┌─────────────────────────────────────────────────────────────────────┐
│                      Layer1: Foundation                              │
│  ├── Tool trait                                                      │
│  ├── Permission System                                               │
│  ├── Configuration                                                   │
│  └── Error Types                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 3. DynamicRegistry 시스템

### 3.1 핵심 컴포넌트

```rust
/// 동적 레지스트리 - 모든 Registry의 기반
pub struct DynamicRegistry<T: ?Sized + Send + Sync> {
    /// 항목 저장소 (RwLock으로 내부 가변성)
    entries: RwLock<HashMap<String, RegistryEntry<T>>>,

    /// 카테고리별 인덱스
    categories: RwLock<HashMap<String, Vec<String>>>,

    /// 이벤트 채널 (변경 시 브로드캐스트)
    event_tx: broadcast::Sender<RegistryEvent>,

    /// 이벤트 핸들러 (동기 처리)
    handlers: RwLock<Vec<Arc<dyn RegistryEventHandler>>>,
}
```

### 3.2 RegistryEntry

```rust
/// 레지스트리 항목
pub struct RegistryEntry<T: ?Sized> {
    /// 실제 값
    pub value: Arc<T>,

    /// 메타데이터
    pub metadata: EntryMetadata,
}

/// 메타데이터
pub struct EntryMetadata {
    pub key: String,
    pub category: String,
    pub version: String,
    pub provider: Option<String>,  // 제공 Plugin 이름
    pub priority: i32,             // 같은 키 충돌 시 우선순위
    pub state: EntryState,         // Active, Inactive, Error, Deprecated
    pub registered_at: DateTime<Utc>,
    pub replace_count: u32,        // 교체 횟수 (디버깅용)
}
```

### 3.3 이벤트 시스템

```rust
pub enum RegistryEvent {
    Registered { key, category, version, provider },
    Unregistered { key, reason },
    Replaced { key, old_version, new_version },
    Enabled { key },
    Disabled { key },
    Cleared,
    BulkChange { added, removed, replaced },
}
```

---

## 4. Skill 시스템

### 4.1 Skill 정의

```rust
#[async_trait]
pub trait Skill: Send + Sync {
    /// 스킬 정의 (이름, 명령어, 설명, 인자 등)
    fn definition(&self) -> SkillDefinition;

    /// 시스템 프롬프트 (Agent Loop 사용 시)
    fn system_prompt(&self) -> Option<String>;

    /// Agent Loop 필요 여부
    fn requires_agent_loop(&self) -> bool;

    /// 스킬 실행
    async fn execute(&self, ctx: &SkillContext<'_>, input: SkillInput) -> Result<SkillOutput>;
}
```

### 4.2 Skill 실행 흐름

```
User Input: "/commit -m 'fix bug'"
           │
           ▼
┌─────────────────────────────────────┐
│  SkillRegistry.find_for_input()     │
│  → CommitSkill 찾음                 │
└─────────────────────────────────────┘
           │
           ▼
┌─────────────────────────────────────┐
│  skill.requires_agent_loop()?       │
│  → true면 Agent Loop 실행           │
│  → false면 직접 실행                │
└─────────────────────────────────────┘
           │
           ▼ (requires_agent_loop = true)
┌─────────────────────────────────────┐
│  SkillExecutor                      │
│  1. skill.system_prompt() 설정     │
│  2. skill.execute() → 초기 프롬프트 │
│  3. Agent Loop 시작                 │
│     - LLM 호출                      │
│     - Tool 실행                     │
│     - 반복                          │
│  4. 결과 반환                       │
└─────────────────────────────────────┘
```

### 4.3 빌트인 Skills

| Skill | 명령어 | 설명 | Agent Loop |
|-------|--------|------|------------|
| CommitSkill | /commit | Git 커밋 자동화 | ✓ |
| ReviewPrSkill | /review-pr | PR 리뷰 | ✓ |
| ExplainSkill | /explain | 코드 설명 | ✓ |
| HelpSkill | /help | 도움말 | ✗ |

---

## 5. Plugin 시스템

### 5.1 Plugin 정의

```rust
#[async_trait]
pub trait Plugin: Send + Sync {
    /// 매니페스트 (메타데이터)
    fn manifest(&self) -> PluginManifest;

    /// 지원 기능
    fn capabilities(&self) -> Vec<PluginCapability>;

    /// 로드 시 호출 (Tool/Skill 등록)
    async fn on_load(&self, ctx: &PluginContext) -> Result<()>;

    /// 언로드 시 호출
    async fn on_unload(&self, ctx: &PluginContext) -> Result<()>;

    /// 시스템 프롬프트 수정
    fn modify_system_prompt(&self, prompt: &str) -> Option<String>;
}
```

### 5.2 Plugin 매니페스트

```rust
pub struct PluginManifest {
    pub id: String,           // 고유 ID (예: "forge.git-enhanced")
    pub name: String,         // 표시 이름
    pub version: PluginVersion,
    pub description: String,
    pub author: Option<String>,
    pub dependencies: Vec<PluginDependency>,
    pub provides: PluginProvides,  // 제공하는 Tool/Skill 목록
    pub plugin_type: PluginType,   // Native, Wasm, Script, Remote
}
```

### 5.3 Plugin 라이프사이클

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│   Created   │────▶│   Loading   │────▶│   Active    │
└─────────────┘     └─────────────┘     └─────────────┘
                                              │
                          ┌───────────────────┤
                          ▼                   ▼
                    ┌─────────────┐     ┌─────────────┐
                    │   Inactive  │     │   Unloaded  │
                    └─────────────┘     └─────────────┘
```

### 5.4 Plugin 유형

| 유형 | 설명 | 로딩 방식 |
|------|------|----------|
| Native | Rust로 작성된 컴파일타임 플러그인 | 직접 링크 |
| Wasm | WebAssembly 플러그인 | wasmtime 로드 |
| Script | JS/Lua 스크립트 | 인터프리터 |
| Remote | 원격 MCP 서버 | HTTP/stdio |

---

## 6. Hot-reload 메커니즘

### 6.1 교체 전략

```rust
// 안전한 교체: 새 버전이 준비되면 교체
pub async fn safe_replace(&self, key: &str, new_value: Arc<T>) -> Result<()> {
    // 1. 새 항목 검증
    self.validate(new_value.clone())?;

    // 2. 기존 항목 비활성화 (요청은 계속 처리)
    self.set_state(key, EntryState::Deprecated).await;

    // 3. 새 항목으로 교체
    self.replace(key, new_value, new_version).await;

    // 4. 이벤트 발행
    self.emit_event(RegistryEvent::Replaced { ... }).await;

    Ok(())
}
```

### 6.2 롤백 지원

```rust
pub struct RegistrySnapshot<T> {
    entries: HashMap<String, RegistryEntry<T>>,
    timestamp: DateTime<Utc>,
}

impl DynamicRegistry<T> {
    /// 스냅샷 생성
    pub async fn create_snapshot(&self) -> RegistrySnapshot<T>;

    /// 스냅샷으로 복원
    pub async fn restore_snapshot(&self, snapshot: RegistrySnapshot<T>);
}
```

---

## 7. 통합 설계

### 7.1 AgentContext 확장

```rust
pub struct AgentContext {
    // 기존
    pub gateway: Arc<Gateway>,
    pub permissions: Arc<PermissionService>,
    pub working_dir: PathBuf,
    pub system_prompt: String,

    // 새로 추가
    pub tools: Arc<DynamicToolRegistry>,      // 동적 Tool
    pub skills: Arc<DynamicSkillRegistry>,    // 동적 Skill
    pub plugins: Arc<PluginManager>,          // Plugin 관리
    pub event_bus: Arc<EventBus>,             // 이벤트 버스
}
```

### 7.2 사용 예시

```rust
// 런타임에 Tool 추가
ctx.tools.register(Arc::new(MyCustomTool::new())).await?;

// 런타임에 Skill 교체
ctx.skills.replace("commit", Arc::new(MyBetterCommitSkill::new()), "2.0.0").await?;

// Plugin 로드
ctx.plugins.load(Arc::new(GitEnhancedPlugin::new())).await?;

// 변경 이벤트 구독
let mut rx = ctx.tools.subscribe();
tokio::spawn(async move {
    while let Ok(event) = rx.recv().await {
        match event {
            RegistryEvent::Registered { key, .. } => {
                println!("New tool available: {}", key);
            }
            RegistryEvent::Replaced { key, old_version, new_version } => {
                println!("Tool {} upgraded: {} -> {}", key, old_version, new_version);
            }
            _ => {}
        }
    }
});
```

---

## 8. 보안 고려사항

### 8.1 Plugin 샌드박싱
- Wasm 플러그인: wasmtime 샌드박스
- Script 플러그인: 제한된 API만 노출
- 파일 시스템 접근 제한

### 8.2 권한 검사
- Plugin이 등록하는 Tool은 권한 시스템 적용
- 민감한 Tool은 명시적 승인 필요

### 8.3 검증
- Plugin 서명 검증 (향후)
- 버전 호환성 검사
- 의존성 충돌 감지

---

## 9. 구현 우선순위

### Phase 1 (핵심)
- [x] DynamicRegistry 기본 구조
- [x] DynamicToolRegistry
- [x] DynamicSkillRegistry
- [x] 기본 이벤트 시스템

### Phase 2 (통합)
- [ ] AgentContext에 동적 레지스트리 통합
- [ ] SkillExecutor 구현
- [ ] PluginManager 동적 레지스트리 사용

### Phase 3 (고급)
- [ ] Hot-reload 완전 지원
- [ ] 스냅샷/롤백
- [ ] Wasm 플러그인 로더

### Phase 4 (생태계)
- [ ] Plugin 마켓플레이스
- [ ] 원격 Plugin 설치
- [ ] Plugin 서명 검증

---

## 10. 테스트 전략

### 10.1 단위 테스트
- Registry CRUD 연산
- 이벤트 발행/수신
- 상태 전이

### 10.2 통합 테스트
- Plugin 로드/언로드 사이클
- Tool/Skill 교체 시 기존 요청 처리
- 여러 Plugin 간 충돌

### 10.3 부하 테스트
- 동시 등록/조회
- 대량 이벤트 처리
- 메모리 누수 검사
