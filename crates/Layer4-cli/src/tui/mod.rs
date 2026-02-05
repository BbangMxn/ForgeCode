//! TUI (Terminal User Interface) module
//!
//! ForgeCode의 터미널 UI 시스템
//!
//! ## 아키텍처
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                        forge_app                            │
//! │  ┌─────────┐  ┌─────────────┐  ┌───────────┐  ┌──────────┐ │
//! │  │ Header  │  │  ChatView   │  │ InputArea │  │StatusBar │ │
//! │  └─────────┘  └─────────────┘  └───────────┘  └──────────┘ │
//! │                      ↓                                      │
//! │              ┌─────────────┐                                │
//! │              │   widgets   │ ← 재사용 가능한 위젯           │
//! │              └─────────────┘                                │
//! │                      ↓                                      │
//! │              ┌─────────────┐                                │
//! │              │    theme    │ ← 테마/스타일 시스템           │
//! │              └─────────────┘                                │
//! └─────────────────────────────────────────────────────────────┘
//! ```

// 기존 모듈
mod app;
mod components;
mod event;
mod pages;

// 새로운 Claude Code 스타일 모듈
pub mod forge_app;
pub mod theme;
pub mod widgets;

// Re-exports
pub use app::run;
pub use forge_app::HelpOverlay;
pub use theme::{current_theme, Theme};
