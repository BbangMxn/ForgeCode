//! # forge-foundation
//!
//! Foundation layer for ForgeCode:
//! - Permission: 권한 관리 (동적 등록 + 런타임)
//! - Registry: MCP, Provider 등록 (각각 독립 로드/저장)
//! - Storage: SQLite (런타임), JsonStore (범용)

pub mod error;
pub mod permission;
pub mod registry;
pub mod storage;

// Error
pub use error::{Error, Result};

// Permission
pub use permission::{
    categories as permission_categories,
    dangerous_commands,
    register as register_permission,
    register_all as register_permissions,
    registry as permission_registry,
    sensitive_paths,
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
    PERMISSIONS_FILE,
};

// Registry
pub use registry::{
    // MCP
    McpConfig,
    McpConfigFile,
    McpServer,
    McpTransport,
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
