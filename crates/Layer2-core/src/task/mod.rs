//! Task System - 독립 실행 컨테이너
//!
//! Agent와 분리된 독립 실행 환경을 제공합니다.
//! 서버, 장시간 프로세스 등을 관리합니다.
//!
//! ## 특징
//! - 독립 PTY 쉘 세션
//! - 프로세스 생명주기 관리
//! - 출력 캡처 및 스트리밍
//! - Agent 대화와 독립적 실행
//!
//! ## 사용 예시
//! - `npm start` - 개발 서버
//! - `docker-compose up` - 컨테이너 실행
//! - `cargo watch` - 파일 감시 빌드

mod container;
mod executor;
mod tracker;

pub use container::{TaskContainer, TaskContainerId};
pub use executor::TaskExecutor;
pub use tracker::{TaskInfo, TaskStatus, TaskTracker};

/// Task 도구 (Agent가 호출)
pub mod tool;

/// Task 시스템 초기화
pub fn init() -> TaskExecutor {
    TaskExecutor::new()
}
