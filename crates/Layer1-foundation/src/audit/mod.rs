//! Audit System - 감사 로깅 시스템
//!
//! 권한 요청, 도구 실행, 에러 등 중요 이벤트를 기록합니다.
//!
//! ## 아키텍처
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                      AuditLogger                             │
//! │  ┌─────────────────────────────────────────────────────┐    │
//! │  │  log(entry) ──► SQLite DB                           │    │
//! │  └─────────────────────────────────────────────────────┘    │
//! │         ▲                                                   │
//! │         │                                                   │
//! │  ┌──────────────┐                                           │
//! │  │EventListener │ ◄── EventBus (자동 연동)                   │
//! │  └──────────────┘                                           │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## 사용법
//!
//! ```ignore
//! use forge_foundation::audit::{
//!     AuditLogger, AuditEntry, AuditAction, AuditResult, AuditQuery,
//! };
//!
//! // 1. 로거 생성
//! let logger = AuditLogger::new()?;
//!
//! // 2. 감사 로그 기록
//! let entry = AuditEntry::new(AuditAction::ToolSucceeded, "read")
//!     .with_result(AuditResult::Success)
//!     .with_target("/path/to/file")
//!     .with_duration(150);
//!
//! logger.log(entry).await?;
//!
//! // 3. 조회
//! let query = AuditQuery::new()
//!     .with_actions(vec![AuditAction::ToolFailed])
//!     .with_min_risk(5)
//!     .with_limit(100);
//!
//! let entries = logger.query(&query).await?;
//!
//! // 4. 통계
//! let stats = logger.statistics().await?;
//! println!("Total entries: {}", stats.total_entries);
//!
//! // 5. EventBus 연동 (자동 감사 로깅)
//! use forge_foundation::event::global_event_bus;
//! AuditEventListener::register(Arc::new(logger), &global_event_bus()).await;
//! ```
//!
//! ## 감사 대상 이벤트
//!
//! | 카테고리 | 이벤트 | 위험도 |
//! |---------|--------|-------|
//! | Permission | 권한 요청/승인/거부 | 1-3 |
//! | Tool | 도구 시작/성공/실패 | 0-4 |
//! | File | 읽기/쓰기/삭제 | 1-7 |
//! | Command | 실행/차단 | 6-8 |
//! | Session | 시작/종료 | 0 |
//! | Error | 에러 발생 | 5 |

pub mod logger;
pub mod types;

// Re-exports
pub use logger::{AuditEventListener, AuditLogger, AuditLoggerConfig};
pub use types::{AuditAction, AuditEntry, AuditId, AuditQuery, AuditResult, AuditStatistics};
