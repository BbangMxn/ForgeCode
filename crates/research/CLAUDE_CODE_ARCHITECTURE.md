# Claude Code & OpenCode Architecture Analysis

## Overview

This document analyzes how Claude Code and OpenCode implement their AI coding assistant functionality.
The key insight: **These systems are NOT complex agent architectures with ReAct/CoT loops in code**.
Instead, they rely on:

1. **Well-crafted System Prompts** - LLM handles reasoning internally
2. **Tool Definitions with JSON Schema** - Structured tool interfaces
3. **Context Management** - Efficient context window usage
4. **Permission System** - Security and user control

---

## 1. System Prompt Architecture

### Claude Code Structure

Claude Code uses **modular, composable system prompts** (~110+ prompt files):

```
system-prompts/
├── system-prompt-main-system-prompt.md      # Core identity
├── system-prompt-tone-and-style.md          # Communication guidelines
├── system-prompt-tool-usage-policy.md       # Tool selection rules
├── system-prompt-doing-tasks.md             # Task execution guidelines
├── system-prompt-task-management.md         # TodoWrite usage
├── agent-prompt-task-tool.md                # Sub-agent system prompt
├── agent-prompt-explore.md                  # Explore agent prompt
├── agent-prompt-plan-mode-enhanced.md       # Plan mode prompt
├── tool-description-*.md                    # Per-tool descriptions
└── system-reminder-*.md                     # Context reminders (~40 types)
```

### Key Prompt Sections

#### 1.1 Main System Prompt
```markdown
You are an interactive CLI tool that helps users with software engineering tasks.
Use the instructions below and the tools available to you to assist the user.

${SECURITY_POLICY}
IMPORTANT: You must NEVER generate or guess URLs...
```
- Uses **template variables**: `${OUTPUT_STYLE_CONFIG}`, `${SECURITY_POLICY}`
- Conditional assembly based on context

#### 1.2 Tone and Style
```markdown
# Tone and style
- Only use emojis if the user explicitly requests it
- Your output will be displayed on a command line interface
- Output text to communicate with the user; all text you output outside of tool use is displayed
- NEVER create files unless absolutely necessary
- Do not use a colon before tool calls

# Professional objectivity
Prioritize technical accuracy and truthfulness over validating the user's beliefs...

# No time estimates
Never give time estimates or predictions...
```

#### 1.3 Tool Usage Policy
```markdown
# Tool usage policy
- You can call multiple tools in a single response
- If you intend to call multiple tools and there are no dependencies, make all independent calls in parallel
- Use specialized tools instead of bash commands when possible
- VERY IMPORTANT: When exploring the codebase, use the Task tool with subagent_type=Explore
```

#### 1.4 Doing Tasks
```markdown
# Doing tasks
- NEVER propose changes to code you haven't read
- Be careful not to introduce security vulnerabilities (OWASP top 10)
- Avoid over-engineering. Only make changes that are directly requested
  - Don't add features beyond what was asked
  - Don't add error handling for scenarios that can't happen
  - Don't create helpers or abstractions for one-time operations
- Avoid backwards-compatibility hacks
```

---

## 2. Sub-Agent Architecture

### Claude Code Sub-Agents

Sub-agents are **separate LLM calls with specialized system prompts**:

| Agent | Purpose | Tools Available |
|-------|---------|-----------------|
| **Explore** | Fast codebase search | Glob, Grep, Read, Bash (read-only) |
| **Plan** | Implementation planning | All read tools, no Write/Edit |
| **General** | Multi-step research | All tools except TodoWrite |
| **Compaction** | Context summarization | None (pure LLM) |
| **Title** | Session title generation | None |

### Explore Agent Prompt
```markdown
You are a file search specialist for Claude Code.
You excel at thoroughly navigating and exploring codebases.

=== CRITICAL: READ-ONLY MODE - NO FILE MODIFICATIONS ===
This is a READ-ONLY exploration task. You are STRICTLY PROHIBITED from:
- Creating new files
- Modifying existing files
- Deleting files
...

Your strengths:
- Rapidly finding files using glob patterns
- Searching code and text with powerful regex patterns
- Reading and analyzing file contents

NOTE: You are meant to be a fast agent that returns output as quickly as possible.
- Make efficient use of the tools at your disposal
- Wherever possible spawn multiple parallel tool calls
```

### Task Tool Description
```markdown
Launch a new agent to handle complex, multi-step tasks autonomously.

Available agent types:
- Bash: Command execution specialist
- general-purpose: Research and multi-step tasks
- Explore: Fast codebase exploration
- Plan: Implementation planning

Usage notes:
- Always include a short description (3-5 words)
- Launch multiple agents concurrently when possible
- The agent's outputs should generally be trusted
- Clearly tell the agent whether you expect it to write code or just research
```

---

## 3. Tool Definition Pattern

### Tool Description Format

Each tool has a dedicated description file with:
1. **Purpose** - What the tool does
2. **Usage notes** - When and how to use it
3. **Examples** - Good and bad patterns
4. **Warnings** - Common mistakes to avoid

### Example: Edit Tool
```markdown
Performs exact string replacements in files.

Usage:
- You must use your Read tool at least once before editing
- When editing text from Read tool output, preserve exact indentation
- ALWAYS prefer editing existing files. NEVER write new files unless required
- The edit will FAIL if `old_string` is not unique in the file
- Use `replace_all` for renaming strings across the file
```

### Example: Bash Tool
```markdown
Executes a given bash command with optional timeout.

IMPORTANT: This tool is for terminal operations like git, npm, docker, etc.
DO NOT use it for file operations - use specialized tools instead.

Before executing:
1. Directory Verification: Use `ls` to verify parent directory exists
2. Command Execution: Always quote file paths with spaces

Usage notes:
- Avoid using Bash with find, grep, cat, head, tail, sed, awk, or echo
  - File search: Use Glob (NOT find or ls)
  - Content search: Use Grep (NOT grep or rg)
  - Read files: Use Read (NOT cat/head/tail)
  - Edit files: Use Edit (NOT sed/awk)
  - Write files: Use Write (NOT echo >/cat <<EOF)
```

---

## 4. OpenCode Architecture

### Agent Types

```typescript
const agents = {
  build: {
    name: "build",
    description: "The default agent. Executes tools based on configured permissions.",
    mode: "primary",
    permission: { question: "allow", plan_enter: "allow" }
  },
  plan: {
    name: "plan", 
    description: "Plan mode. Disallows all edit tools.",
    mode: "primary",
    permission: { plan_exit: "allow", edit: { "*": "deny" } }
  },
  general: {
    name: "general",
    description: "General-purpose agent for research and multi-step tasks.",
    mode: "subagent"
  },
  explore: {
    name: "explore",
    description: "Fast agent specialized for exploring codebases.",
    mode: "subagent",
    prompt: PROMPT_EXPLORE,
    permission: { "*": "deny", grep: "allow", glob: "allow", read: "allow" }
  }
}
```

### Tool Registry Pattern

```typescript
// Tools are loaded from .txt files for descriptions
import PROMPT_BASH from "./bash.txt"
import PROMPT_EDIT from "./edit.txt"
import PROMPT_READ from "./read.txt"

// Tool definitions with JSON schema
const tools = {
  read: {
    name: "read",
    description: PROMPT_READ,
    parameters: z.object({
      file_path: z.string().describe("Absolute path to the file"),
      offset: z.number().optional().describe("Line number to start from"),
      limit: z.number().optional().describe("Number of lines to read")
    })
  }
}
```

### Permission System

```typescript
const defaultPermissions = {
  "*": "allow",
  doom_loop: "ask",
  external_directory: { "*": "ask" },
  question: "deny",
  plan_enter: "deny",
  read: {
    "*": "allow",
    "*.env": "ask",
    "*.env.*": "ask"
  }
}
```

---

## 5. Context Management

### System Reminders (~40 types)

Claude Code injects contextual reminders based on state:

```markdown
<!-- File modified by user -->
<system-reminder>
The file {path} was modified by the user or linter since you last read it.
You should re-read the file before making further edits.
</system-reminder>

<!-- Plan mode active -->
<system-reminder>
Plan mode is active. You are in planning phase.
Use the ExitPlanMode tool when your plan is complete.
</system-reminder>

<!-- Token usage warning -->
<system-reminder>
You have used {used} tokens out of {limit}.
Consider using the compaction feature if context is running low.
</system-reminder>
```

### Context Compaction

When context window fills up:
1. Summarize conversation history
2. Preserve critical information (file paths, decisions)
3. Remove redundant tool call details

---

## 6. Key Implementation Insights

### What Claude Code Does NOT Do

1. **No ReAct/CoT loops in code** - LLM handles reasoning internally
2. **No complex agent state machines** - Simple request-response with tool calls
3. **No explicit planning phases** - LLM decides when to plan
4. **No memory databases** - Context window is the memory

### What Claude Code DOES Do

1. **Modular system prompts** - Composed based on context
2. **Specialized sub-agents** - Different tool sets for different tasks
3. **Rich tool descriptions** - Detailed usage guidelines
4. **Permission system** - User control over tool execution
5. **Context reminders** - State-aware hints to LLM

### The Simple Loop

```
while not done:
    1. Compose system prompt (base + context-specific sections)
    2. Add available tools based on permissions
    3. Send to LLM with conversation history
    4. LLM returns text + tool calls
    5. Execute tool calls
    6. Add results to conversation
    7. Repeat
```

---

## 7. Recommendations for ForgeCode

### System Prompt Design

1. **Modular structure** - Separate files for each concern
2. **Template variables** - Dynamic composition
3. **Clear tool guidance** - When to use each tool
4. **Anti-patterns** - What NOT to do

### Tool Definitions

1. **Detailed descriptions** - Include usage notes, examples
2. **JSON Schema parameters** - Structured inputs
3. **Clear error messages** - Help LLM recover

### Sub-Agent System

1. **Specialized prompts** - Focus on specific tasks
2. **Limited tool sets** - Only what's needed
3. **Fast execution** - Parallel tool calls

### Permission System

1. **Glob patterns** - Flexible matching
2. **Action types** - allow, deny, ask
3. **Per-agent config** - Different permissions per agent

---

## 8. File Structure Recommendation

```
crates/Layer3-agent/
├── prompts/
│   ├── system/
│   │   ├── main.md
│   │   ├── tone_and_style.md
│   │   ├── tool_usage_policy.md
│   │   └── doing_tasks.md
│   ├── agents/
│   │   ├── explore.md
│   │   ├── plan.md
│   │   └── general.md
│   ├── tools/
│   │   ├── read.md
│   │   ├── edit.md
│   │   ├── bash.md
│   │   └── ...
│   └── reminders/
│       ├── file_modified.md
│       ├── plan_mode_active.md
│       └── ...
├── src/
│   ├── prompt/
│   │   ├── composer.rs      # Assembles system prompt
│   │   ├── template.rs      # Variable substitution
│   │   └── loader.rs        # Loads .md files
│   ├── agent/
│   │   ├── executor.rs      # Main loop
│   │   ├── subagent.rs      # Sub-agent spawning
│   │   └── registry.rs      # Agent definitions
│   ├── tool/
│   │   ├── registry.rs      # Tool definitions
│   │   └── permission.rs    # Permission checks
│   └── context/
│       ├── manager.rs       # Context window management
│       ├── reminder.rs      # System reminder injection
│       └── compaction.rs    # Context summarization
```

---

## References

- [Piebald-AI/claude-code-system-prompts](https://github.com/Piebald-AI/claude-code-system-prompts)
- [sst/opencode](https://github.com/sst/opencode)
- Claude Code v2.1.31 (analyzed version)
