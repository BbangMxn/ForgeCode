//! # forge-foundation
//!
//! Foundation layer for ForgeCode:
//! - Core: 핵심 Trait 정의 (Tool, Provider, Task, PermissionDelegate)
//! - Permission: 권한 관리 (동적 등록 + 런타임 + 보안 분석)
//! - Registry: MCP, Provider, Model, Shell 등록
//! - Storage: SQLite (런타임), JsonStore (범용)
//! - Config: 통합 설정 (ForgeConfig, Limits 등)
//!
//! ## 아키텍처
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────┐
//! │  Tool Registry (통합)                                    │
//! │  ├── Builtin Tools (Bash, Read, Write...)              │
//! │  └── MCP Tools (Notion, Chrome, GitHub...)             │
//! │                     │                                   │
//! │                     ▼                                   │
//! │          Permission System (Allow/Ask/Deny)             │
//! │                     │                                   │
//! │          ┌─────────┴─────────┐                         │
//! │          ▼                   ▼                         │
//! │   Shell Executor      MCP Transport                    │
//! │   (cmd, bash, pwsh)   (stdio, sse)                     │
//! └─────────────────────────────────────────────────────────┘
//! ```

pub mod audit;
pub mod cache;
pub mod config;
pub mod core;
pub mod error;
pub mod event;
pub mod permission;
pub mod registry;
pub mod storage;
pub mod tokenizer;

// ============================================================================
// Error
// ============================================================================
pub use error::{Error, Result};

// ============================================================================
// Core (핵심 Trait 및 타입)
// ============================================================================
pub use core::{
    // Traits
    ChatMessage,
    ChatRequest,
    ChatResponse,
    Configurable,
    // Types
    ExecutionEnv,
    MessageRole,
    ModelHint,
    PermissionDelegate,
    PermissionResponse,
    PermissionRule,
    PermissionRuleAction,
    Provider,
    ProviderMeta,
    SessionInfo,
    ShellConfig,
    ShellType,
    StreamEvent,
    Task,
    TaskArtifact,
    TaskContext,
    TaskMeta,
    TaskObserver,
    TaskResult,
    TaskState,
    TokenUsage,
    Tool,
    ToolCall,
    ToolContext,
    ToolMeta,
    ToolResult,
    ToolSource,
};

// ============================================================================
// Config (설정)
// ============================================================================
pub use config::{
    // Forge (통합 설정)
    AutoSaveConfig,
    CustomColors,
    // Limits (사용량 제한)
    DailyLimits,
    EditorConfig,
    ExperimentalConfig,
    ForgeConfig,
    LimitsConfig,
    MonthlyLimits,
    SessionLimits,
    ThemeConfig,
    FORGE_CONFIG_FILE,
};

// ============================================================================
// Permission (권한 시스템)
// ============================================================================
pub use permission::{
    // Categories
    categories as permission_categories,
    // Security (명령어/경로 분석)
    command_analyzer,
    dangerous_commands,
    path_analyzer,
    // Types (동적 등록)
    register as register_permission,
    register_all as register_permissions,
    registry as permission_registry,
    sensitive_paths,
    CommandAnalysis,
    CommandAnalyzer,
    CommandRisk,
    PathAnalyzer,
    // Runtime (서비스)
    Permission,
    PermissionAction,
    // Settings (JSON 저장)
    PermissionActionType,
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

// ============================================================================
// Registry (레지스트리)
// ============================================================================
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
    ProviderConfig,
    ProviderType,
    // Shell (저장용 - Serialize/Deserialize 지원)
    ShellRunner,
    ShellSettings,
    MCP_FILE,
    PROVIDERS_FILE,
    SHELL_FILE,
};

// Shell 설정 저장소 (registry::shell의 ShellConfig와 ShellType)
// core의 trait과 구분하기 위해 별도 모듈로 re-export
pub mod shell_store {
    pub use crate::registry::shell::{ShellConfig, ShellType, SHELL_FILE};
}

// Provider 설정 (registry::provider::Provider와 core::Provider trait 구분)
pub mod provider_store {
    pub use crate::registry::provider::{Provider, ProviderConfig, ProviderType, PROVIDERS_FILE};
}

// ============================================================================
// Storage (저장소)
// ============================================================================
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

// ============================================================================
// Event (이벤트 시스템)
// ============================================================================
pub use event::{
    // Global
    global_event_bus,
    init_global_event_bus,
    // Bus
    EventBus,
    EventBusConfig,
    // Types
    EventCategory,
    EventFilter,
    EventId,
    EventListener,
    EventSeverity,
    ForgeEvent,
    ListenerId,
};

// ============================================================================
// Audit (감사 로깅)
// ============================================================================
pub use audit::{
    // Types
    AuditAction,
    AuditEntry,
    // Logger
    AuditEventListener,
    AuditId,
    AuditLogger,
    AuditLoggerConfig,
    AuditQuery,
    AuditResult,
    AuditStatistics,
};

// ============================================================================
// Cache (캐시 시스템)
// ============================================================================
pub use cache::{
    // Config
    CacheConfig,
    CacheLimitsConfig,
    // Manager
    CacheManager,
    CacheManagerStats,
    CachedToolDefinition,
    CachedToolResult,
    ContentId,
    ContextCacheConfig,
    ContextCompactor,
    ConversationSummarizer,
    // Utilities
    LruCache,
    McpCache,
    // Context Management
    ObservationMasker,
    ResponseCacheConfig,
    // Response Cache
    ToolCache,
    TtlLruCache,
};

// ============================================================================
// Tokenizer (모델별 토큰 계산)
// ============================================================================
pub use tokenizer::{
    // Estimators
    ClaudeEstimator,
    // Dynamic (Ollama, vLLM, LM Studio 등)
    DynamicTokenizerRegistry,
    // Types
    EncodingResult,
    GeminiEstimator,
    LlamaEstimator,
    ModelFamily,
    ModelTokenConfig,
    OllamaTokenizer,
    OpenAICompatTokenizer,
    TiktokenEstimator,
    TokenBudget,
    TokenCount,
    TokenDistribution,
    // Trait
    Tokenizer,
    TokenizerError,
    // Factory
    TokenizerFactory,
    TokenizerType,
};
