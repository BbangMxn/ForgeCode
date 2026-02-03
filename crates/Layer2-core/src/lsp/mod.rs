//! LSP Integration - 경량 Language Server Protocol 통합
//!
//! AI 코딩 어시스턴트를 위한 최소 LSP 클라이언트 구현
//!
//! ## 설계 철학
//!
//! 1. **경량화**: lsp-types 의존성 없이 필요한 타입만 직접 정의
//! 2. **Lazy Loading**: Agent가 실제로 요청할 때만 LSP 서버 시작
//! 3. **최소 기능**: 핵심 기능만 구현 (definition, references, hover)
//! 4. **자동 정리**: 일정 시간 미사용 시 서버 자동 종료
//!
//! ## 핵심 기능 (Agent 필수)
//!
//! - `textDocument/definition` - 정의로 이동
//! - `textDocument/references` - 참조 찾기
//! - `textDocument/hover` - 심볼 정보
//!
//! ## 지원 언어
//!
//! - Rust (rust-analyzer)
//! - TypeScript/JavaScript (typescript-language-server)
//! - Python (pylsp)
//! - Go (gopls)
//!
//! ## 사용 예시
//!
//! ```rust,ignore
//! use forge_core::lsp::{LspManager, Position};
//!
//! let manager = LspManager::new();
//!
//! // 파일에 대한 클라이언트 가져오기 (자동으로 서버 시작)
//! let client = manager.get_for_file(Path::new("src/main.rs")).await?;
//!
//! // 정의로 이동
//! let locations = client.goto_definition(
//!     "file:///path/to/file.rs",
//!     Position::new(10, 5)
//! ).await?;
//!
//! // 참조 찾기
//! let refs = client.find_references(
//!     "file:///path/to/file.rs",
//!     Position::new(10, 5)
//! ).await?;
//!
//! // 호버 정보
//! if let Some(hover) = client.hover(
//!     "file:///path/to/file.rs",
//!     Position::new(10, 5)
//! ).await? {
//!     println!("Type: {}", hover.to_string());
//! }
//! ```
//!
//! ## 효율성 특징
//!
//! - **On-demand**: LSP 서버는 필요할 때만 시작
//! - **Idle timeout**: 10분 미사용 시 자동 종료
//! - **Availability cache**: 서버 설치 여부 5분 캐싱
//! - **No diagnostics**: 실시간 진단 스트림 없음 (Agent에 불필요)

mod client;
mod manager;
mod types;

pub use client::{LspClient, LspClientState};
pub use manager::LspManager;
pub use types::*;

/// LSP 매니저 팩토리 함수
pub fn create_lsp_manager() -> LspManager {
    LspManager::new()
}

/// LSP 비활성화 매니저 생성 (성능 모드)
pub fn create_disabled_lsp_manager() -> LspManager {
    LspManager::disabled()
}
