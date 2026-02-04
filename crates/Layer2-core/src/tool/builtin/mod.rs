//! Builtin Tools - 내장 도구들
//!
//! Agent가 사용하는 핵심 도구 구현
//!
//! ## 도구 목록
//!
//! ### 파일시스템 (Filesystem)
//! - `read` - 파일 읽기 (줄 번호 포함)
//! - `write` - 파일 쓰기 (생성 또는 덮어쓰기)
//! - `edit` - 파일 편집 (문자열 치환)
//! - `glob` - 파일 패턴 검색
//! - `grep` - 내용 검색 (정규식)
//!
//! ### 실행 (Execute)
//! - `bash` - Shell 명령 실행
//!
//! ### 웹 (Web)
//! - `web_search` - 웹 검색 (Brave, DuckDuckGo, Google, Tavily)
//! - `web_fetch` - URL 콘텐츠 가져오기 (HTML → Markdown 변환)
//!
//! ## Layer1 연동
//! - 모든 도구는 `forge_foundation::Tool` trait 구현
//! - `required_permission()`으로 권한 요청
//! - `PermissionService`와 연동
//! - `CommandAnalyzer`로 위험 명령어 분석

// Filesystem tools
pub mod edit;
pub mod glob;
pub mod grep;
pub mod read;
pub mod write;

// Execute tools
pub mod bash;

// Web tools (temporarily disabled due to API mismatch)
// TODO: Fix web_fetch and web_search to match Layer1 Tool trait
// pub mod web_fetch;
// pub mod web_search;

// Re-exports
pub use bash::BashTool;
pub use edit::EditTool;
pub use glob::GlobTool;
pub use grep::GrepTool;
pub use read::ReadTool;
// pub use web_fetch::WebFetchTool;
// pub use web_search::WebSearchTool;
pub use write::WriteTool;

use forge_foundation::Tool;
use std::sync::Arc;

/// 모든 builtin 도구 인스턴스 생성
pub fn all_tools() -> Vec<Arc<dyn Tool>> {
    vec![
        // Filesystem
        Arc::new(ReadTool::new()) as Arc<dyn Tool>,
        Arc::new(WriteTool::new()),
        Arc::new(EditTool::new()),
        Arc::new(GlobTool::new()),
        Arc::new(GrepTool::new()),
        // Execute
        Arc::new(BashTool::new()),
        // Web (temporarily disabled)
        // Arc::new(WebSearchTool::new()),
        // Arc::new(WebFetchTool::new()),
    ]
}

/// 핵심 도구만 반환 (빠른 시작용)
///
/// Read, Write, Bash만 포함 - 최소한의 기능
pub fn core_tools() -> Vec<Arc<dyn Tool>> {
    vec![
        Arc::new(ReadTool::new()) as Arc<dyn Tool>,
        Arc::new(WriteTool::new()),
        Arc::new(BashTool::new()),
    ]
}

/// 파일시스템 도구만 반환
pub fn filesystem_tools() -> Vec<Arc<dyn Tool>> {
    vec![
        Arc::new(ReadTool::new()) as Arc<dyn Tool>,
        Arc::new(WriteTool::new()),
        Arc::new(EditTool::new()),
        Arc::new(GlobTool::new()),
        Arc::new(GrepTool::new()),
    ]
}

// ============================================================================
// 테스트
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_tools() {
        let tools = all_tools();
        // 6 tools (web_search and web_fetch temporarily disabled)
        assert_eq!(tools.len(), 6);

        let names: Vec<_> = tools.iter().map(|t| t.name()).collect();
        assert!(names.contains(&"read"));
        assert!(names.contains(&"write"));
        assert!(names.contains(&"edit"));
        assert!(names.contains(&"glob"));
        assert!(names.contains(&"grep"));
        assert!(names.contains(&"bash"));
        // Temporarily disabled
        // assert!(names.contains(&"web_search"));
        // assert!(names.contains(&"web_fetch"));
    }

    #[test]
    fn test_core_tools() {
        let tools = core_tools();
        assert_eq!(tools.len(), 3);

        let names: Vec<_> = tools.iter().map(|t| t.name()).collect();
        assert!(names.contains(&"read"));
        assert!(names.contains(&"write"));
        assert!(names.contains(&"bash"));
    }

    #[test]
    fn test_filesystem_tools() {
        let tools = filesystem_tools();
        assert_eq!(tools.len(), 5);

        // bash는 filesystem_tools에 없어야 함
        let names: Vec<_> = tools.iter().map(|t| t.name()).collect();
        assert!(!names.contains(&"bash"));
    }

    #[test]
    fn test_all_tools_have_schemas() {
        let tools = all_tools();
        for tool in tools {
            let schema = tool.schema();
            assert!(
                schema.get("type").is_some(),
                "Tool {} missing type in schema",
                tool.name()
            );
            assert!(
                schema.get("properties").is_some(),
                "Tool {} missing properties in schema",
                tool.name()
            );
        }
    }

    #[test]
    fn test_all_tools_have_meta() {
        let tools = all_tools();
        for tool in tools {
            let meta = tool.meta();
            assert!(!meta.name.is_empty(), "Tool has empty name");
            assert!(
                !meta.category.is_empty(),
                "Tool {} has empty category",
                meta.name
            );
        }
    }
}
