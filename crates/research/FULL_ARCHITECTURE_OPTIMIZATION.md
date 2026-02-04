# ForgeCode 전체 아키텍처 최적화 계획

## 현재 상태 요약

| Layer | 완성도 | 핵심 기능 | 부족한 부분 |
|-------|--------|----------|------------|
| **Layer1-foundation** | 95% ✅ | Permission, Traits, Registries, Cache | Audit 통합 |
| **Layer2-core** | 85% ✅ | Tools, MCP, LSP, Plugins, Skills, Hooks | Repomap 통합 |
| **Layer2-provider** | 90% ✅ | 5개 Provider, Gateway, Retry | Vision 지원 |
| **Layer2-task** | 75% 🔄 | TaskManager, Log, SubAgent | Container 보안 |
| **Layer3-agent** | 70% 🔄 | Classic Variant, Registry | 전략 통합, 에러 복구 |
| **Layer4-cli** | 60% ⚠️ | CLI Mode, 기본 TUI | Permission UI, 설정 UI |

---

## Layer1: Foundation 최적화

### 1.1 현재 잘 되어 있는 부분

```
✅ Permission System
   ├── PermissionService (session/permanent 구분)
   ├── CommandAnalyzer (위험도 분석)
   ├── PathAnalyzer (민감 경로 검사)
   └── 5가지 Scope (Tool, Command, Path, Resource, Network)

✅ Core Traits
   ├── Tool trait (모든 도구 기반)
   ├── Provider trait (LLM 추상화)
   ├── Task trait (태스크 추상화)
   ├── ToolContext trait (실행 컨텍스트)
   ├── TaskObserver trait (진행 관찰) ← Layer4에서 구현 필요
   └── PermissionDelegate trait (권한 UI) ← Layer4에서 구현 필요

✅ Registries
   ├── MCP Registry (서버 설정)
   ├── Provider Registry (LLM 설정)
   ├── Model Registry (모델 메타데이터)
   └── Shell Registry (쉘 설정)

✅ Cache System
   ├── CacheManager (LRU, TTL)
   ├── Context Masker (민감 데이터)
   ├── Context Summarizer (요약)
   └── Context Compactor (압축)
```

### 1.2 최적화 필요 사항

#### A. Audit 시스템 통합
```rust
// 현재: AuditLogger 구조만 존재
// 필요: Permission 결정과 Tool 실행에 자동 연결

// crates/Layer1-foundation/src/audit/integration.rs (신규)
pub struct AuditIntegration {
    logger: Arc<AuditLogger>,
    event_bus: Arc<EventBus>,
}

impl AuditIntegration {
    /// Permission 결정 자동 로깅
    pub fn on_permission_decision(
        &self,
        tool: &str,
        action: &PermissionAction,
        decision: PermissionStatus,
    ) {
        self.logger.log(AuditEvent::Permission {
            tool: tool.to_string(),
            action: action.clone(),
            decision,
            timestamp: Utc::now(),
        });
    }
    
    /// Tool 실행 자동 로깅
    pub fn on_tool_execution(
        &self,
        tool: &str,
        input: &Value,
        result: &ToolResult,
        duration_ms: u64,
    ) {
        self.logger.log(AuditEvent::ToolExecution {
            tool: tool.to_string(),
            success: result.success,
            duration_ms,
            timestamp: Utc::now(),
        });
    }
}
```

#### B. Event Bus 통합 강화
```rust
// 현재: EventBus 존재하지만 전체 연결 부족
// 필요: 모든 주요 이벤트 자동 발행

pub enum ForgeEvent {
    // Permission 이벤트
    PermissionRequested { tool: String, action: PermissionAction },
    PermissionGranted { tool: String, scope: PermissionScope },
    PermissionDenied { tool: String, reason: String },
    
    // Tool 이벤트
    ToolStarted { tool: String, input: Value },
    ToolCompleted { tool: String, success: bool, duration_ms: u64 },
    
    // Task 이벤트
    TaskSubmitted { task_id: TaskId },
    TaskStateChanged { task_id: TaskId, old: TaskState, new: TaskState },
    
    // Agent 이벤트
    AgentTurnStarted { session_id: String, turn: usize },
    AgentToolCall { session_id: String, tool: String },
    AgentCompleted { session_id: String, turns: usize },
}
```

---

## Layer2-core: 도구 시스템 최적화

### 2.1 현재 잘 되어 있는 부분

```
✅ Tool Registry
   ├── 6개 Builtin Tools (bash, read, write, edit, glob, grep)
   ├── MCP Tool 통합 (McpBridge → ToolRegistry)
   └── Dynamic 등록/해제

✅ MCP Bridge
   ├── McpClient (JSON-RPC 2.0)
   ├── StdioTransport (프로세스 통신)
   ├── SseTransport (HTTP SSE)
   └── McpToolAdapter (Tool trait 변환)

✅ LSP Manager
   ├── Rust, TypeScript, Python, Go 지원
   ├── Lazy Loading (첫 사용 시 시작)
   └── 10분 Idle Timeout

✅ Plugin/Skill/Hook Systems
   └── 완전 구현됨
```

### 2.2 최적화 필요 사항

#### A. Edit Tool 안정화
```rust
// crates/Layer2-core/src/tool/builtin/edit.rs
// 현재: FIXME 주석 존재

impl EditTool {
    /// 개선: 더 정확한 문자열 매칭
    fn find_and_replace(
        &self,
        content: &str,
        old_string: &str,
        new_string: &str,
        replace_all: bool,
    ) -> Result<String> {
        // 1. 정확한 매칭 우선
        if content.contains(old_string) {
            return Ok(self.do_replace(content, old_string, new_string, replace_all));
        }
        
        // 2. 공백 정규화 후 매칭 시도
        let normalized_old = self.normalize_whitespace(old_string);
        let normalized_content = self.normalize_whitespace(content);
        
        if normalized_content.contains(&normalized_old) {
            // 원본에서 위치 찾아서 교체
            return self.replace_with_normalization(content, old_string, new_string);
        }
        
        // 3. 실패 시 상세 에러
        Err(Error::EditFailed(format!(
            "Could not find '{}' in file. Did you mean one of:\n{}",
            &old_string[..50.min(old_string.len())],
            self.suggest_similar(content, old_string)
        )))
    }
}
```

#### B. Tool 병렬 실행 최적화
```rust
// crates/Layer2-core/src/tool/parallel.rs (신규)

pub struct ParallelToolExecutor {
    max_concurrent: usize,
}

impl ParallelToolExecutor {
    /// 의존성 분석 기반 병렬 실행
    pub async fn execute(
        &self,
        ctx: &AgentContext,
        calls: &[ToolCall],
    ) -> Vec<ToolExecutionResult> {
        let graph = self.build_dependency_graph(calls);
        let levels = graph.topological_levels();
        
        let mut results = Vec::new();
        
        for level in levels {
            // 같은 레벨은 병렬 실행
            let level_futures: Vec<_> = level.iter()
                .map(|call| ctx.execute_tool(&call.name, call.arguments.clone()))
                .collect();
            
            let level_results = futures::future::join_all(level_futures).await;
            results.extend(level_results.into_iter().filter_map(|r| r.ok()));
        }
        
        results
    }
    
    fn build_dependency_graph(&self, calls: &[ToolCall]) -> DependencyGraph {
        let mut graph = DependencyGraph::new();
        let mut written_paths = HashSet::new();
        
        for (i, call) in calls.iter().enumerate() {
            let paths = self.extract_paths(call);
            
            // 이전에 쓴 경로를 읽으면 의존성
            for path in &paths {
                if written_paths.contains(path) {
                    // 이전 write → 현재 read 의존성
                    let writer_idx = self.find_writer(&calls[..i], path);
                    if let Some(w) = writer_idx {
                        graph.add_edge(w, i);
                    }
                }
            }
            
            // write/edit 도구는 경로 추적
            if call.name == "write" || call.name == "edit" {
                for path in paths {
                    written_paths.insert(path);
                }
            }
        }
        
        graph
    }
}
```

#### C. Repomap 통합
```rust
// crates/Layer2-core/src/repomap/integration.rs (신규)

pub struct RepoMapService {
    analyzer: RepoAnalyzer,
    graph: RwLock<Option<RepoGraph>>,
    ranker: Ranker,
}

impl RepoMapService {
    /// 프로젝트 분석 및 그래프 구축
    pub async fn analyze(&self, root: &Path) -> Result<()> {
        let analysis = self.analyzer.analyze(root).await?;
        let graph = RepoGraph::from_analysis(&analysis);
        *self.graph.write().await = Some(graph);
        Ok(())
    }
    
    /// 쿼리에 가장 관련 있는 파일들 반환
    pub async fn get_relevant_files(
        &self,
        query: &str,
        limit: usize,
    ) -> Vec<RankedFile> {
        let graph = self.graph.read().await;
        if let Some(g) = graph.as_ref() {
            self.ranker.rank(g, query, limit)
        } else {
            Vec::new()
        }
    }
    
    /// LLM 컨텍스트용 RepoMap 생성
    pub async fn generate_context_map(&self) -> String {
        // Aider 스타일 repomap 문자열 생성
    }
}
```

---

## Layer2-task: 태스크 시스템 최적화

### 2.3 현재 잘 되어 있는 부분

```
✅ Task Manager
   ├── submit/wait/cancel/force_kill
   ├── max_concurrent 제한
   └── Timeout 처리

✅ Log System (새로 구현됨)
   ├── TaskLogBuffer (실시간 버퍼)
   ├── LogAnalysisReport (LLM용 분석)
   ├── Subscribe 패턴
   └── format_for_llm()

✅ SubAgent System
   ├── 5가지 타입 (Explore, Plan, General, Bash, Custom)
   ├── ContextWindowConfig (토큰 관리)
   ├── ModelSelection (Haiku/Sonnet/Opus)
   └── PermissionMode (Auto/Ask/Deny)

✅ LocalExecutor
   ├── 프로세스 스폰
   ├── SIGTERM → SIGKILL 에스컬레이션
   └── 로그 스트리밍
```

### 2.4 최적화 필요 사항

#### A. Container Executor 보안 강화
```rust
// crates/Layer2-task/src/executor/container.rs

pub struct ContainerExecutorConfig {
    /// 메모리 제한 (기본: 512MB)
    pub memory_limit: Option<u64>,
    
    /// CPU 제한 (기본: 1.0 = 1 코어)
    pub cpu_limit: Option<f64>,
    
    /// 네트워크 모드 (기본: none = 격리)
    pub network_mode: NetworkMode,
    
    /// 읽기 전용 루트 파일시스템
    pub read_only_rootfs: bool,
    
    /// 허용된 마운트 경로만
    pub allowed_mounts: Vec<PathBuf>,
}

#[derive(Clone, Debug)]
pub enum NetworkMode {
    /// 네트워크 완전 격리 (가장 안전)
    None,
    /// 호스트와 공유 (개발용)
    Host,
    /// 브릿지 네트워크 (제한된 접근)
    Bridge { allowed_hosts: Vec<String> },
}

impl ContainerExecutor {
    pub async fn execute(&self, task: &Task) -> Result<TaskResult> {
        let config = &self.config;
        
        // 보안 검증
        self.validate_mounts(&task.volumes)?;
        
        let container_config = bollard::container::Config {
            image: Some(task.image.clone()),
            cmd: Some(vec!["/bin/sh", "-c", &task.command]),
            
            // 리소스 제한
            host_config: Some(bollard::models::HostConfig {
                memory: config.memory_limit,
                nano_cpus: config.cpu_limit.map(|c| (c * 1e9) as i64),
                network_mode: Some(config.network_mode.to_docker_string()),
                read_only_rootfs: Some(config.read_only_rootfs),
                // 권한 제거
                cap_drop: Some(vec!["ALL".to_string()]),
                // 최소 권한만 추가
                cap_add: Some(vec!["CHOWN".to_string(), "SETUID".to_string()]),
                ..Default::default()
            }),
            
            ..Default::default()
        };
        
        // 컨테이너 생성 및 실행
        let id = self.docker.create_container(None, container_config).await?.id;
        
        // ... 실행 및 로그 수집
    }
}
```

#### B. 다중 Task 서버 지원
```rust
// crates/Layer2-task/src/server/mod.rs (신규)

/// 여러 Task 서버를 관리하고 API 검증
pub struct TaskServerCluster {
    servers: Vec<TaskServer>,
    load_balancer: LoadBalancer,
}

pub struct TaskServer {
    /// 서버 ID
    id: String,
    
    /// 서버 주소
    address: SocketAddr,
    
    /// 상태
    status: ServerStatus,
    
    /// 로컬 TaskManager
    manager: TaskManager,
}

impl TaskServerCluster {
    /// 새 서버 시작
    pub async fn spawn_server(&mut self) -> Result<String> {
        let server = TaskServer::new().await?;
        let id = server.id.clone();
        
        // 컨테이너로 실행
        self.spawn_in_container(&server).await?;
        
        // 헬스체크 대기
        self.wait_for_ready(&server).await?;
        
        self.servers.push(server);
        Ok(id)
    }
    
    /// 서버 간 API 통신 테스트
    pub async fn verify_inter_server_communication(&self) -> Result<HealthReport> {
        let mut report = HealthReport::new();
        
        for (i, server_a) in self.servers.iter().enumerate() {
            for server_b in self.servers.iter().skip(i + 1) {
                let latency = self.ping(server_a, server_b).await?;
                report.add_connection(server_a.id.clone(), server_b.id.clone(), latency);
            }
        }
        
        Ok(report)
    }
    
    /// 태스크 제출 (로드 밸런싱)
    pub async fn submit(&self, task: Task) -> Result<TaskId> {
        let server = self.load_balancer.select(&self.servers);
        server.manager.submit(task).await
    }
    
    /// 모든 서버의 로그 수집
    pub async fn collect_all_logs(&self) -> Vec<(String, Vec<LogEntry>)> {
        let mut all_logs = Vec::new();
        
        for server in &self.servers {
            let tasks = server.manager.get_all_tasks().await;
            for task_id in tasks {
                if let Ok(logs) = server.manager.get_logs(&task_id, 1000).await {
                    all_logs.push((format!("{}:{}", server.id, task_id), logs));
                }
            }
        }
        
        all_logs
    }
}
```

#### C. 프로젝트 로그 뷰어
```rust
// crates/Layer2-task/src/log/viewer.rs (신규)

pub struct ProjectLogViewer {
    log_manager: Arc<TaskLogManager>,
}

impl ProjectLogViewer {
    /// 프로젝트의 모든 실행 로그 조회
    pub async fn get_project_logs(
        &self,
        project_path: &Path,
        filter: LogFilter,
    ) -> Vec<TaskLogEntry> {
        // 프로젝트 경로 기준 필터링
    }
    
    /// 실시간 로그 스트림
    pub fn stream_logs(&self) -> impl Stream<Item = LogEntry> {
        // 모든 활성 태스크 로그 스트림
    }
    
    /// 에러 패턴 분석
    pub async fn analyze_errors(&self, task_id: &TaskId) -> ErrorAnalysis {
        let report = self.log_manager.get_analysis(task_id).await?;
        
        ErrorAnalysis {
            error_count: report.error_count,
            patterns: report.detect_patterns(),
            suggested_fixes: report.suggest_fixes(),
        }
    }
}
```

---

## Layer3-agent: 에이전트 최적화

### 3.1 현재 잘 되어 있는 부분

```
✅ Agent Variants
   ├── ClassicAgent (ReACT 루프)
   └── AgentRegistry (변형 관리)

✅ Runtime System
   ├── AgentRuntime trait
   ├── RuntimeHooks
   └── RuntimeConfig

✅ Benchmark System
   ├── Scenario, Metrics
   ├── Runner, Report
   └── 성능 측정 가능
```

### 3.2 최적화 필요 사항

#### A. Layer2 도구 통합 강화
```rust
// crates/Layer3-agent/src/tool_integration.rs (신규)

pub struct ToolIntegration {
    /// Layer2-core AgentContext
    ctx: Arc<AgentContext>,
    
    /// 병렬 실행기
    parallel_executor: ParallelToolExecutor,
    
    /// 결과 캐시
    result_cache: Arc<RwLock<HashMap<u64, ToolResult>>>,
}

impl ToolIntegration {
    /// 도구 실행 (캐시 + 병렬 최적화)
    pub async fn execute_tools(
        &self,
        calls: &[ToolCall],
    ) -> Vec<ToolExecutionResult> {
        // 1. 캐시에서 결과 확인
        let (cached, uncached) = self.partition_by_cache(calls).await;
        
        // 2. 캐시 안 된 것만 실행
        let results = if uncached.len() > 1 {
            // 여러 개면 병렬 실행
            self.parallel_executor.execute(&self.ctx, &uncached).await
        } else if uncached.len() == 1 {
            // 하나면 직접 실행
            vec![self.ctx.execute_tool(&uncached[0].name, uncached[0].arguments.clone()).await?]
        } else {
            Vec::new()
        };
        
        // 3. 결과 캐시 업데이트
        self.update_cache(&uncached, &results).await;
        
        // 4. 캐시 + 새 결과 병합
        self.merge_results(cached, results)
    }
}
```

#### B. 에러 복구 메커니즘
```rust
// crates/Layer3-agent/src/recovery.rs (신규)

pub struct ErrorRecovery {
    max_retries: usize,
    strategies: Vec<Box<dyn RecoveryStrategy>>,
}

#[async_trait]
pub trait RecoveryStrategy: Send + Sync {
    /// 이 전략이 에러를 처리할 수 있는지
    fn can_handle(&self, error: &ToolError) -> bool;
    
    /// 복구 시도
    async fn recover(
        &self,
        ctx: &AgentContext,
        call: &ToolCall,
        error: &ToolError,
    ) -> Result<RecoveryAction>;
}

pub enum RecoveryAction {
    /// 재시도
    Retry { modified_input: Option<Value> },
    /// 대체 도구 사용
    UseFallback { tool: String, input: Value },
    /// 사용자에게 질문
    AskUser { question: String },
    /// 포기
    GiveUp { reason: String },
}

impl ErrorRecovery {
    pub async fn handle_error(
        &self,
        ctx: &AgentContext,
        call: &ToolCall,
        error: &ToolError,
    ) -> Result<RecoveryAction> {
        for strategy in &self.strategies {
            if strategy.can_handle(error) {
                return strategy.recover(ctx, call, error).await;
            }
        }
        
        Ok(RecoveryAction::GiveUp {
            reason: format!("No recovery strategy for: {}", error)
        })
    }
}

// 구체적인 전략들
pub struct FileNotFoundRecovery;

#[async_trait]
impl RecoveryStrategy for FileNotFoundRecovery {
    fn can_handle(&self, error: &ToolError) -> bool {
        matches!(error, ToolError::FileNotFound(_))
    }
    
    async fn recover(
        &self,
        ctx: &AgentContext,
        call: &ToolCall,
        error: &ToolError,
    ) -> Result<RecoveryAction> {
        if let ToolError::FileNotFound(path) = error {
            // glob으로 유사한 파일 찾기
            let similar = ctx.execute_tool("glob", json!({
                "pattern": format!("**/*{}*", Path::new(path).file_name().unwrap().to_str().unwrap())
            })).await?;
            
            if !similar.output.is_empty() {
                return Ok(RecoveryAction::Retry {
                    modified_input: Some(json!({
                        "path": similar.output.lines().next().unwrap()
                    }))
                });
            }
        }
        
        Ok(RecoveryAction::GiveUp { reason: "Similar file not found".to_string() })
    }
}
```

#### C. 컨텍스트 최적화
```rust
// crates/Layer3-agent/src/context_optimizer.rs (신규)

pub struct ContextOptimizer {
    /// Layer1 Cache 시스템 활용
    cache: Arc<CacheManager>,
    
    /// 컨텍스트 압축기
    compactor: ContextCompactor,
    
    /// 토큰 계산기
    tokenizer: Tokenizer,
}

impl ContextOptimizer {
    /// 대화 히스토리 최적화
    pub async fn optimize_history(
        &self,
        messages: &mut Vec<Message>,
        max_tokens: usize,
    ) -> TokenReport {
        let current_tokens = self.count_tokens(messages);
        
        if current_tokens <= max_tokens {
            return TokenReport::within_limit(current_tokens);
        }
        
        // 압축 필요
        let target_tokens = (max_tokens as f32 * 0.8) as usize;
        
        // 1. 도구 결과 압축 (가장 토큰 많이 사용)
        self.compress_tool_results(messages, target_tokens);
        
        // 2. 여전히 초과면 오래된 메시지 요약
        let current = self.count_tokens(messages);
        if current > target_tokens {
            self.summarize_old_messages(messages, target_tokens).await;
        }
        
        TokenReport::compressed(current_tokens, self.count_tokens(messages))
    }
    
    fn compress_tool_results(&self, messages: &mut Vec<Message>, target: usize) {
        for msg in messages.iter_mut() {
            if let Message::ToolResult { content, .. } = msg {
                // 긴 출력 압축
                if content.len() > 1000 {
                    *content = self.truncate_with_summary(content, 500);
                }
            }
        }
    }
}
```

---

## Layer4-cli: UI 최적화

### 4.1 현재 잘 되어 있는 부분

```
✅ CLI Mode
   ├── run_once() 단일 실행
   └── 이벤트 스트리밍

✅ TUI 기본
   ├── Ratatui + Crossterm
   ├── EventHandler (키보드, 타이머)
   ├── ChatPage (기본 렌더링)
   └── InputBox, MessageList 컴포넌트
```

### 4.2 최적화 필요 사항

#### A. Permission Delegate 구현
```rust
// crates/Layer4-cli/src/tui/components/permission.rs (신규)

pub struct PermissionModal {
    tool_name: String,
    action: PermissionAction,
    description: String,
    risk_score: u8,
    options: Vec<PermissionOption>,
    selected: usize,
    visible: bool,
}

#[derive(Clone)]
struct PermissionOption {
    label: String,
    response: PermissionResponse,
    key: char,
}

impl PermissionModal {
    pub fn new(
        tool_name: &str,
        action: &PermissionAction,
        description: &str,
        risk_score: u8,
    ) -> Self {
        Self {
            tool_name: tool_name.to_string(),
            action: action.clone(),
            description: description.to_string(),
            risk_score,
            options: vec![
                PermissionOption {
                    label: "Allow Once".to_string(),
                    response: PermissionResponse::AllowOnce,
                    key: 'o',
                },
                PermissionOption {
                    label: "Allow Session".to_string(),
                    response: PermissionResponse::AllowSession,
                    key: 's',
                },
                PermissionOption {
                    label: "Allow Permanent".to_string(),
                    response: PermissionResponse::AllowPermanent,
                    key: 'p',
                },
                PermissionOption {
                    label: "Deny".to_string(),
                    response: PermissionResponse::Deny,
                    key: 'd',
                },
            ],
            selected: 0,
            visible: false,
        }
    }
    
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        if !self.visible {
            return;
        }
        
        // 반투명 오버레이
        let overlay = Block::default()
            .style(Style::default().bg(Color::Black));
        frame.render_widget(overlay, area);
        
        // 모달 박스
        let modal_area = self.centered_rect(60, 40, area);
        let modal = Block::default()
            .title(format!(" Permission Required: {} ", self.tool_name))
            .borders(Borders::ALL)
            .border_style(self.risk_style());
        
        frame.render_widget(modal, modal_area);
        
        // 내용 렌더링
        let inner = modal_area.inner(&Margin::new(2, 1));
        self.render_content(frame, inner);
    }
    
    fn risk_style(&self) -> Style {
        match self.risk_score {
            0..=3 => Style::default().fg(Color::Green),
            4..=6 => Style::default().fg(Color::Yellow),
            7..=10 => Style::default().fg(Color::Red),
            _ => Style::default(),
        }
    }
}

// TUI Permission Delegate
pub struct TuiPermissionDelegate {
    modal_tx: mpsc::Sender<PermissionModal>,
    response_rx: mpsc::Receiver<PermissionResponse>,
}

#[async_trait]
impl PermissionDelegate for TuiPermissionDelegate {
    async fn request_permission(
        &self,
        tool_name: &str,
        action: &PermissionAction,
        description: &str,
        risk_score: u8,
    ) -> PermissionResponse {
        let modal = PermissionModal::new(tool_name, action, description, risk_score);
        self.modal_tx.send(modal).await.ok();
        
        // 사용자 응답 대기
        self.response_rx.recv().await
            .unwrap_or(PermissionResponse::Deny)
    }
    
    fn notify(&self, message: &str) {
        // 상태 바에 알림 표시
    }
    
    fn show_error(&self, error: &str) {
        // 에러 팝업 표시
    }
}
```

#### B. Model Switcher UI
```rust
// crates/Layer4-cli/src/tui/components/model_switcher.rs (신규)

pub struct ModelSwitcher {
    providers: Vec<ProviderInfo>,
    models: Vec<ModelInfo>,
    selected_provider: usize,
    selected_model: usize,
    visible: bool,
}

impl ModelSwitcher {
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        if !self.visible {
            return;
        }
        
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
            .split(area);
        
        // 프로바이더 목록
        self.render_providers(frame, chunks[0]);
        
        // 모델 목록
        self.render_models(frame, chunks[1]);
    }
    
    pub fn handle_key(&mut self, key: KeyCode) -> Option<ModelSelection> {
        match key {
            KeyCode::Up => self.select_prev(),
            KeyCode::Down => self.select_next(),
            KeyCode::Enter => return Some(self.get_selection()),
            KeyCode::Esc => self.visible = false,
            _ => {}
        }
        None
    }
}
```

#### C. Task Observer 구현
```rust
// crates/Layer4-cli/src/tui/components/task_progress.rs (신규)

pub struct TaskProgressWidget {
    tasks: HashMap<TaskId, TaskProgress>,
}

struct TaskProgress {
    state: TaskState,
    progress: f32,
    message: String,
    start_time: Instant,
}

impl TaskObserver for TuiTaskObserver {
    fn on_state_change(&self, task_id: &str, state: TaskState) {
        let mut widget = self.widget.write().unwrap();
        if let Some(task) = widget.tasks.get_mut(&TaskId::from(task_id)) {
            task.state = state;
        }
    }
    
    fn on_progress(&self, task_id: &str, progress: f32, message: &str) {
        let mut widget = self.widget.write().unwrap();
        if let Some(task) = widget.tasks.get_mut(&TaskId::from(task_id)) {
            task.progress = progress;
            task.message = message.to_string();
        }
    }
    
    fn on_complete(&self, task_id: &str, result: &TaskResult) {
        let mut widget = self.widget.write().unwrap();
        widget.tasks.remove(&TaskId::from(task_id));
    }
}

impl TaskProgressWidget {
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        if self.tasks.is_empty() {
            return;
        }
        
        let block = Block::default()
            .title(" Running Tasks ")
            .borders(Borders::ALL);
        
        let inner = block.inner(area);
        frame.render_widget(block, area);
        
        // 각 태스크 진행률 바
        let task_height = 2;
        for (i, (id, task)) in self.tasks.iter().enumerate() {
            let task_area = Rect::new(
                inner.x,
                inner.y + (i as u16 * task_height),
                inner.width,
                task_height,
            );
            
            self.render_task(frame, task_area, id, task);
        }
    }
    
    fn render_task(&self, frame: &mut Frame, area: Rect, id: &TaskId, task: &TaskProgress) {
        // 진행률 바
        let gauge = Gauge::default()
            .label(format!("{}: {}", id.short(), task.message))
            .ratio(task.progress as f64)
            .gauge_style(Style::default().fg(Color::Cyan));
        
        frame.render_widget(gauge, area);
    }
}
```

#### D. Settings Page
```rust
// crates/Layer4-cli/src/tui/pages/settings.rs (신규)

pub struct SettingsPage {
    sections: Vec<SettingsSection>,
    selected_section: usize,
    selected_item: usize,
}

enum SettingsSection {
    Provider {
        items: Vec<ProviderSetting>,
    },
    Permissions {
        items: Vec<PermissionSetting>,
    },
    Appearance {
        items: Vec<AppearanceSetting>,
    },
}

impl SettingsPage {
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(25), Constraint::Percentage(75)])
            .split(area);
        
        // 섹션 목록
        self.render_sections(frame, chunks[0]);
        
        // 선택된 섹션의 설정들
        self.render_settings(frame, chunks[1]);
    }
}
```

---

## 구현 우선순위

### Phase 1: 핵심 기능 완성 (1-2주)

| 우선순위 | 작업 | Layer | 파일 |
|---------|------|-------|------|
| 🔴 1 | Permission Modal | Layer4 | `components/permission.rs` |
| 🔴 2 | Task Progress Widget | Layer4 | `components/task_progress.rs` |
| 🔴 3 | Error Recovery | Layer3 | `recovery.rs` |
| 🔴 4 | Edit Tool 안정화 | Layer2 | `tool/builtin/edit.rs` |

### Phase 2: 최적화 (2-3주)

| 우선순위 | 작업 | Layer | 파일 |
|---------|------|-------|------|
| 🟡 5 | Parallel Tool Execution | Layer2 | `tool/parallel.rs` |
| 🟡 6 | Context Optimizer | Layer3 | `context_optimizer.rs` |
| 🟡 7 | Model Switcher UI | Layer4 | `components/model_switcher.rs` |
| 🟡 8 | Settings Page | Layer4 | `pages/settings.rs` |

### Phase 3: 고급 기능 (3-4주)

| 우선순위 | 작업 | Layer | 파일 |
|---------|------|-------|------|
| 🟢 9 | Container Security | Layer2 | `executor/container.rs` |
| 🟢 10 | Task Server Cluster | Layer2 | `server/mod.rs` |
| 🟢 11 | Repomap 통합 | Layer2 | `repomap/integration.rs` |
| 🟢 12 | Audit 통합 | Layer1 | `audit/integration.rs` |

---

## 테스트 전략

### 통합 테스트

```rust
// tests/integration/layer_integration.rs

#[tokio::test]
async fn test_permission_flow() {
    // Layer1 → Layer2 → Layer3 → Layer4 권한 흐름 테스트
    let permission_service = PermissionService::new();
    let tool_registry = ToolRegistry::with_builtins();
    let agent_context = AgentContext::builder()
        .with_permission_service(permission_service)
        .build();
    
    // 위험한 명령 실행 시 권한 요청 확인
    let result = agent_context.execute_tool("bash", json!({
        "command": "rm -rf /tmp/test"
    })).await;
    
    assert_eq!(result.permission_required, true);
}

#[tokio::test]
async fn test_task_log_flow() {
    // Layer2-task 로그 시스템 테스트
    let manager = TaskManager::new(TaskManagerConfig::default());
    
    let task = Task::new("test", "echo 'hello'");
    let task_id = manager.submit(task).await?;
    
    // 로그 스트림 구독
    let mut stream = manager.subscribe_logs(&task_id).await?;
    
    // 완료 대기
    manager.wait(&task_id).await?;
    
    // 로그 확인
    let logs = manager.get_logs(&task_id, 100).await?;
    assert!(logs.iter().any(|l| l.content.contains("hello")));
}

#[tokio::test]
async fn test_model_switching() {
    // Layer2-provider 모델 전환 테스트
    let gateway = Gateway::new().await?;
    
    // Anthropic으로 시작
    let response1 = gateway.complete_with_provider("anthropic", request.clone()).await?;
    
    // OpenAI로 전환
    let response2 = gateway.complete_with_provider("openai", request).await?;
    
    // 둘 다 성공해야 함
    assert!(response1.content.len() > 0);
    assert!(response2.content.len() > 0);
}
```

---

## 요약

### 각 Layer 역할 명확화

| Layer | 역할 | 핵심 구현 |
|-------|------|----------|
| **Layer1** | 기반, 권한, 설정 | Permission, Traits, Registries, Cache |
| **Layer2-core** | 도구, MCP, 플러그인 | ToolRegistry, McpBridge, Skills, Hooks |
| **Layer2-provider** | LLM 추상화 | Provider trait, Gateway, 5개 구현체 |
| **Layer2-task** | 실행, 로그, 컨테이너 | TaskManager, Executors, SubAgent, Log |
| **Layer3** | 에이전트 루프 | AgentRuntime, Variants, Recovery |
| **Layer4** | UI, 사용자 접근 | TUI, Permission Modal, Model Switcher |

### 최종 완성도 목표

- Layer1: 95% → 98%
- Layer2-core: 85% → 95%
- Layer2-provider: 90% → 95%
- Layer2-task: 75% → 90%
- Layer3: 70% → 90%
- Layer4: 60% → 85%

**전체: 75% → 92%**
