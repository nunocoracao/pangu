//! Search tool for web search queries

use std::time::Duration;

use reqwest::Client;
use serde::Deserialize;

use super::{Tool, ToolError};

/// Request timeout
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Maximum results to return
const MAX_RESULTS: usize = 5;

/// DuckDuckGo API response structures
#[derive(Debug, Deserialize)]
struct DuckDuckGoResponse {
    #[serde(rename = "AbstractText")]
    abstract_text: Option<String>,
    #[serde(rename = "AbstractSource")]
    abstract_source: Option<String>,
    #[serde(rename = "AbstractURL")]
    abstract_url: Option<String>,
    #[serde(rename = "RelatedTopics")]
    related_topics: Option<Vec<RelatedTopic>>,
}

#[derive(Debug, Deserialize)]
struct RelatedTopic {
    #[serde(rename = "Text")]
    text: Option<String>,
    #[serde(rename = "FirstURL")]
    first_url: Option<String>,
}

/// Tool for searching the web
pub struct SearchTool {
    client: Client,
}

impl SearchTool {
    /// Create a new SearchTool
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .user_agent("Pangu/1.0 (Terminal AI Assistant)")
            .build()
            .expect("Failed to create HTTP client");

        Self { client }
    }
}

impl Default for SearchTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Tool for SearchTool {
    fn name(&self) -> &str {
        "search"
    }

    fn description(&self) -> &str {
        "Searches the web for information. Use this to find current information.\n\
         Parameters: The search query (e.g., 'rust programming language')"
    }

    async fn execute(&self, params: &str) -> Result<String, ToolError> {
        let query = params.trim();

        if query.is_empty() {
            return Err(ToolError::InvalidUrl("Search query cannot be empty".to_string()));
        }

        // Use DuckDuckGo instant answer API
        let url = format!(
            "https://api.duckduckgo.com/?q={}&format=json&no_html=1&skip_disambig=1",
            urlencoding::encode(query)
        );

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    ToolError::Timeout
                } else {
                    ToolError::NetworkError(e.to_string())
                }
            })?;

        if !response.status().is_success() {
            return Err(ToolError::NetworkError(format!(
                "Search failed: HTTP {}",
                response.status()
            )));
        }

        let ddg: DuckDuckGoResponse = response
            .json()
            .await
            .map_err(|e| ToolError::NetworkError(format!("Failed to parse response: {}", e)))?;

        let mut results = Vec::new();

        // Add abstract if available
        if let Some(ref text) = ddg.abstract_text {
            if !text.is_empty() {
                let source = ddg.abstract_source.as_deref().unwrap_or("Unknown");
                let url = ddg.abstract_url.as_deref().unwrap_or("");
                results.push(format!("## Summary ({})\n{}\nSource: {}\n", source, text, url));
            }
        }

        // Add related topics
        if let Some(topics) = ddg.related_topics {
            let topic_results: Vec<String> = topics
                .iter()
                .filter_map(|t| {
                    let text = t.text.as_ref()?;
                    let url = t.first_url.as_deref().unwrap_or("");
                    Some(format!("- {}\n  Link: {}", text, url))
                })
                .take(MAX_RESULTS)
                .collect();

            if !topic_results.is_empty() {
                results.push(format!("## Related Results\n{}", topic_results.join("\n\n")));
            }
        }

        if results.is_empty() {
            Ok(format!("No results found for: {}\n\nTry using the fetch tool with a specific URL instead.", query))
        } else {
            Ok(format!("# Search Results for: {}\n\n{}", query, results.join("\n\n")))
        }
    }
}
