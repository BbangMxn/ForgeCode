//! Message types for LLM communication
//!
//! 이 모듈은 Layer1-foundation의 핵심 타입들을 re-export합니다.
//! Layer1의 types.rs에서 정의된 타입들이 ForgeCode 전체의 표준입니다.

// Re-export all message types from Layer1-foundation
pub use forge_foundation::{Message, MessageRole, ToolCall, ToolResultMessage};

// ToolResult는 traits.rs의 ToolExecutionResult alias입니다 (도구 실행 결과용)
// LLM 메시지의 도구 결과는 ToolResultMessage를 사용하세요.
pub use forge_foundation::ToolResultMessage as ToolResult;

// NOTE: Message, MessageRole, ToolCall은 forge_foundation::core::types에서 정의됩니다.
// ToolResultMessage는 LLM 메시지에서 도구 결과를 담는 타입입니다.
