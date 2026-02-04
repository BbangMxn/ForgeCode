//! Tokenizer Module - 모델별 토큰 계산
//!
//! 각 LLM 모델마다 다른 토크나이저를 추상화하여 정확한 토큰 수를 계산합니다.
//!
//! ## 지원 토크나이저
//!
//! | Provider | 토크나이저 | 라이브러리 |
//! |----------|-----------|-----------|
//! | OpenAI | tiktoken (BPE) | tiktoken-rs |
//! | Anthropic | Claude tokenizer | 추정 기반 |
//! | Google | SentencePiece | 추정 기반 |
//! | Llama | SentencePiece | tokenizers |
//! | Mistral | SentencePiece | tokenizers |
//! | Ollama | API/추정 | 동적 |
//!
//! ## 사용법
//!
//! ```ignore
//! use forge_foundation::tokenizer::{Tokenizer, TokenizerFactory, TokenCount};
//!
//! // 모델별 토크나이저 생성
//! let tokenizer = TokenizerFactory::for_model("gpt-4o")?;
//!
//! // 토큰 수 계산
//! let count = tokenizer.count("Hello, world!");
//! println!("Tokens: {}", count);
//!
//! // 토큰 제한 내에서 텍스트 자르기
//! let truncated = tokenizer.truncate("very long text...", 100);
//!
//! // 메시지 배열의 토큰 수 계산
//! let messages_tokens = tokenizer.count_messages(&messages);
//!
//! // Ollama 동적 토크나이저
//! use forge_foundation::tokenizer::{OllamaTokenizer, ModelFamily};
//!
//! let ollama = OllamaTokenizer::local("llama3.3");
//! let count = ollama.count("안녕하세요"); // 모델 패밀리 기반 추정
//!
//! // 비동기로 정확한 토큰 수 조회
//! let exact_count = ollama.count_via_api("Hello").await;
//! ```

mod dynamic;
mod estimator;
mod factory;
mod traits;
mod types;

pub use dynamic::{DynamicTokenizerRegistry, ModelFamily, OllamaTokenizer, OpenAICompatTokenizer};
pub use estimator::{
    ClaudeApiConfig, ClaudeEstimator, GeminiEstimator, LlamaEstimator, TiktokenEstimator,
};
pub use factory::TokenizerFactory;
pub use traits::Tokenizer;
pub use types::{
    EncodingResult, ModelTokenConfig, TokenBudget, TokenCount, TokenDistribution, TokenizerError,
    TokenizerType,
};
