# Agent Architecture Research

## Claude Code Architecture Analysis (2025)

### 1. Master Agent Loop "nO"

```
┌─────────────────────────────────────────────────────────────┐
│                    Master Loop "nO"                          │
│                                                              │
│   while has_tool_calls:                                     │
│       1. Send message to LLM                                │
│       2. Receive response (may include tool calls)          │
│       3. Execute tool calls                                 │
│       4. Append results to history                          │
│       5. Check steering queue (h2A)                         │
│       6. Repeat                                             │
│                                                              │
│   → Plain text response (no tools) = loop terminates        │
└─────────────────────────────────────────────────────────────┘
```

**Key Principles:**
- Single-threaded, flat message history
- Debuggability > Complexity
- No multi-agent swarms (controlled sub-agents only)

### 2. Real-Time Steering "h2A"

```rust
// Async dual-buffer queue for mid-task course correction
struct SteeringQueue {
    instructions: Vec<String>,      // New instructions
    constraints: Vec<String>,       // Additional constraints
    redirections: Vec<String>,      // Course corrections
}

// Allows injection while agent is running
fn inject_instruction(&mut self, instruction: String);
fn inject_context(&mut self, context: String);
```

### 3. Context Management

| Component | Trigger | Action |
|-----------|---------|--------|
| Compressor "wU2" | 92% context usage | Auto-summarize, move to Markdown files |
| Reminder Injection | After each tool use | Insert TODO state as system message |
| Sub-agents | Complex exploration | Isolated context, return summary only |

### 4. Tool Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  Discovery Tools                                             │
│  ├── View (file read, ~2000 lines default)                  │
│  ├── LS (directory listing)                                 │
│  ├── Glob (wildcard search)                                 │
│  └── GrepTool (regex, ripgrep-style - NO vector DB!)        │
├─────────────────────────────────────────────────────────────┤
│  Editing Tools                                               │
│  ├── Edit (surgical diffs)                                  │
│  ├── Write/Replace (whole file)                             │
│  └── Create (new files)                                     │
├─────────────────────────────────────────────────────────────┤
│  Execution Tools                                             │
│  ├── Bash (risk classification, confirmation prompts)       │
│  └── Sub-agent dispatch (depth-limited)                     │
└─────────────────────────────────────────────────────────────┘
```

### 5. Planning - TodoWrite

```json
{
  "id": "task-001",
  "content": "Implement user authentication",
  "status": "in_progress",
  "priority": 1,
  "dependencies": [],
  "notes": "Using JWT tokens"
}
```

- Renders as interactive checklist in UI
- Reminder injection after tool uses
- Prevents losing track of objectives

### 6. Agentic Loop Pattern

```
┌──────────────┐
│   GATHER     │ ← File system, agentic search, subagents
│   CONTEXT    │
└──────┬───────┘
       ↓
┌──────────────┐
│    TAKE      │ ← Tools, bash, code generation, MCPs
│   ACTION     │
└──────┬───────┘
       ↓
┌──────────────┐
│   VERIFY     │ ← Rules (lint), visual feedback, LLM judge
│    WORK      │
└──────┬───────┘
       ↓
       └──────────→ REPEAT

```

---

## Long-Running Agents (Multi-Context Window)

### The Problem

Each new context window starts with NO memory:
- Agent tries to "one-shot" complex tasks
- Half-implemented features left undocumented
- Agent declares victory prematurely

### Solution: Initializer + Coding Agent

#### Initializer Agent (First Session Only)

Sets up environment:
1. `init.sh` - Development server startup script
2. `claude-progress.txt` - Progress log file
3. `feature_list.json` - Comprehensive feature list (200+ items)
4. Initial git commit

#### Coding Agent (Every Subsequent Session)

```
1. pwd → Understand working directory
2. Read git logs + progress file → Get up to speed
3. Read feature_list.json → Choose highest-priority incomplete feature
4. Run init.sh → Start dev server
5. Basic end-to-end test → Verify nothing is broken
6. Work on ONE feature at a time
7. Test feature thoroughly
8. Git commit + progress update
9. Leave environment in clean state
```

### Feature List Format (JSON)

```json
{
  "category": "functional",
  "description": "New chat button creates a fresh conversation",
  "steps": [
    "Navigate to main interface",
    "Click the 'New Chat' button",
    "Verify a new conversation is created"
  ],
  "passes": false
}
```

**Rules:**
- Only change `passes` field
- NEVER delete or edit test descriptions
- Mark as passing ONLY after thorough testing

### Testing Strategy

1. **Unit tests** - Code-level verification
2. **Browser automation** (Puppeteer MCP) - End-to-end testing
3. **Screenshots** - Visual verification
4. **curl commands** - API testing

---

## ForgeCode Improvement Plan

### Current State ✅

- [x] Single-threaded master loop
- [x] Parallel tool execution (ExecutionPlanner)
- [x] Task/PTY routing
- [x] Context compression (92% threshold)
- [x] Steering system (pause/resume/stop/inject)
- [x] Hook system (before/after tool)

### Missing/Improvements 🔧

#### 1. TODO-Based Planning System

```rust
// Add TodoManager to Agent
struct TodoManager {
    tasks: Vec<TodoItem>,
    current: Option<TaskId>,
}

impl TodoManager {
    fn inject_as_reminder(&self) -> String;
    fn mark_complete(&mut self, id: &str);
}
```

#### 2. Progress File System

```rust
// Auto-generate progress updates
struct ProgressTracker {
    session_id: String,
    started_at: DateTime,
    features_completed: Vec<String>,
    current_task: Option<String>,
    git_commits: Vec<String>,
}
```

#### 3. Feature List Tracking

```rust
struct FeatureList {
    features: Vec<Feature>,
    path: PathBuf,  // feature_list.json
}

struct Feature {
    id: String,
    category: String,
    description: String,
    steps: Vec<String>,
    passes: bool,
}
```

#### 4. Session Start Routine

```rust
async fn session_start_routine(&self) -> Result<()> {
    // 1. Read progress file
    let progress = self.read_progress_file().await?;
    
    // 2. Read git log
    let git_log = self.execute_tool("bash", json!({"command": "git log --oneline -20"})).await?;
    
    // 3. Read feature list
    let features = self.read_feature_list().await?;
    
    // 4. Run init.sh if exists
    if self.init_script_exists() {
        self.execute_tool("bash", json!({"command": "./init.sh"})).await?;
    }
    
    // 5. Basic sanity test
    self.run_basic_tests().await?;
    
    // 6. Choose next task
    let next_task = features.next_incomplete();
    self.set_current_task(next_task);
    
    Ok(())
}
```

#### 5. Verification Tools

- Integrate Puppeteer/Playwright for browser testing
- Screenshot capture for visual verification
- Lint integration for code quality

#### 6. Sub-Agent Improvements

```rust
struct SubAgentConfig {
    max_depth: usize,           // Prevent infinite recursion
    isolated_context: bool,     // Separate context window
    return_summary_only: bool,  // Don't pollute parent context
}
```

---

## Performance Optimizations

### 1. Agentic Search > Vector DB

Claude Code uses regex (GrepTool) instead of vector embeddings:
- Model decides which search strategy to use
- More transparent and debuggable
- No embedding maintenance overhead

### 2. Reminder Injection

After each tool use, inject current state:
```
[System] Current TODO state:
- [ ] Task 1 (priority: high)
- [x] Task 2 (completed)
- [ ] Task 3 (blocked by Task 1)
```

### 3. Incremental Work

One feature at a time:
- Reduces context pollution
- Easier to test and verify
- Clean git history

### 4. Clean State Policy

Before ending session:
- All tests pass
- Code compiles
- No uncommitted changes
- Progress file updated
- Ready for next agent

---

## References

1. [Claude Agent SDK](https://docs.claude.com/en/api/agent-sdk/overview)
2. [Effective Harnesses for Long-Running Agents](https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents)
3. [Building Agents with Claude Agent SDK](https://www.anthropic.com/engineering/building-agents-with-the-claude-agent-sdk)
4. [ZenML - Claude Code Architecture Analysis](https://www.zenml.io/llmops-database/claude-code-agent-architecture-single-threaded-master-loop-for-autonomous-coding)
