# Plugin System Architecture

## 목표

Claude Code와 호환되면서 쉽게 플러그인/스킬을 추가하고 다운로드할 수 있는 시스템.

## 핵심 원칙

1. **Layer 분리 유지**: Layer1(Foundation)의 Permission, Storage와 연동
2. **동적 등록**: 런타임에 Tool/Skill 추가/제거 가능
3. **Claude Code 호환**: .claude/, SKILL.md 형식 지원
4. **확장성**: Native(Rust), WASM, Script 플러그인 지원

## 디렉토리 구조

```
~/.forgecode/                        # 사용자 레벨
├── plugins/                         # 플러그인 저장소
│   ├── installed.json              # 설치된 플러그인 목록
│   └── {plugin-id}/                # 플러그인별 디렉토리
│       ├── plugin.json             # 플러그인 메타데이터
│       ├── config.json             # 플러그인 설정
│       └── ...                     # 플러그인 파일들
├── skills/                         # 파일 기반 스킬
│   └── {skill-name}/
│       └── SKILL.md               # Claude Code 호환
└── settings.json                   # 전역 설정

.forgecode/ (또는 .claude/)          # 프로젝트 레벨
├── plugins/                        # 프로젝트별 플러그인
├── skills/                         # 프로젝트별 스킬
└── settings.json                   # 프로젝트 설정
```

## 아키텍처

```
┌─────────────────────────────────────────────────────────────────────┐
│                         Layer2-Core                                  │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │                     PluginManager                              │  │
│  │  - load(plugin)       : 플러그인 로드                          │  │
│  │  - unload(id)         : 플러그인 언로드                        │  │
│  │  - install(source)    : 플러그인 설치 (다운로드)               │  │
│  │  - uninstall(id)      : 플러그인 제거                          │  │
│  │  - discover()         : 파일 기반 플러그인 발견                │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                              │                                       │
│            ┌─────────────────┼─────────────────┐                    │
│            ▼                 ▼                 ▼                    │
│  ┌─────────────────┐ ┌─────────────────┐ ┌─────────────────┐       │
│  │ DynamicTool     │ │ DynamicSkill    │ │ PluginRegistry  │       │
│  │ Registry        │ │ Registry        │ │                 │       │
│  │ (Interior Mut)  │ │ (Interior Mut)  │ │ - plugins       │       │
│  │ - register()    │ │ - register()    │ │ - status        │       │
│  │ - unregister()  │ │ - unregister()  │ │ - metadata      │       │
│  └─────────────────┘ └─────────────────┘ └─────────────────┘       │
│                              │                                       │
│  ┌───────────────────────────┼───────────────────────────────────┐  │
│  │                    PluginStore                                 │  │
│  │  - installed.json 관리                                         │  │
│  │  - 플러그인 영속화                                              │  │
│  │  - 버전 관리                                                    │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                              │                                       │
│  ┌───────────────────────────┼───────────────────────────────────┐  │
│  │                  PluginDiscovery                               │  │
│  │  - 디렉토리 스캔                                                │  │
│  │  - plugin.json 파싱                                            │  │
│  │  - SKILL.md 파싱 (SkillLoader 사용)                            │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                              │                                       │
│  ┌───────────────────────────┼───────────────────────────────────┐  │
│  │                  PluginInstaller                               │  │
│  │  - GitHub에서 다운로드                                          │  │
│  │  - 압축 해제                                                    │  │
│  │  - 의존성 설치                                                  │  │
│  │  - 버전 체크                                                    │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                                                                      │
├─────────────────────────────────────────────────────────────────────┤
│                       Layer1-Foundation                              │
├─────────────────────────────────────────────────────────────────────┤
│  PermissionService          Storage (SQLite/JSON)                   │
│  - check_permission()       - plugin state                          │
│  - request_permission()     - tool/skill metadata                   │
└─────────────────────────────────────────────────────────────────────┘
```

## 모듈 구조

```
crates/Layer2-core/src/plugin/
├── mod.rs                  # 모듈 export
├── CLAUDE.md              # 이 문서
│
├── traits.rs              # Plugin, PluginContext trait (기존)
├── registry.rs            # PluginRegistry (기존)
├── events.rs              # EventBus, PluginEvent (기존)
├── manifest.rs            # PluginManifest (기존)
│
├── manager.rs             # PluginManager (개선)
│                          # - DynamicToolRegistry/SkillRegistry 사용
│                          # - install/uninstall 추가
│
├── store.rs               # [NEW] PluginStore
│                          # - installed.json 관리
│                          # - 플러그인 상태 영속화
│
├── discovery.rs           # [NEW] PluginDiscovery
│                          # - 디렉토리 스캔
│                          # - plugin.json/SKILL.md 파싱
│
└── installer.rs           # [NEW] PluginInstaller
                           # - 다운로드/설치/제거
```

## 데이터 모델

### installed.json

```json
{
  "version": "1.0",
  "plugins": [
    {
      "id": "forge.git-enhanced",
      "version": "1.2.0",
      "installed_at": "2024-01-15T10:30:00Z",
      "source": "github:forgecode/plugins/git-enhanced",
      "enabled": true,
      "path": "~/.forgecode/plugins/forge.git-enhanced"
    }
  ]
}
```

### plugin.json (Claude Code 호환)

```json
{
  "id": "forge.git-enhanced",
  "name": "Git Enhanced",
  "version": "1.2.0",
  "description": "Enhanced git operations",
  "author": "ForgeCode Team",
  "license": "MIT",
  "main": "index.js",           // 또는 "main.wasm"
  "type": "script",             // "native" | "wasm" | "script"
  "provides": {
    "tools": ["git-diff-summary", "git-branch-cleanup"],
    "skills": ["smart-commit"]
  },
  "dependencies": {
    "forge.core": ">=0.1.0"
  },
  "permissions": [
    "execute:git",
    "read:.**/.git/**"
  ]
}
```

## API 설계

### PluginManager (개선)

```rust
impl PluginManager {
    // 기존 메서드
    pub async fn load(&self, plugin: Arc<dyn Plugin>) -> Result<()>;
    pub async fn unload(&self, id: &str) -> Result<()>;

    // 새 메서드
    /// 플러그인 디렉토리에서 발견된 플러그인 모두 로드
    pub async fn discover_and_load(&self) -> Result<Vec<String>>;

    /// 소스에서 플러그인 설치 (GitHub URL, 로컬 경로 등)
    pub async fn install(&self, source: &str) -> Result<InstalledPlugin>;

    /// 플러그인 제거
    pub async fn uninstall(&self, id: &str) -> Result<()>;

    /// 플러그인 업데이트
    pub async fn update(&self, id: &str) -> Result<()>;

    /// 설치된 플러그인 목록
    pub async fn list_installed(&self) -> Vec<InstalledPlugin>;
}
```

### PluginStore

```rust
pub struct PluginStore {
    base_dir: PathBuf,          // ~/.forgecode/plugins
}

impl PluginStore {
    pub fn new(base_dir: PathBuf) -> Self;

    /// 설치된 플러그인 목록 로드
    pub async fn load_installed(&self) -> Result<Vec<InstalledPlugin>>;

    /// 플러그인 설치 기록
    pub async fn record_install(&self, plugin: &InstalledPlugin) -> Result<()>;

    /// 플러그인 제거 기록
    pub async fn record_uninstall(&self, id: &str) -> Result<()>;

    /// 플러그인 상태 업데이트
    pub async fn update_status(&self, id: &str, enabled: bool) -> Result<()>;

    /// 플러그인 디렉토리 경로
    pub fn plugin_dir(&self, id: &str) -> PathBuf;
}
```

### PluginDiscovery

```rust
pub struct PluginDiscovery {
    search_paths: Vec<PathBuf>,
}

impl PluginDiscovery {
    pub fn new(working_dir: &Path) -> Self;

    /// 모든 플러그인 디렉토리 스캔
    pub async fn discover_plugins(&self) -> Vec<DiscoveredPlugin>;

    /// 모든 스킬 파일 스캔
    pub async fn discover_skills(&self) -> Vec<FileBasedSkill>;

    /// plugin.json 파싱
    fn parse_plugin_json(&self, path: &Path) -> Result<PluginManifest>;
}
```

### PluginInstaller

```rust
pub struct PluginInstaller {
    store: Arc<PluginStore>,
    http_client: reqwest::Client,
}

impl PluginInstaller {
    /// GitHub에서 플러그인 다운로드
    pub async fn install_from_github(&self, repo: &str, tag: Option<&str>) -> Result<InstalledPlugin>;

    /// 로컬 경로에서 플러그인 설치
    pub async fn install_from_path(&self, path: &Path) -> Result<InstalledPlugin>;

    /// 플러그인 제거
    pub async fn uninstall(&self, id: &str) -> Result<()>;

    /// 플러그인 업데이트 체크
    pub async fn check_updates(&self, id: &str) -> Result<Option<String>>;
}
```

## 구현 순서

### Phase 1: 기본 인프라 ✓

1. [x] DynamicToolRegistry - Interior Mutability (기존 구현됨)
2. [x] DynamicSkillRegistry - Interior Mutability (기존 구현됨)
3. [x] PluginManager 수정 - Dynamic 레지스트리 사용

### Phase 2: 저장소 및 발견 ✓

4. [x] PluginStore - installed.json 관리
5. [x] PluginDiscovery - 디렉토리 스캔, plugin.json 파싱
6. [x] SkillLoader 통합 - SKILL.md 발견 및 로드

### Phase 3: 설치 시스템 ✓

7. [x] PluginInstaller - GitHub 다운로드
8. [ ] CLI 명령어 인터페이스 (Layer4에서)
9. [ ] 의존성 해결

### Phase 4: 고급 기능

10. [ ] WASM 플러그인 런타임
11. [ ] Script 플러그인 (JavaScript/Lua)
12. [ ] 플러그인 마켓플레이스 연동

## 사용 예시

```rust
// PluginManager 생성
let manager = PluginManager::new(PathBuf::from("."));

// 발견된 플러그인/스킬 자동 로드
let loaded = manager.discover_and_load().await?;
println!("Loaded {} plugins", loaded.len());

// GitHub에서 플러그인 설치
manager.install("github:forgecode/plugins/git-enhanced@v1.2.0").await?;

// 설치된 플러그인 목록
for plugin in manager.list_installed().await {
    println!("{}: {} ({})", plugin.id, plugin.name, plugin.version);
}

// 플러그인 제거
manager.uninstall("forge.git-enhanced").await?;
```

## CLI 명령어 (Layer4에서 구현)

```bash
# 플러그인 관리
forge plugin list                    # 설치된 플러그인 목록
forge plugin install <source>        # 플러그인 설치
forge plugin uninstall <id>          # 플러그인 제거
forge plugin update [id]             # 플러그인 업데이트
forge plugin enable/disable <id>     # 플러그인 활성화/비활성화

# 스킬 관리
forge skill list                     # 사용 가능한 스킬 목록
forge skill info <name>              # 스킬 상세 정보
```

## Layer 연동

### Layer1 Permission 연동

```rust
// 플러그인이 Tool 등록 시 권한 정의
impl Plugin for MyPlugin {
    async fn on_load(&self, ctx: &PluginContext) -> Result<()> {
        // Tool과 필요한 권한 함께 등록
        ctx.register_tool_with_permissions(
            Arc::new(MyTool::new()),
            &["execute:git", "read:.**/.git/**"],
        ).await;
        Ok(())
    }
}
```

### Layer1 Storage 연동

```rust
// PluginStore가 Layer1 Storage 사용
impl PluginStore {
    pub fn new(storage: Arc<dyn Storage>) -> Self {
        // Layer1의 JsonStore 또는 SQLite 사용
    }
}
```
