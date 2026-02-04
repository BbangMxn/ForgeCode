//! Repository Map - AST 기반 코드베이스 구조 요약
//!
//! Aider의 Repo Map과 유사한 기능으로, 코드베이스의 구조를 AST 기반으로
//! 분석하여 LLM에게 효율적인 컨텍스트를 제공합니다.
//!
//! ## 기능
//! - 파일 구조 분석 (클래스, 함수, 모듈 등)
//! - 심볼 추출 및 의존성 그래프
//! - 관련 파일 추천 (PageRank 기반)
//! - 토큰 예산 내에서 최적화된 맵 생성
//!
//! ## 지원 언어
//! - Rust, Python, JavaScript/TypeScript, Go, Java, C/C++

mod analyzer;
mod graph;
mod ranker;
mod types;

pub use analyzer::RepoAnalyzer;
pub use graph::DependencyGraph;
pub use ranker::FileRanker;
pub use types::{FileInfo, RepoMap, RepoMapConfig, SymbolDef, SymbolKind, SymbolRef, SymbolUsage};
