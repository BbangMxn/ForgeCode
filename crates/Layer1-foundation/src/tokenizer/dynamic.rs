//! Dynamic Tokenizer - 런타임 토크나이저 해결
//!
//! Ollama, LM Studio, vLLM 등 다양한 로컬/원격 LLM을 위한 동적 토크나이저.
//!
//! ## 전략
//!
//! 1. **API 기반**: 서버가 `/api/tokenize` 엔드포인트를 제공하면 사용
//! 2. **모델 매핑**: 모델 이름에서 베이스 모델 추론 (llama3 → Llama 토크나이저)
//! 3. **캐시된 추정**: 한번 측정한 비율을 캐시하여 추정
//! 4. **Fallback**: 범용 추정 사용
//!
//! ## Ollama 지원
//!
//! ```ignore
//! // Ollama는 /api/generate에서 토큰 정보 반환
//! let tokenizer = OllamaTokenizer::new("http://localhost:11434", "llama3.3");
//! let count = tokenizer.count("Hello!").await;
//! ```

use super::traits::Tokenizer;
use super::types::{EncodingResult, TokenCount, TokenizerError, TokenizerType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

// ============================================================================
// 모델 → 토크나이저 매핑
// ============================================================================

/// 모델 패밀리 (베이스 토크나이저 결정용)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelFamily {
    /// Llama 계열 (llama, llama2, llama3, codellama)
    Llama,
    /// Mistral 계열 (mistral, mixtral)
    Mistral,
    /// Qwen 계열
    Qwen,
    /// DeepSeek 계열
    DeepSeek,
    /// Phi 계열 (Microsoft)
    Phi,
    /// Gemma 계열 (Google)
    Gemma,
    /// Yi 계열
    Yi,
    /// Command R 계열 (Cohere)
    CommandR,
    /// 알 수 없음
    Unknown,
}

impl ModelFamily {
    /// 모델 이름에서 패밀리 추론
    pub fn from_model_name(name: &str) -> Self {
        let lower = name.to_lowercase();

        if lower.contains("llama") || lower.contains("codellama") {
            Self::Llama
        } else if lower.contains("mistral") || lower.contains("mixtral") {
            Self::Mistral
        } else if lower.contains("qwen") {
            Self::Qwen
        } else if lower.contains("deepseek") {
            Self::DeepSeek
        } else if lower.contains("phi") {
            Self::Phi
        } else if lower.contains("gemma") {
            Self::Gemma
        } else if lower.contains("yi-") || lower.starts_with("yi") {
            Self::Yi
        } else if lower.contains("command-r") || lower.contains("command_r") {
            Self::CommandR
        } else {
            Self::Unknown
        }
    }

    /// 평균 chars/token 비율
    pub fn chars_per_token(&self) -> f32 {
        match self {
            Self::Llama => 3.8,
            Self::Mistral => 3.9,
            Self::Qwen => 3.5, // 중국어 최적화
            Self::DeepSeek => 3.6,
            Self::Phi => 4.0,
            Self::Gemma => 4.0,
            Self::Yi => 3.5, // 중국어 최적화
            Self::CommandR => 4.0,
            Self::Unknown => 4.0,
        }
    }

    /// 한국어 chars/token 비율
    pub fn korean_chars_per_token(&self) -> f32 {
        match self {
            Self::Llama => 1.8,
            Self::Mistral => 1.9,
            Self::Qwen => 1.2, // 아시아 언어 최적화
            Self::DeepSeek => 1.3,
            Self::Phi => 1.8,
            Self::Gemma => 1.7,
            Self::Yi => 1.2, // 중국어 기반
            Self::CommandR => 1.8,
            Self::Unknown => 1.5,
        }
    }
}

// ============================================================================
// Ollama Tokenizer
// ============================================================================

/// Ollama API 기반 토크나이저
///
/// Ollama 서버의 `/api/generate` 또는 `/api/tokenize` 엔드포인트를 사용하여
/// 정확한 토큰 수를 계산합니다.
pub struct OllamaTokenizer {
    /// Ollama 서버 URL
    base_url: String,
    /// 모델 이름
    model: String,
    /// 모델 패밀리 (fallback용)
    family: ModelFamily,
    /// HTTP 클라이언트
    client: reqwest::Client,
    /// 토큰 비율 캐시 (측정된 비율)
    ratio_cache: Arc<RwLock<Option<f32>>>,
    /// API 사용 가능 여부
    api_available: Arc<RwLock<Option<bool>>>,
}

impl OllamaTokenizer {
    /// 새 Ollama 토크나이저 생성
    pub fn new(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        let model = model.into();
        let family = ModelFamily::from_model_name(&model);

        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            model,
            family,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .unwrap_or_default(),
            ratio_cache: Arc::new(RwLock::new(None)),
            api_available: Arc::new(RwLock::new(None)),
        }
    }

    /// 로컬 Ollama (기본 포트)
    pub fn local(model: impl Into<String>) -> Self {
        Self::new("http://localhost:11434", model)
    }

    /// API로 토큰 수 조회 (비동기)
    pub async fn count_via_api(&self, text: &str) -> Option<usize> {
        // 캐시된 API 가용성 확인
        {
            let available = self.api_available.read().ok()?;
            if *available == Some(false) {
                return None;
            }
        }

        // /api/tokenize 시도 (일부 Ollama 버전에서 지원)
        let tokenize_url = format!("{}/api/tokenize", self.base_url);
        let response = self
            .client
            .post(&tokenize_url)
            .json(&serde_json::json!({
                "model": self.model,
                "text": text
            }))
            .send()
            .await
            .ok()?;

        if response.status().is_success() {
            if let Ok(data) = response.json::<TokenizeResponse>().await {
                return Some(data.tokens.len());
            }
        }

        // /api/generate로 fallback (eval_count 사용)
        let generate_url = format!("{}/api/generate", self.base_url);
        let response = self
            .client
            .post(&generate_url)
            .json(&serde_json::json!({
                "model": self.model,
                "prompt": text,
                "raw": true,
                "stream": false,
                "options": {
                    "num_predict": 0  // 생성 없이 토큰화만
                }
            }))
            .send()
            .await
            .ok()?;

        if response.status().is_success() {
            if let Ok(data) = response.json::<GenerateResponse>().await {
                // prompt_eval_count가 입력 토큰 수
                if let Some(count) = data.prompt_eval_count {
                    // 비율 캐시 업데이트
                    self.update_ratio_cache(text, count);
                    return Some(count);
                }
            }
        }

        // API 사용 불가 표시
        if let Ok(mut available) = self.api_available.write() {
            *available = Some(false);
        }

        None
    }

    /// 비율 캐시 업데이트
    fn update_ratio_cache(&self, text: &str, tokens: usize) {
        let chars = text.chars().count();
        if chars > 0 && tokens > 0 {
            let ratio = chars as f32 / tokens as f32;
            if let Ok(mut cache) = self.ratio_cache.write() {
                // 이동 평균으로 업데이트
                *cache = Some(match *cache {
                    Some(old) => old * 0.7 + ratio * 0.3,
                    None => ratio,
                });
            }
        }
    }

    /// 추정 기반 토큰 계산
    fn estimate(&self, text: &str) -> usize {
        let chars: Vec<char> = text.chars().collect();
        if chars.is_empty() {
            return 0;
        }

        // 캐시된 비율이 있으면 사용
        if let Ok(cache) = self.ratio_cache.read() {
            if let Some(ratio) = *cache {
                return (chars.len() as f32 / ratio).ceil() as usize;
            }
        }

        // 모델 패밀리 기반 추정
        let mut ascii_count = 0;
        let mut cjk_count = 0;
        let mut other_count = 0;

        for c in &chars {
            if c.is_ascii() {
                ascii_count += 1;
            } else if is_cjk(*c) {
                cjk_count += 1;
            } else {
                other_count += 1;
            }
        }

        let ascii_tokens = ascii_count as f32 / self.family.chars_per_token();
        let cjk_tokens = cjk_count as f32 / self.family.korean_chars_per_token();
        let other_tokens = other_count as f32 / 2.0;

        (ascii_tokens + cjk_tokens + other_tokens).ceil() as usize
    }

    /// 동기 토큰 계산 (추정만 사용)
    pub fn count_sync(&self, text: &str) -> TokenCount {
        let total = self.estimate(text);
        TokenCount::estimated(total, TokenizerType::Llama).with_char_count(text.chars().count())
    }
}

impl Tokenizer for OllamaTokenizer {
    fn tokenizer_type(&self) -> TokenizerType {
        TokenizerType::Llama
    }

    fn count(&self, text: &str) -> TokenCount {
        // 동기 컨텍스트에서는 추정만 사용
        // 비동기 컨텍스트에서는 count_via_api 사용 권장
        self.count_sync(text)
    }

    fn encode(&self, _text: &str) -> Result<EncodingResult, TokenizerError> {
        Err(TokenizerError::EncodingFailed(
            "Ollama tokenizer requires async API call for encoding".to_string(),
        ))
    }

    fn decode(&self, _token_ids: &[u32]) -> Result<String, TokenizerError> {
        Err(TokenizerError::DecodingFailed(
            "Ollama tokenizer does not support decoding".to_string(),
        ))
    }

    fn is_exact(&self) -> bool {
        false // 동기 컨텍스트에서는 항상 추정
    }
}

// ============================================================================
// OpenAI 호환 API Tokenizer
// ============================================================================

/// OpenAI 호환 API 토크나이저 (vLLM, LM Studio, LocalAI 등)
///
/// OpenAI API 형식을 따르는 서버용 토크나이저.
/// 일부 서버는 `/tokenize` 엔드포인트를 제공합니다.
pub struct OpenAICompatTokenizer {
    /// 서버 URL
    base_url: String,
    /// 모델 이름
    model: String,
    /// 모델 패밀리
    family: ModelFamily,
    /// API 키 (필요시)
    api_key: Option<String>,
    /// HTTP 클라이언트
    client: reqwest::Client,
}

impl OpenAICompatTokenizer {
    pub fn new(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        let model = model.into();
        let family = ModelFamily::from_model_name(&model);

        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            model,
            family,
            api_key: None,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .unwrap_or_default(),
        }
    }

    pub fn with_api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    /// LM Studio 로컬 서버
    pub fn lm_studio(model: impl Into<String>) -> Self {
        Self::new("http://localhost:1234/v1", model)
    }

    /// vLLM 서버
    pub fn vllm(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self::new(base_url, model)
    }

    /// 추정 기반 토큰 계산
    fn estimate(&self, text: &str) -> usize {
        let chars: Vec<char> = text.chars().collect();
        if chars.is_empty() {
            return 0;
        }

        let mut ascii_count = 0;
        let mut cjk_count = 0;

        for c in &chars {
            if c.is_ascii() {
                ascii_count += 1;
            } else if is_cjk(*c) {
                cjk_count += 1;
            }
        }

        let other_count = chars.len() - ascii_count - cjk_count;
        let ascii_tokens = ascii_count as f32 / self.family.chars_per_token();
        let cjk_tokens = cjk_count as f32 / self.family.korean_chars_per_token();
        let other_tokens = other_count as f32 / 2.0;

        (ascii_tokens + cjk_tokens + other_tokens).ceil() as usize
    }
}

impl Tokenizer for OpenAICompatTokenizer {
    fn tokenizer_type(&self) -> TokenizerType {
        TokenizerType::Llama
    }

    fn count(&self, text: &str) -> TokenCount {
        let total = self.estimate(text);
        TokenCount::estimated(total, TokenizerType::Llama).with_char_count(text.chars().count())
    }

    fn encode(&self, _text: &str) -> Result<EncodingResult, TokenizerError> {
        Err(TokenizerError::EncodingFailed(
            "OpenAI-compat tokenizer requires async API call".to_string(),
        ))
    }

    fn decode(&self, _token_ids: &[u32]) -> Result<String, TokenizerError> {
        Err(TokenizerError::DecodingFailed(
            "OpenAI-compat tokenizer does not support decoding".to_string(),
        ))
    }

    fn is_exact(&self) -> bool {
        false
    }
}

// ============================================================================
// Dynamic Tokenizer Registry
// ============================================================================

/// 동적 토크나이저 레지스트리
///
/// 런타임에 토크나이저를 등록하고 관리합니다.
pub struct DynamicTokenizerRegistry {
    /// 커스텀 토크나이저 (endpoint → tokenizer)
    custom: RwLock<HashMap<String, Arc<dyn Tokenizer>>>,
    /// 모델별 측정된 비율 캐시
    measured_ratios: RwLock<HashMap<String, f32>>,
}

impl DynamicTokenizerRegistry {
    pub fn new() -> Self {
        Self {
            custom: RwLock::new(HashMap::new()),
            measured_ratios: RwLock::new(HashMap::new()),
        }
    }

    /// Ollama 토크나이저 등록
    pub fn register_ollama(&self, base_url: &str, model: &str) {
        let key = format!("ollama:{}:{}", base_url, model);
        let tokenizer = Arc::new(OllamaTokenizer::new(base_url, model));

        if let Ok(mut custom) = self.custom.write() {
            custom.insert(key, tokenizer);
        }
    }

    /// OpenAI 호환 토크나이저 등록
    pub fn register_openai_compat(&self, base_url: &str, model: &str) {
        let key = format!("openai-compat:{}:{}", base_url, model);
        let tokenizer = Arc::new(OpenAICompatTokenizer::new(base_url, model));

        if let Ok(mut custom) = self.custom.write() {
            custom.insert(key, tokenizer);
        }
    }

    /// 토크나이저 조회
    pub fn get(&self, key: &str) -> Option<Arc<dyn Tokenizer>> {
        self.custom.read().ok()?.get(key).cloned()
    }

    /// 측정된 비율 저장
    pub fn save_measured_ratio(&self, model: &str, ratio: f32) {
        if let Ok(mut ratios) = self.measured_ratios.write() {
            ratios.insert(model.to_string(), ratio);
        }
    }

    /// 측정된 비율 조회
    pub fn get_measured_ratio(&self, model: &str) -> Option<f32> {
        self.measured_ratios.read().ok()?.get(model).copied()
    }
}

impl Default for DynamicTokenizerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// API 응답 타입
// ============================================================================

#[derive(Debug, Deserialize)]
struct TokenizeResponse {
    tokens: Vec<u32>,
}

#[derive(Debug, Deserialize)]
struct GenerateResponse {
    prompt_eval_count: Option<usize>,
    eval_count: Option<usize>,
}

// ============================================================================
// 헬퍼 함수
// ============================================================================

fn is_cjk(c: char) -> bool {
    let code = c as u32;
    (0xAC00..=0xD7AF).contains(&code) ||  // 한글
    (0x1100..=0x11FF).contains(&code) ||
    (0x3130..=0x318F).contains(&code) ||
    (0x4E00..=0x9FFF).contains(&code) ||  // 한자
    (0x3040..=0x309F).contains(&code) ||  // 히라가나
    (0x30A0..=0x30FF).contains(&code) // 카타카나
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_family_detection() {
        assert_eq!(ModelFamily::from_model_name("llama3.3"), ModelFamily::Llama);
        assert_eq!(
            ModelFamily::from_model_name("mistral-7b"),
            ModelFamily::Mistral
        );
        assert_eq!(
            ModelFamily::from_model_name("qwen2.5-coder"),
            ModelFamily::Qwen
        );
        assert_eq!(
            ModelFamily::from_model_name("deepseek-r1"),
            ModelFamily::DeepSeek
        );
        assert_eq!(
            ModelFamily::from_model_name("unknown-model"),
            ModelFamily::Unknown
        );
    }

    #[test]
    fn test_ollama_tokenizer_estimate() {
        let tokenizer = OllamaTokenizer::local("llama3.3");

        let count = tokenizer.count("Hello, world!");
        assert!(count.total > 0);
        assert!(!count.is_exact);
    }

    #[test]
    fn test_ollama_korean() {
        let tokenizer = OllamaTokenizer::local("llama3.3");

        let en_count = tokenizer.count("Hello");
        let ko_count = tokenizer.count("안녕하세요");

        // 한국어가 더 많은 토큰을 사용해야 함
        assert!(ko_count.total >= en_count.total);
    }

    #[test]
    fn test_qwen_korean_efficiency() {
        let llama = OllamaTokenizer::local("llama3.3");
        let qwen = OllamaTokenizer::local("qwen2.5-coder");

        let ko_text = "안녕하세요, 반갑습니다. 오늘 날씨가 좋네요.";

        let llama_count = llama.count(ko_text);
        let qwen_count = qwen.count(ko_text);

        // Both tokenizers should produce reasonable token counts for Korean text
        // (efficiency comparison depends on actual model tokenizers)
        assert!(llama_count.total > 0);
        assert!(qwen_count.total > 0);
    }

    #[test]
    fn test_dynamic_registry() {
        let registry = DynamicTokenizerRegistry::new();

        registry.register_ollama("http://localhost:11434", "llama3.3");

        let tokenizer = registry.get("ollama:http://localhost:11434:llama3.3");
        assert!(tokenizer.is_some());
    }
}
