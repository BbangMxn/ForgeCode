//! Cost Tracker - 토큰 사용량 및 비용 추적
//!
//! 모델별 비용 계산 및 세션/일별 사용량 추적

use chrono::{DateTime, Local, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// 모델 가격 정보 (USD per 1M tokens)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPricing {
    pub model_id: String,
    pub input_price: f64,   // per 1M input tokens
    pub output_price: f64,  // per 1M output tokens
    pub cached_price: Option<f64>, // per 1M cached tokens (if supported)
}

impl ModelPricing {
    pub fn new(model_id: &str, input: f64, output: f64) -> Self {
        Self {
            model_id: model_id.to_string(),
            input_price: input,
            output_price: output,
            cached_price: None,
        }
    }

    pub fn with_cached(mut self, price: f64) -> Self {
        self.cached_price = Some(price);
        self
    }

    /// 비용 계산
    pub fn calculate(&self, input_tokens: u64, output_tokens: u64, cached_tokens: u64) -> f64 {
        let input_cost = (input_tokens as f64 / 1_000_000.0) * self.input_price;
        let output_cost = (output_tokens as f64 / 1_000_000.0) * self.output_price;
        let cached_cost = self.cached_price
            .map(|p| (cached_tokens as f64 / 1_000_000.0) * p)
            .unwrap_or(0.0);
        
        input_cost + output_cost + cached_cost
    }
}

/// 기본 모델 가격표 (2025년 기준 추정)
pub fn default_pricing() -> HashMap<String, ModelPricing> {
    let mut pricing = HashMap::new();

    // Anthropic Claude
    pricing.insert(
        "claude-opus-4".to_string(),
        ModelPricing::new("claude-opus-4", 15.0, 75.0).with_cached(1.5),
    );
    pricing.insert(
        "claude-sonnet-4".to_string(),
        ModelPricing::new("claude-sonnet-4", 3.0, 15.0).with_cached(0.3),
    );
    pricing.insert(
        "claude-3.5-sonnet".to_string(),
        ModelPricing::new("claude-3.5-sonnet", 3.0, 15.0).with_cached(0.3),
    );
    pricing.insert(
        "claude-3-haiku".to_string(),
        ModelPricing::new("claude-3-haiku", 0.25, 1.25).with_cached(0.03),
    );

    // OpenAI GPT-4
    pricing.insert(
        "gpt-4o".to_string(),
        ModelPricing::new("gpt-4o", 2.5, 10.0).with_cached(1.25),
    );
    pricing.insert(
        "gpt-4o-mini".to_string(),
        ModelPricing::new("gpt-4o-mini", 0.15, 0.6).with_cached(0.075),
    );
    pricing.insert(
        "gpt-4-turbo".to_string(),
        ModelPricing::new("gpt-4-turbo", 10.0, 30.0),
    );

    // Google Gemini
    pricing.insert(
        "gemini-2.0-flash".to_string(),
        ModelPricing::new("gemini-2.0-flash", 0.10, 0.40),
    );
    pricing.insert(
        "gemini-1.5-pro".to_string(),
        ModelPricing::new("gemini-1.5-pro", 1.25, 5.0),
    );

    // Local (free)
    pricing.insert(
        "ollama".to_string(),
        ModelPricing::new("ollama", 0.0, 0.0),
    );
    pricing.insert(
        "local".to_string(),
        ModelPricing::new("local", 0.0, 0.0),
    );

    pricing
}

/// 사용량 기록
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UsageRecord {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_tokens: u64,
    pub cost_usd: f64,
    pub requests: u64,
}

impl UsageRecord {
    pub fn add(&mut self, input: u64, output: u64, cached: u64, cost: f64) {
        self.input_tokens += input;
        self.output_tokens += output;
        self.cached_tokens += cached;
        self.cost_usd += cost;
        self.requests += 1;
    }

    pub fn total_tokens(&self) -> u64 {
        self.input_tokens + self.output_tokens
    }
}

/// 일별 사용량
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DailyUsage {
    pub date: String,
    pub by_model: HashMap<String, UsageRecord>,
    pub total: UsageRecord,
}

/// 세션 사용량
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionUsage {
    pub session_id: String,
    pub started_at: Option<DateTime<Local>>,
    pub model: String,
    pub usage: UsageRecord,
}

/// 비용 추적기
pub struct CostTracker {
    /// 가격표
    pricing: HashMap<String, ModelPricing>,
    /// 일별 사용량
    daily_usage: HashMap<String, DailyUsage>,
    /// 현재 세션 사용량
    current_session: SessionUsage,
    /// 저장 경로
    storage_path: PathBuf,
    /// 예산 제한 (USD)
    daily_budget: Option<f64>,
    monthly_budget: Option<f64>,
}

impl CostTracker {
    pub fn new() -> Self {
        let storage_path = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".forgecode")
            .join("usage");

        let _ = fs::create_dir_all(&storage_path);

        let mut tracker = Self {
            pricing: default_pricing(),
            daily_usage: HashMap::new(),
            current_session: SessionUsage::default(),
            storage_path,
            daily_budget: None,
            monthly_budget: None,
        };

        tracker.load_today();
        tracker
    }

    pub fn with_path(path: PathBuf) -> Self {
        let _ = fs::create_dir_all(&path);

        let mut tracker = Self {
            pricing: default_pricing(),
            daily_usage: HashMap::new(),
            current_session: SessionUsage::default(),
            storage_path: path,
            daily_budget: None,
            monthly_budget: None,
        };

        tracker.load_today();
        tracker
    }

    /// 예산 설정
    pub fn set_daily_budget(&mut self, budget: f64) {
        self.daily_budget = Some(budget);
    }

    pub fn set_monthly_budget(&mut self, budget: f64) {
        self.monthly_budget = Some(budget);
    }

    /// 새 세션 시작
    pub fn start_session(&mut self, session_id: &str, model: &str) {
        self.current_session = SessionUsage {
            session_id: session_id.to_string(),
            started_at: Some(Local::now()),
            model: model.to_string(),
            usage: UsageRecord::default(),
        };
    }

    /// 토큰 사용량 기록
    pub fn record_usage(
        &mut self,
        model: &str,
        input_tokens: u64,
        output_tokens: u64,
        cached_tokens: u64,
    ) -> f64 {
        // 비용 계산
        let cost = self.calculate_cost(model, input_tokens, output_tokens, cached_tokens);

        // 세션 사용량 업데이트
        self.current_session
            .usage
            .add(input_tokens, output_tokens, cached_tokens, cost);

        // 일별 사용량 업데이트
        let today = Local::now().format("%Y-%m-%d").to_string();
        let daily = self.daily_usage.entry(today.clone()).or_insert_with(|| {
            DailyUsage {
                date: today,
                ..Default::default()
            }
        });

        daily.total.add(input_tokens, output_tokens, cached_tokens, cost);

        let model_usage = daily
            .by_model
            .entry(model.to_string())
            .or_insert_with(UsageRecord::default);
        model_usage.add(input_tokens, output_tokens, cached_tokens, cost);

        // 저장
        self.save_today();

        cost
    }

    /// 비용 계산
    pub fn calculate_cost(
        &self,
        model: &str,
        input_tokens: u64,
        output_tokens: u64,
        cached_tokens: u64,
    ) -> f64 {
        // 모델 ID 정규화 (예: claude-3-sonnet-20240229 -> claude-3-sonnet)
        let normalized = self.normalize_model_id(model);

        self.pricing
            .get(&normalized)
            .or_else(|| self.pricing.get(model))
            .map(|p| p.calculate(input_tokens, output_tokens, cached_tokens))
            .unwrap_or(0.0)
    }

    fn normalize_model_id(&self, model: &str) -> String {
        // 날짜 suffix 제거
        let model = model.split('-').take(3).collect::<Vec<_>>().join("-");
        
        // 알려진 모델 매핑
        if model.contains("claude") && model.contains("opus") {
            return "claude-opus-4".to_string();
        }
        if model.contains("claude") && model.contains("sonnet") {
            return "claude-3.5-sonnet".to_string();
        }
        if model.contains("claude") && model.contains("haiku") {
            return "claude-3-haiku".to_string();
        }
        if model.contains("gpt-4o") && model.contains("mini") {
            return "gpt-4o-mini".to_string();
        }
        if model.contains("gpt-4o") {
            return "gpt-4o".to_string();
        }
        if model.contains("gemini") && model.contains("flash") {
            return "gemini-2.0-flash".to_string();
        }
        if model.contains("gemini") && model.contains("pro") {
            return "gemini-1.5-pro".to_string();
        }
        // Ollama/로컬 모델
        if model.contains("qwen") || model.contains("llama") || model.contains("mistral") {
            return "ollama".to_string();
        }

        model
    }

    /// 오늘 사용량
    pub fn today_usage(&self) -> Option<&DailyUsage> {
        let today = Local::now().format("%Y-%m-%d").to_string();
        self.daily_usage.get(&today)
    }

    /// 현재 세션 사용량
    pub fn session_usage(&self) -> &SessionUsage {
        &self.current_session
    }

    /// 예산 경고 확인
    pub fn check_budget(&self) -> Option<BudgetWarning> {
        let today = Local::now().format("%Y-%m-%d").to_string();
        
        // 일별 예산 확인
        if let (Some(budget), Some(usage)) = (self.daily_budget, self.daily_usage.get(&today)) {
            let ratio = usage.total.cost_usd / budget;
            if ratio >= 1.0 {
                return Some(BudgetWarning::DailyExceeded {
                    spent: usage.total.cost_usd,
                    budget,
                });
            } else if ratio >= 0.8 {
                return Some(BudgetWarning::DailyApproaching {
                    spent: usage.total.cost_usd,
                    budget,
                    percentage: ratio * 100.0,
                });
            }
        }

        // 월별 예산 확인
        if let Some(budget) = self.monthly_budget {
            let monthly_spent = self.monthly_cost();
            let ratio = monthly_spent / budget;
            if ratio >= 1.0 {
                return Some(BudgetWarning::MonthlyExceeded {
                    spent: monthly_spent,
                    budget,
                });
            } else if ratio >= 0.8 {
                return Some(BudgetWarning::MonthlyApproaching {
                    spent: monthly_spent,
                    budget,
                    percentage: ratio * 100.0,
                });
            }
        }

        None
    }

    /// 월간 비용
    pub fn monthly_cost(&self) -> f64 {
        let now = Local::now();
        let month_prefix = now.format("%Y-%m").to_string();

        self.daily_usage
            .iter()
            .filter(|(date, _)| date.starts_with(&month_prefix))
            .map(|(_, usage)| usage.total.cost_usd)
            .sum()
    }

    /// 통계 요약
    pub fn summary(&self) -> CostSummary {
        let today = Local::now().format("%Y-%m-%d").to_string();
        
        CostSummary {
            session: self.current_session.usage.clone(),
            today: self.daily_usage.get(&today).map(|d| d.total.clone()).unwrap_or_default(),
            month: UsageRecord {
                cost_usd: self.monthly_cost(),
                ..Default::default()
            },
        }
    }

    /// 포맷된 비용 문자열
    pub fn format_cost(&self, cost: f64) -> String {
        if cost < 0.01 {
            format!("${:.4}", cost)
        } else if cost < 1.0 {
            format!("${:.3}", cost)
        } else {
            format!("${:.2}", cost)
        }
    }

    /// 오늘 사용량 로드
    fn load_today(&mut self) {
        let today = Local::now().format("%Y-%m-%d").to_string();
        let path = self.storage_path.join(format!("{}.json", today));

        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(usage) = serde_json::from_str::<DailyUsage>(&content) {
                self.daily_usage.insert(today, usage);
            }
        }
    }

    /// 오늘 사용량 저장
    fn save_today(&self) {
        let today = Local::now().format("%Y-%m-%d").to_string();
        
        if let Some(usage) = self.daily_usage.get(&today) {
            let path = self.storage_path.join(format!("{}.json", today));
            if let Ok(json) = serde_json::to_string_pretty(usage) {
                let _ = fs::write(&path, json);
            }
        }
    }

    /// 기간별 사용량 조회
    pub fn usage_range(&mut self, start: NaiveDate, end: NaiveDate) -> Vec<DailyUsage> {
        let mut results = Vec::new();
        let mut current = start;

        while current <= end {
            let date_str = current.format("%Y-%m-%d").to_string();
            
            // 캐시에 없으면 파일에서 로드
            if !self.daily_usage.contains_key(&date_str) {
                let path = self.storage_path.join(format!("{}.json", date_str));
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(usage) = serde_json::from_str::<DailyUsage>(&content) {
                        self.daily_usage.insert(date_str.clone(), usage);
                    }
                }
            }

            if let Some(usage) = self.daily_usage.get(&date_str) {
                results.push(usage.clone());
            }

            current = current.succ_opt().unwrap_or(current);
        }

        results
    }
}

impl Default for CostTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// 예산 경고
#[derive(Debug, Clone)]
pub enum BudgetWarning {
    DailyApproaching {
        spent: f64,
        budget: f64,
        percentage: f64,
    },
    DailyExceeded {
        spent: f64,
        budget: f64,
    },
    MonthlyApproaching {
        spent: f64,
        budget: f64,
        percentage: f64,
    },
    MonthlyExceeded {
        spent: f64,
        budget: f64,
    },
}

impl BudgetWarning {
    pub fn message(&self) -> String {
        match self {
            BudgetWarning::DailyApproaching { spent, budget, percentage } => {
                format!(
                    "⚠️ Daily budget {:.0}% used (${:.2} / ${:.2})",
                    percentage, spent, budget
                )
            }
            BudgetWarning::DailyExceeded { spent, budget } => {
                format!(
                    "🚫 Daily budget exceeded! (${:.2} / ${:.2})",
                    spent, budget
                )
            }
            BudgetWarning::MonthlyApproaching { spent, budget, percentage } => {
                format!(
                    "⚠️ Monthly budget {:.0}% used (${:.2} / ${:.2})",
                    percentage, spent, budget
                )
            }
            BudgetWarning::MonthlyExceeded { spent, budget } => {
                format!(
                    "🚫 Monthly budget exceeded! (${:.2} / ${:.2})",
                    spent, budget
                )
            }
        }
    }
}

/// 비용 요약
#[derive(Debug, Clone, Default)]
pub struct CostSummary {
    pub session: UsageRecord,
    pub today: UsageRecord,
    pub month: UsageRecord,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pricing_calculation() {
        let pricing = ModelPricing::new("gpt-4o", 2.5, 10.0);
        
        // 1M tokens = $2.5 input, $10 output
        let cost = pricing.calculate(1_000_000, 1_000_000, 0);
        assert!((cost - 12.5).abs() < 0.001);
    }

    #[test]
    fn test_usage_record() {
        let mut record = UsageRecord::default();
        record.add(1000, 500, 0, 0.1);
        record.add(2000, 1000, 0, 0.2);

        assert_eq!(record.input_tokens, 3000);
        assert_eq!(record.output_tokens, 1500);
        assert_eq!(record.requests, 2);
        assert!((record.cost_usd - 0.3).abs() < 0.001);
    }

    #[test]
    fn test_model_normalization() {
        let tracker = CostTracker::new();

        assert_eq!(
            tracker.normalize_model_id("claude-3-sonnet-20240229"),
            "claude-3.5-sonnet"
        );
        assert_eq!(
            tracker.normalize_model_id("gpt-4o-2024-08-06"),
            "gpt-4o"
        );
        assert_eq!(
            tracker.normalize_model_id("qwen3:8b"),
            "ollama"
        );
    }

    #[test]
    fn test_budget_warning() {
        let mut tracker = CostTracker::new();
        tracker.set_daily_budget(1.0);
        
        // 아직 사용 안 함 - 경고 없음
        assert!(tracker.check_budget().is_none());
    }

    #[test]
    fn test_format_cost() {
        let tracker = CostTracker::new();

        assert_eq!(tracker.format_cost(0.001), "$0.0010");
        assert_eq!(tracker.format_cost(0.1), "$0.100");
        assert_eq!(tracker.format_cost(1.5), "$1.50");
    }
}
