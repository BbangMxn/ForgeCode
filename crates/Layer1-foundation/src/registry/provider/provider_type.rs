use serde::{Deserialize, Serialize};

/// 프로바이더 종류
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderType {
    Anthropic,
    Openai,
    Gemini,
    Ollama,
    Groq,
}

impl ProviderType {
    /// 표시 이름
    pub fn name(&self) -> &'static str {
        match self {
            Self::Anthropic => "Anthropic",
            Self::Openai => "OpenAI",
            Self::Gemini => "Gemini",
            Self::Ollama => "Ollama",
            Self::Groq => "Groq",
        }
    }

    /// API Key 필요 여부
    pub fn requires_api_key(&self) -> bool {
        !matches!(self, Self::Ollama)
    }

    /// 로컬 서비스 여부
    pub fn is_local(&self) -> bool {
        matches!(self, Self::Ollama)
    }

    /// 기본 Base URL
    pub fn default_base_url(&self) -> &'static str {
        match self {
            Self::Anthropic => "https://api.anthropic.com",
            Self::Openai => "https://api.openai.com",
            Self::Gemini => "https://generativelanguage.googleapis.com",
            Self::Groq => "https://api.groq.com",
            Self::Ollama => "http://localhost:11434",
        }
    }

    /// 기본 모델
    pub fn default_model(&self) -> &'static str {
        match self {
            Self::Anthropic => "claude-sonnet-4-20250514",
            Self::Openai => "gpt-4o",
            Self::Gemini => "gemini-2.0-flash",
            Self::Groq => "llama-3.3-70b-versatile",
            Self::Ollama => "llama3",
        }
    }

    /// 기본 max_tokens
    pub fn default_max_tokens(&self) -> u32 {
        match self {
            Self::Openai => 4096,
            _ => 8192,
        }
    }

    /// 기본 타임아웃 (초)
    pub fn default_timeout(&self) -> u64 {
        match self {
            Self::Ollama => 600,
            Self::Groq => 60,
            _ => 300,
        }
    }
}

impl std::fmt::Display for ProviderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name().to_lowercase())
    }
}

impl Default for ProviderType {
    fn default() -> Self {
        Self::Anthropic
    }
}
