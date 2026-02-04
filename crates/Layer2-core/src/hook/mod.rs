//! # Hook System
//!
//! Claude Code 호환 Hook 시스템 구현
//!
//! ## 개요
//!
//! Hook은 특정 이벤트 발생 시 자동으로 실행되는 액션입니다.
//! Claude Code와 동일한 이벤트 타입과 액션 형식을 지원합니다.
//!
//! ## 이벤트 타입
//!
//! - `PreToolUse`: Tool 실행 전 (블로킹 가능)
//! - `PostToolUse`: Tool 실행 후
//! - `SessionStart`: 세션 시작 시
//! - `SessionStop`: 세션 종료 시
//! - `PromptSubmit`: 프롬프트 제출 시
//!
//! ## 액션 타입
//!
//! - `command`: Shell 명령어 실행
//! - `prompt`: LLM에게 프롬프트 전달
//! - `agent`: Subagent 실행
//!
//! ## 예시
//!
//! ```ignore
//! // hooks.json 형식
//! {
//!   "PreToolUse": [{
//!     "matcher": "Bash",
//!     "hooks": [{
//!       "type": "command",
//!       "command": "echo 'Running bash command'",
//!       "timeout": 5
//!     }]
//!   }]
//! }
//! ```

mod executor;
mod loader;
mod types;

pub use executor::{
    AgentCallback, AgentRequest, AgentResponse, AgentResult, HookActionHandlers, HookContext,
    HookEventSource, HookExecutor, PromptCallback, PromptRequest, PromptResponse,
};
pub use loader::{load_hooks_from_dir, load_hooks_from_file, HookLoader};
pub use types::{
    BlockReason, HookAction, HookConfig, HookEvent, HookEventType, HookMatcher, HookOutcome,
    HookResult,
};
