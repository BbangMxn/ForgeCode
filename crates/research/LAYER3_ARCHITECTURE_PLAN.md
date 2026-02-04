# Layer3 Architecture Plan

## Overview

Layer3는 macOS 스타일의 모듈형 Agent 시스템을 구현합니다.
**핵심 원칙**: LLM이 추론을 담당하고, 코드는 실행만 담당합니다.

```
┌─────────────────────────────────────────────────────────────┐
│  Layer4-CLI/TUI (미래)                                      │
│  └── PermissionDelegate, TaskObserver 구현                  │
├─────────────────────────────────────────────────────────────┤
│  Layer3-Agent (이 레이어)                                   │
│  ├── Prompt Composer (시스템 프롬프트 조합)                  │
│  ├── Agent Executor (메인 루프)                             │
│  ├── Sub-Agent Spawner (Task 도구 처리)                     │
│  ├── Context Manager (컨텍스트 관리)                        │
│  └── Reminder Injector (시스템 리마인더)                    │
├─────────────────────────────────────────────────────────────┤
│  Layer2-Core (기존)                                         │
│  ├── AgentContext (도구 실행, 권한 검사)                     │
│  ├── ToolRegistry (도구 관리)                               │
│  ├── McpBridge (MCP 통합)                                   │
│  ├── SkillRegistry (스킬 관리)                              │
│  ├── HookExecutor (훅 실행)                                 │
│  └── PluginManager (플러그인)                               │
├─────────────────────────────────────────────────────────────┤
│  Layer2-Task (기존)                                         │
│  ├── TaskManager (태스크 오케스트레이션)                     │
│  ├── SubAgentManager (서브 에이전트 관리)                    │
│  └── Executors (Local, PTY, Container)                     │
├─────────────────────────────────────────────────────────────┤
│  Layer1-Foundation (기존)                                   │
│  ├── Core Traits (Tool, Provider, Task, ToolContext)       │
│  ├── Permission System (권한 관리)                          │
│  ├── Registry (MCP, Provider, Model)                       │
│  ├── EventBus (이벤트 시스템)                               │
│  └── AuditLogger (감사 로그)                                │
└─────────────────────────────────────────────────────────────┘
```

---

## 1. Layer3 모듈 구조

```
crates/Layer3-agent/
├── Cargo.toml
├── prompts/                          # 시스템 프롬프트 파일들
│   ├── system/
│   │   ├── main.md                   # 코어 아이덴티티
│   │   ├── tone_and_style.md         # 커뮤니케이션 스타일
│   │   ├── tool_usage_policy.md      # 도구 선택 규칙
│   │   ├── doing_tasks.md            # 태스크 수행 가이드
│   │   └── task_management.md        # TodoWrite 사용법
│   ├── agents/
│   │   ├── explore.md                # Explore 서브에이전트
│   │   ├── plan.md                   # Plan 모드
│   │   ├── general.md                # General 서브에이전트
│   │   └── bash.md                   # Bash 전문 에이전트
│   ├── tools/                        # 도구별 상세 설명
│   │   ├── read.md
│   │   ├── write.md
│   │   ├── edit.md
│   │   ├── bash.md
│   │   ├── glob.md
│   │   ├── grep.md
│   │   └── task.md
│   └── reminders/                    # 시스템 리마인더
│       ├── file_modified.md
│       ├── plan_mode_active.md
│       ├── token_usage.md
│       └── todo_reminder.md
└── src/
    ├── lib.rs
    ├── prompt/                       # 프롬프트 시스템
    │   ├── mod.rs
    │   ├── template.rs               # 템플릿 변수 치환
    │   ├── composer.rs               # 시스템 프롬프트 조합
    │   └── loader.rs                 # .md 파일 로딩
    ├── executor/                     # 에이전트 실행
    │   ├── mod.rs
    │   ├── loop.rs                   # 메인 실행 루프
    │   ├── message.rs                # 메시지 관리
    │   └── tool_handler.rs           # 도구 호출 처리
    ├── agent/                        # 에이전트 정의
    │   ├── mod.rs
    │   ├── config.rs                 # 에이전트 설정
    │   ├── registry.rs               # 에이전트 레지스트리
    │   ├── builtin.rs                # 기본 에이전트들
    │   └── spawner.rs                # 서브에이전트 생성
    ├── context/                      # 컨텍스트 관리
    │   ├── mod.rs
    │   ├── manager.rs                # 컨텍스트 윈도우 관리
    │   ├── reminder.rs               # 시스템 리마인더 주입
    │   └── compaction.rs             # 컨텍스트 압축
    └── session/                      # 세션 관리
        ├── mod.rs
        ├── state.rs                  # 세션 상태
        └── history.rs                # 대화 히스토리
```

---

## 2. 핵심 컴포넌트

### 2.1 Prompt Composer

Layer1/Layer2의 기존 구조를 활용하면서 시스템 프롬프트를 조합합니다.

```rust
// crates/Layer3-agent/src/prompt/composer.rs

use forge_foundation::{Tool, ToolMeta};
use forge_core::AgentContext;
use std::collections::HashMap;

/// 시스템 프롬프트 조합기
pub struct PromptComposer {
    /// 프롬프트 템플릿 캐시
    templates: HashMap<String, String>,
    
    /// 프롬프트 로더
    loader: PromptLoader,
}

impl PromptComposer {
    pub fn new() -> Self {
        let mut composer = Self {
            templates: HashMap::new(),
            loader: PromptLoader::new(),
        };
        composer.load_all_prompts();
        composer
    }
    
    /// 에이전트 컨텍스트에 맞는 시스템 프롬프트 생성
    pub fn compose(&self, ctx: &SessionContext) -> String {
        let mut sections = Vec::new();
        
        // 1. 메인 시스템 프롬프트
        sections.push(self.render_section("system/main", ctx));
        
        // 2. 톤과 스타일
        sections.push(self.render_section("system/tone_and_style", ctx));
        
        // 3. 도구 사용 정책
        sections.push(self.render_section("system/tool_usage_policy", ctx));
        
        // 4. 태스크 수행 가이드라인
        sections.push(self.render_section("system/doing_tasks", ctx));
        
        // 5. 태스크 관리 (TodoWrite가 활성화된 경우)
        if ctx.has_tool("todowrite") {
            sections.push(self.render_section("system/task_management", ctx));
        }
        
        // 6. 에이전트별 프롬프트 (서브에이전트인 경우)
        if let Some(agent_type) = &ctx.agent_type {
            let agent_section = format!("agents/{}", agent_type);
            sections.push(self.render_section(&agent_section, ctx));
        }
        
        // 7. 환경 정보
        sections.push(self.render_env_info(ctx));
        
        // 8. Git 상태 (있는 경우)
        if let Some(git_status) = &ctx.git_status {
            sections.push(self.render_git_status(git_status));
        }
        
        sections.join("\n\n")
    }
    
    fn render_section(&self, name: &str, ctx: &SessionContext) -> String {
        let template = self.templates.get(name)
            .cloned()
            .unwrap_or_default();
        
        self.substitute_variables(&template, ctx)
    }
    
    fn substitute_variables(&self, template: &str, ctx: &SessionContext) -> String {
        let mut result = template.to_string();
        
        // ${VARIABLE_NAME} 패턴 치환
        for (key, value) in &ctx.variables {
            let pattern = format!("${{{}}}", key);
            result = result.replace(&pattern, value);
        }
        
        result
    }
    
    fn render_env_info(&self, ctx: &SessionContext) -> String {
        format!(r#"
<env>
Working directory: {}
Is directory a git repo: {}
Platform: {}
Today's date: {}
</env>
"#, 
            ctx.working_dir.display(),
            ctx.is_git_repo,
            std::env::consts::OS,
            chrono::Local::now().format("%Y-%m-%d")
        )
    }
}
```

### 2.2 Agent Executor

Layer2의 `AgentContext`를 활용하여 메인 실행 루프를 구현합니다.

```rust
// crates/Layer3-agent/src/executor/loop.rs

use forge_foundation::{Provider, ChatRequest, ChatResponse, ToolCall, Result};
use forge_core::AgentContext;
use crate::prompt::PromptComposer;
use crate::context::{ContextManager, ReminderInjector};

/// 에이전트 실행기
pub struct AgentExecutor {
    /// Layer2 AgentContext (도구 실행, 권한 관리)
    agent_ctx: AgentContext,
    
    /// LLM Provider
    provider: Arc<dyn Provider>,
    
    /// 프롬프트 조합기
    prompt_composer: PromptComposer,
    
    /// 컨텍스트 관리자
    context_manager: ContextManager,
    
    /// 리마인더 주입기
    reminder_injector: ReminderInjector,
}

impl AgentExecutor {
    /// 메인 에이전트 루프
    pub async fn run(&mut self, session: &mut SessionContext) -> Result<AgentResult> {
        loop {
            // 1. 시스템 프롬프트 조합
            let system_prompt = self.prompt_composer.compose(session);
            
            // 2. 사용 가능한 도구 스키마 가져오기
            //    (Layer2 AgentContext 활용)
            let tools = self.get_available_tools(session).await;
            
            // 3. 메시지 히스토리 구성
            let messages = self.build_messages(session, &system_prompt);
            
            // 4. LLM 호출 (Layer1 Provider trait 활용)
            let response = self.provider.chat(ChatRequest {
                model: session.model.clone(),
                messages,
                tools: Some(tools),
                temperature: session.temperature,
                max_tokens: session.max_tokens,
                stream: false,
            }).await?;
            
            // 5. 응답 처리
            let (text, tool_calls) = self.parse_response(&response);
            
            // 6. 어시스턴트 메시지 추가
            session.add_assistant_message(&text, &tool_calls);
            
            // 7. 도구 호출이 없으면 완료
            if tool_calls.is_empty() {
                return Ok(AgentResult::Complete { output: text });
            }
            
            // 8. 도구 실행 (Layer2 AgentContext 활용)
            let results = self.execute_tools(session, &tool_calls).await?;
            
            // 9. 도구 결과에 시스템 리마인더 주입
            let results_with_reminders = self.reminder_injector.inject(session, results);
            
            // 10. 도구 결과를 히스토리에 추가
            for result in results_with_reminders {
                session.add_tool_result(&result);
            }
            
            // 11. 완료 신호 확인
            if self.should_stop(session, &results) {
                return Ok(AgentResult::Complete { output: text });
            }
            
            // 12. 컨텍스트 압축 필요 여부 확인
            if self.context_manager.needs_compaction(session) {
                self.context_manager.compact(session).await?;
            }
        }
    }
    
    /// 도구 실행 (Layer2 AgentContext 활용)
    async fn execute_tools(
        &self,
        session: &SessionContext,
        tool_calls: &[ToolCall],
    ) -> Result<Vec<ToolExecutionResult>> {
        // 독립적인 호출은 병렬 실행
        let (parallel, sequential) = self.partition_tool_calls(tool_calls);
        
        let mut results = Vec::new();
        
        // 병렬 실행
        if !parallel.is_empty() {
            let calls: Vec<_> = parallel.iter()
                .map(|tc| (tc.name.as_str(), tc.arguments.clone()))
                .collect();
            
            // Layer2 AgentContext의 병렬 실행 활용
            let parallel_results = self.agent_ctx.execute_tools_parallel(calls).await;
            results.extend(parallel_results.into_iter().filter_map(|r| r.ok()));
        }
        
        // 순차 실행
        for tc in sequential {
            let result = self.agent_ctx.execute_tool(&tc.name, tc.arguments.clone()).await?;
            results.push(result);
        }
        
        Ok(results)
    }
    
    /// 사용 가능한 도구 스키마 (Layer2에서 가져옴)
    async fn get_available_tools(&self, session: &SessionContext) -> Vec<Value> {
        // 에이전트 타입에 따른 도구 필터링
        let all_schemas = self.agent_ctx.get_tool_schemas().await;
        
        if let Some(allowed_tools) = &session.allowed_tools {
            all_schemas.into_iter()
                .filter(|schema| {
                    schema.get("name")
                        .and_then(|v| v.as_str())
                        .map(|name| allowed_tools.contains(&name.to_string()))
                        .unwrap_or(false)
                })
                .collect()
        } else {
            all_schemas
        }
    }
}
```

### 2.3 Sub-Agent Spawner

Layer2-task의 `SubAgentManager`와 통합하여 서브에이전트를 생성합니다.

```rust
// crates/Layer3-agent/src/agent/spawner.rs

use forge_task::subagent::{SubAgentManager, SubAgentConfig, SubAgentContext};
use crate::agent::{AgentConfig, AgentRegistry};
use crate::executor::AgentExecutor;

/// 서브에이전트 생성기
pub struct SubAgentSpawner {
    /// 에이전트 레지스트리
    registry: Arc<AgentRegistry>,
    
    /// Layer2 서브에이전트 매니저
    subagent_manager: Arc<SubAgentManager>,
    
    /// 프롬프트 조합기
    prompt_composer: Arc<PromptComposer>,
}

impl SubAgentSpawner {
    /// 서브에이전트 생성 및 실행
    pub async fn spawn(
        &self,
        agent_type: &str,
        prompt: &str,
        parent_session: &SessionContext,
    ) -> Result<String> {
        // 1. 에이전트 설정 가져오기
        let agent_config = self.registry.get(agent_type)
            .ok_or_else(|| Error::NotFound(format!("Agent '{}' not found", agent_type)))?;
        
        // 2. Layer2 SubAgentConfig 생성
        let subagent_config = SubAgentConfig {
            agent_type: agent_type.to_string(),
            max_steps: agent_config.max_steps,
            timeout: agent_config.timeout,
            model: agent_config.model.clone(),
            permission_mode: self.map_permission_mode(&agent_config),
        };
        
        // 3. 세션 컨텍스트 생성 (권한 상속)
        let mut session = SessionContext::new_subagent(
            agent_type,
            parent_session,
        );
        
        // 에이전트별 허용 도구 설정
        session.allowed_tools = Some(agent_config.tools.clone());
        
        // 에이전트별 커스텀 프롬프트
        if let Some(custom_prompt) = &agent_config.custom_prompt {
            session.variables.insert(
                "AGENT_PROMPT".to_string(),
                custom_prompt.clone()
            );
        }
        
        // 4. 태스크 프롬프트 추가
        session.add_user_message(prompt);
        
        // 5. 실행
        let executor = AgentExecutor::new_for_subagent(
            &session,
            &agent_config,
        );
        
        let result = executor.run(&mut session).await?;
        
        Ok(result.output)
    }
    
    fn map_permission_mode(&self, config: &AgentConfig) -> PermissionMode {
        match config.permission_preset {
            PermissionPreset::ReadOnly => PermissionMode::ReadOnly,
            PermissionPreset::ReadWrite => PermissionMode::Inherit,
            PermissionPreset::Custom(ref rules) => PermissionMode::Custom(rules.clone()),
        }
    }
}
```

### 2.4 Context Manager & Reminder Injector

컨텍스트 관리와 시스템 리마인더를 담당합니다.

```rust
// crates/Layer3-agent/src/context/manager.rs

use forge_task::subagent::context::{ContextWindow, ContextWindowConfig};

/// 컨텍스트 관리자
pub struct ContextManager {
    /// 컨텍스트 윈도우 설정
    config: ContextWindowConfig,
    
    /// 압축기
    compactor: ContextCompactor,
}

impl ContextManager {
    /// 컨텍스트 압축 필요 여부
    pub fn needs_compaction(&self, session: &SessionContext) -> bool {
        let usage_ratio = session.token_usage as f32 / session.token_limit as f32;
        usage_ratio > self.config.compaction_threshold
    }
    
    /// 컨텍스트 압축
    pub async fn compact(&self, session: &mut SessionContext) -> Result<()> {
        self.compactor.compact(session).await
    }
}

// crates/Layer3-agent/src/context/reminder.rs

/// 시스템 리마인더 주입기
pub struct ReminderInjector {
    templates: HashMap<ReminderType, String>,
}

#[derive(Hash, Eq, PartialEq, Clone)]
pub enum ReminderType {
    FileModified,
    PlanModeActive,
    TokenUsage,
    TodoReminder,
    HookBlocked,
    SessionContinuation,
}

impl ReminderInjector {
    /// 도구 결과에 적절한 리마인더 주입
    pub fn inject(
        &self,
        session: &SessionContext,
        results: Vec<ToolExecutionResult>,
    ) -> Vec<ToolExecutionResult> {
        let reminders = self.collect_reminders(session);
        
        if reminders.is_empty() {
            return results;
        }
        
        // 마지막 결과에 리마인더 추가
        let mut results = results;
        if let Some(last) = results.last_mut() {
            let reminder_text = reminders.into_iter()
                .map(|r| format!("<system-reminder>\n{}\n</system-reminder>", r))
                .collect::<Vec<_>>()
                .join("\n\n");
            
            last.output.push_str("\n\n");
            last.output.push_str(&reminder_text);
        }
        
        results
    }
    
    fn collect_reminders(&self, session: &SessionContext) -> Vec<String> {
        let mut reminders = Vec::new();
        
        // 파일 수정 감지
        if !session.modified_files.is_empty() {
            for path in &session.modified_files {
                reminders.push(self.render(ReminderType::FileModified, &[
                    ("path", path.to_str().unwrap_or_default()),
                ]));
            }
            session.clear_modified_files();
        }
        
        // 플랜 모드
        if session.is_plan_mode {
            reminders.push(self.render(ReminderType::PlanModeActive, &[]));
        }
        
        // 토큰 사용량 경고
        if session.token_usage_ratio() > 0.8 {
            reminders.push(self.render(ReminderType::TokenUsage, &[
                ("used", &session.token_usage.to_string()),
                ("limit", &session.token_limit.to_string()),
            ]));
        }
        
        // TodoWrite 리마인더 (주기적)
        if session.should_remind_todo() {
            reminders.push(self.render(ReminderType::TodoReminder, &[]));
        }
        
        reminders
    }
}
```

---

## 3. Layer1/Layer2 통합 포인트

### 3.1 Layer1 Traits 구현

```rust
// Layer3에서 Layer1 ToolContext 구현
impl ToolContext for SessionContext {
    fn working_dir(&self) -> &Path {
        &self.working_dir
    }
    
    fn session_id(&self) -> &str {
        &self.session_id
    }
    
    fn env(&self) -> &HashMap<String, String> {
        &self.environment
    }
    
    async fn check_permission(&self, tool: &str, action: &PermissionAction) -> PermissionStatus {
        // Layer2 AgentContext를 통해 권한 확인
        self.agent_ctx.check_permission(tool, action)
    }
    
    async fn request_permission(
        &self,
        tool: &str,
        description: &str,
        action: PermissionAction,
    ) -> Result<bool> {
        // Layer2 AgentContext를 통해 권한 요청
        self.agent_ctx.request_permission(tool, description, action).await
    }
    
    fn shell_config(&self) -> &dyn ShellConfig {
        &self.shell_config
    }
}
```

### 3.2 Layer2 활용

```rust
// Layer2 AgentContext 활용
impl AgentExecutor {
    pub async fn new(config: ExecutorConfig) -> Result<Self> {
        // Layer2 AgentContext 빌더 사용
        let agent_ctx = AgentContext::builder()
            .working_directory(config.working_dir.clone())
            .session_id(&config.session_id)
            .with_permission_service(config.permission_service.clone())
            .enable_mcp()
            .build();
        
        // MCP 서버 연결
        for (name, mcp_config) in &config.mcp_servers {
            agent_ctx.connect_mcp_server(name, mcp_config.clone()).await?;
        }
        
        Ok(Self {
            agent_ctx,
            provider: config.provider,
            prompt_composer: PromptComposer::new(),
            context_manager: ContextManager::new(config.context_config),
            reminder_injector: ReminderInjector::new(),
        })
    }
}

// Layer2 Hook 시스템 활용
impl AgentExecutor {
    async fn execute_with_hooks(
        &self,
        tool_name: &str,
        input: Value,
        session: &SessionContext,
    ) -> Result<ToolExecutionResult> {
        // Pre-hook 실행
        let hook_result = self.hook_executor.execute_pre_tool(
            tool_name,
            &input,
            session,
        ).await?;
        
        if hook_result.blocked {
            return Err(Error::HookBlocked(hook_result.reason));
        }
        
        // 도구 실행
        let result = self.agent_ctx.execute_tool(tool_name, input).await?;
        
        // Post-hook 실행
        self.hook_executor.execute_post_tool(
            tool_name,
            &result,
            session,
        ).await?;
        
        Ok(result)
    }
}
```

### 3.3 Layer2-Task 통합

```rust
// Layer2-Task SubAgentManager 활용
impl SubAgentSpawner {
    pub fn new(
        registry: Arc<AgentRegistry>,
        task_manager: Arc<TaskManager>,
    ) -> Self {
        // Layer2 SubAgentManager 생성
        let subagent_manager = Arc::new(SubAgentManager::new(
            SubAgentManagerConfig {
                max_concurrent: 4,
                default_timeout: Duration::from_secs(300),
            }
        ));
        
        Self {
            registry,
            subagent_manager,
            prompt_composer: Arc::new(PromptComposer::new()),
        }
    }
}
```

---

## 4. 에이전트 설정 시스템

### 4.1 에이전트 정의

```rust
// crates/Layer3-agent/src/agent/config.rs

/// 에이전트 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// 에이전트 이름
    pub name: String,
    
    /// 설명
    pub description: String,
    
    /// 모드 (primary, subagent, all)
    pub mode: AgentMode,
    
    /// 커스텀 프롬프트
    pub custom_prompt: Option<String>,
    
    /// 모델 설정 (없으면 기본값 사용)
    pub model: Option<ModelSpec>,
    
    /// 온도 설정
    pub temperature: Option<f32>,
    
    /// 허용된 도구 목록
    pub tools: Vec<String>,
    
    /// 권한 프리셋
    pub permission_preset: PermissionPreset,
    
    /// 최대 스텝 수
    pub max_steps: Option<usize>,
    
    /// 타임아웃
    pub timeout: Option<Duration>,
    
    /// 숨김 여부
    pub hidden: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentMode {
    Primary,    // 메인 대화 에이전트
    SubAgent,   // Task 도구로 생성되는 서브에이전트
    All,        // 둘 다 가능
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PermissionPreset {
    /// 읽기 전용 (Explore 에이전트용)
    ReadOnly,
    
    /// 읽기/쓰기 (기본)
    ReadWrite,
    
    /// 커스텀 규칙
    Custom(Vec<PermissionRule>),
}
```

### 4.2 기본 에이전트들

```rust
// crates/Layer3-agent/src/agent/builtin.rs

pub fn builtin_agents() -> Vec<AgentConfig> {
    vec![
        // Build 에이전트 (기본)
        AgentConfig {
            name: "build".to_string(),
            description: "The default agent. Executes tools based on permissions.".to_string(),
            mode: AgentMode::Primary,
            custom_prompt: None,
            model: None,
            temperature: None,
            tools: all_tools(),
            permission_preset: PermissionPreset::ReadWrite,
            max_steps: None,
            timeout: None,
            hidden: false,
        },
        
        // Plan 에이전트
        AgentConfig {
            name: "plan".to_string(),
            description: "Plan mode. Disallows all edit tools.".to_string(),
            mode: AgentMode::Primary,
            custom_prompt: Some(include_str!("../prompts/agents/plan.md").to_string()),
            model: None,
            temperature: None,
            tools: read_only_tools(),
            permission_preset: PermissionPreset::ReadOnly,
            max_steps: None,
            timeout: None,
            hidden: false,
        },
        
        // Explore 에이전트
        AgentConfig {
            name: "explore".to_string(),
            description: "Fast agent for exploring codebases.".to_string(),
            mode: AgentMode::SubAgent,
            custom_prompt: Some(include_str!("../prompts/agents/explore.md").to_string()),
            model: None,
            temperature: Some(0.0),
            tools: vec![
                "glob".to_string(),
                "grep".to_string(),
                "read".to_string(),
                "bash".to_string(),  // read-only commands only
            ],
            permission_preset: PermissionPreset::ReadOnly,
            max_steps: Some(20),
            timeout: Some(Duration::from_secs(60)),
            hidden: false,
        },
        
        // General 에이전트
        AgentConfig {
            name: "general".to_string(),
            description: "General-purpose agent for research and multi-step tasks.".to_string(),
            mode: AgentMode::SubAgent,
            custom_prompt: None,
            model: None,
            temperature: None,
            tools: all_tools_except(&["todowrite"]),
            permission_preset: PermissionPreset::ReadWrite,
            max_steps: Some(50),
            timeout: Some(Duration::from_secs(300)),
            hidden: false,
        },
        
        // Bash 전문 에이전트
        AgentConfig {
            name: "bash".to_string(),
            description: "Command execution specialist.".to_string(),
            mode: AgentMode::SubAgent,
            custom_prompt: Some(include_str!("../prompts/agents/bash.md").to_string()),
            model: None,
            temperature: None,
            tools: vec!["bash".to_string()],
            permission_preset: PermissionPreset::Custom(bash_permission_rules()),
            max_steps: Some(10),
            timeout: Some(Duration::from_secs(120)),
            hidden: false,
        },
    ]
}

fn all_tools() -> Vec<String> {
    vec![
        "read", "write", "edit", "bash",
        "glob", "grep", "task", "todowrite",
        "webfetch", "websearch", "question",
        "plan_enter", "plan_exit",
    ].into_iter().map(String::from).collect()
}

fn read_only_tools() -> Vec<String> {
    vec![
        "read", "glob", "grep", "bash",
        "webfetch", "websearch",
        "plan_exit",
    ].into_iter().map(String::from).collect()
}
```

---

## 5. 구현 순서

### Phase 1: 기본 구조
1. `crates/Layer3-agent/Cargo.toml` 생성
2. `src/lib.rs` 기본 모듈 구조
3. `src/prompt/` - 프롬프트 로더, 템플릿, 조합기
4. `prompts/` 폴더에 기본 프롬프트 파일들

### Phase 2: 실행 루프
1. `src/session/` - 세션 상태 관리
2. `src/executor/` - 메인 실행 루프
3. Layer2 AgentContext 통합

### Phase 3: 에이전트 시스템
1. `src/agent/` - 에이전트 설정, 레지스트리
2. 기본 에이전트 정의
3. 서브에이전트 스포너

### Phase 4: 컨텍스트 관리
1. `src/context/` - 컨텍스트 매니저
2. 리마인더 인젝터
3. 컨텍스트 압축

### Phase 5: 통합 테스트
1. Layer1 traits 구현 테스트
2. Layer2 통합 테스트
3. 전체 에이전트 루프 테스트

---

## 6. 핵심 설계 원칙

1. **LLM이 추론 담당**: 코드는 실행만 담당, 복잡한 ReAct/CoT 루프 없음
2. **Layer1/Layer2 재사용**: 기존 인프라 최대 활용
3. **모듈형 설계**: macOS 스타일로 컴포넌트 교체 가능
4. **프롬프트 중심**: 모든 지능은 시스템 프롬프트에
5. **권한 통합**: Layer1 Permission 시스템 완전 활용
