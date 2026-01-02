//! Fetch tool for retrieving web content

use std::time::Duration;

use reqwest::Client;

use super::{Tool, ToolError};

/// Maximum response size (1MB)
const MAX_RESPONSE_SIZE: usize = 1024 * 1024;

/// Maximum content length to return (8000 chars to fit in context)
const MAX_CONTENT_LENGTH: usize = 8000;

/// Request timeout
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Tool for fetching content from URLs
pub struct FetchTool {
    client: Client,
}

impl FetchTool {
    /// Create a new FetchTool
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .user_agent("Pangu/1.0 (Terminal AI Assistant)")
            .build()
            .expect("Failed to create HTTP client");

        Self { client }
    }
}

impl Default for FetchTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Tool for FetchTool {
    fn name(&self) -> &str {
        "fetch"
    }

    fn description(&self) -> &str {
        "Retrieves content from a URL. Use this to look up information online.\n\
         Parameters: The URL to fetch (e.g., https://example.com)"
    }

    async fn execute(&self, params: &str) -> Result<String, ToolError> {
        let url = params.trim();

        // Validate URL
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(ToolError::InvalidUrl(format!(
                "URL must start with http:// or https://, got: {}",
                url
            )));
        }

        // Make request
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    ToolError::Timeout
                } else {
                    ToolError::NetworkError(e.to_string())
                }
            })?;

        // Check status
        let status = response.status();
        if !status.is_success() {
            return Err(ToolError::NetworkError(format!(
                "HTTP error: {} {}",
                status.as_u16(),
                status.canonical_reason().unwrap_or("Unknown")
            )));
        }

        // Check content length
        if let Some(content_length) = response.content_length() {
            if content_length > MAX_RESPONSE_SIZE as u64 {
                return Err(ToolError::ResponseTooLarge);
            }
        }

        // Read response body
        let body = response
            .text()
            .await
            .map_err(|e| ToolError::NetworkError(e.to_string()))?;

        // Truncate if too long
        let content = if body.len() > MAX_CONTENT_LENGTH {
            let truncated = &body[..MAX_CONTENT_LENGTH];
            format!(
                "{}\n\n[Content truncated - {} bytes total]",
                truncated,
                body.len()
            )
        } else {
            body
        };

        Ok(content)
    }
}
