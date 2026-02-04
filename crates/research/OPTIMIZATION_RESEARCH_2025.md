# ForgeCode Optimization Research 2025

## Overview

Layer3/Layer4 최적화 및 부족한 부분에 대한 연구 결과입니다.

---

## 1. Layer4 현황 분석

### 1.1 현재 구현 상태

```
Layer4-cli/
├── src/
│   ├── main.rs
│   ├── cli.rs              ✅ 기본 구현
│   └── tui/
│       ├── app.rs          ✅ Ratatui 기반 TUI
│       ├── event.rs        ✅ 이벤트 핸들링
│       ├── theme.rs        ✅ 테마
│       ├── components/
│       │   ├── input.rs    ✅ InputBox
│       │   └── message_list.rs  ✅ ChatMessage
│       └── pages/
│           └── chat.rs     ⚠️ 부분 구현 (auto_approve 사용 중)
```

### 1.2 미구현 핵심 기능

| 기능 | 상태 | Layer1 Trait |
|------|------|--------------|
| Permission Modal | ❌ 미구현 | `PermissionDelegate` |
| Task Progress Display | ❌ 미구현 | `TaskObserver` |
| Error Display Component | ❌ 미구현 | - |
| Tool Execution Feedback | ⚠️ 부분 | - |

### 1.3 필요한 구현

#### PermissionDelegate 구현
```rust
// Layer4-cli/src/tui/components/permission.rs (신규)

pub struct PermissionModal {
    tool_name: String,
    action: PermissionAction,
    description: String,
    risk_score: u8,
    selected_option: usize,
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
        // TUI 모달 표시
        // 사용자 선택 대기
        // AllowOnce/AllowSession/AllowPermanent/Deny/DenyPermanent 반환
    }
}
```

#### TaskObserver 구현
```rust
// Layer4-cli/src/tui/components/progress.rs (신규)

pub struct TaskProgressWidget {
    tasks: HashMap<String, TaskInfo>,
}

impl TaskObserver for TuiTaskObserver {
    fn on_state_change(&self, task_id: &str, state: TaskState) {
        // 상태 변경 UI 업데이트
    }
    
    fn on_progress(&self, task_id: &str, progress: f32, message: &str) {
        // 진행률 바 업데이트
    }
    
    fn on_complete(&self, task_id: &str, result: &TaskResult) {
        // 완료 표시
    }
}
```

---

## 2. Layer3 최적화 연구

### 2.1 Context Engineering (컨텍스트 엔지니어링)

**문제점**: LLM은 컨텍스트가 커질수록 성능이 저하됨 ("Context Rot")

**최적화 전략**:

#### 2.1.1 Prompt Caching
```rust
/// 프롬프트 캐싱 시스템
pub struct PromptCache {
    /// 정적 시스템 프롬프트 캐시 (hash -> cached_tokens)
    static_cache: HashMap<u64, CachedPrompt>,
    
    /// 캐시 히트율 통계
    stats: CacheStats,
}

impl PromptCache {
    /// 캐시된 토큰은 75% 저렴
    /// Docker 레이어처럼 변경된 부분만 재처리
    pub fn get_or_compute(&mut self, prompt: &str) -> CachedPrompt {
        let hash = self.compute_hash(prompt);
        
        if let Some(cached) = self.static_cache.get(&hash) {
            self.stats.hits += 1;
            return cached.clone();
        }
        
        self.stats.misses += 1;
        let cached = CachedPrompt::new(prompt);
        self.static_cache.insert(hash, cached.clone());
        cached
    }
}
```

#### 2.1.2 Context Compaction (ADK 스타일)
```rust
/// 컨텍스트 압축 시스템
pub struct ContextCompactor {
    /// 압축 임계값 (예: 80% 사용 시 압축)
    threshold: f32,
    
    /// 슬라이딩 윈도우 크기
    window_size: usize,
}

impl ContextCompactor {
    /// ADK 스타일: 오래된 이벤트를 LLM으로 요약
    pub async fn compact(&self, session: &mut SessionContext) -> Result<()> {
        if session.token_usage_ratio() < self.threshold {
            return Ok(());
        }
        
        // 최근 N개 메시지 유지
        let keep_recent = self.window_size;
        let to_summarize = session.messages.len().saturating_sub(keep_recent);
        
        if to_summarize < 5 {
            return Ok(());
        }
        
        // 오래된 메시지 요약
        let old_messages: Vec<_> = session.messages.drain(..to_summarize).collect();
        let summary = self.summarize(&old_messages).await?;
        
        // 요약으로 대체
        session.messages.insert(0, Message::Summary(summary));
        
        Ok(())
    }
}
```

#### 2.1.3 Token-Efficient Serialization
```rust
/// 토큰 효율적 직렬화
/// 문제: JSON 포맷팅이 40-70% 토큰을 낭비
pub struct TokenEfficientSerializer;

impl TokenEfficientSerializer {
    /// 파일 내용 압축
    pub fn serialize_file_content(content: &str, max_lines: usize) -> String {
        let lines: Vec<_> = content.lines().collect();
        
        if lines.len() <= max_lines {
            return content.to_string();
        }
        
        // 앞/뒤 일부만 포함 + 생략 표시
        let half = max_lines / 2;
        let mut result = lines[..half].join("\n");
        result.push_str(&format!("\n... ({} lines omitted) ...\n", lines.len() - max_lines));
        result.push_str(&lines[lines.len() - half..].join("\n"));
        result
    }
    
    /// 도구 결과 압축
    pub fn compress_tool_result(result: &ToolResult) -> String {
        // 불필요한 공백, 중복 정보 제거
        // 핵심 정보만 추출
    }
}
```

### 2.2 Parallel Tool Execution

**연구 결과**: 병렬 실행으로 12-22% 레이턴시 감소

```rust
/// 병렬 도구 실행 최적화
pub struct ParallelToolExecutor {
    /// 최대 동시 실행 수
    max_concurrent: usize,
}

impl ParallelToolExecutor {
    /// 독립적인 도구 호출은 병렬 실행
    pub async fn execute_parallel(
        &self,
        ctx: &AgentContext,
        tool_calls: &[ToolCall],
    ) -> Vec<ToolExecutionResult> {
        // 의존성 분석
        let (independent, dependent) = self.analyze_dependencies(tool_calls);
        
        let mut results = Vec::new();
        
        // 독립적 호출은 병렬
        if !independent.is_empty() {
            let futures: Vec<_> = independent.iter()
                .map(|tc| ctx.execute_tool(&tc.name, tc.arguments.clone()))
                .collect();
            
            let parallel_results = futures::future::join_all(futures).await;
            results.extend(parallel_results.into_iter().filter_map(|r| r.ok()));
        }
        
        // 의존적 호출은 순차
        for tc in dependent {
            if let Ok(result) = ctx.execute_tool(&tc.name, tc.arguments.clone()).await {
                results.push(result);
            }
        }
        
        results
    }
    
    fn analyze_dependencies(&self, calls: &[ToolCall]) -> (Vec<&ToolCall>, Vec<&ToolCall>) {
        // 파일 경로 기반 의존성 분석
        // 예: write → read 같은 파일이면 순차 실행
        let mut independent = Vec::new();
        let mut dependent = Vec::new();
        
        let mut written_paths: HashSet<String> = HashSet::new();
        
        for call in calls {
            let paths = self.extract_paths(call);
            
            // 이전에 쓴 파일을 읽으면 의존적
            if paths.iter().any(|p| written_paths.contains(p)) {
                dependent.push(call);
            } else {
                independent.push(call);
            }
            
            // write 도구면 경로 추적
            if call.name == "write" || call.name == "edit" {
                for path in paths {
                    written_paths.insert(path);
                }
            }
        }
        
        (independent, dependent)
    }
}
```

### 2.3 Model Selection Optimization

**OpenCode 접근법**: 작업 유형에 따라 모델 전환

```rust
/// 모델 선택 최적화
pub struct ModelSelector {
    /// 추론용 모델 (Claude, o1)
    reasoning_model: ModelSpec,
    
    /// 실행용 모델 (GPT-4o, Codestral)
    execution_model: ModelSpec,
    
    /// 빠른 작업용 모델 (GPT-4o-mini, Haiku)
    fast_model: ModelSpec,
}

impl ModelSelector {
    pub fn select_for_task(&self, task_type: TaskType) -> &ModelSpec {
        match task_type {
            // 복잡한 아키텍처 결정
            TaskType::Planning | TaskType::Architecture => &self.reasoning_model,
            
            // 코드 작성/리팩토링
            TaskType::Coding | TaskType::Refactoring => &self.execution_model,
            
            // 간단한 검색/요약
            TaskType::Search | TaskType::Summary => &self.fast_model,
        }
    }
}

pub enum TaskType {
    Planning,
    Architecture,
    Coding,
    Refactoring,
    Search,
    Summary,
    Debug,
}
```

### 2.4 Streaming & Incremental Context

**DuoAttention 접근법**: Streaming Heads + Retrieval Heads

```rust
/// 스트리밍 컨텍스트 관리
pub struct StreamingContext {
    /// 고정 KV 캐시 (시스템 프롬프트, 중요 컨텍스트)
    fixed_context: Vec<Message>,
    
    /// 스트리밍 윈도우 (최근 메시지)
    streaming_window: VecDeque<Message>,
    
    /// 윈도우 크기
    window_size: usize,
}

impl StreamingContext {
    pub fn add_message(&mut self, message: Message) {
        self.streaming_window.push_back(message);
        
        // 윈도우 크기 초과 시 오래된 것 제거
        while self.streaming_window.len() > self.window_size {
            let old = self.streaming_window.pop_front();
            // 중요한 메시지는 요약하여 fixed_context로 이동
            if self.is_important(&old) {
                self.fixed_context.push(self.summarize(&old));
            }
        }
    }
    
    pub fn get_context(&self) -> Vec<&Message> {
        self.fixed_context.iter()
            .chain(self.streaming_window.iter())
            .collect()
    }
}
```

---

## 3. 아키텍처 갭 분석

### 3.1 현재 부족한 부분

| 영역 | 현재 상태 | 필요한 구현 | 우선순위 |
|------|----------|------------|---------|
| **Permission UI** | auto_approve 사용 | TUI 모달 | 🔴 HIGH |
| **Prompt Caching** | 없음 | 해시 기반 캐싱 | 🔴 HIGH |
| **Context Compaction** | Layer2-task에 기본 구조 | 실제 구현 | 🔴 HIGH |
| **Parallel Execution** | 기본 지원 | 의존성 분석 추가 | 🟡 MEDIUM |
| **Model Selection** | 단일 모델 | 작업별 모델 전환 | 🟡 MEDIUM |
| **Task Progress** | 없음 | TUI 위젯 | 🟡 MEDIUM |
| **Token Serialization** | 기본 JSON | 압축 직렬화 | 🟢 LOW |

### 3.2 Layer2-task SubAgent 갭

현재 `crates/Layer2-task/src/subagent/` 구조:
```
subagent/
├── types.rs      ✅ 기본 타입
├── config.rs     ✅ 설정
├── context.rs    ⚠️ 부분 (ContextWindow, PreRot)
├── handoff.rs    ⚠️ 부분 (Amp 스타일 핸드오프)
└── manager.rs    ⚠️ 부분 (매니저)
```

**필요한 추가 구현**:
1. `ContextCompactor` 실제 LLM 요약 로직
2. `PreRotation` (사전 압축) 실행 로직
3. `HandoffManager` 완전한 구현

### 3.3 Layer3 에이전트 갭

Layer3-agent는 아직 생성되지 않음. 필요한 것:
1. `prompts/` 디렉토리와 시스템 프롬프트 파일들
2. `PromptComposer` 구현
3. `AgentExecutor` 메인 루프
4. `AgentRegistry` 에이전트 설정 관리

---

## 4. 최신 기술 적용 제안

### 4.1 Prompt Caching (Claude 스타일)

```rust
/// Claude API 프롬프트 캐싱 활용
pub struct ClaudePromptCache {
    /// 캐시 가능한 프롬프트 블록 마킹
    cache_control: CacheControl,
}

impl ClaudePromptCache {
    /// 시스템 프롬프트에 cache_control 마킹
    pub fn mark_cacheable(&self, messages: &mut Vec<Message>) {
        // 첫 번째 시스템 메시지는 항상 캐시
        if let Some(first) = messages.first_mut() {
            first.cache_control = Some(CacheControl::Ephemeral);
        }
        
        // 도구 정의도 캐시
        for msg in messages.iter_mut() {
            if msg.is_tool_definition() {
                msg.cache_control = Some(CacheControl::Ephemeral);
            }
        }
    }
}
```

### 4.2 Git Worktree 기반 병렬 에이전트

```rust
/// Git Worktree 기반 병렬 작업
pub struct WorktreeParallelizer {
    /// 기본 저장소 경로
    base_repo: PathBuf,
}

impl WorktreeParallelizer {
    /// 병렬 작업을 위한 worktree 생성
    pub async fn create_worktree(&self, task_id: &str) -> Result<PathBuf> {
        let worktree_path = self.base_repo
            .parent()
            .unwrap()
            .join(format!(".worktrees/{}", task_id));
        
        let branch_name = format!("agent/{}", task_id);
        
        // git worktree add
        Command::new("git")
            .args(["worktree", "add", "-b", &branch_name])
            .arg(&worktree_path)
            .current_dir(&self.base_repo)
            .output()
            .await?;
        
        Ok(worktree_path)
    }
    
    /// 작업 완료 후 병합
    pub async fn merge_worktree(&self, task_id: &str) -> Result<()> {
        let branch_name = format!("agent/{}", task_id);
        
        // git merge
        Command::new("git")
            .args(["merge", "--no-ff", &branch_name])
            .current_dir(&self.base_repo)
            .output()
            .await?;
        
        // worktree 정리
        self.cleanup_worktree(task_id).await
    }
}
```

### 4.3 Shared Memory for Agents

```rust
/// 에이전트 간 공유 메모리
pub struct SharedAgentMemory {
    /// 공유 지식 (markdown 파일 기반)
    knowledge_file: PathBuf,
    
    /// 잠금
    lock: Arc<RwLock<()>>,
}

impl SharedAgentMemory {
    /// 지식 추가
    pub async fn add_knowledge(&self, fact: &str) -> Result<()> {
        let _guard = self.lock.write().await;
        
        let mut content = fs::read_to_string(&self.knowledge_file).await
            .unwrap_or_default();
        
        content.push_str(&format!("\n- {}", fact));
        
        fs::write(&self.knowledge_file, content).await?;
        Ok(())
    }
    
    /// 지식 조회
    pub async fn get_knowledge(&self) -> Result<String> {
        let _guard = self.lock.read().await;
        fs::read_to_string(&self.knowledge_file).await
            .map_err(Into::into)
    }
}
```

---

## 5. 구현 우선순위

### Phase 1: 핵심 기능 (HIGH)
1. ✅ Layer4 `PermissionDelegate` TUI 구현
2. ✅ Layer3 `PromptComposer` 기본 구현
3. ✅ Prompt Caching 시스템
4. ✅ Context Compaction 실제 구현

### Phase 2: 최적화 (MEDIUM)
1. 병렬 도구 실행 의존성 분석
2. 모델 선택 최적화
3. Layer4 `TaskObserver` TUI 구현
4. Token-efficient 직렬화

### Phase 3: 고급 기능 (LOW)
1. Git Worktree 병렬 작업
2. 에이전트 간 공유 메모리
3. 자동 모델 전환

---

## 6. 참고 자료

### Context Engineering
- [Context Window Management Strategies](https://www.getmaxim.ai/articles/context-window-management-strategies-for-long-context-ai-agents-and-chatbots/)
- [Context Engineering for AI Agents](https://www.flowhunt.io/blog/context-engineering-ai-agents-token-optimization/)
- [JetBrains Efficient Context Management](https://blog.jetbrains.com/research/2025/12/efficient-context-management/)

### Agent Architecture
- [Claude Code vs OpenCode](https://www.infralovers.com/blog/2026-01-29-claude-code-vs-opencode/)
- [Optimizing Agentic Coding](https://research.aimultiple.com/agentic-coding/)
- [Multi-Agent Parallel Execution](https://skywork.ai/blog/agent/multi-agent-parallel-execution-running-multiple-ai-agents-simultaneously/)

### Token Optimization
- [Token Optimization Strategies](https://medium.com/elementor-engineers/optimizing-token-usage-in-agent-based-assistants-ffd1822ece9c)
- [DuoAttention Paper](https://proceedings.iclr.cc/paper_files/paper/2025/file/5c1ddd2e59df46fd2aa85c833b1b36ed-Paper-Conference.pdf)
- [Parallelizing AI Coding Agents](https://ainativedev.io/news/how-to-parallelize-ai-coding-agents)
