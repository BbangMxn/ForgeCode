//! Tool System - Agent가 사용하는 도구 시스템
//!
//! Layer1의 Tool trait을 구현하고, ToolRegistry로 도구를 관리합니다.
//!
//! ## 아키텍처
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │  ToolRegistry                                                │
//! │  ├── register(tool) - 도구 등록                              │
//! │  ├── get(name) - 도구 조회                                   │
//! │  └── schemas() - MCP 호환 스키마                             │
//! ├─────────────────────────────────────────────────────────────┤
//! │  RuntimeContext (ToolContext 구현)                           │
//! │  ├── check_permission() - 권한 검사                          │
//! │  ├── request_permission() - 권한 요청                        │
//! │  └── shell_config() - Shell 설정                             │
//! ├─────────────────────────────────────────────────────────────┤
//! │  Builtin Tools (Tool trait 구현)                             │
//! │  ├── ReadTool - 파일 읽기                                    │
//! │  ├── WriteTool - 파일 쓰기                                   │
//! │  ├── EditTool - 파일 편집                                    │
//! │  ├── GlobTool - 패턴 검색                                    │
//! │  ├── GrepTool - 내용 검색                                    │
//! │  └── BashTool - Shell 실행                                   │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## 사용법
//!
//! ```ignore
//! use forge_core::tool::{ToolRegistry, RuntimeContext};
//! use forge_foundation::PermissionService;
//! use std::sync::Arc;
//!
//! // 레지스트리 생성
//! let registry = ToolRegistry::with_builtins();
//!
//! // 컨텍스트 생성
//! let permissions = Arc::new(PermissionService::new());
//! let ctx = RuntimeContext::new("session-1", PathBuf::from("."), permissions);
//!
//! // 도구 실행
//! if let Some(tool) = registry.get("read") {
//!     let result = tool.execute(input, &ctx).await?;
//! }
//! ```

pub mod builtin;
mod context;
mod registry;
pub mod security;

// Re-exports: Tool trait from Layer1
pub use forge_foundation::{Tool, ToolContext};

// Re-exports: Tools
pub use builtin::{
    all_tools, core_tools, filesystem_tools, BashTool, EditTool, GlobTool, GrepTool, ReadTool,
    WriteTool,
};

// Re-exports: Context
pub use context::{DefaultShellConfig, RuntimeContext};

// Re-exports: Registry
pub use registry::{
    ParallelExecutionConfig, ParallelExecutionStats, ParallelToolCall, ToolDefinition,
    ToolExecuteResult, ToolParameters, ToolRegistry,
};

// Re-exports: Security
pub use security::{is_safe_extension, is_sensitive_path, PathValidation, PathValidator};
