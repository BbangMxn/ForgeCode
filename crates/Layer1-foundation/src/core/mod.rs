//! Core Module - 핵심 인터페이스 및 타입
//!
//! ForgeCode의 핵심 아키텍처를 정의합니다.
//!
//! ## 설계 철학
//!
//! 1. **통합 권한 시스템**: MCP 도구든 내장 도구든 동일한 Permission 시스템 사용
//! 2. **계층화된 설계**: macOS TCC 스타일로 플러그인이 권한을 등록
//! 3. **유연한 Shell 지원**: bash, powershell, cmd 등 다양한 쉘 지원
//!
//! ## 타입 계층
//!
//! - `types.rs`: 데이터 타입 (Message, ToolCall, TokenUsage 등)
//! - `traits.rs`: 인터페이스 (Tool, Provider, Task 등)
//!
//! ## 도구 유형
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                     Tool Registry                           │
//! │  ┌─────────────────┐  ┌─────────────────┐                   │
//! │  │  Built-in Tools │  │   MCP Tools     │                   │
//! │  │  ├── Bash       │  │  ├── Notion     │                   │
//! │  │  ├── Read       │  │  ├── Chrome     │                   │
//! │  │  ├── Write      │  │  ├── GitHub     │                   │
//! │  │  ├── Edit       │  │  └── Custom...  │                   │
//! │  │  └── Glob/Grep  │  │                 │                   │
//! │  └────────┬────────┘  └────────┬────────┘                   │
//! │           │                    │                            │
//! │           └────────┬───────────┘                            │
//! │                    ▼                                        │
//! │           ┌────────────────┐                                │
//! │           │ Permission     │ ← 통합 권한 시스템              │
//! │           │ System         │                                │
//! │           └────────────────┘                                │
//! │                    │                                        │
//! │           ┌────────┴────────┐                               │
//! │           ▼                 ▼                               │
//! │  ┌─────────────────┐  ┌─────────────────┐                   │
//! │  │  Shell Executor │  │  MCP Transport  │                   │
//! │  │  (cmd, bash...) │  │  (stdio, sse)   │                   │
//! │  └─────────────────┘  └─────────────────┘                   │
//! └─────────────────────────────────────────────────────────────┘
//! ```

pub mod traits;
pub mod types;

// ============================================================================
// Types - 데이터 타입 (types.rs)
// ============================================================================

// Message & Role
pub use types::{Message, MessageRole};

// Tool Call
pub use types::ToolCall;

// Tool Result Message (LLM 메시지용)
pub use types::ToolResultMessage;

// Token & Stream
pub use types::{StreamEvent, TokenUsage};

// Execution Context
pub use types::{ExecutionEnv, ToolSource};

// Permission Rules
pub use types::{PermissionRule, PermissionRuleAction};

// Session & Model
pub use types::{ModelHint, SessionInfo};

// ============================================================================
// Traits - 인터페이스 (traits.rs)
// ============================================================================

// Tool trait & related
pub use traits::{Tool, ToolContext, ToolExecutionResult, ToolMeta};

// ToolResult alias (traits::ToolExecutionResult의 별칭)
pub use traits::ToolResult;

// Provider trait & related
pub use traits::{
    ChatMessage, ChatRequest, ChatResponse, Configurable, Provider, ProviderMeta,
};

// Task trait & related
pub use traits::{Task, TaskArtifact, TaskContext, TaskMeta, TaskObserver, TaskResult, TaskState};

// Shell configuration
pub use traits::{ShellConfig, ShellType};

// Permission delegation
pub use traits::{PermissionDelegate, PermissionResponse};

// Backward compatibility aliases
pub use traits::ToolResult as ToolExecResult;
#[allow(deprecated)]
pub use types::ToolResultMsg;
