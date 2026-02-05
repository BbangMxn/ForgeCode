//! Permission system for ForgeCode
//!
//! - `types`: 권한 타입 동적 등록 (PermissionDef, PermissionRegistry)
//! - `service`: 런타임 권한 관리 (PermissionService)
//! - `settings`: JSON 설정 저장/로드 (PermissionSettings)
//! - `security`: 위험 명령어/민감 경로 분석 (CommandAnalyzer, PathAnalyzer)
//! - `oversight`: 다중 에이전트 보안 감독 (OversightAgent)
//!
//! ## 사용 예시
//!
//! ```rust,ignore
//! // forge-tool에서 권한 등록
//! use forge_foundation::permission::{register, PermissionDef, categories};
//!
//! register(
//!     PermissionDef::new("bash.execute", categories::EXECUTE)
//!         .risk_level(9)
//!         .description("Execute shell command")
//! );
//!
//! // 권한 확인
//! let service = PermissionService::load()?;
//! if service.is_permitted("bash", &action) {
//!     // 실행
//! }
//!
//! // 명령어 분석
//! use forge_foundation::permission::security::{analyzer, CommandRisk};
//! let result = analyzer().analyze("rm -rf /");
//! if result.risk == CommandRisk::Forbidden {
//!     // 차단
//! }
//! ```

pub mod oversight;
pub mod security;
mod service;
mod settings;
mod types;

// Service (런타임 권한 관리)
pub use service::{
    Permission, PermissionAction, PermissionScope, PermissionService, PermissionStatus,
};

// Settings (JSON 저장/로드)
pub use settings::{
    PermissionActionType, PermissionDeny, PermissionGrant, PermissionSettings, PERMISSIONS_FILE,
};

// Types (동적 권한 등록)
pub use types::{
    // 표준 카테고리
    categories,
    // 유틸리티 (deprecated, use security module instead)
    dangerous_commands,
    // 전역 레지스트리 접근
    register,
    register_all,
    registry,
    sensitive_paths,
    // 권한 정의
    PermissionDef,
    PermissionRegistry,
};

// Security (명령어/경로 분석) - 주요 타입 재export
pub use security::{
    analyzer as command_analyzer, path_analyzer, CommandAnalysis, CommandAnalyzer, CommandRisk,
    PathAnalyzer, SensitivePath,
};

// Oversight (다중 에이전트 보안 감독)
pub use oversight::{
    AuditEntry, OversightAgent, OversightConfig, OversightStats, RiskPattern,
    SourceAnalyzer, SourceContext, SourceTag, ToolCallRequest, ValidationResult,
};
