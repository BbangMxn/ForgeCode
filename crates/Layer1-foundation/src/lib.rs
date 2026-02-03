//! # forge-foundation
//!
//! Foundation layer for ForgeCode:
//! - Permission: 권한 관리 (동적 등록 + 런타임 + 보안 분석)
//! - Registry: MCP, Provider, Model 등록
//! - Storage: SQLite (런타임), JsonStore (범용)
//! - Config: 통합 설정 (Limits 등)

pub mod config;
pub mod error;
pub mod permission;
pub mod registry;
pub mod storage;

// Error
pub use error::{Error, Result};

// Config
pub use config::{DailyLimits, LimitsConfig, MonthlyLimits, SessionLimits};

// Permission
pub use permission::{
    categories as permission_categories,
    // Security (명령어/경로 분석)
    command_analyzer,
    dangerous_commands,
    path_analyzer,
    register as register_permission,
    register_all as register_permissions,
    registry as permission_registry,
    sensitive_paths,
    CommandAnalysis,
    CommandAnalyzer,
    CommandRisk,
    PathAnalyzer,
    // Runtime
    Permission,
    PermissionAction,
    // Settings (JSON 저장)
    PermissionActionType,
    // Types (동적 등록)
    PermissionDef,
    PermissionDeny,
    PermissionGrant,
    PermissionRegistry,
    PermissionScope,
    PermissionService,
    PermissionSettings,
    PermissionStatus,
    SensitivePath,
    PERMISSIONS_FILE,
};

// Registry
pub use registry::{
    // Model
    model_registry,
    // MCP
    McpConfig,
    McpConfigFile,
    McpServer,
    McpTransport,
    ModelCapabilities,
    ModelInfo,
    ModelPricing,
    ModelRegistry,
    // Provider
    Provider,
    ProviderConfig,
    ProviderType,
    MCP_FILE,
    PROVIDERS_FILE,
};

// Storage
pub use storage::{
    // JSON (범용)
    JsonStore,
    // SQLite (런타임 데이터)
    MessageRecord,
    SessionRecord,
    Storage,
    TokenUsageRecord,
    ToolExecutionRecord,
    UsageSummary,
};
