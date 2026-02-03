//! LLM Provider System
//!
//! Layer1의 `ProviderConfig`, `ProviderType`, `ModelRegistry`를 활용하여
//! 다양한 LLM 프로바이더를 통합 관리합니다.
//!
//! # 구조
//!
//! - `Gateway`: Provider 라우팅, 폴백, 로드밸런싱
//! - `ProviderImpl`: 각 LLM 프로바이더의 구현 trait
//! - `Message`, `ToolCall`: 메시지 및 도구 호출 타입
//!
//! # Layer1 연동
//!
//! ```rust,ignore
//! use forge_foundation::ProviderConfig;
//! use forge_core::Gateway;
//!
//! // Layer1 설정에서 Gateway 생성
//! let config = ProviderConfig::load()?;
//! let gateway = Gateway::from_config(&config)?;
//!
//! // 또는 자동 로드
//! let gateway = Gateway::load()?;
//! ```

mod error;
mod gateway;
mod message;
pub mod providers;
mod retry;

pub use error::ProviderError;
pub use gateway::Gateway;
pub use message::{Message, MessageRole, ToolCall, ToolResult as ToolCallResult};
pub use retry::RetryConfig;

use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};
use std::pin::Pin;

// ============================================================================
// Token Usage
// ============================================================================

/// 토큰 사용량
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    /// 입력 토큰 (프롬프트)
    pub input_tokens: u32,

    /// 출력 토큰 (응답)
    pub output_tokens: u32,

    /// 캐시에서 읽은 토큰
    pub cache_read_tokens: u32,

    /// 캐시 생성에 사용된 토큰
    pub cache_creation_tokens: u32,
}

impl TokenUsage {
    pub fn total(&self) -> u32 {
        self.input_tokens + self.output_tokens
    }

    pub fn add(&mut self, other: &TokenUsage) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.cache_read_tokens += other.cache_read_tokens;
        self.cache_creation_tokens += other.cache_creation_tokens;
    }

    /// 비용 추정 (USD)
    ///
    /// Layer1의 ModelPricing을 사용하여 계산 가능
    pub fn estimate_cost(&self, input_price_per_1m: f64, output_price_per_1m: f64) -> f64 {
        let input_cost = (self.input_tokens as f64 / 1_000_000.0) * input_price_per_1m;
        let output_cost = (self.output_tokens as f64 / 1_000_000.0) * output_price_per_1m;
        input_cost + output_cost
    }
}

impl std::ops::Add for TokenUsage {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            input_tokens: self.input_tokens + other.input_tokens,
            output_tokens: self.output_tokens + other.output_tokens,
            cache_read_tokens: self.cache_read_tokens + other.cache_read_tokens,
            cache_creation_tokens: self.cache_creation_tokens + other.cache_creation_tokens,
        }
    }
}

// ============================================================================
// Stream Event
// ============================================================================

/// 스트리밍 이벤트
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// 텍스트 델타
    Text(String),

    /// Thinking/reasoning (지원하는 모델용)
    Thinking(String),

    /// 도구 호출 시작
    ToolCallStart {
        index: usize,
        id: String,
        name: String,
    },

    /// 도구 호출 인자 델타
    ToolCallDelta { index: usize, arguments_delta: String },

    /// 도구 호출 완료
    ToolCall(ToolCall),

    /// 토큰 사용량 업데이트
    Usage(TokenUsage),

    /// 스트림 완료
    Done,

    /// 에러 발생
    Error(ProviderError),
}

// ============================================================================
// Provider Response
// ============================================================================

/// 완료 응답 (비스트리밍)
#[derive(Debug, Clone)]
pub struct ProviderResponse {
    /// 텍스트 내용
    pub content: String,

    /// 도구 호출 (있는 경우)
    pub tool_calls: Vec<ToolCall>,

    /// 토큰 사용량
    pub usage: TokenUsage,

    /// 완료 이유
    pub finish_reason: FinishReason,

    /// 사용된 모델 (폴백 시 다를 수 있음)
    pub model: String,
}

/// 완료 이유
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FinishReason {
    /// 정상 완료
    Stop,

    /// 최대 토큰 도달
    MaxTokens,

    /// 도구 사용 요청
    ToolUse,

    /// 콘텐츠 필터링됨
    ContentFilter,

    /// 기타/알 수 없음
    #[default]
    Other,
}

// ============================================================================
// Tool Definition (for LLM)
// ============================================================================

/// LLM에 전달할 도구 정의
///
/// Layer1의 Tool trait에서 schema()로 생성
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    /// 도구 이름
    pub name: String,

    /// 설명
    pub description: String,

    /// JSON Schema 파라미터
    pub parameters: ToolParameters,
}

/// 도구 파라미터 스키마
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolParameters {
    #[serde(rename = "type")]
    pub schema_type: String,

    pub properties: serde_json::Value,

    #[serde(default)]
    pub required: Vec<String>,
}

// ============================================================================
// Provider Implementation Trait
// ============================================================================

/// LLM Provider 구현 trait
///
/// 각 LLM 프로바이더(Anthropic, OpenAI 등)는 이 trait을 구현합니다.
/// Layer1의 Provider trait과 연동되어 설정을 받습니다.
#[async_trait]
pub trait ProviderImpl: Send + Sync {
    /// Provider 메타데이터
    fn metadata(&self) -> &ProviderMetadata;

    /// 현재 모델 정보
    fn model(&self) -> &ModelInfo;

    /// 스트리밍 응답
    fn stream(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolDef>,
        system_prompt: Option<String>,
    ) -> Pin<Box<dyn Stream<Item = StreamEvent> + Send + '_>>;

    /// 완료 응답 (비스트리밍)
    async fn complete(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolDef>,
        system_prompt: Option<String>,
    ) -> Result<ProviderResponse, ProviderError>;

    /// 사용 가능 여부 (API 키 설정 등)
    fn is_available(&self) -> bool;

    /// 모델 변경
    fn set_model(&mut self, model_id: &str) -> Result<(), ProviderError>;

    /// 사용 가능한 모델 목록
    fn list_models(&self) -> &[ModelInfo] {
        &self.metadata().models
    }
}

// ============================================================================
// Provider Metadata
// ============================================================================

/// Provider 메타데이터
#[derive(Debug, Clone)]
pub struct ProviderMetadata {
    /// Provider ID (e.g., "anthropic")
    pub id: String,

    /// 표시 이름 (e.g., "Anthropic")
    pub display_name: String,

    /// 사용 가능한 모델들
    pub models: Vec<ModelInfo>,

    /// 기본 모델 ID
    pub default_model: String,

    /// 필요한 설정 키들
    pub config_keys: Vec<ConfigKey>,

    /// 베이스 URL (OpenAI 호환 등)
    pub base_url: Option<String>,
}

/// 모델 정보
///
/// Layer1의 ModelInfo, ModelCapabilities, ModelPricing을 통합
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// 모델 ID
    pub id: String,

    /// Provider 이름
    pub provider: String,

    /// 표시 이름
    pub display_name: String,

    /// 컨텍스트 윈도우 (토큰)
    pub context_window: u32,

    /// 최대 출력 토큰
    pub max_output_tokens: u32,

    /// 도구 사용 지원
    pub supports_tools: bool,

    /// 비전/이미지 지원
    pub supports_vision: bool,

    /// Extended thinking 지원
    pub supports_thinking: bool,

    /// 입력 가격 (1M 토큰당 USD)
    pub input_price_per_1m: f64,

    /// 출력 가격 (1M 토큰당 USD)
    pub output_price_per_1m: f64,
}

impl ModelInfo {
    pub fn new(id: impl Into<String>, provider: impl Into<String>) -> Self {
        let id = id.into();
        Self {
            display_name: id.clone(),
            id,
            provider: provider.into(),
            context_window: 128000,
            max_output_tokens: 8192,
            supports_tools: true,
            supports_vision: false,
            supports_thinking: false,
            input_price_per_1m: 0.0,
            output_price_per_1m: 0.0,
        }
    }
}

/// 설정 키 정의
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigKey {
    pub name: String,
    pub required: bool,
    pub secret: bool,
    pub env_var: Option<String>,
    pub description: String,
}
