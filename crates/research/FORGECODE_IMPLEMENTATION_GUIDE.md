# ForgeCode Implementation Guide

Based on Claude Code and OpenCode analysis, this guide outlines how to implement
a similar AI coding assistant in Rust.

## Core Principle

**LET THE LLM DO THE THINKING**

The code should NOT implement:
- Complex reasoning loops (ReAct, CoT, ToT)
- Agent state machines
- Explicit planning phases
- Memory retrieval systems

The code SHOULD implement:
- System prompt composition
- Tool execution
- Context management
- Permission enforcement

---

## 1. System Prompt Module

### 1.1 Prompt Template System

```rust
// crates/Layer3-agent/src/prompt/template.rs

use std::collections::HashMap;

/// A template with ${VARIABLE} placeholders
pub struct PromptTemplate {
    content: String,
    variables: Vec<String>,
}

impl PromptTemplate {
    pub fn new(content: &str) -> Self {
        let variables = Self::extract_variables(content);
        Self {
            content: content.to_string(),
            variables,
        }
    }
    
    fn extract_variables(content: &str) -> Vec<String> {
        // Extract ${VAR_NAME} patterns
        let re = regex::Regex::new(r"\$\{([A-Z_]+)\}").unwrap();
        re.captures_iter(content)
            .map(|cap| cap[1].to_string())
            .collect()
    }
    
    pub fn render(&self, vars: &HashMap<String, String>) -> String {
        let mut result = self.content.clone();
        for (key, value) in vars {
            let pattern = format!("${{{}}}", key);
            result = result.replace(&pattern, value);
        }
        result
    }
}
```

### 1.2 Prompt Composer

```rust
// crates/Layer3-agent/src/prompt/composer.rs

use std::path::Path;

pub struct PromptComposer {
    base_dir: PathBuf,
    cache: HashMap<String, PromptTemplate>,
}

impl PromptComposer {
    /// Compose the full system prompt based on context
    pub fn compose(&self, ctx: &AgentContext) -> String {
        let mut sections = Vec::new();
        
        // 1. Main system prompt
        sections.push(self.load("system/main.md"));
        
        // 2. Tone and style
        sections.push(self.load("system/tone_and_style.md"));
        
        // 3. Tool usage policy
        sections.push(self.load("system/tool_usage_policy.md"));
        
        // 4. Doing tasks
        sections.push(self.load("system/doing_tasks.md"));
        
        // 5. Task management (if TodoWrite enabled)
        if ctx.tools.contains("TodoWrite") {
            sections.push(self.load("system/task_management.md"));
        }
        
        // 6. Agent-specific prompt (if sub-agent)
        if let Some(agent_type) = &ctx.agent_type {
            sections.push(self.load(&format!("agents/{}.md", agent_type)));
        }
        
        // 7. Environment info
        sections.push(self.render_env_info(ctx));
        
        // 8. Git status (if available)
        if let Some(git_status) = &ctx.git_status {
            sections.push(self.render_git_status(git_status));
        }
        
        sections.join("\n\n")
    }
    
    fn render_env_info(&self, ctx: &AgentContext) -> String {
        format!(r#"
<env>
Working directory: {}
Is directory a git repo: {}
Platform: {}
Today's date: {}
</env>
"#, ctx.working_dir, ctx.is_git_repo, ctx.platform, ctx.date)
    }
}
```

### 1.3 Prompt Files Structure

```
prompts/
├── system/
│   ├── main.md                 # Core identity
│   ├── tone_and_style.md       # Communication style
│   ├── tool_usage_policy.md    # Tool selection rules
│   ├── doing_tasks.md          # Task execution guidelines
│   └── task_management.md      # TodoWrite usage
├── agents/
│   ├── explore.md              # Explore sub-agent
│   ├── plan.md                 # Plan mode agent
│   ├── general.md              # General-purpose agent
│   └── bash.md                 # Command execution specialist
├── tools/
│   ├── read.md
│   ├── write.md
│   ├── edit.md
│   ├── bash.md
│   ├── glob.md
│   ├── grep.md
│   └── task.md
└── reminders/
    ├── file_modified.md
    ├── plan_mode_active.md
    ├── token_usage.md
    └── todo_reminder.md
```

---

## 2. Agent Executor

### 2.1 Main Loop

```rust
// crates/Layer3-agent/src/executor.rs

pub struct AgentExecutor {
    provider: Arc<dyn Provider>,
    tool_registry: ToolRegistry,
    prompt_composer: PromptComposer,
    permission_checker: PermissionChecker,
}

impl AgentExecutor {
    /// Execute the agent loop
    pub async fn execute(&self, ctx: &mut AgentContext) -> Result<AgentResult> {
        loop {
            // 1. Compose system prompt
            let system_prompt = self.prompt_composer.compose(ctx);
            
            // 2. Get available tools based on permissions
            let tools = self.get_available_tools(ctx);
            
            // 3. Build messages
            let messages = self.build_messages(ctx, &system_prompt);
            
            // 4. Call LLM
            let response = self.provider.chat(ChatRequest {
                messages,
                tools: Some(tools),
                temperature: ctx.temperature,
                max_tokens: ctx.max_tokens,
            }).await?;
            
            // 5. Process response
            let (text, tool_calls) = self.parse_response(&response);
            
            // 6. Add assistant message to history
            ctx.add_message(Message::assistant(text.clone(), tool_calls.clone()));
            
            // 7. If no tool calls, we're done
            if tool_calls.is_empty() {
                return Ok(AgentResult::Complete { output: text });
            }
            
            // 8. Execute tool calls (parallel if independent)
            let results = self.execute_tools(ctx, &tool_calls).await?;
            
            // 9. Add tool results to history
            for result in &results {
                ctx.add_message(Message::tool_result(result));
            }
            
            // 10. Check for completion signals
            if self.should_stop(ctx, &results) {
                return Ok(AgentResult::Complete { output: text });
            }
            
            // 11. Check context limits
            if ctx.needs_compaction() {
                self.compact_context(ctx).await?;
            }
        }
    }
    
    async fn execute_tools(
        &self,
        ctx: &mut AgentContext,
        tool_calls: &[ToolCall],
    ) -> Result<Vec<ToolResult>> {
        // Group independent calls for parallel execution
        let (parallel, sequential) = self.partition_tool_calls(tool_calls);
        
        let mut results = Vec::new();
        
        // Execute parallel calls
        if !parallel.is_empty() {
            let futures: Vec<_> = parallel.iter()
                .map(|tc| self.execute_single_tool(ctx, tc))
                .collect();
            let parallel_results = futures::future::join_all(futures).await;
            results.extend(parallel_results.into_iter().filter_map(Result::ok));
        }
        
        // Execute sequential calls
        for tc in sequential {
            let result = self.execute_single_tool(ctx, &tc).await?;
            results.push(result);
        }
        
        Ok(results)
    }
}
```

### 2.2 Sub-Agent Spawning

```rust
// crates/Layer3-agent/src/subagent.rs

pub struct SubAgentSpawner {
    executor: Arc<AgentExecutor>,
}

impl SubAgentSpawner {
    /// Spawn a sub-agent with specialized configuration
    pub async fn spawn(
        &self,
        agent_type: SubAgentType,
        prompt: &str,
        parent_ctx: &AgentContext,
    ) -> Result<String> {
        // Create sub-agent context
        let mut ctx = AgentContext::new_subagent(
            agent_type.clone(),
            parent_ctx,
        );
        
        // Set agent-specific permissions
        ctx.permissions = self.get_permissions(&agent_type);
        
        // Set agent-specific tools
        ctx.available_tools = self.get_tools(&agent_type);
        
        // Add the task prompt
        ctx.add_message(Message::user(prompt));
        
        // Execute
        let result = self.executor.execute(&mut ctx).await?;
        
        Ok(result.output)
    }
    
    fn get_permissions(&self, agent_type: &SubAgentType) -> Permissions {
        match agent_type {
            SubAgentType::Explore => Permissions {
                default: Permission::Deny,
                rules: vec![
                    ("glob", Permission::Allow),
                    ("grep", Permission::Allow),
                    ("read", Permission::Allow),
                    ("bash", Permission::Allow), // read-only commands
                ],
            },
            SubAgentType::Plan => Permissions {
                default: Permission::Allow,
                rules: vec![
                    ("edit", Permission::Deny),
                    ("write", Permission::Deny),
                ],
            },
            SubAgentType::General => Permissions {
                default: Permission::Allow,
                rules: vec![
                    ("todowrite", Permission::Deny),
                ],
            },
            _ => Permissions::default(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum SubAgentType {
    Explore,
    Plan,
    General,
    Bash,
}
```

---

## 3. Tool System

### 3.1 Tool Definition

```rust
// crates/Layer3-agent/src/tool/definition.rs

pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: JsonSchema,
    pub handler: Box<dyn ToolHandler>,
}

pub trait ToolHandler: Send + Sync {
    fn execute(&self, params: Value, ctx: &ToolContext) -> BoxFuture<Result<ToolOutput>>;
}

/// Load tool description from markdown file
pub fn load_tool_description(name: &str) -> String {
    let path = format!("prompts/tools/{}.md", name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|_| format!("Tool: {}", name))
}
```

### 3.2 Tool Registry

```rust
// crates/Layer3-agent/src/tool/registry.rs

pub struct ToolRegistry {
    tools: HashMap<String, ToolDefinition>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        let mut registry = Self { tools: HashMap::new() };
        
        // Register built-in tools
        registry.register(read_tool());
        registry.register(write_tool());
        registry.register(edit_tool());
        registry.register(bash_tool());
        registry.register(glob_tool());
        registry.register(grep_tool());
        registry.register(task_tool());
        registry.register(todo_write_tool());
        registry.register(web_fetch_tool());
        registry.register(web_search_tool());
        
        registry
    }
    
    /// Get tools as JSON schema for LLM
    pub fn get_tools_schema(&self, names: &[&str]) -> Vec<Tool> {
        names.iter()
            .filter_map(|name| self.tools.get(*name))
            .map(|def| Tool {
                name: def.name.clone(),
                description: def.description.clone(),
                input_schema: def.parameters.clone(),
            })
            .collect()
    }
}

fn read_tool() -> ToolDefinition {
    ToolDefinition {
        name: "Read".to_string(),
        description: load_tool_description("read"),
        parameters: json_schema!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Absolute path to the file to read"
                },
                "offset": {
                    "type": "number",
                    "description": "Line number to start from"
                },
                "limit": {
                    "type": "number",
                    "description": "Number of lines to read"
                }
            },
            "required": ["file_path"]
        }),
        handler: Box::new(ReadToolHandler),
    }
}
```

---

## 4. Context Management

### 4.1 System Reminders

```rust
// crates/Layer3-agent/src/context/reminder.rs

pub struct ReminderInjector {
    templates: HashMap<ReminderType, String>,
}

#[derive(Hash, Eq, PartialEq)]
pub enum ReminderType {
    FileModified,
    PlanModeActive,
    TokenUsage,
    TodoReminder,
    SessionContinuation,
    HookBlocked,
}

impl ReminderInjector {
    /// Inject reminders into tool results based on context
    pub fn inject(&self, ctx: &AgentContext, message: &mut Message) {
        let mut reminders = Vec::new();
        
        // Check for file modifications
        if let Some(modified) = ctx.get_modified_files() {
            for path in modified {
                reminders.push(self.render(ReminderType::FileModified, &[
                    ("path", path.to_str().unwrap_or_default()),
                ]));
            }
        }
        
        // Check plan mode
        if ctx.is_plan_mode() {
            reminders.push(self.render(ReminderType::PlanModeActive, &[]));
        }
        
        // Check token usage
        if ctx.token_usage_ratio() > 0.8 {
            reminders.push(self.render(ReminderType::TokenUsage, &[
                ("used", &ctx.tokens_used.to_string()),
                ("limit", &ctx.token_limit.to_string()),
            ]));
        }
        
        // Check todo reminder
        if ctx.should_remind_todo() {
            reminders.push(self.render(ReminderType::TodoReminder, &[]));
        }
        
        // Append reminders to message
        if !reminders.is_empty() {
            message.content.push_str("\n\n");
            for reminder in reminders {
                message.content.push_str(&format!(
                    "<system-reminder>\n{}\n</system-reminder>\n",
                    reminder
                ));
            }
        }
    }
}
```

### 4.2 Context Compaction

```rust
// crates/Layer3-agent/src/context/compaction.rs

pub struct ContextCompactor {
    summarizer: Arc<AgentExecutor>,
}

impl ContextCompactor {
    /// Compact conversation history when context is running low
    pub async fn compact(&self, ctx: &mut AgentContext) -> Result<()> {
        // Get messages to summarize (keep recent ones)
        let keep_recent = 4;
        let to_summarize = ctx.messages.len().saturating_sub(keep_recent);
        
        if to_summarize < 5 {
            return Ok(()); // Not enough to summarize
        }
        
        let messages_to_summarize: Vec<_> = ctx.messages
            .drain(..to_summarize)
            .collect();
        
        // Create compaction summary
        let summary = self.create_summary(&messages_to_summarize).await?;
        
        // Insert summary at the beginning
        ctx.messages.insert(0, Message::system(format!(
            "<session-summary>\n{}\n</session-summary>",
            summary
        )));
        
        Ok(())
    }
    
    async fn create_summary(&self, messages: &[Message]) -> Result<String> {
        // Use the compaction sub-agent
        let prompt = format!(
            "Summarize this conversation, preserving:\n\
             - Key decisions made\n\
             - Files modified and why\n\
             - Important context for continuing\n\n\
             Conversation:\n{}",
            self.format_messages(messages)
        );
        
        let mut ctx = AgentContext::new_subagent(
            SubAgentType::Compaction,
            &AgentContext::default(),
        );
        ctx.add_message(Message::user(&prompt));
        
        let result = self.summarizer.execute(&mut ctx).await?;
        Ok(result.output)
    }
}
```

---

## 5. Permission System

### 5.1 Permission Rules

```rust
// crates/Layer3-agent/src/permission.rs

use glob::Pattern;

#[derive(Clone, Debug)]
pub enum Permission {
    Allow,
    Deny,
    Ask,
}

#[derive(Clone)]
pub struct PermissionRule {
    pub tool: String,
    pub pattern: Option<Pattern>,
    pub action: Permission,
}

pub struct PermissionChecker {
    rules: Vec<PermissionRule>,
}

impl PermissionChecker {
    pub fn check(&self, tool: &str, params: &Value) -> Permission {
        // Find matching rules (most specific first)
        for rule in &self.rules {
            if rule.tool != tool && rule.tool != "*" {
                continue;
            }
            
            // Check pattern if applicable
            if let Some(pattern) = &rule.pattern {
                if let Some(path) = self.extract_path(params) {
                    if pattern.matches(&path) {
                        return rule.action.clone();
                    }
                }
            } else {
                return rule.action.clone();
            }
        }
        
        Permission::Allow // Default
    }
    
    fn extract_path(&self, params: &Value) -> Option<String> {
        params.get("file_path")
            .or_else(|| params.get("path"))
            .and_then(|v| v.as_str())
            .map(String::from)
    }
}
```

### 5.2 Default Permissions

```rust
pub fn default_permissions() -> Vec<PermissionRule> {
    vec![
        // Allow most operations
        PermissionRule {
            tool: "*".to_string(),
            pattern: None,
            action: Permission::Allow,
        },
        // Ask for .env files
        PermissionRule {
            tool: "read".to_string(),
            pattern: Some(Pattern::new("*.env").unwrap()),
            action: Permission::Ask,
        },
        PermissionRule {
            tool: "read".to_string(),
            pattern: Some(Pattern::new("*.env.*").unwrap()),
            action: Permission::Ask,
        },
        // Ask for external directories
        PermissionRule {
            tool: "external_directory".to_string(),
            pattern: Some(Pattern::new("*").unwrap()),
            action: Permission::Ask,
        },
    ]
}
```

---

## 6. Agent Variants

### 6.1 Agent Configuration

```rust
// crates/Layer3-agent/src/agent/config.rs

pub struct AgentConfig {
    pub name: String,
    pub description: String,
    pub mode: AgentMode,
    pub prompt: Option<String>,
    pub model: Option<ModelSpec>,
    pub temperature: Option<f32>,
    pub permissions: Vec<PermissionRule>,
    pub tools: Vec<String>,
    pub hidden: bool,
}

#[derive(Clone, Debug)]
pub enum AgentMode {
    Primary,    // Main conversation agent
    SubAgent,   // Spawned by Task tool
    All,        // Can be used as either
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            name: "build".to_string(),
            description: "The default agent.".to_string(),
            mode: AgentMode::Primary,
            prompt: None,
            model: None,
            temperature: None,
            permissions: default_permissions(),
            tools: vec![
                "read", "write", "edit", "bash",
                "glob", "grep", "task", "todowrite",
                "webfetch", "websearch", "question",
                "plan_enter", "plan_exit",
            ].into_iter().map(String::from).collect(),
            hidden: false,
        }
    }
}
```

### 6.2 Built-in Agents

```rust
pub fn builtin_agents() -> Vec<AgentConfig> {
    vec![
        AgentConfig {
            name: "build".to_string(),
            description: "The default agent. Executes tools based on permissions.".to_string(),
            mode: AgentMode::Primary,
            ..Default::default()
        },
        AgentConfig {
            name: "plan".to_string(),
            description: "Plan mode. Disallows all edit tools.".to_string(),
            mode: AgentMode::Primary,
            permissions: plan_permissions(),
            ..Default::default()
        },
        AgentConfig {
            name: "explore".to_string(),
            description: "Fast agent for exploring codebases.".to_string(),
            mode: AgentMode::SubAgent,
            prompt: Some(include_str!("../prompts/agents/explore.md").to_string()),
            tools: vec!["glob", "grep", "read", "bash"]
                .into_iter().map(String::from).collect(),
            permissions: explore_permissions(),
            ..Default::default()
        },
        AgentConfig {
            name: "general".to_string(),
            description: "General-purpose agent for research.".to_string(),
            mode: AgentMode::SubAgent,
            permissions: general_permissions(),
            ..Default::default()
        },
    ]
}
```

---

## 7. Integration with Existing Crates

### Layer2-core Integration

```rust
// Use existing tool infrastructure
use layer2_core::tool::{ToolRegistry, ToolContext};
use layer2_core::mcp::McpBridge;
use layer2_core::provider::Provider;

// Use existing hook system
use layer2_core::hook::HookExecutor;
```

### Layer2-provider Integration

```rust
// Use existing provider abstraction
use layer2_provider::anthropic::AnthropicProvider;
use layer2_provider::openai::OpenAIProvider;
```

### Layer2-task Integration

```rust
// Use existing task execution
use layer2_task::executor::TaskExecutor;
use layer2_task::manager::TaskManager;
```

---

## 8. Next Steps

1. **Create prompt files** - Write the .md files for system prompts, tool descriptions
2. **Implement PromptComposer** - Template loading and composition
3. **Implement AgentExecutor** - Main execution loop
4. **Implement SubAgentSpawner** - Task tool handler
5. **Implement ReminderInjector** - Context-aware hints
6. **Implement ContextCompactor** - Conversation summarization
7. **Create agent configs** - Define built-in agents
8. **Add tests** - Unit and integration tests

---

## Summary

The key to implementing a Claude Code-style assistant:

1. **Simple execution loop** - No complex agent logic
2. **Rich system prompts** - All intelligence is in the prompts
3. **Tool guidance** - Detailed usage instructions
4. **Context management** - Reminders and compaction
5. **Permission system** - User control over tool execution

The LLM handles reasoning, planning, and decision-making internally.
Our code just provides the right context and executes the requested tools.
