# forge-task

Task 관리 및 실행 시스템 - Sub-agent 오케스트레이션 지원

## 1. 설계 철학

### 1.1 핵심 개념

Claude Code의 Task Tool 패턴을 참고하여 설계:
- **Sub-agent 생성**: 전문화된 에이전트를 동적으로 생성
- **컨텍스트 격리**: 각 sub-agent는 독립적인 컨텍스트 윈도우
- **백그라운드 실행**: 장시간 작업을 비동기로 실행
- **결과 반환**: 완료 후 요약된 결과만 메인 세션에 반환

### 1.2 오케스트레이션 패턴

Microsoft/Google의 AI Agent 패턴 연구 기반:

| 패턴 | 설명 | 사용 시점 |
|------|------|-----------|
| **Sequential** | 선형 파이프라인 | 단계별 의존성 있는 작업 |
| **Concurrent** | 병렬 실행 후 집계 | 독립적인 분석 작업 |
| **Handoff** | 동적 라우팅 | 전문가 에이전트로 위임 |
| **Supervisor** | 중앙 조율자 | 복잡한 멀티 에이전트 |

---

## 2. 현재 구현 상태

### 2.1 완성된 모듈

```
forge-task/
├── task.rs          ✅ Task, TaskId, TaskResult, ExecutionMode
├── state.rs         ✅ TaskState (7개 상태)
├── manager.rs       ✅ TaskManager (동시성 제어)
└── executor/
    ├── trait.rs     ✅ Executor trait
    ├── local.rs     ✅ LocalExecutor
    └── container.rs ✅ ContainerExecutor (Docker)
```

### 2.2 핵심 타입

```rust
// Task 구조체
pub struct Task {
    pub id: TaskId,
    pub session_id: String,
    pub tool_name: String,
    pub command: String,
    pub input: serde_json::Value,
    pub state: TaskState,
    pub execution_mode: ExecutionMode,
    pub timeout: Duration,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

// 실행 모드
pub enum ExecutionMode {
    Local,
    Container {
        image: String,
        workdir: Option<String>,
        env: Vec<(String, String)>,
        volumes: Vec<(String, String)>,
    },
}

// 상태 머신
pub enum TaskState {
    Pending,
    Queued,
    Running,
    Completed(TaskResult),
    Failed(String),
    Timeout,
    Cancelled,
}
```

### 2.3 TaskManager API

```rust
impl TaskManager {
    pub async fn new(config: TaskManagerConfig) -> Self;
    pub async fn submit(&self, task: Task) -> TaskId;
    pub async fn execute_task(&self, task_id: TaskId);
    pub async fn get(&self, task_id: TaskId) -> Option<Task>;
    pub async fn get_by_session(&self, session_id: &str) -> Vec<Task>;
    pub async fn cancel(&self, task_id: TaskId) -> Result<()>;
    pub async fn wait(&self, task_id: TaskId) -> Option<TaskResult>;
    pub async fn running_count(&self) -> usize;
    pub async fn pending_count(&self) -> usize;
}
```

---

## 3. 추가 구현 필요 사항

### 3.1 Sub-agent 시스템 🔴 HIGH

```rust
// 새로 추가할 모듈: subagent/

/// Sub-agent 타입
pub enum SubAgentType {
    /// 읽기 전용, 코드베이스 탐색 최적화
    Explore,

    /// 아키텍처 설계, 수정 불가
    Plan,

    /// 모든 도구 접근 가능
    General,

    /// 명령 실행 전문
    Bash,

    /// 사용자 정의
    Custom(String),
}

/// Sub-agent 설정
pub struct SubAgentConfig {
    /// 에이전트 타입
    pub agent_type: SubAgentType,

    /// 시스템 프롬프트
    pub system_prompt: String,

    /// 허용된 도구 목록
    pub allowed_tools: Vec<String>,

    /// 거부된 도구 목록
    pub disallowed_tools: Vec<String>,

    /// 사용할 모델 (sonnet, opus, haiku, inherit)
    pub model: ModelSelection,

    /// 권한 모드
    pub permission_mode: PermissionMode,

    /// 백그라운드 실행 여부
    pub run_in_background: bool,
}

/// Sub-agent 인스턴스
pub struct SubAgent {
    pub id: SubAgentId,
    pub config: SubAgentConfig,
    pub context: SubAgentContext,
    pub state: SubAgentState,
    pub parent_session_id: String,
}

/// Sub-agent 컨텍스트 (격리된 대화 히스토리)
pub struct SubAgentContext {
    pub messages: Vec<Message>,
    pub tool_results: Vec<ToolResult>,
    pub discoveries: Vec<Discovery>,
}
```

### 3.2 백그라운드 실행 🔴 HIGH

```rust
// TaskManager 확장

impl TaskManager {
    /// 백그라운드에서 sub-agent 실행
    pub async fn spawn_background(
        &self,
        config: SubAgentConfig,
        prompt: String,
    ) -> SubAgentId;

    /// 백그라운드 작업 상태 조회
    pub async fn get_background_status(&self, id: SubAgentId) -> SubAgentState;

    /// 백그라운드 작업 결과 조회 (파일 경로)
    pub async fn get_output_file(&self, id: SubAgentId) -> PathBuf;

    /// 이전 sub-agent 재개
    pub async fn resume(&self, id: SubAgentId, prompt: String) -> SubAgentId;
}
```

### 3.3 Context Store 🟡 MEDIUM

Deep Agent Architecture의 Context Store 패턴:

```rust
/// 지식 저장소 (sub-agent 간 공유)
pub struct ContextStore {
    /// 발견된 지식 항목
    discoveries: HashMap<DiscoveryId, Discovery>,

    /// 지식 카테고리별 인덱스
    by_category: HashMap<String, Vec<DiscoveryId>>,
}

/// 발견된 지식 항목
pub struct Discovery {
    pub id: DiscoveryId,
    pub category: String,      // "file_structure", "api_endpoint", etc.
    pub content: String,       // 정제된 지식
    pub source_agent: SubAgentId,
    pub created_at: DateTime<Utc>,
}

impl ContextStore {
    /// 지식 추가
    pub fn add(&mut self, discovery: Discovery);

    /// 카테고리별 조회
    pub fn get_by_category(&self, category: &str) -> Vec<&Discovery>;

    /// sub-agent에 주입할 컨텍스트 생성
    pub fn inject_context(&self, refs: &[DiscoveryId]) -> String;
}
```

### 3.4 Task 출력 스트리밍 🟡 MEDIUM

```rust
/// 실시간 출력 스트리밍
pub trait TaskOutputStream: Send + Sync {
    fn on_stdout(&self, line: &str);
    fn on_stderr(&self, line: &str);
    fn on_progress(&self, progress: f32, message: &str);
}

impl TaskManager {
    /// 스트리밍 출력과 함께 실행
    pub async fn execute_with_stream(
        &self,
        task_id: TaskId,
        stream: Arc<dyn TaskOutputStream>,
    );
}
```

### 3.5 우선순위 및 재시도 🟢 LOW

```rust
/// 작업 우선순위
pub enum TaskPriority {
    Low,
    Normal,
    High,
    Critical,
}

/// 재시도 설정
pub struct RetryConfig {
    pub max_attempts: u32,
    pub initial_delay: Duration,
    pub max_delay: Duration,
    pub backoff_factor: f64,
}
```

---

## 4. 사용 시나리오

### 4.1 Explore Sub-agent

```
사용자: "이 프로젝트의 API 엔드포인트 구조를 분석해줘"

[Agent 판단: 읽기 전용 탐색 필요]
    ↓
Task Tool 호출:
{
    "subagent_type": "Explore",
    "description": "API 구조 분석",
    "prompt": "src/ 디렉토리에서 API 엔드포인트를 찾고 문서화해줘",
    "model": "haiku"  // 빠른 응답
}
    ↓
[Explore Sub-agent 생성]
    - 허용 도구: Read, Grep, Glob
    - 거부 도구: Write, Edit, Bash
    ↓
[독립적 컨텍스트에서 실행]
    - 파일 검색
    - 패턴 분석
    - 결과 정리
    ↓
[메인 세션에 요약 반환]
    "API 엔드포인트 5개 발견:
     - GET /users (src/api/users.rs:25)
     - POST /auth/login (src/api/auth.rs:42)
     ..."
```

### 4.2 백그라운드 테스트 실행

```
사용자: "전체 테스트를 백그라운드에서 실행하고 결과 알려줘"

[Agent 판단: 장시간 작업, 백그라운드 적합]
    ↓
Task Tool 호출:
{
    "subagent_type": "Bash",
    "description": "테스트 실행",
    "prompt": "cargo test --all 실행하고 실패한 테스트만 보고해줘",
    "run_in_background": true
}
    ↓
[백그라운드 실행 시작]
    - 출력 파일: ~/.forgecode/tasks/{task_id}.output
    ↓
[메인 세션 계속 진행]
    "테스트가 백그라운드에서 실행 중입니다.
     진행 상황: /tasks 명령으로 확인 가능
     완료 시 알림 드리겠습니다."
    ↓
[30분 후 완료]
    "테스트 완료: 245 passed, 3 failed
     실패한 테스트:
     - test_auth_expired_token
     - test_db_connection_timeout
     - test_api_rate_limit"
```

### 4.3 병렬 분석

```
사용자: "인증, 데이터베이스, API 모듈을 병렬로 분석해줘"

[Agent 판단: 독립적 작업, Concurrent 패턴]
    ↓
3개의 Explore Sub-agent 동시 생성:
├── Auth Analyzer: src/auth/ 분석
├── DB Analyzer: src/db/ 분석
└── API Analyzer: src/api/ 분석
    ↓
[병렬 실행]
    ↓
[결과 집계]
    "분석 완료:

     인증 모듈:
     - JWT 기반 인증 사용
     - 토큰 만료: 24시간

     데이터베이스 모듈:
     - SQLite 사용
     - 마이그레이션 5개

     API 모듈:
     - RESTful 설계
     - 엔드포인트 12개"
```

---

## 5. 구현 로드맵

### Phase 1: Sub-agent 기본 구조 (1주)
- [ ] SubAgentType, SubAgentConfig 정의
- [ ] SubAgent 생성 및 실행 로직
- [ ] 도구 제한 (allowed/disallowed)

### Phase 2: 백그라운드 실행 (1주)
- [ ] spawn_background() 구현
- [ ] 출력 파일 저장
- [ ] 상태 조회 API

### Phase 3: Context Store (1주)
- [ ] Discovery 타입 정의
- [ ] ContextStore 구현
- [ ] Sub-agent 간 컨텍스트 공유

### Phase 4: 통합 및 테스트 (1주)
- [ ] Layer3-agent 연동
- [ ] 통합 테스트
- [ ] 문서화

---

## 6. 참고 자료

### 연구 출처

- [Claude Code Task Tool](https://dev.to/bhaidar/the-task-tool-claude-codes-agent-orchestration-system-4bf2)
- [Claude Code Sub-agents](https://code.claude.com/docs/en/sub-agents)
- [Microsoft AI Agent Design Patterns](https://learn.microsoft.com/en-us/azure/architecture/ai-ml/guide/ai-agent-design-patterns)
- [Google Agentic AI Design Patterns](https://docs.cloud.google.com/architecture/choose-design-pattern-agentic-ai-system)
- [Deep Agent Architecture](https://dev.to/apssouza22/a-deep-dive-into-deep-agent-architecture-for-ai-coding-assistants-3c8b)

### 핵심 인사이트

1. **컨텍스트 격리**: Sub-agent는 메인 대화 히스토리를 자동으로 받지 않음
2. **도구 제한**: 각 sub-agent는 필요한 도구만 접근
3. **정제된 결과**: 전체 출력이 아닌 요약만 반환
4. **재개 가능**: 이전 sub-agent 컨텍스트 유지하며 재개
5. **모델 선택**: 작업 특성에 따라 haiku/sonnet/opus 선택

---

## 7. 컨테이너 격리 및 보안 설계

### 7.1 격리 기술 비교

| 기술 | 격리 수준 | 시작 시간 | 메모리 오버헤드 | 사용 사례 |
|------|----------|-----------|----------------|-----------|
| **Docker 컨테이너** | 커널 공유 | ~50ms | 낮음 | 신뢰할 수 있는 코드 |
| **gVisor** | 사용자 공간 커널 | 50-100ms | 중간 | 반신뢰 코드 |
| **Kata Containers** | 경량 VM | 150-300ms | 수십 MB | 고보안 요구 |
| **Firecracker MicroVM** | 전용 커널 | 100-200ms | ~5MB | 서버리스/FaaS |

### 7.2 현재 ContainerExecutor 분석

```rust
// 현재 구현 (container.rs)
pub struct ContainerExecutor {
    docker: Arc<Docker>,           // Bollard Docker 클라이언트
    containers: Arc<Mutex<HashMap<String, String>>>,
    available: bool,
}

// 실행 흐름
// 1. create_container() - Docker 컨테이너 생성
// 2. start_container() - 컨테이너 시작
// 3. exec_in_container() - sh -c <command> 실행
// 4. remove_container() - 정리 및 삭제
```

**현재 구현의 한계**:
- 리소스 제한 미적용 (CPU, 메모리)
- 네트워크 격리 없음
- 파일시스템 마운트 보안 취약
- MicroVM 격리 미지원

### 7.3 보안 강화 설계

#### 7.3.1 ExecutionMode 확장

```rust
pub enum ExecutionMode {
    /// 호스트에서 직접 실행 (권한 시스템만 의존)
    Local,

    /// Docker 컨테이너 격리
    Container {
        image: String,
        workdir: Option<String>,
        env: Vec<(String, String)>,
        volumes: Vec<(String, String)>,
        // 새로 추가
        security: ContainerSecurity,
    },

    /// MicroVM 격리 (최고 보안)
    MicroVM {
        runtime: MicroVMRuntime,
        image: String,
        security: MicroVMSecurity,
    },
}

/// 컨테이너 보안 설정
pub struct ContainerSecurity {
    /// CPU 제한 (코어 수, 예: 0.5 = 50%)
    pub cpu_limit: Option<f64>,

    /// 메모리 제한 (바이트)
    pub memory_limit: Option<u64>,

    /// 네트워크 모드
    pub network_mode: NetworkMode,

    /// 읽기 전용 루트 파일시스템
    pub read_only_rootfs: bool,

    /// 권한 드롭 (capabilities)
    pub drop_capabilities: Vec<String>,

    /// seccomp 프로필
    pub seccomp_profile: Option<String>,
}

/// 네트워크 모드
pub enum NetworkMode {
    /// 네트워크 없음 (가장 안전)
    None,

    /// 호스트 네트워크 (위험)
    Host,

    /// 브리지 네트워크 (기본)
    Bridge,

    /// 허용된 호스트만 접근
    Allowlist(Vec<String>),
}

/// MicroVM 런타임
pub enum MicroVMRuntime {
    Firecracker,
    KataContainers,
    CloudHypervisor,
}

/// MicroVM 보안 설정
pub struct MicroVMSecurity {
    pub cpu_count: u32,
    pub memory_mb: u64,
    pub network_mode: NetworkMode,
    pub timeout: Duration,
}
```

#### 7.3.2 SecurityPolicy

```rust
/// 작업 유형별 보안 정책
pub struct SecurityPolicy {
    /// 정책 이름
    pub name: String,

    /// 기본 격리 수준
    pub isolation_level: IsolationLevel,

    /// 허용된 명령 패턴
    pub allowed_commands: Vec<String>,

    /// 거부된 명령 패턴
    pub denied_commands: Vec<String>,

    /// 파일시스템 접근 규칙
    pub filesystem_rules: FilesystemRules,

    /// 네트워크 규칙
    pub network_rules: NetworkRules,
}

/// 격리 수준
pub enum IsolationLevel {
    /// 격리 없음 (Local)
    None,

    /// 프로세스 격리 (Docker)
    Process,

    /// 사용자 공간 커널 (gVisor)
    UserKernel,

    /// 하드웨어 격리 (MicroVM)
    Hardware,
}

/// 사전 정의된 보안 정책
impl SecurityPolicy {
    /// 읽기 전용 탐색 (Explore 에이전트용)
    pub fn read_only() -> Self {
        Self {
            name: "read_only".into(),
            isolation_level: IsolationLevel::Process,
            allowed_commands: vec![
                "ls", "cat", "head", "tail", "grep", "find",
                "file", "stat", "wc", "tree",
            ].into_iter().map(String::from).collect(),
            denied_commands: vec![
                "rm", "mv", "cp", "chmod", "chown",
                "curl", "wget", "ssh", "scp",
            ].into_iter().map(String::from).collect(),
            filesystem_rules: FilesystemRules::ReadOnly,
            network_rules: NetworkRules::Deny,
        }
    }

    /// 빌드/테스트 (Bash 에이전트용)
    pub fn build_test() -> Self {
        Self {
            name: "build_test".into(),
            isolation_level: IsolationLevel::Process,
            allowed_commands: vec![
                "cargo", "npm", "yarn", "pnpm", "python",
                "go", "make", "cmake", "gradle", "mvn",
            ].into_iter().map(String::from).collect(),
            denied_commands: vec![
                "rm -rf /", "sudo", "su",
                "curl | bash", "wget | sh",
            ].into_iter().map(String::from).collect(),
            filesystem_rules: FilesystemRules::ProjectOnly,
            network_rules: NetworkRules::AllowPackageRegistries,
        }
    }

    /// 신뢰할 수 없는 코드 (MicroVM 필수)
    pub fn untrusted() -> Self {
        Self {
            name: "untrusted".into(),
            isolation_level: IsolationLevel::Hardware,
            allowed_commands: vec![],  // 모든 명령 허용 (격리로 보호)
            denied_commands: vec![],
            filesystem_rules: FilesystemRules::Ephemeral,
            network_rules: NetworkRules::Deny,
        }
    }
}
```

### 7.4 Executor 확장 계획

```rust
/// Executor 구현 계층
pub trait Executor: Send + Sync {
    async fn execute(&self, task: &Task) -> Result<TaskResult>;
    async fn cancel(&self, task: &Task) -> Result<()>;
    fn is_available(&self) -> bool;
    fn name(&self) -> &'static str;
    fn isolation_level(&self) -> IsolationLevel;  // 추가
}

/// 구현체
// 1. LocalExecutor      - IsolationLevel::None
// 2. ContainerExecutor  - IsolationLevel::Process
// 3. GVisorExecutor     - IsolationLevel::UserKernel (새로 추가)
// 4. MicroVMExecutor    - IsolationLevel::Hardware (새로 추가)
```

### 7.5 Docker Sandboxes 통합

Docker Sandboxes는 2025년에 MicroVM 기반 격리를 제공:

```rust
/// Docker Sandboxes 통합 (선택적)
pub struct DockerSandboxExecutor {
    /// Docker 클라이언트
    docker: Arc<Docker>,

    /// Sandbox 설정
    config: SandboxConfig,
}

pub struct SandboxConfig {
    /// 에이전트 타입 (claude, codex, etc.)
    pub agent_type: String,

    /// 프로젝트 디렉토리
    pub project_dir: PathBuf,

    /// 네트워크 접근 제어
    pub network_access: bool,
}

// 사용 예
// docker sandbox run claude ~/my-project
```

### 7.6 보안 베스트 프랙티스

#### 핵심 원칙

1. **AI 생성 코드는 신뢰할 수 없음**
   - 모든 코드 실행에 샌드박스 필수
   - 정적 필터링만으로는 불충분

2. **방어 계층화**
   - OS 프리미티브 + 하드웨어 가상화 + 네트워크 분리
   - 단일 방어선에 의존하지 않음

3. **최소 권한 원칙**
   - 필요한 권한만 부여
   - 기본적으로 모든 것을 거부

4. **위협 모델**
   - 프롬프트 인젝션 (OWASP Top 1)
   - 컨테이너 탈출
   - 데이터 유출
   - 리소스 고갈 (DoS)

#### 구현 체크리스트

- [ ] CPU/메모리 제한 적용
- [ ] 네트워크 격리 (기본: 차단)
- [ ] 읽기 전용 루트 파일시스템
- [ ] 권한 드롭 (capabilities)
- [ ] seccomp 프로필 적용
- [ ] 시간 제한 (타임아웃)
- [ ] 출력 크기 제한
- [ ] 민감 경로 접근 차단

---

## 8. 구현 로드맵 (컨테이너 보안)

### Phase 1: 기존 ContainerExecutor 강화
- [ ] 리소스 제한 (CPU, 메모리) 추가
- [ ] 네트워크 모드 옵션 추가
- [ ] seccomp 프로필 적용

### Phase 2: SecurityPolicy 시스템
- [ ] SecurityPolicy 타입 정의
- [ ] 사전 정의 정책 (read_only, build_test, untrusted)
- [ ] Task에 정책 연결

### Phase 3: MicroVM 지원 (선택적)
- [ ] Firecracker 연동 연구
- [ ] MicroVMExecutor 프로토타입
- [ ] Docker Sandboxes 통합 검토

---

## 9. 참고 자료 (컨테이너 보안)

### 연구 출처

- [Docker Sandboxes](https://docs.docker.com/ai/sandboxes) - Docker 공식 AI 샌드박스
- [gVisor vs Kata vs Firecracker](https://northflank.com/blog/kata-containers-vs-firecracker-vs-gvisor) - 격리 기술 비교
- [NVIDIA AI Code Execution Risks](https://developer.nvidia.com/blog/how-code-execution-drives-key-risks-in-agentic-ai-systems/) - 보안 위협 분석
- [E2B Firecracker](https://e2b.dev) - MicroVM 기반 샌드박스
- [Northflank AI Sandbox](https://northflank.com/blog/best-code-execution-sandbox-for-ai-agents) - 샌드박스 비교

### 핵심 인사이트

1. **MicroVM이 골드 스탠다드**: 신뢰할 수 없는 코드에는 Firecracker/Kata 권장
2. **컨테이너만으로는 부족**: 커널 공유로 인한 탈출 위험
3. **gVisor는 좋은 중간점**: VM 없이 강화된 격리
4. **네트워크 제어 필수**: 데이터 유출 방지
5. **Docker Sandboxes**: macOS/Windows에서 MicroVM 지원 (2025)
