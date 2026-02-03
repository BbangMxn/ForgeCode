//! LLM Gateway - routes requests to appropriate providers
//!
//! The Gateway manages multiple LLM providers and provides a unified interface
//! for sending requests. It handles provider selection, fallback, and routing.

use crate::{
    providers::{
        anthropic::AnthropicProvider, gemini::GeminiProvider, groq::GroqProvider,
        ollama::OllamaProvider, openai::OpenAiProvider,
    },
    retry::{with_retry, RetryConfig},
    Message, Provider, ProviderResponse, ToolDef,
};
use forge_foundation::{Error, ProviderConfig, ProviderType, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Gateway that manages multiple LLM providers
pub struct Gateway {
    providers: HashMap<String, Arc<dyn Provider>>,
    default_provider: RwLock<String>,
    retry_config: RetryConfig,
}

impl Gateway {
    /// Create a new gateway from ProviderConfig
    pub fn from_config(config: &ProviderConfig) -> Result<Self> {
        let mut providers: HashMap<String, Arc<dyn Provider>> = HashMap::new();

        for (name, provider_config) in config.list_enabled() {
            let provider: Arc<dyn Provider> = match provider_config.provider_type {
                ProviderType::Anthropic => {
                    let api_key = provider_config.api_key.as_deref().unwrap_or("");
                    let model = provider_config.effective_model();
                    let max_tokens = provider_config.effective_max_tokens();
                    Arc::new(AnthropicProvider::new(api_key, model, max_tokens))
                }
                ProviderType::Openai => {
                    let api_key = provider_config.api_key.as_deref().unwrap_or("");
                    let model = provider_config.effective_model();
                    let max_tokens = provider_config.effective_max_tokens();
                    Arc::new(OpenAiProvider::new(api_key, model, max_tokens))
                }
                ProviderType::Ollama => {
                    let base_url = provider_config.effective_base_url();
                    let model = provider_config.effective_model();
                    Arc::new(OllamaProvider::new(base_url, model))
                }
                ProviderType::Gemini => {
                    let api_key = provider_config.api_key.as_deref().unwrap_or("");
                    let model = provider_config.effective_model();
                    let max_tokens = provider_config.effective_max_tokens();
                    Arc::new(GeminiProvider::new(api_key, model, max_tokens))
                }
                ProviderType::Groq => {
                    let api_key = provider_config.api_key.as_deref().unwrap_or("");
                    let model = provider_config.effective_model();
                    let max_tokens = provider_config.effective_max_tokens();
                    Arc::new(GroqProvider::new(api_key, model, max_tokens))
                }
            };

            providers.insert(name.clone(), provider);
        }

        if providers.is_empty() {
            return Err(Error::Config(
                "No LLM providers configured. Set ANTHROPIC_API_KEY, OPENAI_API_KEY, or configure Ollama.".to_string(),
            ));
        }

        // Determine default provider
        let default_provider = config
            .default
            .clone()
            .filter(|d| providers.contains_key(d))
            .or_else(|| providers.keys().next().cloned())
            .unwrap_or_default();

        Ok(Self {
            providers,
            default_provider: RwLock::new(default_provider),
            retry_config: RetryConfig::default(),
        })
    }

    /// Create gateway by loading config from files and environment
    pub fn load() -> Result<Self> {
        let config = ProviderConfig::load()?;
        Self::from_config(&config)
    }

    /// Create an empty gateway (for testing or manual provider setup)
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
            default_provider: RwLock::new(String::new()),
            retry_config: RetryConfig::default(),
        }
    }

    /// Add a provider to the gateway
    pub fn add_provider(&mut self, name: impl Into<String>, provider: Arc<dyn Provider>) {
        let name = name.into();
        if self.providers.is_empty() {
            *self.default_provider.get_mut() = name.clone();
        }
        self.providers.insert(name, provider);
    }

    /// Remove a provider from the gateway
    pub fn remove_provider(&mut self, name: &str) -> Option<Arc<dyn Provider>> {
        self.providers.remove(name)
    }

    /// Set retry configuration
    pub fn with_retry_config(mut self, config: RetryConfig) -> Self {
        self.retry_config = config;
        self
    }

    /// Get the default provider
    pub async fn default_provider(&self) -> Result<Arc<dyn Provider>> {
        let name = self.default_provider.read().await;
        self.get_provider(&name)
    }

    /// Get a specific provider by name
    pub fn get_provider(&self, name: &str) -> Result<Arc<dyn Provider>> {
        self.providers
            .get(name)
            .cloned()
            .ok_or_else(|| Error::ProviderNotFound(name.to_string()))
    }

    /// List available providers
    pub fn list_providers(&self) -> Vec<&str> {
        self.providers.keys().map(|s| s.as_str()).collect()
    }

    /// Get provider metadata for all providers
    pub fn list_provider_info(&self) -> Vec<(&str, &crate::ProviderMetadata)> {
        self.providers
            .iter()
            .map(|(name, provider)| (name.as_str(), provider.metadata()))
            .collect()
    }

    /// Get default provider name
    pub async fn default_provider_name(&self) -> String {
        self.default_provider.read().await.clone()
    }

    /// Set default provider
    pub async fn set_default(&self, name: &str) -> Result<()> {
        if self.providers.contains_key(name) {
            let mut default = self.default_provider.write().await;
            *default = name.to_string();
            Ok(())
        } else {
            Err(Error::ProviderNotFound(name.to_string()))
        }
    }

    /// Check if a provider is available
    pub fn is_provider_available(&self, name: &str) -> bool {
        self.providers
            .get(name)
            .map(|p| p.is_available())
            .unwrap_or(false)
    }

    /// Get the first available provider (for fallback)
    pub fn first_available(&self) -> Option<(&str, Arc<dyn Provider>)> {
        self.providers
            .iter()
            .find(|(_, p)| p.is_available())
            .map(|(name, provider)| (name.as_str(), provider.clone()))
    }

    /// Get default provider for streaming
    pub async fn get_default_provider_for_stream(&self) -> Result<Arc<dyn Provider>> {
        self.default_provider().await
    }

    /// Get provider by name for streaming
    pub fn get_provider_for_stream(&self, provider_name: &str) -> Result<Arc<dyn Provider>> {
        self.get_provider(provider_name)
    }

    /// Complete request using default provider
    pub async fn complete(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolDef>,
        system_prompt: Option<String>,
    ) -> Result<ProviderResponse> {
        let provider = self.default_provider().await?;
        provider
            .complete(messages, tools, system_prompt)
            .await
            .map_err(|e| Error::Provider(e.to_string()))
    }

    /// Complete request with retry logic
    pub async fn complete_with_retry(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolDef>,
        system_prompt: Option<String>,
    ) -> Result<ProviderResponse> {
        let provider = self.default_provider().await?;

        with_retry(&self.retry_config, "gateway_complete", || async {
            provider
                .complete(messages.clone(), tools.clone(), system_prompt.clone())
                .await
        })
        .await
        .map_err(|e| Error::Provider(e.to_string()))
    }

    /// Complete request using a specific provider
    pub async fn complete_with_provider(
        &self,
        provider_name: &str,
        messages: Vec<Message>,
        tools: Vec<ToolDef>,
        system_prompt: Option<String>,
    ) -> Result<ProviderResponse> {
        let provider = self.get_provider(provider_name)?;
        provider
            .complete(messages, tools, system_prompt)
            .await
            .map_err(|e| Error::Provider(e.to_string()))
    }

    /// Complete request with automatic fallback to next available provider
    pub async fn complete_with_fallback(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolDef>,
        system_prompt: Option<String>,
    ) -> Result<ProviderResponse> {
        let default_name = self.default_provider_name().await;

        // Try default provider first
        if let Ok(provider) = self.get_provider(&default_name) {
            match provider
                .complete(messages.clone(), tools.clone(), system_prompt.clone())
                .await
            {
                Ok(response) => return Ok(response),
                Err(e) => {
                    tracing::warn!(
                        "Default provider '{}' failed: {}, trying fallback",
                        default_name,
                        e
                    );
                }
            }
        }

        // Try other available providers
        for (name, provider) in &self.providers {
            if name == &default_name {
                continue;
            }
            if !provider.is_available() {
                continue;
            }

            match provider
                .complete(messages.clone(), tools.clone(), system_prompt.clone())
                .await
            {
                Ok(response) => {
                    tracing::info!("Fallback to provider '{}' succeeded", name);
                    return Ok(response);
                }
                Err(e) => {
                    tracing::warn!("Fallback provider '{}' failed: {}", name, e);
                }
            }
        }

        Err(Error::Provider("All providers failed".to_string()))
    }
}

impl Default for Gateway {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_gateway_empty() {
        let gateway = Gateway::new();
        assert!(gateway.list_providers().is_empty());
    }

    #[tokio::test]
    async fn test_set_default() {
        let gateway = Gateway::new();
        let result = gateway.set_default("nonexistent").await;
        assert!(result.is_err());
    }
}
