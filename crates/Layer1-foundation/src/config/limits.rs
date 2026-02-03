//! Limits Configuration - 토큰 및 비용 제한 설정
//!
//! 세션별, 일별, 월별 사용량 제한을 설정합니다.

use crate::storage::JsonStore;
use crate::Result;
use serde::{Deserialize, Serialize};

/// 제한 설정 파일명
pub const LIMITS_FILE: &str = "limits.json";

/// 세션별 제한
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionLimits {
    /// 세션당 최대 입력 토큰
    pub max_input_tokens: Option<u64>,
    /// 세션당 최대 출력 토큰
    pub max_output_tokens: Option<u64>,
    /// 세션당 최대 총 토큰
    pub max_total_tokens: Option<u64>,
    /// 세션당 최대 비용 (USD)
    pub max_cost_usd: Option<f64>,
    /// 세션당 최대 메시지 수
    pub max_messages: Option<u32>,
    /// 세션당 최대 도구 실행 횟수
    pub max_tool_executions: Option<u32>,
}

impl Default for SessionLimits {
    fn default() -> Self {
        Self {
            max_input_tokens: None,
            max_output_tokens: None,
            max_total_tokens: Some(1_000_000), // 1M tokens per session
            max_cost_usd: Some(10.0),          // $10 per session
            max_messages: Some(200),           // 200 messages per session
            max_tool_executions: Some(500),    // 500 tool calls per session
        }
    }
}

impl SessionLimits {
    pub fn unlimited() -> Self {
        Self {
            max_input_tokens: None,
            max_output_tokens: None,
            max_total_tokens: None,
            max_cost_usd: None,
            max_messages: None,
            max_tool_executions: None,
        }
    }

    /// 토큰 제한 확인
    pub fn check_tokens(&self, input: u64, output: u64) -> LimitCheckResult {
        if let Some(max) = self.max_input_tokens {
            if input > max {
                return LimitCheckResult::Exceeded {
                    limit_type: "session_input_tokens".to_string(),
                    current: input,
                    max,
                };
            }
        }

        if let Some(max) = self.max_output_tokens {
            if output > max {
                return LimitCheckResult::Exceeded {
                    limit_type: "session_output_tokens".to_string(),
                    current: output,
                    max,
                };
            }
        }

        if let Some(max) = self.max_total_tokens {
            let total = input + output;
            if total > max {
                return LimitCheckResult::Exceeded {
                    limit_type: "session_total_tokens".to_string(),
                    current: total,
                    max,
                };
            }
        }

        LimitCheckResult::Ok
    }

    /// 비용 제한 확인
    pub fn check_cost(&self, cost_usd: f64) -> LimitCheckResult {
        if let Some(max) = self.max_cost_usd {
            if cost_usd > max {
                return LimitCheckResult::ExceededCost {
                    limit_type: "session_cost".to_string(),
                    current: cost_usd,
                    max,
                };
            }
        }
        LimitCheckResult::Ok
    }

    /// 메시지 제한 확인
    pub fn check_messages(&self, count: u32) -> LimitCheckResult {
        if let Some(max) = self.max_messages {
            if count > max {
                return LimitCheckResult::Exceeded {
                    limit_type: "session_messages".to_string(),
                    current: count as u64,
                    max: max as u64,
                };
            }
        }
        LimitCheckResult::Ok
    }

    /// 도구 실행 제한 확인
    pub fn check_tool_executions(&self, count: u32) -> LimitCheckResult {
        if let Some(max) = self.max_tool_executions {
            if count > max {
                return LimitCheckResult::Exceeded {
                    limit_type: "session_tool_executions".to_string(),
                    current: count as u64,
                    max: max as u64,
                };
            }
        }
        LimitCheckResult::Ok
    }
}

/// 일별 제한
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyLimits {
    /// 일별 최대 토큰
    pub max_tokens: Option<u64>,
    /// 일별 최대 비용 (USD)
    pub max_cost_usd: Option<f64>,
    /// 일별 최대 요청 수
    pub max_requests: Option<u32>,
}

impl Default for DailyLimits {
    fn default() -> Self {
        Self {
            max_tokens: Some(10_000_000), // 10M tokens per day
            max_cost_usd: Some(50.0),     // $50 per day
            max_requests: Some(1000),     // 1000 requests per day
        }
    }
}

impl DailyLimits {
    pub fn unlimited() -> Self {
        Self {
            max_tokens: None,
            max_cost_usd: None,
            max_requests: None,
        }
    }

    /// 토큰 제한 확인
    pub fn check_tokens(&self, total: u64) -> LimitCheckResult {
        if let Some(max) = self.max_tokens {
            if total > max {
                return LimitCheckResult::Exceeded {
                    limit_type: "daily_tokens".to_string(),
                    current: total,
                    max,
                };
            }
        }
        LimitCheckResult::Ok
    }

    /// 비용 제한 확인
    pub fn check_cost(&self, cost_usd: f64) -> LimitCheckResult {
        if let Some(max) = self.max_cost_usd {
            if cost_usd > max {
                return LimitCheckResult::ExceededCost {
                    limit_type: "daily_cost".to_string(),
                    current: cost_usd,
                    max,
                };
            }
        }
        LimitCheckResult::Ok
    }

    /// 요청 수 제한 확인
    pub fn check_requests(&self, count: u32) -> LimitCheckResult {
        if let Some(max) = self.max_requests {
            if count > max {
                return LimitCheckResult::Exceeded {
                    limit_type: "daily_requests".to_string(),
                    current: count as u64,
                    max: max as u64,
                };
            }
        }
        LimitCheckResult::Ok
    }
}

/// 월별 제한
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonthlyLimits {
    /// 월별 최대 토큰
    pub max_tokens: Option<u64>,
    /// 월별 최대 비용 (USD)
    pub max_cost_usd: Option<f64>,
}

impl Default for MonthlyLimits {
    fn default() -> Self {
        Self {
            max_tokens: Some(100_000_000), // 100M tokens per month
            max_cost_usd: Some(500.0),     // $500 per month
        }
    }
}

impl MonthlyLimits {
    pub fn unlimited() -> Self {
        Self {
            max_tokens: None,
            max_cost_usd: None,
        }
    }

    /// 토큰 제한 확인
    pub fn check_tokens(&self, total: u64) -> LimitCheckResult {
        if let Some(max) = self.max_tokens {
            if total > max {
                return LimitCheckResult::Exceeded {
                    limit_type: "monthly_tokens".to_string(),
                    current: total,
                    max,
                };
            }
        }
        LimitCheckResult::Ok
    }

    /// 비용 제한 확인
    pub fn check_cost(&self, cost_usd: f64) -> LimitCheckResult {
        if let Some(max) = self.max_cost_usd {
            if cost_usd > max {
                return LimitCheckResult::ExceededCost {
                    limit_type: "monthly_cost".to_string(),
                    current: cost_usd,
                    max,
                };
            }
        }
        LimitCheckResult::Ok
    }
}

/// 제한 확인 결과
#[derive(Debug, Clone)]
pub enum LimitCheckResult {
    /// 제한 내
    Ok,
    /// 토큰/횟수 제한 초과
    Exceeded {
        limit_type: String,
        current: u64,
        max: u64,
    },
    /// 비용 제한 초과
    ExceededCost {
        limit_type: String,
        current: f64,
        max: f64,
    },
    /// 경고 (제한의 80% 도달)
    Warning {
        limit_type: String,
        current: u64,
        max: u64,
        percentage: u8,
    },
}

impl LimitCheckResult {
    pub fn is_ok(&self) -> bool {
        matches!(self, LimitCheckResult::Ok)
    }

    pub fn is_exceeded(&self) -> bool {
        matches!(
            self,
            LimitCheckResult::Exceeded { .. } | LimitCheckResult::ExceededCost { .. }
        )
    }

    pub fn message(&self) -> Option<String> {
        match self {
            LimitCheckResult::Ok => None,
            LimitCheckResult::Exceeded {
                limit_type,
                current,
                max,
            } => Some(format!(
                "Limit exceeded: {} ({} / {})",
                limit_type, current, max
            )),
            LimitCheckResult::ExceededCost {
                limit_type,
                current,
                max,
            } => Some(format!(
                "Cost limit exceeded: {} (${:.2} / ${:.2})",
                limit_type, current, max
            )),
            LimitCheckResult::Warning {
                limit_type,
                current,
                max,
                percentage,
            } => Some(format!(
                "Warning: {} at {}% ({} / {})",
                limit_type, percentage, current, max
            )),
        }
    }
}

/// 통합 제한 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimitsConfig {
    /// 제한 활성화 여부
    pub enabled: bool,
    /// 세션별 제한
    pub session: SessionLimits,
    /// 일별 제한
    pub daily: DailyLimits,
    /// 월별 제한
    pub monthly: MonthlyLimits,
    /// 경고 임계값 (0-100, 기본 80%)
    pub warning_threshold_percent: u8,
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            session: SessionLimits::default(),
            daily: DailyLimits::default(),
            monthly: MonthlyLimits::default(),
            warning_threshold_percent: 80,
        }
    }
}

impl LimitsConfig {
    /// 모든 제한 비활성화
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            session: SessionLimits::unlimited(),
            daily: DailyLimits::unlimited(),
            monthly: MonthlyLimits::unlimited(),
            warning_threshold_percent: 80,
        }
    }

    /// 개발용 (관대한 제한)
    pub fn development() -> Self {
        Self {
            enabled: true,
            session: SessionLimits {
                max_total_tokens: Some(5_000_000),
                max_cost_usd: Some(50.0),
                max_messages: Some(500),
                max_tool_executions: Some(1000),
                ..SessionLimits::unlimited()
            },
            daily: DailyLimits {
                max_tokens: Some(50_000_000),
                max_cost_usd: Some(200.0),
                max_requests: Some(5000),
            },
            monthly: MonthlyLimits::unlimited(),
            warning_threshold_percent: 90,
        }
    }

    /// 프로덕션용 (엄격한 제한)
    pub fn production() -> Self {
        Self {
            enabled: true,
            session: SessionLimits {
                max_total_tokens: Some(500_000),
                max_cost_usd: Some(5.0),
                max_messages: Some(100),
                max_tool_executions: Some(200),
                ..SessionLimits::unlimited()
            },
            daily: DailyLimits {
                max_tokens: Some(5_000_000),
                max_cost_usd: Some(25.0),
                max_requests: Some(500),
            },
            monthly: MonthlyLimits {
                max_tokens: Some(50_000_000),
                max_cost_usd: Some(200.0),
            },
            warning_threshold_percent: 80,
        }
    }

    /// 글로벌 설정 로드
    pub fn load_global() -> Result<Self> {
        let store = JsonStore::global()?;
        Ok(store.load_or_default(LIMITS_FILE))
    }

    /// 프로젝트 설정 로드
    pub fn load_project() -> Result<Self> {
        let store = JsonStore::current_project()?;
        Ok(store.load_or_default(LIMITS_FILE))
    }

    /// 설정 로드 (글로벌 + 프로젝트 병합)
    pub fn load() -> Result<Self> {
        let global = Self::load_global().unwrap_or_default();
        let project = Self::load_project().ok();

        match project {
            Some(proj) => Ok(Self::merge(global, proj)),
            None => Ok(global),
        }
    }

    /// 두 설정 병합 (project가 global을 오버라이드)
    fn merge(global: Self, project: Self) -> Self {
        // 프로젝트 설정이 더 엄격하면 프로젝트 설정 사용
        Self {
            enabled: project.enabled,
            session: SessionLimits {
                max_input_tokens: project
                    .session
                    .max_input_tokens
                    .or(global.session.max_input_tokens),
                max_output_tokens: project
                    .session
                    .max_output_tokens
                    .or(global.session.max_output_tokens),
                max_total_tokens: Self::min_option(
                    project.session.max_total_tokens,
                    global.session.max_total_tokens,
                ),
                max_cost_usd: Self::min_option_f64(
                    project.session.max_cost_usd,
                    global.session.max_cost_usd,
                ),
                max_messages: Self::min_option_u32(
                    project.session.max_messages,
                    global.session.max_messages,
                ),
                max_tool_executions: Self::min_option_u32(
                    project.session.max_tool_executions,
                    global.session.max_tool_executions,
                ),
            },
            daily: DailyLimits {
                max_tokens: Self::min_option(project.daily.max_tokens, global.daily.max_tokens),
                max_cost_usd: Self::min_option_f64(
                    project.daily.max_cost_usd,
                    global.daily.max_cost_usd,
                ),
                max_requests: Self::min_option_u32(
                    project.daily.max_requests,
                    global.daily.max_requests,
                ),
            },
            monthly: MonthlyLimits {
                max_tokens: Self::min_option(project.monthly.max_tokens, global.monthly.max_tokens),
                max_cost_usd: Self::min_option_f64(
                    project.monthly.max_cost_usd,
                    global.monthly.max_cost_usd,
                ),
            },
            warning_threshold_percent: project.warning_threshold_percent,
        }
    }

    fn min_option(a: Option<u64>, b: Option<u64>) -> Option<u64> {
        match (a, b) {
            (Some(x), Some(y)) => Some(x.min(y)),
            (Some(x), None) => Some(x),
            (None, Some(y)) => Some(y),
            (None, None) => None,
        }
    }

    fn min_option_f64(a: Option<f64>, b: Option<f64>) -> Option<f64> {
        match (a, b) {
            (Some(x), Some(y)) => Some(x.min(y)),
            (Some(x), None) => Some(x),
            (None, Some(y)) => Some(y),
            (None, None) => None,
        }
    }

    fn min_option_u32(a: Option<u32>, b: Option<u32>) -> Option<u32> {
        match (a, b) {
            (Some(x), Some(y)) => Some(x.min(y)),
            (Some(x), None) => Some(x),
            (None, Some(y)) => Some(y),
            (None, None) => None,
        }
    }

    /// 글로벌 설정 저장
    pub fn save_global(&self) -> Result<()> {
        let store = JsonStore::global()?;
        store.save(LIMITS_FILE, self)
    }

    /// 프로젝트 설정 저장
    pub fn save_project(&self) -> Result<()> {
        let store = JsonStore::current_project()?;
        store.save(LIMITS_FILE, self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_limits() {
        let limits = SessionLimits::default();

        // 제한 내
        assert!(limits.check_tokens(100, 50).is_ok());
        assert!(limits.check_cost(5.0).is_ok());

        // 비용 초과
        assert!(limits.check_cost(15.0).is_exceeded());
    }

    #[test]
    fn test_limit_check_result() {
        let result = LimitCheckResult::Exceeded {
            limit_type: "test".to_string(),
            current: 100,
            max: 50,
        };

        assert!(result.is_exceeded());
        assert!(result.message().is_some());
    }

    #[test]
    fn test_limits_config_profiles() {
        let dev = LimitsConfig::development();
        let prod = LimitsConfig::production();

        // 개발용이 더 관대해야 함
        assert!(dev.session.max_total_tokens.unwrap() > prod.session.max_total_tokens.unwrap());
        assert!(dev.session.max_cost_usd.unwrap() > prod.session.max_cost_usd.unwrap());
    }
}
