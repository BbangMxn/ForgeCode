//! 토크나이저 구현체들
//!
//! - TiktokenEstimator: OpenAI 모델용 (tiktoken)
//! - ClaudeEstimator: Anthropic 모델용
//! - GeminiEstimator: Google 모델용
//! - LlamaEstimator: Llama/Mistral 모델용
//! - EstimateTokenizer: 범용 추정

use super::traits::Tokenizer;
use super::types::{EncodingResult, TokenCount, TokenizerError, TokenizerType};

// ============================================================================
// 기본 추정 토크나이저
// ============================================================================

/// 문자 기반 추정 토크나이저 (fallback)
pub struct EstimateTokenizer {
    tokenizer_type: TokenizerType,
}

impl EstimateTokenizer {
    pub fn new(tokenizer_type: TokenizerType) -> Self {
        Self { tokenizer_type }
    }

    /// 텍스트의 언어 특성을 분석하여 토큰 수 추정
    ///
    /// Performance optimized:
    /// - Single-pass iteration (no Vec allocation)
    /// - Early exit for empty text
    /// - Inline character classification
    #[inline]
    fn estimate_tokens(&self, text: &str) -> usize {
        if text.is_empty() {
            return 0;
        }

        // Single-pass character analysis (no allocation)
        let mut ascii_count = 0u32;
        let mut cjk_count = 0u32;
        let mut other_count = 0u32;

        for c in text.chars() {
            if c.is_ascii() {
                ascii_count += 1;
            } else if is_cjk(c) {
                cjk_count += 1;
            } else {
                other_count += 1;
            }
        }

        // 각 언어별 토큰 비율 적용
        let chars_per_token = self.tokenizer_type.chars_per_token();
        let korean_ratio = self.tokenizer_type.korean_chars_per_token();

        let ascii_tokens = ascii_count as f32 / chars_per_token;
        let cjk_tokens = cjk_count as f32 / korean_ratio;
        let other_tokens = other_count as f32 / 2.0; // 기타 유니코드

        (ascii_tokens + cjk_tokens + other_tokens).ceil() as usize
    }
}

impl Tokenizer for EstimateTokenizer {
    fn tokenizer_type(&self) -> TokenizerType {
        self.tokenizer_type
    }

    fn count(&self, text: &str) -> TokenCount {
        let total = self.estimate_tokens(text);
        TokenCount::estimated(total, self.tokenizer_type).with_char_count(text.chars().count())
    }

    fn encode(&self, _text: &str) -> Result<EncodingResult, TokenizerError> {
        // 추정 기반이므로 실제 인코딩은 지원하지 않음
        Err(TokenizerError::EncodingFailed(
            "Estimate tokenizer does not support encoding".to_string(),
        ))
    }

    fn decode(&self, _token_ids: &[u32]) -> Result<String, TokenizerError> {
        Err(TokenizerError::DecodingFailed(
            "Estimate tokenizer does not support decoding".to_string(),
        ))
    }

    fn is_exact(&self) -> bool {
        false
    }
}

// ============================================================================
// Tiktoken (OpenAI)
// ============================================================================

/// OpenAI tiktoken 기반 토크나이저
///
/// tiktoken-rs 크레이트를 사용하여 정확한 토큰 계산을 수행합니다.
/// tiktoken-rs가 없는 경우 추정 기반으로 동작합니다.
pub struct TiktokenEstimator {
    tokenizer_type: TokenizerType,
    /// tiktoken 인코더 (옵션)
    #[cfg(feature = "tiktoken")]
    encoder: Option<tiktoken_rs::CoreBPE>,
}

impl TiktokenEstimator {
    pub fn cl100k() -> Self {
        Self::new(TokenizerType::TiktokenCl100k)
    }

    pub fn o200k() -> Self {
        Self::new(TokenizerType::TiktokenO200k)
    }

    pub fn new(tokenizer_type: TokenizerType) -> Self {
        Self {
            tokenizer_type,
            #[cfg(feature = "tiktoken")]
            encoder: Self::init_encoder(&tokenizer_type),
        }
    }

    #[cfg(feature = "tiktoken")]
    fn init_encoder(tokenizer_type: &TokenizerType) -> Option<tiktoken_rs::CoreBPE> {
        match tokenizer_type {
            TokenizerType::TiktokenCl100k => tiktoken_rs::cl100k_base().ok(),
            TokenizerType::TiktokenO200k => tiktoken_rs::o200k_base().ok(),
            _ => None,
        }
    }

    fn estimate_tokens(&self, text: &str) -> usize {
        // tiktoken이 없거나 초기화 실패 시 추정
        EstimateTokenizer::new(self.tokenizer_type)
            .count(text)
            .total
    }
}

impl Tokenizer for TiktokenEstimator {
    fn tokenizer_type(&self) -> TokenizerType {
        self.tokenizer_type
    }

    fn count(&self, text: &str) -> TokenCount {
        #[cfg(feature = "tiktoken")]
        if let Some(ref encoder) = self.encoder {
            let tokens = encoder.encode_with_special_tokens(text);
            return TokenCount::exact(tokens.len(), self.tokenizer_type)
                .with_char_count(text.chars().count());
        }

        // Fallback to estimation
        let total = self.estimate_tokens(text);
        TokenCount::estimated(total, self.tokenizer_type).with_char_count(text.chars().count())
    }

    fn encode(&self, _text: &str) -> Result<EncodingResult, TokenizerError> {
        #[cfg(feature = "tiktoken")]
        if let Some(ref encoder) = self.encoder {
            let tokens = encoder.encode_with_special_tokens(_text);
            return Ok(EncodingResult::new(
                tokens.iter().map(|&t| t as u32).collect(),
            ));
        }

        Err(TokenizerError::EncodingFailed(
            "tiktoken not available".to_string(),
        ))
    }

    fn decode(&self, _token_ids: &[u32]) -> Result<String, TokenizerError> {
        #[cfg(feature = "tiktoken")]
        if let Some(ref encoder) = self.encoder {
            let ids: Vec<usize> = _token_ids.iter().map(|&t| t as usize).collect();
            return encoder
                .decode(ids)
                .map_err(|e| TokenizerError::DecodingFailed(e.to_string()));
        }

        Err(TokenizerError::DecodingFailed(
            "tiktoken not available".to_string(),
        ))
    }

    fn is_exact(&self) -> bool {
        #[cfg(feature = "tiktoken")]
        return self.encoder.is_some();

        #[cfg(not(feature = "tiktoken"))]
        false
    }
}

// ============================================================================
// Claude (Anthropic)
// ============================================================================

/// Claude 토크나이저
///
/// Anthropic API의 `count_tokens` 엔드포인트를 사용하여 정확한 토큰 계산이 가능합니다.
/// API 키가 없거나 API 호출이 실패할 경우 추정 기반으로 폴백합니다.
///
/// ## 정확도 향상 전략
///
/// 1. **API 기반 (정확)**: `ANTHROPIC_API_KEY` 설정 시 API 호출
/// 2. **캐시 기반 학습**: 이전 API 결과에서 chars/token 비율을 학습
/// 3. **추정 기반 (폴백)**: 언어별 최적화된 비율 사용
///
/// ## 언어별 토큰 효율 (Claude 특성)
///
/// - 영어: ~3.5 chars/token
/// - 한국어: ~1.3 chars/token (매우 효율적)
/// - 중국어: ~1.5 chars/token
/// - 코드: ~3.0 chars/token
pub struct ClaudeEstimator {
    base: EstimateTokenizer,
    /// API 기반 토큰 계산용 설정
    api_config: Option<ClaudeApiConfig>,
    /// 학습된 chars/token 비율 캐시
    learned_ratio: std::sync::RwLock<Option<LearnedRatio>>,
}

/// Claude API 설정
#[derive(Debug, Clone)]
pub struct ClaudeApiConfig {
    /// API 키
    pub api_key: String,
    /// 베이스 URL (기본: https://api.anthropic.com)
    pub base_url: String,
    /// 모델 ID (토큰 계산에 사용)
    pub model: String,
    /// 타임아웃 (밀리초)
    pub timeout_ms: u64,
}

impl Default for ClaudeApiConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: "https://api.anthropic.com".to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
            timeout_ms: 5000,
        }
    }
}

impl ClaudeApiConfig {
    /// 환경변수에서 API 설정 로드
    pub fn from_env() -> Option<Self> {
        let api_key = std::env::var("ANTHROPIC_API_KEY").ok()?;
        Some(Self {
            api_key,
            ..Default::default()
        })
    }

    /// 모델 지정
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }
}

/// 학습된 비율 정보
#[derive(Debug, Clone)]
struct LearnedRatio {
    /// 영어 chars/token
    english_ratio: f32,
    /// 한국어 chars/token
    korean_ratio: f32,
    /// 코드 chars/token
    code_ratio: f32,
    /// 샘플 수
    sample_count: u32,
    /// 마지막 업데이트
    last_updated: std::time::Instant,
}

impl Default for LearnedRatio {
    fn default() -> Self {
        Self {
            english_ratio: 3.5,
            korean_ratio: 1.3,
            code_ratio: 3.0,
            sample_count: 0,
            last_updated: std::time::Instant::now(),
        }
    }
}

impl ClaudeEstimator {
    pub fn new() -> Self {
        Self {
            base: EstimateTokenizer::new(TokenizerType::Claude),
            api_config: ClaudeApiConfig::from_env(),
            learned_ratio: std::sync::RwLock::new(None),
        }
    }

    /// API 설정으로 생성
    pub fn with_api(api_config: ClaudeApiConfig) -> Self {
        Self {
            base: EstimateTokenizer::new(TokenizerType::Claude),
            api_config: Some(api_config),
            learned_ratio: std::sync::RwLock::new(None),
        }
    }

    /// API 키만으로 생성
    pub fn with_api_key(api_key: impl Into<String>) -> Self {
        Self {
            base: EstimateTokenizer::new(TokenizerType::Claude),
            api_config: Some(ClaudeApiConfig {
                api_key: api_key.into(),
                ..Default::default()
            }),
            learned_ratio: std::sync::RwLock::new(None),
        }
    }

    /// API 사용 가능 여부
    pub fn has_api(&self) -> bool {
        self.api_config
            .as_ref()
            .map(|c| !c.api_key.is_empty())
            .unwrap_or(false)
    }

    /// 학습된 비율 업데이트 (API 결과 기반)
    pub fn update_learned_ratio(&self, text: &str, actual_tokens: usize) {
        if actual_tokens == 0 || text.is_empty() {
            return;
        }

        let chars: Vec<char> = text.chars().collect();
        let total_chars = chars.len();

        // 텍스트 특성 분석
        let mut ascii_count = 0;
        let mut cjk_count = 0;

        for c in &chars {
            if c.is_ascii() {
                ascii_count += 1;
            } else if is_cjk(*c) {
                cjk_count += 1;
            }
        }

        let actual_ratio = total_chars as f32 / actual_tokens as f32;

        if let Ok(mut guard) = self.learned_ratio.write() {
            let ratio = guard.get_or_insert_with(LearnedRatio::default);

            // 이동 평균으로 비율 업데이트
            let weight = 0.3; // 새 데이터 가중치

            // 주요 언어 비율 업데이트
            if ascii_count > total_chars / 2 {
                // 영어 위주
                ratio.english_ratio = ratio.english_ratio * (1.0 - weight) + actual_ratio * weight;
            } else if cjk_count > total_chars / 3 {
                // CJK 위주
                ratio.korean_ratio = ratio.korean_ratio * (1.0 - weight) + actual_ratio * weight;
            }

            // 코드 감지
            if detect_code_ratio(text) > 0.3 {
                ratio.code_ratio = ratio.code_ratio * (1.0 - weight) + actual_ratio * weight;
            }

            ratio.sample_count += 1;
            ratio.last_updated = std::time::Instant::now();
        }
    }

    /// Claude 특화 토큰 추정 (학습된 비율 사용)
    fn estimate_with_learned_ratio(&self, text: &str) -> usize {
        let chars: Vec<char> = text.chars().collect();
        let total_chars = chars.len();

        if total_chars == 0 {
            return 0;
        }

        // 텍스트 특성 분석
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

        // 학습된 비율 또는 기본값 사용
        let (english_ratio, korean_ratio, code_ratio) = if let Ok(guard) = self.learned_ratio.read()
        {
            if let Some(ref ratio) = *guard {
                (ratio.english_ratio, ratio.korean_ratio, ratio.code_ratio)
            } else {
                (3.5, 1.3, 3.0) // Claude 기본값
            }
        } else {
            (3.5, 1.3, 3.0)
        };

        // 코드 비율 감지
        let code_ratio_in_text = detect_code_ratio(text);

        // 토큰 계산
        let ascii_tokens = if code_ratio_in_text > 0.3 {
            ascii_count as f32 / code_ratio
        } else {
            ascii_count as f32 / english_ratio
        };

        let cjk_tokens = cjk_count as f32 / korean_ratio;
        let other_tokens = other_count as f32 / 2.0;

        (ascii_tokens + cjk_tokens + other_tokens).ceil() as usize
    }

    /// 동기 API 호출로 토큰 수 계산 (blocking)
    ///
    /// 주의: 이 함수는 blocking I/O를 수행합니다.
    /// 비동기 컨텍스트에서는 `count_tokens_async`를 사용하세요.
    #[cfg(feature = "blocking-http")]
    pub fn count_tokens_via_api(&self, text: &str) -> Option<usize> {
        let config = self.api_config.as_ref()?;
        if config.api_key.is_empty() {
            return None;
        }

        // Anthropic API count_tokens 엔드포인트 호출
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_millis(config.timeout_ms))
            .build()
            .ok()?;

        let url = format!("{}/v1/messages/count_tokens", config.base_url);

        let body = serde_json::json!({
            "model": config.model,
            "messages": [
                {
                    "role": "user",
                    "content": text
                }
            ]
        });

        let response = client
            .post(&url)
            .header("x-api-key", &config.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .ok()?;

        if !response.status().is_success() {
            return None;
        }

        let json: serde_json::Value = response.json().ok()?;
        let input_tokens = json.get("input_tokens")?.as_u64()? as usize;

        // 학습된 비율 업데이트
        self.update_learned_ratio(text, input_tokens);

        Some(input_tokens)
    }

    /// 추정 기반 계산 (API 없이)
    fn estimate_with_claude_adjustments(&self, text: &str) -> usize {
        // 학습된 비율 우선 사용
        if self
            .learned_ratio
            .read()
            .map(|r| r.is_some())
            .unwrap_or(false)
        {
            return self.estimate_with_learned_ratio(text);
        }

        // 기본 추정
        let base_count = self.base.count(text).total;

        // 코드 블록 감지 및 조정
        let code_ratio = detect_code_ratio(text);
        if code_ratio > 0.3 {
            let adjustment = (base_count as f32 * code_ratio * 0.1) as usize;
            return base_count + adjustment;
        }

        base_count
    }
}

impl Default for ClaudeEstimator {
    fn default() -> Self {
        Self::new()
    }
}

impl Tokenizer for ClaudeEstimator {
    fn tokenizer_type(&self) -> TokenizerType {
        TokenizerType::Claude
    }

    fn count(&self, text: &str) -> TokenCount {
        // API 기반 정확한 계산 시도 (feature 활성화 시)
        #[cfg(feature = "blocking-http")]
        if self.has_api() {
            if let Some(tokens) = self.count_tokens_via_api(text) {
                return TokenCount::exact(tokens, TokenizerType::Claude)
                    .with_char_count(text.chars().count());
            }
        }

        // 폴백: 추정 기반
        let total = self.estimate_with_claude_adjustments(text);
        TokenCount::estimated(total, TokenizerType::Claude).with_char_count(text.chars().count())
    }

    fn encode(&self, _text: &str) -> Result<EncodingResult, TokenizerError> {
        Err(TokenizerError::EncodingFailed(
            "Claude tokenizer does not support encoding (token IDs not available)".to_string(),
        ))
    }

    fn decode(&self, _token_ids: &[u32]) -> Result<String, TokenizerError> {
        Err(TokenizerError::DecodingFailed(
            "Claude tokenizer does not support decoding (token IDs not available)".to_string(),
        ))
    }

    fn is_exact(&self) -> bool {
        // API 사용 가능하고 blocking-http feature가 있으면 정확
        #[cfg(feature = "blocking-http")]
        return self.has_api();

        #[cfg(not(feature = "blocking-http"))]
        false
    }
}

// ============================================================================
// Gemini (Google)
// ============================================================================

/// Gemini 토크나이저 (추정 기반)
pub struct GeminiEstimator {
    base: EstimateTokenizer,
}

impl GeminiEstimator {
    pub fn new() -> Self {
        Self {
            base: EstimateTokenizer::new(TokenizerType::Gemini),
        }
    }
}

impl Default for GeminiEstimator {
    fn default() -> Self {
        Self::new()
    }
}

impl Tokenizer for GeminiEstimator {
    fn tokenizer_type(&self) -> TokenizerType {
        TokenizerType::Gemini
    }

    fn count(&self, text: &str) -> TokenCount {
        // Gemini는 GPT와 비슷한 토큰화 특성
        self.base.count(text)
    }

    fn encode(&self, _text: &str) -> Result<EncodingResult, TokenizerError> {
        Err(TokenizerError::EncodingFailed(
            "Gemini tokenizer is estimation-based only".to_string(),
        ))
    }

    fn decode(&self, _token_ids: &[u32]) -> Result<String, TokenizerError> {
        Err(TokenizerError::DecodingFailed(
            "Gemini tokenizer is estimation-based only".to_string(),
        ))
    }

    fn is_exact(&self) -> bool {
        false
    }
}

// ============================================================================
// Llama/Mistral (SentencePiece)
// ============================================================================

/// Llama/Mistral 토크나이저
///
/// SentencePiece 기반 모델용. tokenizers 크레이트 사용 가능 시 정확한 계산,
/// 그렇지 않으면 추정 기반으로 동작합니다.
pub struct LlamaEstimator {
    base: EstimateTokenizer,
    /// HuggingFace tokenizers (옵션)
    #[cfg(feature = "hf-tokenizers")]
    tokenizer: Option<tokenizers::Tokenizer>,
}

impl LlamaEstimator {
    pub fn new() -> Self {
        Self {
            base: EstimateTokenizer::new(TokenizerType::Llama),
            #[cfg(feature = "hf-tokenizers")]
            tokenizer: None,
        }
    }

    /// 특정 모델 파일로 초기화
    #[cfg(feature = "hf-tokenizers")]
    pub fn from_file(path: &str) -> Result<Self, TokenizerError> {
        let tokenizer = tokenizers::Tokenizer::from_file(path)
            .map_err(|e| TokenizerError::InitializationFailed(e.to_string()))?;

        Ok(Self {
            base: EstimateTokenizer::new(TokenizerType::Llama),
            tokenizer: Some(tokenizer),
        })
    }
}

impl Default for LlamaEstimator {
    fn default() -> Self {
        Self::new()
    }
}

impl Tokenizer for LlamaEstimator {
    fn tokenizer_type(&self) -> TokenizerType {
        TokenizerType::Llama
    }

    fn count(&self, text: &str) -> TokenCount {
        #[cfg(feature = "hf-tokenizers")]
        if let Some(ref tokenizer) = self.tokenizer {
            if let Ok(encoding) = tokenizer.encode(text, false) {
                return TokenCount::exact(encoding.get_ids().len(), TokenizerType::Llama)
                    .with_char_count(text.chars().count());
            }
        }

        // Fallback: Llama는 일반적으로 토큰이 조금 더 많음
        let base = self.base.count(text);
        let adjusted = (base.total as f32 * 1.05) as usize; // 5% 증가
        TokenCount::estimated(adjusted, TokenizerType::Llama).with_char_count(text.chars().count())
    }

    fn encode(&self, _text: &str) -> Result<EncodingResult, TokenizerError> {
        #[cfg(feature = "hf-tokenizers")]
        if let Some(ref tokenizer) = self.tokenizer {
            let encoding = tokenizer
                .encode(_text, false)
                .map_err(|e| TokenizerError::EncodingFailed(e.to_string()))?;

            return Ok(EncodingResult::new(encoding.get_ids().to_vec()));
        }

        Err(TokenizerError::EncodingFailed(
            "HuggingFace tokenizers not available".to_string(),
        ))
    }

    fn decode(&self, _token_ids: &[u32]) -> Result<String, TokenizerError> {
        #[cfg(feature = "hf-tokenizers")]
        if let Some(ref tokenizer) = self.tokenizer {
            return tokenizer
                .decode(_token_ids, true)
                .map_err(|e| TokenizerError::DecodingFailed(e.to_string()));
        }

        Err(TokenizerError::DecodingFailed(
            "HuggingFace tokenizers not available".to_string(),
        ))
    }

    fn is_exact(&self) -> bool {
        #[cfg(feature = "hf-tokenizers")]
        return self.tokenizer.is_some();

        #[cfg(not(feature = "hf-tokenizers"))]
        false
    }
}

// ============================================================================
// 헬퍼 함수
// ============================================================================

/// CJK (한중일) 문자인지 확인
///
/// Performance optimized:
/// - Fast-path for ASCII (most common case)
/// - Ordered checks by frequency (Korean > CJK > Japanese)
#[inline]
fn is_cjk(c: char) -> bool {
    let code = c as u32;

    // Fast-path: ASCII is never CJK (covers 95%+ of typical code)
    if code < 0x1100 {
        return false;
    }

    // Most frequent first: Korean syllables (AC00-D7AF)
    if code >= 0xAC00 && code <= 0xD7AF {
        return true;
    }

    // CJK Unified Ideographs (4E00-9FFF) - Chinese/Japanese Kanji
    if code >= 0x4E00 && code <= 0x9FFF {
        return true;
    }

    // Less frequent: Japanese Hiragana/Katakana
    if code >= 0x3040 && code <= 0x30FF {
        return true;
    }

    // Rare: Korean Jamo, compatibility
    (code >= 0x1100 && code <= 0x11FF) ||
    (code >= 0x3130 && code <= 0x318F)
}

/// 코드 비율 추정
///
/// Performance optimized:
/// - No Vec allocation (direct iterator)
/// - Early exit for empty text
/// - Short-circuit code detection
#[inline]
fn detect_code_ratio(text: &str) -> f32 {
    if text.is_empty() {
        return 0.0;
    }

    // Static code indicators (compiler optimizes as constant)
    const CODE_INDICATORS: &[&str] = &[
        "fn ", "def ", "class ", "import ", "from ", "const ", "let ", "var ",
        "pub ", "func ", "function ", "return ", "if ", "else ", "for ",
        "while ", "match ", "->", "=>", "::", "//", "/*", "*/", "# ", "```",
    ];

    let mut total_lines = 0u32;
    let mut code_lines = 0u32;

    // Single-pass line iteration (no allocation)
    for line in text.lines() {
        total_lines += 1;
        let trimmed = line.trim();

        // Fast structural checks first (single char comparison)
        if trimmed.starts_with('{')
            || trimmed.starts_with('}')
            || trimmed.ends_with(';')
            || trimmed.ends_with(':')
        {
            code_lines += 1;
            continue;
        }

        // Check code indicators (short-circuit on first match)
        if CODE_INDICATORS.iter().any(|ind| trimmed.contains(ind)) {
            code_lines += 1;
        }
    }

    if total_lines == 0 {
        return 0.0;
    }

    code_lines as f32 / total_lines as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_tokenizer() {
        let tokenizer = EstimateTokenizer::new(TokenizerType::Estimate);

        let count = tokenizer.count("Hello, world!");
        assert!(count.total > 0);
        assert!(!count.is_exact);
    }

    #[test]
    fn test_tiktoken_estimator() {
        let tokenizer = TiktokenEstimator::cl100k();

        let count = tokenizer.count("Hello, world!");
        assert!(count.total > 0);
    }

    #[test]
    fn test_claude_estimator() {
        let tokenizer = ClaudeEstimator::new();

        // 영어
        let en_count = tokenizer.count("Hello, this is a test.");
        assert!(en_count.total > 0);

        // 한국어 (더 많은 토큰 예상)
        let ko_count = tokenizer.count("안녕하세요, 테스트입니다.");
        assert!(ko_count.total > 0);
    }

    #[test]
    fn test_claude_estimator_with_api_config() {
        // API 키 없이 생성 (추정 모드)
        let tokenizer = ClaudeEstimator::new();
        assert!(!tokenizer.has_api());
        assert!(!tokenizer.is_exact());

        // API 키로 생성
        let tokenizer_with_key = ClaudeEstimator::with_api_key("test-key");
        assert!(tokenizer_with_key.has_api());
    }

    #[test]
    fn test_claude_learned_ratio() {
        let tokenizer = ClaudeEstimator::new();

        // 학습 전 기본 추정 (kept for potential future comparison test)
        let _before = tokenizer.count("Hello world").total;

        // 학습된 비율 업데이트 시뮬레이션
        tokenizer.update_learned_ratio("Hello world test", 3);

        // 학습 후에도 정상 동작
        let after = tokenizer.count("Hello world").total;
        assert!(after > 0);

        // 비율이 학습되었는지 확인
        let guard = tokenizer.learned_ratio.read().unwrap();
        assert!(guard.is_some());
        if let Some(ref ratio) = *guard {
            assert!(ratio.sample_count > 0);
        }
    }

    #[test]
    fn test_claude_code_detection() {
        let tokenizer = ClaudeEstimator::new();

        let code = r#"
        fn main() {
            let x = 42;
            println!("{}", x);
        }
        "#;

        let prose = "This is just regular text with no code at all.";

        let code_count = tokenizer.count(code);
        let prose_count = tokenizer.count(prose);

        // 둘 다 정상 작동
        assert!(code_count.total > 0);
        assert!(prose_count.total > 0);
    }

    #[test]
    fn test_cjk_detection() {
        assert!(is_cjk('가'));
        assert!(is_cjk('한'));
        assert!(is_cjk('中'));
        assert!(is_cjk('あ'));
        assert!(!is_cjk('a'));
        assert!(!is_cjk('1'));
    }

    #[test]
    fn test_code_detection() {
        let code = r#"
        fn main() {
            let x = 42;
            println!("{}", x);
        }
        "#;

        let ratio = detect_code_ratio(code);
        assert!(ratio > 0.5);

        let prose = "This is just regular text with no code.";
        let ratio = detect_code_ratio(prose);
        assert!(ratio < 0.1);
    }

    #[test]
    fn test_mixed_language() {
        let tokenizer = EstimateTokenizer::new(TokenizerType::Claude);

        let mixed = "Hello 안녕하세요 World 세계";
        let count = tokenizer.count(mixed);

        // 혼합 텍스트도 정상 처리
        assert!(count.total > 0);
    }
}
