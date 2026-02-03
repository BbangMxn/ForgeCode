//! Model Registry - LLM 모델 메타데이터 관리
//!
//! 각 Provider가 지원하는 모델들의 정보를 중앙에서 관리합니다.
//! - 컨텍스트 윈도우 크기
//! - 최대 출력 토큰
//! - 지원 기능 (vision, tools, thinking 등)
//! - 가격 정보

use crate::registry::ProviderType;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;

/// 전역 모델 레지스트리
static MODEL_REGISTRY: OnceLock<ModelRegistry> = OnceLock::new();

/// 모델 레지스트리 접근
pub fn registry() -> &'static ModelRegistry {
    MODEL_REGISTRY.get_or_init(|| {
        let mut registry = ModelRegistry::new();
        registry.register_defaults();
        registry
    })
}

/// 모델 가격 정보 (USD per 1M tokens)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPricing {
    /// 입력 토큰 가격 (1M 토큰당 USD)
    pub input_per_1m: f64,
    /// 출력 토큰 가격 (1M 토큰당 USD)
    pub output_per_1m: f64,
    /// 캐시 읽기 가격 (1M 토큰당 USD, 지원하는 경우)
    pub cache_read_per_1m: Option<f64>,
    /// 캐시 쓰기 가격 (1M 토큰당 USD, 지원하는 경우)
    pub cache_write_per_1m: Option<f64>,
}

impl ModelPricing {
    pub fn new(input: f64, output: f64) -> Self {
        Self {
            input_per_1m: input,
            output_per_1m: output,
            cache_read_per_1m: None,
            cache_write_per_1m: None,
        }
    }

    pub fn with_cache(mut self, read: f64, write: f64) -> Self {
        self.cache_read_per_1m = Some(read);
        self.cache_write_per_1m = Some(write);
        self
    }

    /// 비용 계산
    pub fn calculate_cost(
        &self,
        input_tokens: u64,
        output_tokens: u64,
        cache_read_tokens: Option<u64>,
        cache_write_tokens: Option<u64>,
    ) -> f64 {
        let input_cost = (input_tokens as f64 / 1_000_000.0) * self.input_per_1m;
        let output_cost = (output_tokens as f64 / 1_000_000.0) * self.output_per_1m;

        let cache_read_cost = match (cache_read_tokens, self.cache_read_per_1m) {
            (Some(tokens), Some(price)) => (tokens as f64 / 1_000_000.0) * price,
            _ => 0.0,
        };

        let cache_write_cost = match (cache_write_tokens, self.cache_write_per_1m) {
            (Some(tokens), Some(price)) => (tokens as f64 / 1_000_000.0) * price,
            _ => 0.0,
        };

        input_cost + output_cost + cache_read_cost + cache_write_cost
    }
}

/// 모델 기능 (capabilities)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelCapabilities {
    /// 이미지 입력 지원
    pub vision: bool,
    /// Tool/Function calling 지원
    pub tools: bool,
    /// Extended thinking 지원 (Claude)
    pub thinking: bool,
    /// 스트리밍 지원
    pub streaming: bool,
    /// JSON 모드 지원
    pub json_mode: bool,
    /// 시스템 프롬프트 지원
    pub system_prompt: bool,
    /// 프롬프트 캐싱 지원
    pub prompt_caching: bool,
    /// 코드 실행 지원
    pub code_execution: bool,
    /// 웹 검색 지원
    pub web_search: bool,
}

impl ModelCapabilities {
    pub fn new() -> Self {
        Self {
            streaming: true,
            system_prompt: true,
            ..Default::default()
        }
    }

    pub fn with_vision(mut self) -> Self {
        self.vision = true;
        self
    }

    pub fn with_tools(mut self) -> Self {
        self.tools = true;
        self
    }

    pub fn with_thinking(mut self) -> Self {
        self.thinking = true;
        self
    }

    pub fn with_json_mode(mut self) -> Self {
        self.json_mode = true;
        self
    }

    pub fn with_prompt_caching(mut self) -> Self {
        self.prompt_caching = true;
        self
    }

    pub fn with_code_execution(mut self) -> Self {
        self.code_execution = true;
        self
    }

    pub fn with_web_search(mut self) -> Self {
        self.web_search = true;
        self
    }

    /// 모든 기본 기능 활성화
    pub fn full() -> Self {
        Self {
            vision: true,
            tools: true,
            thinking: false,
            streaming: true,
            json_mode: true,
            system_prompt: true,
            prompt_caching: true,
            code_execution: false,
            web_search: false,
        }
    }
}

/// 모델 정보
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// 모델 ID (API에서 사용하는 ID)
    pub id: String,
    /// 표시 이름
    pub display_name: String,
    /// Provider 타입
    pub provider: ProviderType,
    /// 컨텍스트 윈도우 크기 (토큰)
    pub context_window: u32,
    /// 최대 출력 토큰
    pub max_output_tokens: u32,
    /// 지원 기능
    pub capabilities: ModelCapabilities,
    /// 가격 정보
    pub pricing: Option<ModelPricing>,
    /// 모델 설명
    pub description: Option<String>,
    /// Deprecated 여부
    pub deprecated: bool,
    /// 추천 용도 (예: "coding", "general", "vision")
    pub recommended_for: Vec<String>,
}

impl ModelInfo {
    pub fn new(id: impl Into<String>, provider: ProviderType) -> Self {
        Self {
            id: id.into(),
            display_name: String::new(),
            provider,
            context_window: 128_000,
            max_output_tokens: 4_096,
            capabilities: ModelCapabilities::new(),
            pricing: None,
            description: None,
            deprecated: false,
            recommended_for: Vec::new(),
        }
    }

    pub fn display_name(mut self, name: impl Into<String>) -> Self {
        self.display_name = name.into();
        self
    }

    pub fn context_window(mut self, tokens: u32) -> Self {
        self.context_window = tokens;
        self
    }

    pub fn max_output_tokens(mut self, tokens: u32) -> Self {
        self.max_output_tokens = tokens;
        self
    }

    pub fn capabilities(mut self, caps: ModelCapabilities) -> Self {
        self.capabilities = caps;
        self
    }

    pub fn pricing(mut self, pricing: ModelPricing) -> Self {
        self.pricing = Some(pricing);
        self
    }

    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn deprecated(mut self) -> Self {
        self.deprecated = true;
        self
    }

    pub fn recommended_for(mut self, uses: Vec<&str>) -> Self {
        self.recommended_for = uses.into_iter().map(String::from).collect();
        self
    }

    /// 비용 계산
    pub fn calculate_cost(
        &self,
        input_tokens: u64,
        output_tokens: u64,
        cache_read_tokens: Option<u64>,
        cache_write_tokens: Option<u64>,
    ) -> Option<f64> {
        self.pricing.as_ref().map(|p| {
            p.calculate_cost(
                input_tokens,
                output_tokens,
                cache_read_tokens,
                cache_write_tokens,
            )
        })
    }
}

/// 모델 레지스트리
#[derive(Debug, Default)]
pub struct ModelRegistry {
    models: HashMap<String, ModelInfo>,
}

impl ModelRegistry {
    pub fn new() -> Self {
        Self {
            models: HashMap::new(),
        }
    }

    /// 모델 등록
    pub fn register(&mut self, model: ModelInfo) {
        self.models.insert(model.id.clone(), model);
    }

    /// 모델 조회
    pub fn get(&self, model_id: &str) -> Option<&ModelInfo> {
        self.models.get(model_id)
    }

    /// Provider별 모델 목록
    pub fn by_provider(&self, provider: ProviderType) -> Vec<&ModelInfo> {
        self.models
            .values()
            .filter(|m| m.provider == provider && !m.deprecated)
            .collect()
    }

    /// 기능별 모델 필터링
    pub fn with_capability<F>(&self, filter: F) -> Vec<&ModelInfo>
    where
        F: Fn(&ModelCapabilities) -> bool,
    {
        self.models
            .values()
            .filter(|m| !m.deprecated && filter(&m.capabilities))
            .collect()
    }

    /// Vision 지원 모델
    pub fn with_vision(&self) -> Vec<&ModelInfo> {
        self.with_capability(|c| c.vision)
    }

    /// Tools 지원 모델
    pub fn with_tools(&self) -> Vec<&ModelInfo> {
        self.with_capability(|c| c.tools)
    }

    /// 모든 모델 (deprecated 제외)
    pub fn all(&self) -> Vec<&ModelInfo> {
        self.models.values().filter(|m| !m.deprecated).collect()
    }

    /// 모든 모델 ID 목록
    pub fn model_ids(&self) -> Vec<&str> {
        self.models.keys().map(|s| s.as_str()).collect()
    }

    /// 기본 모델 등록 (주요 Provider들의 최신 모델)
    pub fn register_defaults(&mut self) {
        // ================================================================
        // Anthropic Models
        // ================================================================
        self.register(
            ModelInfo::new("claude-sonnet-4-20250514", ProviderType::Anthropic)
                .display_name("Claude Sonnet 4")
                .context_window(200_000)
                .max_output_tokens(16_000)
                .capabilities(
                    ModelCapabilities::new()
                        .with_vision()
                        .with_tools()
                        .with_thinking()
                        .with_prompt_caching(),
                )
                .pricing(ModelPricing::new(3.0, 15.0).with_cache(0.30, 3.75))
                .description("Anthropic's balanced model for most tasks")
                .recommended_for(vec!["coding", "general", "analysis"]),
        );

        self.register(
            ModelInfo::new("claude-opus-4-20250514", ProviderType::Anthropic)
                .display_name("Claude Opus 4")
                .context_window(200_000)
                .max_output_tokens(16_000)
                .capabilities(
                    ModelCapabilities::new()
                        .with_vision()
                        .with_tools()
                        .with_thinking()
                        .with_prompt_caching(),
                )
                .pricing(ModelPricing::new(15.0, 75.0).with_cache(1.50, 18.75))
                .description("Anthropic's most capable model")
                .recommended_for(vec!["complex-reasoning", "research", "coding"]),
        );

        self.register(
            ModelInfo::new("claude-3-5-haiku-20241022", ProviderType::Anthropic)
                .display_name("Claude 3.5 Haiku")
                .context_window(200_000)
                .max_output_tokens(8_192)
                .capabilities(
                    ModelCapabilities::new()
                        .with_vision()
                        .with_tools()
                        .with_prompt_caching(),
                )
                .pricing(ModelPricing::new(0.80, 4.0).with_cache(0.08, 1.0))
                .description("Fast and affordable model")
                .recommended_for(vec!["quick-tasks", "chat", "simple-coding"]),
        );

        // ================================================================
        // OpenAI Models
        // ================================================================
        self.register(
            ModelInfo::new("gpt-4o", ProviderType::Openai)
                .display_name("GPT-4o")
                .context_window(128_000)
                .max_output_tokens(16_384)
                .capabilities(
                    ModelCapabilities::new()
                        .with_vision()
                        .with_tools()
                        .with_json_mode(),
                )
                .pricing(ModelPricing::new(2.50, 10.0))
                .description("OpenAI's flagship multimodal model")
                .recommended_for(vec!["general", "vision", "coding"]),
        );

        self.register(
            ModelInfo::new("gpt-4o-mini", ProviderType::Openai)
                .display_name("GPT-4o Mini")
                .context_window(128_000)
                .max_output_tokens(16_384)
                .capabilities(
                    ModelCapabilities::new()
                        .with_vision()
                        .with_tools()
                        .with_json_mode(),
                )
                .pricing(ModelPricing::new(0.15, 0.60))
                .description("Affordable and fast GPT-4o variant")
                .recommended_for(vec!["quick-tasks", "chat"]),
        );

        self.register(
            ModelInfo::new("o1", ProviderType::Openai)
                .display_name("o1")
                .context_window(200_000)
                .max_output_tokens(100_000)
                .capabilities(
                    ModelCapabilities::new()
                        .with_vision()
                        .with_tools()
                        .with_thinking(),
                )
                .pricing(ModelPricing::new(15.0, 60.0))
                .description("OpenAI's reasoning model")
                .recommended_for(vec!["complex-reasoning", "math", "coding"]),
        );

        self.register(
            ModelInfo::new("o3-mini", ProviderType::Openai)
                .display_name("o3-mini")
                .context_window(200_000)
                .max_output_tokens(100_000)
                .capabilities(ModelCapabilities::new().with_tools().with_thinking())
                .pricing(ModelPricing::new(1.10, 4.40))
                .description("Fast reasoning model")
                .recommended_for(vec!["coding", "math"]),
        );

        // ================================================================
        // Google Gemini Models
        // ================================================================
        self.register(
            ModelInfo::new("gemini-2.0-flash", ProviderType::Gemini)
                .display_name("Gemini 2.0 Flash")
                .context_window(1_000_000)
                .max_output_tokens(8_192)
                .capabilities(
                    ModelCapabilities::new()
                        .with_vision()
                        .with_tools()
                        .with_code_execution()
                        .with_web_search(),
                )
                .pricing(ModelPricing::new(0.10, 0.40))
                .description("Google's fast multimodal model with massive context")
                .recommended_for(vec!["large-context", "vision", "general"]),
        );

        self.register(
            ModelInfo::new("gemini-2.0-flash-thinking", ProviderType::Gemini)
                .display_name("Gemini 2.0 Flash Thinking")
                .context_window(1_000_000)
                .max_output_tokens(8_192)
                .capabilities(
                    ModelCapabilities::new()
                        .with_vision()
                        .with_tools()
                        .with_thinking(),
                )
                .description("Gemini with extended thinking")
                .recommended_for(vec!["complex-reasoning", "analysis"]),
        );

        self.register(
            ModelInfo::new("gemini-1.5-pro", ProviderType::Gemini)
                .display_name("Gemini 1.5 Pro")
                .context_window(2_000_000)
                .max_output_tokens(8_192)
                .capabilities(ModelCapabilities::new().with_vision().with_tools())
                .pricing(ModelPricing::new(1.25, 5.0))
                .description("Google's largest context window model")
                .recommended_for(vec!["large-context", "analysis"]),
        );

        // ================================================================
        // Groq Models (LPU - fast inference)
        // ================================================================
        self.register(
            ModelInfo::new("llama-3.3-70b-versatile", ProviderType::Groq)
                .display_name("Llama 3.3 70B")
                .context_window(128_000)
                .max_output_tokens(32_768)
                .capabilities(ModelCapabilities::new().with_tools().with_json_mode())
                .pricing(ModelPricing::new(0.59, 0.79))
                .description("Meta's Llama 3.3 on Groq LPU - ultra fast")
                .recommended_for(vec!["fast-inference", "coding", "general"]),
        );

        self.register(
            ModelInfo::new("mixtral-8x7b-32768", ProviderType::Groq)
                .display_name("Mixtral 8x7B")
                .context_window(32_768)
                .max_output_tokens(32_768)
                .capabilities(ModelCapabilities::new().with_tools())
                .pricing(ModelPricing::new(0.24, 0.24))
                .description("Mistral's MoE model on Groq")
                .recommended_for(vec!["fast-inference", "general"]),
        );

        self.register(
            ModelInfo::new("deepseek-r1-distill-llama-70b", ProviderType::Groq)
                .display_name("DeepSeek R1 Distill 70B")
                .context_window(128_000)
                .max_output_tokens(16_384)
                .capabilities(ModelCapabilities::new().with_tools().with_thinking())
                .description("DeepSeek R1 distilled model on Groq")
                .recommended_for(vec!["reasoning", "coding"]),
        );

        // ================================================================
        // Ollama Models (Local)
        // ================================================================
        self.register(
            ModelInfo::new("llama3.3", ProviderType::Ollama)
                .display_name("Llama 3.3 (Local)")
                .context_window(128_000)
                .max_output_tokens(4_096)
                .capabilities(ModelCapabilities::new().with_tools())
                .description("Meta's Llama 3.3 running locally")
                .recommended_for(vec!["local", "privacy", "offline"]),
        );

        self.register(
            ModelInfo::new("codellama", ProviderType::Ollama)
                .display_name("Code Llama (Local)")
                .context_window(16_000)
                .max_output_tokens(4_096)
                .capabilities(ModelCapabilities::new())
                .description("Meta's Code Llama for coding tasks")
                .recommended_for(vec!["local", "coding"]),
        );

        self.register(
            ModelInfo::new("deepseek-coder-v2", ProviderType::Ollama)
                .display_name("DeepSeek Coder V2 (Local)")
                .context_window(128_000)
                .max_output_tokens(4_096)
                .capabilities(ModelCapabilities::new().with_tools())
                .description("DeepSeek's coding model")
                .recommended_for(vec!["local", "coding"]),
        );

        self.register(
            ModelInfo::new("qwen2.5-coder", ProviderType::Ollama)
                .display_name("Qwen 2.5 Coder (Local)")
                .context_window(32_000)
                .max_output_tokens(4_096)
                .capabilities(ModelCapabilities::new().with_tools())
                .description("Alibaba's Qwen coding model")
                .recommended_for(vec!["local", "coding"]),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_registry() {
        let registry = registry();

        // Claude 모델 확인
        let claude = registry.get("claude-sonnet-4-20250514");
        assert!(claude.is_some());
        let claude = claude.unwrap();
        assert_eq!(claude.context_window, 200_000);
        assert!(claude.capabilities.tools);
        assert!(claude.capabilities.vision);
    }

    #[test]
    fn test_cost_calculation() {
        let pricing = ModelPricing::new(3.0, 15.0).with_cache(0.30, 3.75);

        // 1000 input, 500 output tokens
        let cost = pricing.calculate_cost(1000, 500, None, None);
        // (1000/1M) * 3.0 + (500/1M) * 15.0 = 0.003 + 0.0075 = 0.0105
        assert!((cost - 0.0105).abs() < 0.0001);
    }

    #[test]
    fn test_filter_by_capability() {
        let registry = registry();

        let vision_models = registry.with_vision();
        assert!(!vision_models.is_empty());

        let tools_models = registry.with_tools();
        assert!(!tools_models.is_empty());
    }

    #[test]
    fn test_filter_by_provider() {
        let registry = registry();

        let anthropic_models = registry.by_provider(ProviderType::Anthropic);
        assert!(!anthropic_models.is_empty());
        for model in anthropic_models {
            assert_eq!(model.provider, ProviderType::Anthropic);
        }
    }
}
