//! web_search tool implementation - search the web via DuckDuckGo

use std::path::Path;
use std::time::Duration;

use crate::tools::{
    PermissionLevel, Tool, ToolContext, ToolError, ToolParameter, ToolParams, ToolResult,
};

/// Default number of results
const DEFAULT_NUM_RESULTS: usize = 5;

/// Maximum number of results
const MAX_NUM_RESULTS: usize = 10;

/// Default timeout in seconds
const DEFAULT_TIMEOUT: u64 = 30;

/// User agent for requests
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

/// Tool for searching the web via DuckDuckGo
pub struct WebSearchTool;

impl Tool for WebSearchTool {
    fn name(&self) -> &'static str {
        "web_search"
    }

    fn description(&self) -> &'static str {
        "Search the web using DuckDuckGo. Returns a list of results with titles, URLs, and snippets."
    }

    fn parameters(&self) -> &[ToolParameter] {
        &[
            ToolParameter {
                name: "query",
                description: "The search query",
                required: true,
            },
            ToolParameter {
                name: "num_results",
                description: "Number of results to return (default: 5, max: 10)",
                required: false,
            },
        ]
    }

    fn permission_level(&self, _path: Option<&Path>, _context: &ToolContext) -> PermissionLevel {
        // All network access requires permission
        PermissionLevel::Required
    }

    fn execute(&self, params: &ToolParams, _context: &ToolContext) -> Result<ToolResult, ToolError> {
        // Get query parameter
        let query = params
            .get("query")
            .ok_or_else(|| ToolError::MissingParameter("query".to_string()))?;

        // Get number of results
        let num_results = params
            .get("num_results")
            .and_then(|n| n.parse::<usize>().ok())
            .unwrap_or(DEFAULT_NUM_RESULTS)
            .min(MAX_NUM_RESULTS);

        // Search DuckDuckGo
        let results = search_duckduckgo(query, num_results)?;

        if results.is_empty() {
            Ok(ToolResult::success("No results found."))
        } else {
            let mut output = String::new();
            output.push_str(&format!("Search results for: {}\n\n", query));

            for (i, result) in results.iter().enumerate() {
                output.push_str(&format!("{}. {}\n", i + 1, result.title));
                output.push_str(&format!("   URL: {}\n", result.url));
                if !result.snippet.is_empty() {
                    output.push_str(&format!("   {}\n", result.snippet));
                }
                output.push('\n');
            }

            Ok(ToolResult::success(output))
        }
    }
}

/// Search result from DuckDuckGo
#[derive(Debug)]
struct SearchResult {
    title: String,
    url: String,
    snippet: String,
}

/// Search DuckDuckGo and parse results
fn search_duckduckgo(query: &str, num_results: usize) -> Result<Vec<SearchResult>, ToolError> {
    let encoded_query = urlencoding::encode(query);
    let url = format!(
        "https://html.duckduckgo.com/html/?q={}",
        encoded_query
    );

    let client = reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(DEFAULT_TIMEOUT))
        .build()
        .map_err(|e| ToolError::ExecutionError(format!("Failed to create HTTP client: {}", e)))?;

    let response = client
        .get(&url)
        .send()
        .map_err(|e| ToolError::ExecutionError(format!("Search request failed: {}", e)))?;

    if !response.status().is_success() {
        return Err(ToolError::ExecutionError(format!(
            "Search failed with status: {}",
            response.status()
        )));
    }

    let html = response
        .text()
        .map_err(|e| ToolError::ExecutionError(format!("Failed to read response: {}", e)))?;

    // Parse results from DuckDuckGo HTML
    let results = parse_duckduckgo_results(&html, num_results);

    Ok(results)
}

/// Parse search results from DuckDuckGo HTML
fn parse_duckduckgo_results(html: &str, max_results: usize) -> Vec<SearchResult> {
    let mut results = Vec::new();

    // DuckDuckGo HTML format has results in divs with class "result"
    // Each result has:
    // - <a class="result__a" href="...">Title</a>
    // - <a class="result__snippet">Snippet</a>

    // Simple regex-free parsing
    let mut pos = 0;
    while let Some(result_start) = html[pos..].find("class=\"result__a\"") {
        if results.len() >= max_results {
            break;
        }

        let result_pos = pos + result_start;

        // Find href
        let href_start = match html[result_pos..].find("href=\"") {
            Some(p) => result_pos + p + 6,
            None => {
                pos = result_pos + 1;
                continue;
            }
        };

        let href_end = match html[href_start..].find('"') {
            Some(p) => href_start + p,
            None => {
                pos = result_pos + 1;
                continue;
            }
        };

        let href = &html[href_start..href_end];

        // DuckDuckGo uses redirect URLs, extract actual URL
        let url = extract_actual_url(href);

        // Find title (between > and </a>)
        let title_start = match html[href_end..].find('>') {
            Some(p) => href_end + p + 1,
            None => {
                pos = result_pos + 1;
                continue;
            }
        };

        let title_end = match html[title_start..].find("</a>") {
            Some(p) => title_start + p,
            None => {
                pos = result_pos + 1;
                continue;
            }
        };

        let title = clean_html_text(&html[title_start..title_end]);

        // Find snippet (optional, in result__snippet)
        let snippet = if let Some(snippet_start) = html[title_end..].find("class=\"result__snippet\"") {
            let snippet_pos = title_end + snippet_start;
            if let Some(content_start) = html[snippet_pos..].find('>') {
                let content_pos = snippet_pos + content_start + 1;
                if let Some(content_end) = html[content_pos..].find("</") {
                    clean_html_text(&html[content_pos..content_pos + content_end])
                } else {
                    String::new()
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        // Skip invalid results
        if !url.is_empty() && !title.is_empty() && !url.contains("duckduckgo.com") {
            results.push(SearchResult {
                title,
                url,
                snippet,
            });
        }

        pos = result_pos + 1;
    }

    results
}

/// Extract actual URL from DuckDuckGo redirect URL
fn extract_actual_url(href: &str) -> String {
    // DuckDuckGo uses //duckduckgo.com/l/?uddg=ENCODED_URL&...
    if href.contains("uddg=") {
        if let Some(start) = href.find("uddg=") {
            let encoded = &href[start + 5..];
            let end = encoded.find('&').unwrap_or(encoded.len());
            let encoded_url = &encoded[..end];
            if let Ok(decoded) = urlencoding::decode(encoded_url) {
                return decoded.to_string();
            }
        }
    }

    // If not a redirect, clean up the URL
    let url = href.trim_start_matches("//");
    if url.starts_with("http") {
        url.to_string()
    } else if !url.is_empty() {
        format!("https://{}", url)
    } else {
        String::new()
    }
}

/// Clean HTML text by removing tags and decoding entities
fn clean_html_text(html: &str) -> String {
    let mut text = String::new();
    let mut in_tag = false;

    for c in html.chars() {
        if c == '<' {
            in_tag = true;
        } else if c == '>' {
            in_tag = false;
        } else if !in_tag {
            text.push(c);
        }
    }

    // Decode common entities
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#x27;", "'")
        .replace("&nbsp;", " ")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_actual_url() {
        let redirect = "//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fpage&rut=abc";
        assert_eq!(extract_actual_url(redirect), "https://example.com/page");

        let direct = "https://example.com";
        assert_eq!(extract_actual_url(direct), "https://example.com");
    }

    #[test]
    fn test_clean_html_text() {
        assert_eq!(clean_html_text("<b>Hello</b> World"), "Hello World");
        assert_eq!(clean_html_text("&amp; test &lt;"), "& test <");
    }

    #[test]
    fn test_permission_required() {
        let temp = tempfile::TempDir::new().unwrap();
        let context = ToolContext::new(temp.path().to_path_buf(), "test".to_string());

        let tool = WebSearchTool;
        let level = tool.permission_level(None, &context);

        assert_eq!(level, PermissionLevel::Required);
    }
}
