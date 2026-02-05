//! Tokenizer Factory - 모델별 토크나이저 생성

// TODO: This module will be used for global tokenizer access
#![allow(dead_code)]

use super::estimator::{ClaudeEstimator, GeminiEstimator, LlamaEstimator, TiktokenEstimator};
use super::traits::Tokenizer;
use super::types::{ModelTokenConfig, TokenBudget, TokenizerType};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

/// 전역 토크나이저 팩토리
static TOKENIZER_FACTORY: OnceLock<TokenizerFactory> = OnceLock::new();

/// 토크나이저 팩토리 접근
pub fn factory() -> &'static TokenizerFactory {
    TOKENIZER_FACTORY.get_or_init(TokenizerFactory::new)
}

/// 토크나이저 팩토리
///
/// 모델 ID를 기반으로 적절한 토크나이저를 생성하고 캐싱합니다.
pub struct TokenizerFactory {
    /// 모델별 토큰 설정
    model_configs: RwLock<HashMap<String, ModelTokenConfig>>,
    /// 캐시된 토크나이저
    cache: RwLock<HashMap<TokenizerType, Arc<dyn Tokenizer>>>,
}

impl TokenizerFactory {
    /// 새 팩토리 생성
    pub fn new() -> Self {
        let factory = Self {
            model_configs: RwLock::new(HashMap::new()),
            cache: RwLock::new(HashMap::new()),
        };
        factory.register_defaults();
        factory
    }

    /// 모델 ID로 토크나이저 가져오기
    pub fn for_model(&self, model_id: &str) -> Arc<dyn Tokenizer> {
        let tokenizer_type = self.get_tokenizer_type(model_id);
        self.get_or_create(tokenizer_type)
    }

    /// 토크나이저 타입으로 직접 가져오기
    pub fn get(&self, tokenizer_type: TokenizerType) -> Arc<dyn Tokenizer> {
        self.get_or_create(tokenizer_type)
    }

    /// 모델의 토큰 예산 가져오기
    pub fn budget_for_model(&self, model_id: &str) -> TokenBudget {
        let configs = self.model_configs.read().unwrap();

        if let Some(config) = configs.get(model_id) {
            config.to_budget()
        } else {
            // 기본값 반환
            self.default_budget_for_model(model_id)
        }
    }

    /// 모델 설정 등록
    pub fn register_model(&self, config: ModelTokenConfig) {
        let mut configs = self.model_configs.write().unwrap();
        configs.insert(config.model_id.clone(), config);
    }

    /// 모델 ID에서 토크나이저 타입 추론
    fn get_tokenizer_type(&self, model_id: &str) -> TokenizerType {
        let configs = self.model_configs.read().unwrap();

        if let Some(config) = configs.get(model_id) {
            return config.tokenizer_type;
        }

        // 모델 ID 패턴으로 추론
        let model_lower = model_id.to_lowercase();

        if model_lower.contains("gpt-4o")
            || model_lower.contains("o1")
            || model_lower.contains("o3")
        {
            TokenizerType::TiktokenO200k
        } else if model_lower.contains("gpt") {
            TokenizerType::TiktokenCl100k
        } else if model_lower.contains("claude") {
            TokenizerType::Claude
        } else if model_lower.contains("gemini") {
            TokenizerType::Gemini
        } else if model_lower.contains("llama")
            || model_lower.contains("mistral")
            || model_lower.contains("mixtral")
            || model_lower.contains("qwen")
            || model_lower.contains("deepseek")
        {
            TokenizerType::Llama
        } else {
            TokenizerType::Estimate
        }
    }

    /// 토크나이저 생성 또는 캐시에서 가져오기
    fn get_or_create(&self, tokenizer_type: TokenizerType) -> Arc<dyn Tokenizer> {
        // 캐시 확인
        {
            let cache = self.cache.read().unwrap();
            if let Some(tokenizer) = cache.get(&tokenizer_type) {
                return Arc::clone(tokenizer);
            }
        }

        // 새로 생성
        let tokenizer: Arc<dyn Tokenizer> = match tokenizer_type {
            TokenizerType::TiktokenCl100k => Arc::new(TiktokenEstimator::cl100k()),
            TokenizerType::TiktokenO200k => Arc::new(TiktokenEstimator::o200k()),
            TokenizerType::Claude => Arc::new(ClaudeEstimator::new()),
            TokenizerType::Gemini => Arc::new(GeminiEstimator::new()),
            TokenizerType::Llama => Arc::new(LlamaEstimator::new()),
            TokenizerType::Estimate => Arc::new(super::estimator::EstimateTokenizer::new(
                TokenizerType::Estimate,
            )),
        };

        // 캐시에 저장
        {
            let mut cache = self.cache.write().unwrap();
            cache.insert(tokenizer_type, Arc::clone(&tokenizer));
        }

        tokenizer
    }

    /// 모델 ID 기반 기본 예산 추론
    fn default_budget_for_model(&self, model_id: &str) -> TokenBudget {
        let model_lower = model_id.to_lowercase();

        // 컨텍스트 윈도우 추론
        let context_window = if model_lower.contains("gemini-1.5-pro") {
            2_000_000
        } else if model_lower.contains("gemini") {
            1_000_000
        } else if model_lower.contains("claude") {
            200_000
        } else if model_lower.contains("gpt-4o") || model_lower.contains("o1") {
            128_000
        } else if model_lower.contains("llama-3.3") || model_lower.contains("deepseek") {
            128_000
        } else {
            32_000 // 기본값
        };

        // 최대 출력 토큰 추론
        let max_output = if model_lower.contains("o1") || model_lower.contains("o3") {
            100_000
        } else if model_lower.contains("claude") {
            16_000
        } else if model_lower.contains("gpt-4o") {
            16_384
        } else {
            4_096
        };

        TokenBudget::new(context_window, max_output)
    }

    /// 기본 모델 설정 등록
    fn register_defaults(&self) {
        let defaults = vec![
            // OpenAI
            ModelTokenConfig::new("gpt-4o", TokenizerType::TiktokenO200k)
                .context_window(128_000)
                .max_output(16_384)
                .message_overhead(4),
            ModelTokenConfig::new("gpt-4o-mini", TokenizerType::TiktokenO200k)
                .context_window(128_000)
                .max_output(16_384),
            ModelTokenConfig::new("o1", TokenizerType::TiktokenO200k)
                .context_window(200_000)
                .max_output(100_000),
            ModelTokenConfig::new("o3-mini", TokenizerType::TiktokenO200k)
                .context_window(200_000)
                .max_output(100_000),
            // Anthropic
            ModelTokenConfig::new("claude-sonnet-4-20250514", TokenizerType::Claude)
                .context_window(200_000)
                .max_output(16_000)
                .message_overhead(3),
            ModelTokenConfig::new("claude-opus-4-20250514", TokenizerType::Claude)
                .context_window(200_000)
                .max_output(16_000),
            ModelTokenConfig::new("claude-3-5-haiku-20241022", TokenizerType::Claude)
                .context_window(200_000)
                .max_output(8_192),
            // Google
            ModelTokenConfig::new("gemini-2.0-flash", TokenizerType::Gemini)
                .context_window(1_000_000)
                .max_output(8_192),
            ModelTokenConfig::new("gemini-1.5-pro", TokenizerType::Gemini)
                .context_window(2_000_000)
                .max_output(8_192),
            // Groq/Local
            ModelTokenConfig::new("llama-3.3-70b-versatile", TokenizerType::Llama)
                .context_window(128_000)
                .max_output(32_768),
            ModelTokenConfig::new("llama3.3", TokenizerType::Llama)
                .context_window(128_000)
                .max_output(4_096),
            ModelTokenConfig::new("deepseek-r1-distill-llama-70b", TokenizerType::Llama)
                .context_window(128_000)
                .max_output(16_384),
        ];

        let mut configs = self.model_configs.write().unwrap();
        for config in defaults {
            configs.insert(config.model_id.clone(), config);
        }
    }
}

impl Default for TokenizerFactory {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// 편의 함수
// ============================================================================

/// 모델 ID로 토큰 수 계산
pub fn count_tokens(model_id: &str, text: &str) -> usize {
    factory().for_model(model_id).count(text).total
}

/// 모델 ID로 토큰 예산 가져오기
pub fn get_budget(model_id: &str) -> TokenBudget {
    factory().budget_for_model(model_id)
}

/// 모델 ID로 텍스트 자르기
pub fn truncate_for_model(model_id: &str, text: &str, max_tokens: usize) -> String {
    factory().for_model(model_id).truncate(text, max_tokens)
}

/// 모델 ID로 토큰 제한 확인
pub fn check_limit(model_id: &str, text: &str) -> (bool, usize, usize) {
    let budget = factory().budget_for_model(model_id);
    let tokenizer = factory().for_model(model_id);
    let count = tokenizer.count(text).total;
    let available = budget.available_input();

    (count <= available, count, available)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_factory_creation() {
        let factory = TokenizerFactory::new();

        let tokenizer = factory.for_model("gpt-4o");
        assert_eq!(tokenizer.tokenizer_type(), TokenizerType::TiktokenO200k);

        let tokenizer = factory.for_model("claude-sonnet-4-20250514");
        assert_eq!(tokenizer.tokenizer_type(), TokenizerType::Claude);
    }

    #[test]
    fn test_tokenizer_inference() {
        let factory = TokenizerFactory::new();

        // 알려지지 않은 모델도 패턴으로 추론
        let tokenizer = factory.for_model("some-new-claude-model");
        assert_eq!(tokenizer.tokenizer_type(), TokenizerType::Claude);

        let tokenizer = factory.for_model("llama-4-mega");
        assert_eq!(tokenizer.tokenizer_type(), TokenizerType::Llama);
    }

    #[test]
    fn test_budget() {
        let factory = TokenizerFactory::new();

        let budget = factory.budget_for_model("gpt-4o");
        assert_eq!(budget.context_window, 128_000);
        assert_eq!(budget.max_output, 16_384);

        let budget = factory.budget_for_model("claude-sonnet-4-20250514");
        assert_eq!(budget.context_window, 200_000);
    }

    #[test]
    fn test_count_tokens() {
        let count = count_tokens("gpt-4o", "Hello, world!");
        assert!(count > 0);

        let count = count_tokens("claude-sonnet-4-20250514", "Hello, world!");
        assert!(count > 0);
    }

    #[test]
    fn test_check_limit() {
        let (within_limit, used, available) = check_limit("gpt-4o", "Hello!");
        assert!(within_limit);
        assert!(used < available);
    }

    #[test]
    fn test_caching() {
        let factory = TokenizerFactory::new();

        // 같은 타입 요청 시 캐시된 인스턴스 반환
        let t1 = factory.for_model("gpt-4o");
        let t2 = factory.for_model("gpt-4o-mini"); // 같은 TokenizerType

        assert_eq!(t1.tokenizer_type(), t2.tokenizer_type());
    }
}
