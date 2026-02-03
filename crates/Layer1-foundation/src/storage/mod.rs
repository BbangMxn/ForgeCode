//! Storage module for ForgeCode
//!
//! - `db`: SQLite - 런타임 데이터 (세션, 메시지, 토큰 사용량)
//! - `json`: JSON - 범용 파일 저장/로드

mod db;
mod json;

// SQLite Storage (런타임 데이터)
pub use db::{
    MessageRecord, SessionRecord, Storage, TokenUsageRecord, ToolExecutionRecord, UsageSummary,
};

// JSON Storage (범용)
pub use json::JsonStore;
