//! Tokenizer 타입 정의

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 토크나이저 타입
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenizerType {
    /// OpenAI tiktoken (cl100k_base - GPT-4, GPT-3.5)
    TiktokenCl100k,
    /// OpenAI tiktoken (o200k_base - GPT-4o, o1)
    TiktokenO200k,
    /// Claude 토크나이저 (추정 기반)
    Claude,
    /// Google Gemini (추정 기반)
    Gemini,
    /// Llama/Mistral SentencePiece
    Llama,
    /// 기본 추정 (문자 기반)
    Estimate,
}

impl Default for TokenizerType {
    fn default() -> Self {
        Self::Estimate
    }
}

impl TokenizerType {
    /// 문자당 평균 토큰 비율 (추정용)
    pub fn chars_per_token(&self) -> f32 {
        match self {
            Self::TiktokenCl100k | Self::TiktokenO200k => 4.0,
            Self::Claude => 3.5,
            Self::Gemini => 4.0,
            Self::Llama => 3.8,
            Self::Estimate => 4.0,
        }
    }

    /// 한국어 문자당 토큰 비율
    pub fn korean_chars_per_token(&self) -> f32 {
        match self {
            Self::TiktokenCl100k | Self::TiktokenO200k => 1.5,
            Self::Claude => 1.3,
            Self::Gemini => 1.5,
            Self::Llama => 2.0,
            Self::Estimate => 1.5,
        }
    }
}

/// 토큰 수 결과
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenCount {
    /// 총 토큰 수
    pub total: usize,
    /// 정확한 계산 여부 (false면 추정치)
    pub is_exact: bool,
    /// 텍스트 문자 수
    pub char_count: usize,
    /// 사용된 토크나이저 타입
    pub tokenizer_type: TokenizerType,
}

impl TokenCount {
    pub fn new(total: usize, is_exact: bool, tokenizer_type: TokenizerType) -> Self {
        Self {
            total,
            is_exact,
            char_count: 0,
            tokenizer_type,
        }
    }

    pub fn exact(total: usize, tokenizer_type: TokenizerType) -> Self {
        Self::new(total, true, tokenizer_type)
    }

    pub fn estimated(total: usize, tokenizer_type: TokenizerType) -> Self {
        Self::new(total, false, tokenizer_type)
    }

    pub fn with_char_count(mut self, count: usize) -> Self {
        self.char_count = count;
        self
    }
}

/// 토큰 분포 (메시지별)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenDistribution {
    /// 시스템 프롬프트 토큰
    pub system: usize,
    /// 사용자 메시지 토큰
    pub user: usize,
    /// 어시스턴트 메시지 토큰
    pub assistant: usize,
    /// 도구 결과 토큰
    pub tool_results: usize,
    /// 총합
    pub total: usize,
}

impl TokenDistribution {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_system(&mut self, tokens: usize) {
        self.system += tokens;
        self.total += tokens;
    }

    pub fn add_user(&mut self, tokens: usize) {
        self.user += tokens;
        self.total += tokens;
    }

    pub fn add_assistant(&mut self, tokens: usize) {
        self.assistant += tokens;
        self.total += tokens;
    }

    pub fn add_tool_result(&mut self, tokens: usize) {
        self.tool_results += tokens;
        self.total += tokens;
    }

    /// 비율 계산 (퍼센트)
    pub fn percentages(&self) -> HashMap<String, f32> {
        let total = self.total as f32;
        if total == 0.0 {
            return HashMap::new();
        }

        let mut map = HashMap::new();
        map.insert("system".to_string(), self.system as f32 / total * 100.0);
        map.insert("user".to_string(), self.user as f32 / total * 100.0);
        map.insert(
            "assistant".to_string(),
            self.assistant as f32 / total * 100.0,
        );
        map.insert(
            "tool_results".to_string(),
            self.tool_results as f32 / total * 100.0,
        );
        map
    }
}

/// 토큰 예산 관리
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBudget {
    /// 컨텍스트 윈도우 크기
    pub context_window: usize,
    /// 최대 출력 토큰
    pub max_output: usize,
    /// 시스템 프롬프트 예약
    pub reserved_system: usize,
    /// 안전 마진 (%)
    pub safety_margin_percent: u8,
}

impl TokenBudget {
    pub fn new(context_window: usize, max_output: usize) -> Self {
        Self {
            context_window,
            max_output,
            reserved_system: 0,
            safety_margin_percent: 10,
        }
    }

    pub fn with_reserved_system(mut self, tokens: usize) -> Self {
        self.reserved_system = tokens;
        self
    }

    pub fn with_safety_margin(mut self, percent: u8) -> Self {
        self.safety_margin_percent = percent.min(50);
        self
    }

    /// 사용 가능한 입력 토큰 수 계산
    pub fn available_input(&self) -> usize {
        let safety = self.context_window * self.safety_margin_percent as usize / 100;
        self.context_window
            .saturating_sub(self.max_output)
            .saturating_sub(self.reserved_system)
            .saturating_sub(safety)
    }

    /// 현재 사용량으로 남은 토큰 계산
    pub fn remaining(&self, current_usage: usize) -> usize {
        self.available_input().saturating_sub(current_usage)
    }

    /// 예산 초과 여부
    pub fn is_over_budget(&self, current_usage: usize) -> bool {
        current_usage > self.available_input()
    }

    /// 사용률 (%)
    pub fn usage_percent(&self, current_usage: usize) -> f32 {
        let available = self.available_input() as f32;
        if available == 0.0 {
            return 100.0;
        }
        (current_usage as f32 / available * 100.0).min(100.0)
    }
}

impl Default for TokenBudget {
    fn default() -> Self {
        Self {
            context_window: 128_000,
            max_output: 4_096,
            reserved_system: 2_000,
            safety_margin_percent: 10,
        }
    }
}

/// 모델별 토큰 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelTokenConfig {
    /// 모델 ID
    pub model_id: String,
    /// 토크나이저 타입
    pub tokenizer_type: TokenizerType,
    /// 컨텍스트 윈도우
    pub context_window: usize,
    /// 최대 출력 토큰
    pub max_output_tokens: usize,
    /// 메시지당 오버헤드 토큰 (포맷팅용)
    pub message_overhead: usize,
    /// 시스템 프롬프트 오버헤드
    pub system_overhead: usize,
}

impl ModelTokenConfig {
    pub fn new(model_id: impl Into<String>, tokenizer_type: TokenizerType) -> Self {
        Self {
            model_id: model_id.into(),
            tokenizer_type,
            context_window: 128_000,
            max_output_tokens: 4_096,
            message_overhead: 4, // 메시지당 기본 토큰 오버헤드
            system_overhead: 10, // 시스템 프롬프트 추가 오버헤드
        }
    }

    pub fn context_window(mut self, tokens: usize) -> Self {
        self.context_window = tokens;
        self
    }

    pub fn max_output(mut self, tokens: usize) -> Self {
        self.max_output_tokens = tokens;
        self
    }

    pub fn message_overhead(mut self, tokens: usize) -> Self {
        self.message_overhead = tokens;
        self
    }

    /// TokenBudget 생성
    pub fn to_budget(&self) -> TokenBudget {
        TokenBudget::new(self.context_window, self.max_output_tokens)
    }
}

/// 인코딩 결과
#[derive(Debug, Clone)]
pub struct EncodingResult {
    /// 토큰 ID 목록
    pub token_ids: Vec<u32>,
    /// 토큰 문자열 목록 (디버깅용)
    pub tokens: Vec<String>,
    /// 토큰 수
    pub count: usize,
}

impl EncodingResult {
    pub fn new(token_ids: Vec<u32>) -> Self {
        let count = token_ids.len();
        Self {
            token_ids,
            tokens: Vec::new(),
            count,
        }
    }

    pub fn with_tokens(mut self, tokens: Vec<String>) -> Self {
        self.tokens = tokens;
        self
    }

    pub fn empty() -> Self {
        Self {
            token_ids: Vec::new(),
            tokens: Vec::new(),
            count: 0,
        }
    }
}

/// 토크나이저 에러
#[derive(Debug, Clone)]
pub enum TokenizerError {
    /// 지원하지 않는 모델
    UnsupportedModel(String),
    /// 토크나이저 초기화 실패
    InitializationFailed(String),
    /// 인코딩 실패
    EncodingFailed(String),
    /// 디코딩 실패
    DecodingFailed(String),
}

impl std::fmt::Display for TokenizerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedModel(m) => write!(f, "Unsupported model: {}", m),
            Self::InitializationFailed(e) => write!(f, "Tokenizer init failed: {}", e),
            Self::EncodingFailed(e) => write!(f, "Encoding failed: {}", e),
            Self::DecodingFailed(e) => write!(f, "Decoding failed: {}", e),
        }
    }
}

impl std::error::Error for TokenizerError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_budget() {
        let budget = TokenBudget::new(128_000, 4_096)
            .with_reserved_system(2_000)
            .with_safety_margin(10);

        // 128000 - 4096 - 2000 - 12800 = 109104
        let available = budget.available_input();
        assert!(available > 100_000);
        assert!(available < 128_000);

        assert!(!budget.is_over_budget(50_000));
        assert!(budget.is_over_budget(200_000));
    }

    #[test]
    fn test_token_distribution() {
        let mut dist = TokenDistribution::new();
        dist.add_system(1000);
        dist.add_user(2000);
        dist.add_assistant(3000);

        assert_eq!(dist.total, 6000);

        let pct = dist.percentages();
        assert!((pct["system"] - 16.67).abs() < 1.0);
    }

    #[test]
    fn test_token_count() {
        let count = TokenCount::exact(100, TokenizerType::TiktokenCl100k);
        assert!(count.is_exact);
        assert_eq!(count.total, 100);
    }
}
