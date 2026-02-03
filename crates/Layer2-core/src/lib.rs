//! forge-core: Core Runtime for ForgeCode
//!
//! Layer2 - Agent 도구 구현 레이어
//!
//! # 현재 구현된 모듈
//!
//! - `lsp`: 경량 LSP (Language Server Protocol) 연동
//!   - Lazy Loading: 요청 시에만 서버 시작
//!   - 최소 기능: definition, references, hover
//!   - 자동 정리: 10분 미사용 시 종료
//!
//! - `tool`: Tool 시스템 및 Builtin 도구들
//!   - Layer1 Tool trait 구현
//!   - ToolRegistry로 도구 관리
//!   - RuntimeContext로 실행 환경 제공
//!   - 6개 핵심 도구: read, write, edit, glob, grep, bash
//!
//! # TODO: 구현 예정
//!
//! - `task`: 독립 실행 컨테이너
//! - `mcp`: MCP 브릿지

// 현재 구현된 모듈
pub mod lsp;
pub mod tool;

// TODO: 추후 구현
// pub mod task;
// pub mod mcp;

// Re-exports: LSP
pub use lsp::{create_disabled_lsp_manager, create_lsp_manager, LspClient, LspClientState, LspManager};
pub use lsp::{
    DocumentSymbol, Hover, Location, LspServerConfig, Position, Range, SymbolKind,
    default_lsp_configs, path_to_uri, uri_to_path,
};

// Re-exports: Tool
pub use tool::{
    // Tools
    BashTool, EditTool, GlobTool, GrepTool, ReadTool, WriteTool,
    // Context
    DefaultShellConfig, RuntimeContext,
    // Registry
    ToolRegistry,
    // Functions
    all_tools, core_tools, filesystem_tools,
};

// Layer1 re-exports
pub use forge_foundation::{Error, Result};

/// Layer2 버전
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
    }

    #[test]
    fn test_lsp_exports() {
        // LSP 타입 export 확인
        let pos = Position::new(10, 5);
        assert_eq!(pos.line, 10);
        assert_eq!(pos.character, 5);
    }

    #[test]
    fn test_tool_exports() {
        // Tool registry 확인
        let registry = ToolRegistry::with_builtins();
        assert!(!registry.is_empty());
        assert!(registry.contains("read"));
        assert!(registry.contains("write"));
        assert!(registry.contains("edit"));
        assert!(registry.contains("glob"));
        assert!(registry.contains("grep"));
        assert!(registry.contains("bash"));
    }

    #[test]
    fn test_all_tools_count() {
        let tools = all_tools();
        assert_eq!(tools.len(), 6);
    }
}
