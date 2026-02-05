//! ForgeCode TUI Widgets
//!
//! Claude Code 스타일의 모던한 TUI 위젯 모음

pub mod chat_view;
pub mod code_block;
pub mod header;
pub mod input_area;
pub mod status_bar;
pub mod welcome;

// Re-exports
pub use chat_view::{ChatMessage, ChatView, ChatViewState, MessageRole, ToolBlock, ToolExecutionState};
pub use header::{AgentStatus, Header, HeaderState, SpinnerState};
pub use input_area::{InputArea, InputState};
pub use status_bar::{StatusBar, StatusBarState};
pub use welcome::WelcomeScreen;
