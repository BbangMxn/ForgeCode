//! Config - 통합 설정 관리
//!
//! - `limits.rs` - 토큰/비용 제한
//! - `forge.rs` - ForgeConfig 통합 설정

mod forge;
mod limits;

// Forge (통합 설정)
pub use forge::{
    AutoSaveConfig, CacheSettings, CustomColors, EditorConfig, ExperimentalConfig, ForgeConfig,
    GitConfig, SecurityConfig, ThemeConfig, FORGE_CONFIG_FILE,
};

// Limits
pub use limits::{DailyLimits, LimitsConfig, MonthlyLimits, SessionLimits};
