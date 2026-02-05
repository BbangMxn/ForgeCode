use crate::storage::JsonStore;
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::provider_type::ProviderType;

/// 설정 파일명
pub const PROVIDERS_FILE: &str = "providers.json";

/// 개별 프로바이더 설정
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Provider {
    /// 프로바이더 타입
    #[serde(rename = "type")]
    pub provider_type: ProviderType,

    /// 표시 이름
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// 활성화 여부
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// API 키
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    /// Base URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,

    /// 모델 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// 최대 출력 토큰
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,

    /// 타임아웃 (초)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
}

impl Provider {
    pub fn new(provider_type: ProviderType) -> Self {
        Self {
            provider_type,
            name: None,
            enabled: true,
            api_key: None,
            base_url: None,
            model: None,
            max_tokens: None,
            timeout_secs: None,
        }
    }

    pub fn validate(&self) -> std::result::Result<(), String> {
        if self.provider_type.requires_api_key() && self.api_key.is_none() {
            return Err(format!("{} requires api_key", self.provider_type));
        }
        Ok(())
    }

    // effective 값들
    pub fn effective_base_url(&self) -> &str {
        self.base_url
            .as_deref()
            .unwrap_or_else(|| self.provider_type.default_base_url())
    }

    pub fn effective_model(&self) -> &str {
        self.model
            .as_deref()
            .unwrap_or_else(|| self.provider_type.default_model())
    }

    pub fn effective_max_tokens(&self) -> u32 {
        self.max_tokens
            .unwrap_or_else(|| self.provider_type.default_max_tokens())
    }

    pub fn effective_timeout(&self) -> u64 {
        self.timeout_secs
            .unwrap_or_else(|| self.provider_type.default_timeout())
    }

    // 빌더
    pub fn api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = Some(url.into());
        self
    }

    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn max_tokens(mut self, tokens: u32) -> Self {
        self.max_tokens = Some(tokens);
        self
    }

    pub fn timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = Some(secs);
        self
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

/// 프로바이더 관리자
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ProviderConfig {
    /// 기본 프로바이더 이름
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,

    /// 프로바이더들
    #[serde(default)]
    pub providers: HashMap<String, Provider>,
}

impl ProviderConfig {
    pub fn new() -> Self {
        Self::default()
    }

    // ========================================================================
    // Load / Save
    // ========================================================================

    /// 글로벌 + 프로젝트 + 환경변수 병합 로드
    pub fn load() -> Result<Self> {
        let mut config = Self::new();

        // 1. 글로벌 설정 (~/.forgecode/providers.json)
        if let Ok(global) = JsonStore::global() {
            if let Some(global_config) = global.load_optional::<ProviderConfig>(PROVIDERS_FILE)? {
                config.merge(global_config);
            }
        }

        // 2. 프로젝트 설정 (.forgecode/providers.json)
        if let Ok(project) = JsonStore::current_project() {
            if let Some(project_config) = project.load_optional::<ProviderConfig>(PROVIDERS_FILE)? {
                config.merge(project_config);
            }
        }

        // 3. 환경변수 오버라이드
        config.apply_env_overrides();

        Ok(config)
    }

    /// 글로벌 설정만 로드
    pub fn load_global() -> Result<Self> {
        let store = JsonStore::global()?;
        Ok(store.load_or_default(PROVIDERS_FILE))
    }

    /// 프로젝트 설정만 로드
    pub fn load_project() -> Result<Self> {
        let store = JsonStore::current_project()?;
        Ok(store.load_or_default(PROVIDERS_FILE))
    }

    /// 글로벌 설정 저장
    pub fn save_global(&self) -> Result<()> {
        let store = JsonStore::global()?;
        store.save(PROVIDERS_FILE, self)
    }

    /// 프로젝트 설정 저장
    pub fn save_project(&self) -> Result<()> {
        let store = JsonStore::current_project()?;
        store.save(PROVIDERS_FILE, self)
    }

    /// 환경변수에서 프로바이더 추가
    fn apply_env_overrides(&mut self) {
        // ANTHROPIC_API_KEY
        if let Ok(api_key) = std::env::var("ANTHROPIC_API_KEY") {
            if !self.providers.contains_key("anthropic") {
                self.add(
                    "anthropic",
                    Provider::new(ProviderType::Anthropic).api_key(api_key),
                );
            } else if let Some(p) = self.providers.get_mut("anthropic") {
                if p.api_key.is_none() {
                    p.api_key = Some(api_key);
                }
            }
        }

        // OPENAI_API_KEY
        if let Ok(api_key) = std::env::var("OPENAI_API_KEY") {
            if !self.providers.contains_key("openai") {
                self.add(
                    "openai",
                    Provider::new(ProviderType::Openai).api_key(api_key),
                );
            } else if let Some(p) = self.providers.get_mut("openai") {
                if p.api_key.is_none() {
                    p.api_key = Some(api_key);
                }
            }
        }

        // GEMINI_API_KEY
        if let Ok(api_key) = std::env::var("GEMINI_API_KEY") {
            if !self.providers.contains_key("gemini") {
                self.add(
                    "gemini",
                    Provider::new(ProviderType::Gemini).api_key(api_key),
                );
            } else if let Some(p) = self.providers.get_mut("gemini") {
                if p.api_key.is_none() {
                    p.api_key = Some(api_key);
                }
            }
        }

        // GROQ_API_KEY
        if let Ok(api_key) = std::env::var("GROQ_API_KEY") {
            if !self.providers.contains_key("groq") {
                self.add("groq", Provider::new(ProviderType::Groq).api_key(api_key));
            } else if let Some(p) = self.providers.get_mut("groq") {
                if p.api_key.is_none() {
                    p.api_key = Some(api_key);
                }
            }
        }
    }

    // ========================================================================
    // CRUD
    // ========================================================================

    /// 프로바이더 추가
    pub fn add(&mut self, name: impl Into<String>, provider: Provider) {
        let name = name.into();
        if self.default.is_none() {
            self.default = Some(name.clone());
        }
        self.providers.insert(name, provider);
    }

    /// 프로바이더 조회
    pub fn get(&self, name: &str) -> Option<&Provider> {
        self.providers.get(name)
    }

    /// 프로바이더 가변 조회
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Provider> {
        self.providers.get_mut(name)
    }

    /// 기본 프로바이더 조회
    pub fn get_default(&self) -> Option<&Provider> {
        self.default.as_ref().and_then(|n| self.providers.get(n))
    }

    /// 기본 프로바이더 설정
    pub fn set_default(&mut self, name: impl Into<String>) {
        self.default = Some(name.into());
    }

    /// API 키 설정 (CLI에서 직접 전달 시 사용)
    pub fn set_api_key(&mut self, provider_name: &str, api_key: &str) {
        if let Some(provider) = self.providers.get_mut(provider_name) {
            provider.api_key = Some(api_key.to_string());
        } else {
            // 프로바이더가 없으면 기본 설정으로 추가
            let provider_type = match provider_name {
                "anthropic" => ProviderType::Anthropic,
                "openai" => ProviderType::Openai,
                "gemini" => ProviderType::Gemini,
                "ollama" => ProviderType::Ollama,
                _ => ProviderType::Anthropic, // 기본값
            };
            let provider = Provider::new(provider_type).api_key(api_key.to_string());
            self.add(provider_name, provider);
        }
    }

    /// 프로바이더 제거
    pub fn remove(&mut self, name: &str) -> Option<Provider> {
        let removed = self.providers.remove(name);
        if self.default.as_deref() == Some(name) {
            self.default = self.providers.keys().next().cloned();
        }
        removed
    }

    /// 전체 목록
    pub fn list(&self) -> impl Iterator<Item = (&String, &Provider)> {
        self.providers.iter()
    }

    /// 활성화된 프로바이더
    pub fn list_enabled(&self) -> impl Iterator<Item = (&String, &Provider)> {
        self.providers.iter().filter(|(_, p)| p.enabled)
    }

    /// 타입별 프로바이더
    pub fn list_by_type(&self, t: ProviderType) -> impl Iterator<Item = (&String, &Provider)> {
        self.providers
            .iter()
            .filter(move |(_, p)| p.provider_type == t)
    }

    /// 프로바이더 존재 여부
    pub fn contains(&self, name: &str) -> bool {
        self.providers.contains_key(name)
    }

    /// 프로바이더 개수
    pub fn len(&self) -> usize {
        self.providers.len()
    }

    /// 비어있는지 확인
    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }

    /// 유효성 검증
    pub fn validate(&self) -> std::result::Result<(), Vec<String>> {
        let errors: Vec<_> = self
            .providers
            .iter()
            .filter_map(|(name, p)| p.validate().err().map(|e| format!("{}: {}", name, e)))
            .collect();
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// 다른 설정과 병합 (other가 우선)
    pub fn merge(&mut self, other: ProviderConfig) {
        if other.default.is_some() {
            self.default = other.default;
        }
        for (name, provider) in other.providers {
            self.providers.insert(name, provider);
        }
    }
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_builder() {
        let provider = Provider::new(ProviderType::Anthropic)
            .api_key("test-key")
            .model("claude-3-opus")
            .max_tokens(4096);

        assert_eq!(provider.provider_type, ProviderType::Anthropic);
        assert_eq!(provider.api_key, Some("test-key".to_string()));
        assert_eq!(provider.model, Some("claude-3-opus".to_string()));
        assert_eq!(provider.max_tokens, Some(4096));
    }

    #[test]
    fn test_provider_config() {
        let mut config = ProviderConfig::new();

        config.add(
            "anthropic",
            Provider::new(ProviderType::Anthropic).api_key("key1"),
        );
        config.add(
            "openai",
            Provider::new(ProviderType::Openai).api_key("key2"),
        );

        assert_eq!(config.len(), 2);
        assert!(config.contains("anthropic"));
        assert_eq!(config.default, Some("anthropic".to_string()));

        // 기본 프로바이더 변경
        config.set_default("openai");
        assert_eq!(config.default, Some("openai".to_string()));
    }

    #[test]
    fn test_provider_merge() {
        let mut base = ProviderConfig::new();
        base.add("anthropic", Provider::new(ProviderType::Anthropic));

        let mut overlay = ProviderConfig::new();
        overlay.add("openai", Provider::new(ProviderType::Openai));
        overlay.set_default("openai");

        base.merge(overlay);

        assert_eq!(base.len(), 2);
        assert_eq!(base.default, Some("openai".to_string()));
    }
}
