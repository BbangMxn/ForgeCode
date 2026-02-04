//! WebFetch Tool
//!
//! Fetches content from URLs and converts HTML to markdown for LLM consumption.

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

/// WebFetch configuration
#[derive(Debug, Clone)]
pub struct WebFetchConfig {
    /// Request timeout
    pub timeout: Duration,
    /// Maximum content length to fetch (bytes)
    pub max_content_length: usize,
    /// Convert HTML to markdown
    pub convert_to_markdown: bool,
    /// Include metadata (title, description)
    pub include_metadata: bool,
    /// Follow redirects
    pub follow_redirects: bool,
    /// Maximum redirects to follow
    pub max_redirects: usize,
}

impl Default for WebFetchConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            max_content_length: 1_000_000, // 1MB
            convert_to_markdown: true,
            include_metadata: true,
            follow_redirects: true,
            max_redirects: 10,
        }
    }
}

// ============================================================================
// Fetch Result
// ============================================================================

/// Result of fetching a URL
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchResult {
    /// URL that was fetched (may differ from requested due to redirects)
    pub url: String,
    /// HTTP status code
    pub status: u16,
    /// Content type
    pub content_type: String,
    /// Page title (if HTML)
    pub title: Option<String>,
    /// Page description (if HTML)
    pub description: Option<String>,
    /// Fetched content (converted to markdown if HTML)
    pub content: String,
    /// Content length in bytes
    pub content_length: usize,
    /// Whether content was truncated
    pub truncated: bool,
}

// ============================================================================
// WebFetch Tool
// ============================================================================

/// WebFetch tool for fetching web content
pub struct WebFetchTool {
    config: WebFetchConfig,
    client: Client,
}

impl WebFetchTool {
    /// Create a new WebFetch tool
    pub fn new() -> Self {
        Self::with_config(WebFetchConfig::default())
    }

    /// Create with custom config
    pub fn with_config(config: WebFetchConfig) -> Self {
        let client = Client::builder()
            .timeout(config.timeout)
            .redirect(if config.follow_redirects {
                reqwest::redirect::Policy::limited(config.max_redirects)
            } else {
                reqwest::redirect::Policy::none()
            })
            .user_agent("ForgeCode/1.0 (AI Coding Assistant)")
            .build()
            .unwrap_or_default();

        Self { config, client }
    }

    /// Fetch URL content
    async fn fetch(&self, url: &str) -> Result<FetchResult> {
        // Validate URL
        let parsed_url =
            url::Url::parse(url).map_err(|e| Error::Validation(format!("Invalid URL: {}", e)))?;

        // Only allow http/https
        if !["http", "https"].contains(&parsed_url.scheme()) {
            return Err(Error::Validation(format!(
                "Only http/https URLs are allowed, got: {}",
                parsed_url.scheme()
            )));
        }

        info!("Fetching URL: {}", url);

        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| Error::Network(e.to_string()))?;

        let status = response.status().as_u16();
        let final_url = response.url().to_string();
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("text/plain")
            .to_string();

        // Check content length
        let content_length = response
            .headers()
            .get("content-length")
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0);

        if content_length > self.config.max_content_length {
            return Err(Error::Validation(format!(
                "Content too large: {} bytes (max: {})",
                content_length, self.config.max_content_length
            )));
        }

        // Get body
        let body = response
            .bytes()
            .await
            .map_err(|e| Error::Network(e.to_string()))?;

        let mut truncated = false;
        let body_vec = if body.len() > self.config.max_content_length {
            truncated = true;
            body[..self.config.max_content_length].to_vec()
        } else {
            body.to_vec()
        };

        // Convert to string
        let raw_content = String::from_utf8_lossy(&body_vec).to_string();

        // Extract metadata and convert HTML
        let (title, description, content) = if content_type.contains("text/html") {
            let (t, d) = self.extract_html_metadata(&raw_content);
            let md = if self.config.convert_to_markdown {
                self.html_to_markdown(&raw_content)
            } else {
                raw_content
            };
            (t, d, md)
        } else {
            (None, None, raw_content)
        };

        Ok(FetchResult {
            url: final_url,
            status,
            content_type,
            title,
            description,
            content,
            content_length: body_vec.len(),
            truncated,
        })
    }

    /// Extract title and description from HTML
    fn extract_html_metadata(&self, html: &str) -> (Option<String>, Option<String>) {
        let mut title = None;
        let mut description = None;

        // Simple regex-free extraction
        // Extract title
        if let Some(start) = html.find("<title>") {
            if let Some(end) = html[start..].find("</title>") {
                let t = &html[start + 7..start + end];
                title = Some(self.decode_html_entities(t.trim()));
            }
        }

        // Extract meta description
        let desc_patterns = [
            r#"name="description" content=""#,
            r#"name='description' content='"#,
            r#"property="og:description" content=""#,
        ];

        for pattern in desc_patterns {
            if let Some(start) = html.find(pattern) {
                let content_start = start + pattern.len();
                let quote = if pattern.contains('"') { '"' } else { '\'' };
                if let Some(end) = html[content_start..].find(quote) {
                    let d = &html[content_start..content_start + end];
                    description = Some(self.decode_html_entities(d.trim()));
                    break;
                }
            }
        }

        (title, description)
    }

    /// Convert HTML to markdown (simplified)
    fn html_to_markdown(&self, html: &str) -> String {
        let mut content = html.to_string();

        // Remove script and style tags
        content = Self::remove_between(&content, "<script", "</script>");
        content = Self::remove_between(&content, "<style", "</style>");
        content = Self::remove_between(&content, "<!--", "-->");

        // Convert common tags
        // Headers
        content = Self::replace_tag(&content, "h1", "# ");
        content = Self::replace_tag(&content, "h2", "## ");
        content = Self::replace_tag(&content, "h3", "### ");
        content = Self::replace_tag(&content, "h4", "#### ");
        content = Self::replace_tag(&content, "h5", "##### ");
        content = Self::replace_tag(&content, "h6", "###### ");

        // Paragraphs and line breaks
        content = content.replace("</p>", "\n\n");
        content = content.replace("<br>", "\n");
        content = content.replace("<br/>", "\n");
        content = content.replace("<br />", "\n");

        // Lists
        content = Self::replace_tag_with(&content, "li", "- ", "\n");
        content = content.replace("<ul>", "\n");
        content = content.replace("</ul>", "\n");
        content = content.replace("<ol>", "\n");
        content = content.replace("</ol>", "\n");

        // Bold and italic
        content = Self::replace_tag_pair(&content, "strong", "**");
        content = Self::replace_tag_pair(&content, "b", "**");
        content = Self::replace_tag_pair(&content, "em", "*");
        content = Self::replace_tag_pair(&content, "i", "*");

        // Code
        content = Self::replace_tag_pair(&content, "code", "`");
        content = Self::replace_tag_with(&content, "pre", "```\n", "\n```\n");

        // Links - extract href
        content = self.convert_links(&content);

        // Remove remaining HTML tags
        content = Self::strip_tags(&content);

        // Decode HTML entities
        content = self.decode_html_entities(&content);

        // Clean up whitespace
        content = Self::clean_whitespace(&content);

        content
    }

    fn remove_between(s: &str, start_tag: &str, end_tag: &str) -> String {
        let mut result = s.to_string();
        while let Some(start) = result.find(start_tag) {
            if let Some(end) = result[start..].find(end_tag) {
                result = format!(
                    "{}{}",
                    &result[..start],
                    &result[start + end + end_tag.len()..]
                );
            } else {
                break;
            }
        }
        result
    }

    fn replace_tag(s: &str, tag: &str, prefix: &str) -> String {
        let open = format!("<{}>", tag);
        let open_attrs = format!("<{} ", tag);
        let close = format!("</{}>", tag);

        let mut result = s.to_string();
        result = result.replace(&close, "\n");
        result = result.replace(&open, prefix);

        // Handle tags with attributes
        while let Some(start) = result.find(&open_attrs) {
            if let Some(end) = result[start..].find('>') {
                result = format!(
                    "{}{}{}",
                    &result[..start],
                    prefix,
                    &result[start + end + 1..]
                );
            } else {
                break;
            }
        }

        result
    }

    fn replace_tag_with(
        s: &str,
        tag: &str,
        open_replacement: &str,
        close_replacement: &str,
    ) -> String {
        let open = format!("<{}>", tag);
        let open_attrs = format!("<{} ", tag);
        let close = format!("</{}>", tag);

        let mut result = s.to_string();
        result = result.replace(&close, close_replacement);
        result = result.replace(&open, open_replacement);

        // Handle tags with attributes
        while let Some(start) = result.find(&open_attrs) {
            if let Some(end) = result[start..].find('>') {
                result = format!(
                    "{}{}{}",
                    &result[..start],
                    open_replacement,
                    &result[start + end + 1..]
                );
            } else {
                break;
            }
        }

        result
    }

    fn replace_tag_pair(s: &str, tag: &str, replacement: &str) -> String {
        Self::replace_tag_with(s, tag, replacement, replacement)
    }

    fn convert_links(&self, s: &str) -> String {
        let mut result = s.to_string();

        // Simple link extraction: <a href="url">text</a> -> [text](url)
        let mut i = 0;
        while let Some(start) = result[i..].find("<a ") {
            let abs_start = i + start;

            if let Some(href_start) = result[abs_start..].find("href=\"") {
                let url_start = abs_start + href_start + 6;
                if let Some(url_end) = result[url_start..].find('"') {
                    let url = &result[url_start..url_start + url_end];

                    if let Some(tag_end) = result[abs_start..].find('>') {
                        let content_start = abs_start + tag_end + 1;
                        if let Some(close) = result[content_start..].find("</a>") {
                            let text = &result[content_start..content_start + close];
                            let markdown_link = format!("[{}]({})", Self::strip_tags(text), url);

                            result = format!(
                                "{}{}{}",
                                &result[..abs_start],
                                markdown_link,
                                &result[content_start + close + 4..]
                            );
                            i = abs_start + markdown_link.len();
                            continue;
                        }
                    }
                }
            }

            i = abs_start + 3;
        }

        result
    }

    fn strip_tags(s: &str) -> String {
        let mut result = String::new();
        let mut in_tag = false;

        for c in s.chars() {
            if c == '<' {
                in_tag = true;
            } else if c == '>' {
                in_tag = false;
            } else if !in_tag {
                result.push(c);
            }
        }

        result
    }

    fn decode_html_entities(&self, s: &str) -> String {
        s.replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&apos;", "'")
            .replace("&nbsp;", " ")
            .replace("&mdash;", "—")
            .replace("&ndash;", "–")
            .replace("&hellip;", "...")
            .replace("&copy;", "©")
            .replace("&reg;", "®")
            .replace("&trade;", "™")
    }

    fn clean_whitespace(s: &str) -> String {
        let lines: Vec<&str> = s.lines().map(|l| l.trim()).collect();

        let mut result = Vec::new();
        let mut last_empty = false;

        for line in lines {
            if line.is_empty() {
                if !last_empty {
                    result.push("");
                    last_empty = true;
                }
            } else {
                result.push(line);
                last_empty = false;
            }
        }

        result.join("\n").trim().to_string()
    }

    fn format_result(&self, result: &FetchResult) -> String {
        let mut output = String::new();

        if self.config.include_metadata {
            output.push_str(&format!("URL: {}\n", result.url));
            output.push_str(&format!("Status: {}\n", result.status));
            output.push_str(&format!("Content-Type: {}\n", result.content_type));

            if let Some(title) = &result.title {
                output.push_str(&format!("Title: {}\n", title));
            }

            if let Some(desc) = &result.description {
                output.push_str(&format!("Description: {}\n", desc));
            }

            output.push_str(&format!("Content Length: {} bytes", result.content_length));
            if result.truncated {
                output.push_str(" (truncated)");
            }
            output.push_str("\n\n---\n\n");
        }

        output.push_str(&result.content);

        output
    }
}

impl Default for WebFetchTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }

    fn description(&self) -> &str {
        "Fetch content from a URL. HTML is automatically converted to markdown for easier reading."
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "web_fetch".to_string(),
            description: self.description().to_string(),
            parameters: ToolParameters {
                schema_type: "object".to_string(),
                properties: json!({
                    "url": {
                        "type": "string",
                        "description": "The URL to fetch"
                    },
                    "prompt": {
                        "type": "string",
                        "description": "Optional prompt describing what information to extract (for context)"
                    }
                }),
                required: vec!["url".to_string()],
            },
        }
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to fetch"
                },
                "prompt": {
                    "type": "string",
                    "description": "What to extract from the page"
                }
            },
            "required": ["url"]
        })
    }

    fn meta(&self) -> ToolMeta {
        ToolMeta {
            name: "web_fetch".to_string(),
            description: self.description().to_string(),
            category: "web".to_string(),
            read_only: true,
            requires_permission: true,
        }
    }

    fn required_permission(&self, args: &Value) -> Option<PermissionRequest> {
        let url = args["url"].as_str().unwrap_or("unknown");
        Some(PermissionRequest {
            permission_type: PermissionType::Network,
            category: PermissionCategory::Network,
            resource: url.to_string(),
            operation: "fetch".to_string(),
            reason: format!("Fetch content from {}", url),
            metadata: Default::default(),
        })
    }

    async fn execute(&self, args: Value, ctx: &dyn ToolContext) -> ToolResult {
        let url = args["url"]
            .as_str()
            .ok_or_else(|| "Missing required parameter: url".to_string());

        let url = match url {
            Ok(u) => u,
            Err(e) => {
                return ToolResult {
                    success: false,
                    content: String::new(),
                    error: Some(e),
                }
            }
        };

        let prompt = args["prompt"].as_str();

        info!("WebFetch: url='{}', prompt={:?}", url, prompt);

        match self.fetch(url).await {
            Ok(result) => {
                let output = self.format_result(&result);
                ToolResult {
                    success: true,
                    content: output,
                    error: None,
                }
            }
            Err(e) => {
                warn!("WebFetch failed: {}", e);
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
    fn test_web_fetch_tool_name() {
        let tool = WebFetchTool::new();
        assert_eq!(tool.name(), "web_fetch");
    }

    #[test]
    fn test_web_fetch_schema() {
        let tool = WebFetchTool::new();
        let schema = tool.schema();

        assert!(schema["properties"]["url"].is_object());
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&json!("url")));
    }

    #[test]
    fn test_html_to_markdown_headers() {
        let tool = WebFetchTool::new();
        let html = "<h1>Title</h1><h2>Subtitle</h2>";
        let md = tool.html_to_markdown(html);

        assert!(md.contains("# Title"));
        assert!(md.contains("## Subtitle"));
    }

    #[test]
    fn test_html_to_markdown_links() {
        let tool = WebFetchTool::new();
        let html = r#"<a href="https://example.com">Example</a>"#;
        let md = tool.html_to_markdown(html);

        assert!(md.contains("[Example](https://example.com)"));
    }

    #[test]
    fn test_html_to_markdown_lists() {
        let tool = WebFetchTool::new();
        let html = "<ul><li>Item 1</li><li>Item 2</li></ul>";
        let md = tool.html_to_markdown(html);

        assert!(md.contains("- Item 1"));
        assert!(md.contains("- Item 2"));
    }

    #[test]
    fn test_strip_tags() {
        let result = WebFetchTool::strip_tags("<p>Hello <b>world</b></p>");
        assert_eq!(result, "Hello world");
    }

    #[test]
    fn test_decode_html_entities() {
        let tool = WebFetchTool::new();
        let result = tool.decode_html_entities("Hello &amp; world &lt;test&gt;");
        assert_eq!(result, "Hello & world <test>");
    }

    #[test]
    fn test_extract_metadata() {
        let tool = WebFetchTool::new();
        let html = r#"
            <html>
            <head>
                <title>Test Page</title>
                <meta name="description" content="A test description">
            </head>
            </html>
        "#;

        let (title, description) = tool.extract_html_metadata(html);
        assert_eq!(title, Some("Test Page".to_string()));
        assert_eq!(description, Some("A test description".to_string()));
    }
}
