//! WebSearch Tool
//!
//! Web search functionality using various search APIs.
//! Supports Brave Search, Google Custom Search, DuckDuckGo, and Tavily.

use async_trait::async_trait;
use forge_foundation::{
    permission::{PermissionCategory, PermissionRequest, PermissionType},
    Error, Result, Tool, ToolDefinition, ToolMeta, ToolParameters, ToolResult,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;
use tracing::{debug, info, warn};

use crate::tool::ToolContext;

// ============================================================================
// Configuration
// ============================================================================

/// Search provider type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchProvider {
    /// Brave Search API
    #[default]
    Brave,
    /// Google Custom Search
    Google,
    /// DuckDuckGo (no API key required)
    DuckDuckGo,
    /// Tavily AI Search
    Tavily,
    /// SerpAPI (aggregator)
    SerpApi,
}

/// WebSearch configuration
#[derive(Debug, Clone)]
pub struct WebSearchConfig {
    /// Search provider
    pub provider: SearchProvider,
    /// API key (provider-specific)
    pub api_key: Option<String>,
    /// Maximum results to return
    pub max_results: usize,
    /// Request timeout
    pub timeout: Duration,
    /// Include snippets in results
    pub include_snippets: bool,
    /// Safe search enabled
    pub safe_search: bool,
}

impl Default for WebSearchConfig {
    fn default() -> Self {
        Self {
            provider: SearchProvider::Brave,
            api_key: std::env::var("BRAVE_API_KEY")
                .or_else(|_| std::env::var("SEARCH_API_KEY"))
                .ok(),
            max_results: 10,
            timeout: Duration::from_secs(30),
            include_snippets: true,
            safe_search: true,
        }
    }
}

// ============================================================================
// Search Result Types
// ============================================================================

/// A single search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Result title
    pub title: String,
    /// URL
    pub url: String,
    /// Description/snippet
    pub description: String,
    /// Source domain
    pub source: String,
}

/// Search response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    /// Query that was searched
    pub query: String,
    /// Results
    pub results: Vec<SearchResult>,
    /// Total results found (if available)
    pub total_results: Option<u64>,
    /// Provider used
    pub provider: String,
}

// ============================================================================
// WebSearch Tool
// ============================================================================

/// WebSearch tool for searching the web
pub struct WebSearchTool {
    config: WebSearchConfig,
    client: Client,
}

impl WebSearchTool {
    /// Create a new WebSearch tool
    pub fn new() -> Self {
        Self::with_config(WebSearchConfig::default())
    }

    /// Create with custom config
    pub fn with_config(config: WebSearchConfig) -> Self {
        let client = Client::builder()
            .timeout(config.timeout)
            .user_agent("ForgeCode/1.0")
            .build()
            .unwrap_or_default();

        Self { config, client }
    }

    /// Perform search using configured provider
    async fn search(&self, query: &str, max_results: usize) -> Result<SearchResponse> {
        match self.config.provider {
            SearchProvider::Brave => self.search_brave(query, max_results).await,
            SearchProvider::DuckDuckGo => self.search_duckduckgo(query, max_results).await,
            SearchProvider::Google => self.search_google(query, max_results).await,
            SearchProvider::Tavily => self.search_tavily(query, max_results).await,
            SearchProvider::SerpApi => self.search_serpapi(query, max_results).await,
        }
    }

    /// Search using Brave Search API
    async fn search_brave(&self, query: &str, max_results: usize) -> Result<SearchResponse> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or_else(|| Error::Config("BRAVE_API_KEY not set".to_string()))?;

        let url = format!(
            "https://api.search.brave.com/res/v1/web/search?q={}&count={}",
            urlencoding::encode(query),
            max_results
        );

        let response = self
            .client
            .get(&url)
            .header("X-Subscription-Token", api_key)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| Error::Network(e.to_string()))?;

        if !response.status().is_success() {
            return Err(Error::Api(format!(
                "Brave API error: {}",
                response.status()
            )));
        }

        let data: Value = response
            .json()
            .await
            .map_err(|e| Error::Parse(e.to_string()))?;

        let results = self.parse_brave_response(&data);

        Ok(SearchResponse {
            query: query.to_string(),
            results,
            total_results: data["web"]["results"].as_array().map(|a| a.len() as u64),
            provider: "Brave".to_string(),
        })
    }

    fn parse_brave_response(&self, data: &Value) -> Vec<SearchResult> {
        let mut results = Vec::new();

        if let Some(web_results) = data["web"]["results"].as_array() {
            for item in web_results {
                let title = item["title"].as_str().unwrap_or_default().to_string();
                let url = item["url"].as_str().unwrap_or_default().to_string();
                let description = item["description"].as_str().unwrap_or_default().to_string();

                let source = url::Url::parse(&url)
                    .map(|u| u.host_str().unwrap_or_default().to_string())
                    .unwrap_or_default();

                if !title.is_empty() && !url.is_empty() {
                    results.push(SearchResult {
                        title,
                        url,
                        description,
                        source,
                    });
                }
            }
        }

        results
    }

    /// Search using DuckDuckGo (HTML scraping - no API key needed)
    async fn search_duckduckgo(&self, query: &str, max_results: usize) -> Result<SearchResponse> {
        // DuckDuckGo instant answer API
        let url = format!(
            "https://api.duckduckgo.com/?q={}&format=json&no_html=1&skip_disambig=1",
            urlencoding::encode(query)
        );

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| Error::Network(e.to_string()))?;

        let data: Value = response
            .json()
            .await
            .map_err(|e| Error::Parse(e.to_string()))?;

        let mut results = Vec::new();

        // Abstract (main answer)
        if let Some(abstract_text) = data["AbstractText"].as_str() {
            if !abstract_text.is_empty() {
                results.push(SearchResult {
                    title: data["Heading"].as_str().unwrap_or("Answer").to_string(),
                    url: data["AbstractURL"].as_str().unwrap_or_default().to_string(),
                    description: abstract_text.to_string(),
                    source: data["AbstractSource"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                });
            }
        }

        // Related topics
        if let Some(topics) = data["RelatedTopics"].as_array() {
            for topic in topics
                .iter()
                .take(max_results.saturating_sub(results.len()))
            {
                if let Some(text) = topic["Text"].as_str() {
                    let url = topic["FirstURL"].as_str().unwrap_or_default();
                    results.push(SearchResult {
                        title: text.chars().take(100).collect(),
                        url: url.to_string(),
                        description: text.to_string(),
                        source: "DuckDuckGo".to_string(),
                    });
                }
            }
        }

        Ok(SearchResponse {
            query: query.to_string(),
            results,
            total_results: None,
            provider: "DuckDuckGo".to_string(),
        })
    }

    /// Search using Google Custom Search
    async fn search_google(&self, query: &str, max_results: usize) -> Result<SearchResponse> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or_else(|| Error::Config("GOOGLE_API_KEY not set".to_string()))?;

        let cx = std::env::var("GOOGLE_CX").map_err(|_| {
            Error::Config("GOOGLE_CX (Custom Search Engine ID) not set".to_string())
        })?;

        let url = format!(
            "https://www.googleapis.com/customsearch/v1?key={}&cx={}&q={}&num={}",
            api_key,
            cx,
            urlencoding::encode(query),
            max_results.min(10)
        );

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| Error::Network(e.to_string()))?;

        let data: Value = response
            .json()
            .await
            .map_err(|e| Error::Parse(e.to_string()))?;

        let mut results = Vec::new();

        if let Some(items) = data["items"].as_array() {
            for item in items {
                results.push(SearchResult {
                    title: item["title"].as_str().unwrap_or_default().to_string(),
                    url: item["link"].as_str().unwrap_or_default().to_string(),
                    description: item["snippet"].as_str().unwrap_or_default().to_string(),
                    source: item["displayLink"].as_str().unwrap_or_default().to_string(),
                });
            }
        }

        Ok(SearchResponse {
            query: query.to_string(),
            results,
            total_results: data["searchInformation"]["totalResults"]
                .as_str()
                .and_then(|s| s.parse().ok()),
            provider: "Google".to_string(),
        })
    }

    /// Search using Tavily AI Search
    async fn search_tavily(&self, query: &str, max_results: usize) -> Result<SearchResponse> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or_else(|| Error::Config("TAVILY_API_KEY not set".to_string()))?;

        let response = self
            .client
            .post("https://api.tavily.com/search")
            .json(&json!({
                "api_key": api_key,
                "query": query,
                "max_results": max_results,
                "include_answer": true
            }))
            .send()
            .await
            .map_err(|e| Error::Network(e.to_string()))?;

        let data: Value = response
            .json()
            .await
            .map_err(|e| Error::Parse(e.to_string()))?;

        let mut results = Vec::new();

        if let Some(items) = data["results"].as_array() {
            for item in items {
                results.push(SearchResult {
                    title: item["title"].as_str().unwrap_or_default().to_string(),
                    url: item["url"].as_str().unwrap_or_default().to_string(),
                    description: item["content"].as_str().unwrap_or_default().to_string(),
                    source: item["url"]
                        .as_str()
                        .and_then(|u| url::Url::parse(u).ok())
                        .map(|u| u.host_str().unwrap_or_default().to_string())
                        .unwrap_or_default(),
                });
            }
        }

        Ok(SearchResponse {
            query: query.to_string(),
            results,
            total_results: None,
            provider: "Tavily".to_string(),
        })
    }

    /// Search using SerpAPI
    async fn search_serpapi(&self, query: &str, max_results: usize) -> Result<SearchResponse> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or_else(|| Error::Config("SERPAPI_KEY not set".to_string()))?;

        let url = format!(
            "https://serpapi.com/search.json?q={}&api_key={}&num={}",
            urlencoding::encode(query),
            api_key,
            max_results
        );

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| Error::Network(e.to_string()))?;

        let data: Value = response
            .json()
            .await
            .map_err(|e| Error::Parse(e.to_string()))?;

        let mut results = Vec::new();

        if let Some(items) = data["organic_results"].as_array() {
            for item in items {
                results.push(SearchResult {
                    title: item["title"].as_str().unwrap_or_default().to_string(),
                    url: item["link"].as_str().unwrap_or_default().to_string(),
                    description: item["snippet"].as_str().unwrap_or_default().to_string(),
                    source: item["displayed_link"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                });
            }
        }

        Ok(SearchResponse {
            query: query.to_string(),
            results,
            total_results: None,
            provider: "SerpAPI".to_string(),
        })
    }

    fn format_results(&self, response: &SearchResponse) -> String {
        let mut output = format!("Search results for: \"{}\"\n", response.query);
        output.push_str(&format!("Provider: {}\n\n", response.provider));

        if response.results.is_empty() {
            output.push_str("No results found.\n");
        } else {
            for (i, result) in response.results.iter().enumerate() {
                output.push_str(&format!("{}. {}\n", i + 1, result.title));
                output.push_str(&format!("   URL: {}\n", result.url));
                if self.config.include_snippets && !result.description.is_empty() {
                    output.push_str(&format!("   {}\n", result.description));
                }
                output.push('\n');
            }
        }

        if let Some(total) = response.total_results {
            output.push_str(&format!("Total results: {}\n", total));
        }

        output
    }
}

impl Default for WebSearchTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Search the web for information. Returns a list of relevant results with titles, URLs, and descriptions."
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "web_search".to_string(),
            description: self.description().to_string(),
            parameters: ToolParameters {
                schema_type: "object".to_string(),
                properties: json!({
                    "query": {
                        "type": "string",
                        "description": "The search query"
                    },
                    "max_results": {
                        "type": "integer",
                        "description": "Maximum number of results to return (default: 10)",
                        "default": 10
                    }
                }),
                required: vec!["query".to_string()],
            },
        }
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results",
                    "default": 10
                }
            },
            "required": ["query"]
        })
    }

    fn meta(&self) -> ToolMeta {
        ToolMeta {
            name: "web_search".to_string(),
            description: self.description().to_string(),
            category: "web".to_string(),
            read_only: true,
            requires_permission: true,
        }
    }

    fn required_permission(&self, _args: &Value) -> Option<PermissionRequest> {
        Some(PermissionRequest {
            permission_type: PermissionType::Network,
            category: PermissionCategory::Network,
            resource: "web_search".to_string(),
            operation: "search".to_string(),
            reason: "Search the web for information".to_string(),
            metadata: Default::default(),
        })
    }

    async fn execute(&self, args: Value, ctx: &dyn ToolContext) -> ToolResult {
        let query = args["query"]
            .as_str()
            .ok_or_else(|| "Missing required parameter: query".to_string())?;

        let max_results = args["max_results"]
            .as_u64()
            .map(|n| n as usize)
            .unwrap_or(self.config.max_results);

        info!("WebSearch: query='{}', max_results={}", query, max_results);

        match self.search(query, max_results).await {
            Ok(response) => {
                let output = self.format_results(&response);
                ToolResult {
                    success: true,
                    content: output,
                    error: None,
                }
            }
            Err(e) => {
                warn!("WebSearch failed: {}", e);
                ToolResult {
                    success: false,
                    content: String::new(),
                    error: Some(e.to_string()),
                }
            }
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_web_search_tool_name() {
        let tool = WebSearchTool::new();
        assert_eq!(tool.name(), "web_search");
    }

    #[test]
    fn test_web_search_schema() {
        let tool = WebSearchTool::new();
        let schema = tool.schema();

        assert!(schema["properties"]["query"].is_object());
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&json!("query")));
    }

    #[test]
    fn test_search_result_serialization() {
        let result = SearchResult {
            title: "Test".to_string(),
            url: "https://example.com".to_string(),
            description: "A test result".to_string(),
            source: "example.com".to_string(),
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("Test"));
    }

    #[test]
    fn test_format_results() {
        let tool = WebSearchTool::new();
        let response = SearchResponse {
            query: "test query".to_string(),
            results: vec![SearchResult {
                title: "Result 1".to_string(),
                url: "https://example.com/1".to_string(),
                description: "Description 1".to_string(),
                source: "example.com".to_string(),
            }],
            total_results: Some(100),
            provider: "Test".to_string(),
        };

        let output = tool.format_results(&response);
        assert!(output.contains("test query"));
        assert!(output.contains("Result 1"));
        assert!(output.contains("Total results: 100"));
    }
}
