//! fetch tool implementation - fetch content from URLs

use std::path::Path;
use std::time::Duration;

use crate::tools::{
    PermissionLevel, Tool, ToolContext, ToolError, ToolParameter, ToolParams, ToolResult,
};

/// Default timeout in seconds
const DEFAULT_TIMEOUT: u64 = 30;

/// Maximum content size in bytes
const MAX_CONTENT_SIZE: usize = 100_000;

/// User agent for requests
const USER_AGENT: &str = "Pangu/0.1 (Local AI Coding Assistant)";

/// Tool for fetching URL contents
pub struct FetchTool;

impl Tool for FetchTool {
    fn name(&self) -> &'static str {
        "fetch"
    }

    fn description(&self) -> &'static str {
        "Fetch content from a URL. Returns the page content as text. Use for reading documentation, API references, or other web content."
    }

    fn parameters(&self) -> &[ToolParameter] {
        &[
            ToolParameter {
                name: "url",
                description: "The URL to fetch",
                required: true,
            },
        ]
    }

    fn permission_level(&self, _path: Option<&Path>, _context: &ToolContext) -> PermissionLevel {
        // All network access requires permission
        PermissionLevel::Required
    }

    fn execute(&self, params: &ToolParams, _context: &ToolContext) -> Result<ToolResult, ToolError> {
        // Get URL parameter
        let url_str = params
            .get("url")
            .ok_or_else(|| ToolError::MissingParameter("url".to_string()))?;

        // Validate URL
        let url = url::Url::parse(url_str).map_err(|e| {
            ToolError::InvalidParameter("url".to_string(), format!("Invalid URL: {}", e))
        })?;

        // Only allow http/https
        if url.scheme() != "http" && url.scheme() != "https" {
            return Err(ToolError::InvalidParameter(
                "url".to_string(),
                "Only HTTP and HTTPS URLs are supported".to_string(),
            ));
        }

        // Fetch the content using blocking reqwest
        // (we're in a sync context from tool execution)
        let content = fetch_url_blocking(url_str)?;

        // Convert HTML to readable text if needed
        let text = if content.contains("<html") || content.contains("<HTML") || content.contains("<!DOCTYPE") {
            html_to_text(&content)
        } else {
            content
        };

        // Truncate if necessary
        if text.len() > MAX_CONTENT_SIZE {
            let truncated = &text[..MAX_CONTENT_SIZE];
            Ok(ToolResult::success_truncated(format!(
                "{}\n\n[Truncated: showing first {} of {} bytes]",
                truncated,
                MAX_CONTENT_SIZE,
                text.len()
            )))
        } else {
            Ok(ToolResult::success(text))
        }
    }
}

/// Fetch URL content using blocking HTTP client
fn fetch_url_blocking(url: &str) -> Result<String, ToolError> {
    // Create a simple blocking HTTP client
    let client = reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(DEFAULT_TIMEOUT))
        .build()
        .map_err(|e| ToolError::ExecutionError(format!("Failed to create HTTP client: {}", e)))?;

    let response = client
        .get(url)
        .send()
        .map_err(|e| ToolError::ExecutionError(format!("HTTP request failed: {}", e)))?;

    if !response.status().is_success() {
        return Err(ToolError::ExecutionError(format!(
            "HTTP error: {} {}",
            response.status().as_u16(),
            response.status().canonical_reason().unwrap_or("Unknown")
        )));
    }

    response
        .text()
        .map_err(|e| ToolError::ExecutionError(format!("Failed to read response: {}", e)))
}

/// Simple HTML to text conversion
fn html_to_text(html: &str) -> String {
    let mut text = String::new();
    let mut in_tag = false;
    let mut in_script = false;
    let mut in_style = false;
    let mut last_was_whitespace = true;

    let html_lower = html.to_lowercase();
    let chars: Vec<char> = html.chars().collect();
    let lower_chars: Vec<char> = html_lower.chars().collect();

    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];

        // Check for script/style tags
        if i + 7 < chars.len() {
            let slice: String = lower_chars[i..i+7].iter().collect();
            if slice == "<script" {
                in_script = true;
            } else if slice == "</scrip" {
                in_script = false;
                // Skip to end of tag
                while i < chars.len() && chars[i] != '>' {
                    i += 1;
                }
                i += 1;
                continue;
            }
        }

        if i + 6 < chars.len() {
            let slice: String = lower_chars[i..i+6].iter().collect();
            if slice == "<style" {
                in_style = true;
            } else if slice == "</styl" {
                in_style = false;
                while i < chars.len() && chars[i] != '>' {
                    i += 1;
                }
                i += 1;
                continue;
            }
        }

        if in_script || in_style {
            i += 1;
            continue;
        }

        if c == '<' {
            in_tag = true;

            // Check for block-level tags that need newlines
            if i + 3 < chars.len() {
                let next3: String = lower_chars[i..i+3].iter().collect();
                if next3 == "<p>" || next3 == "<br" || next3 == "<di" ||
                   next3 == "<li" || next3 == "<h1" || next3 == "<h2" ||
                   next3 == "<h3" || next3 == "<h4" || next3 == "<tr" {
                    if !last_was_whitespace {
                        text.push('\n');
                        last_was_whitespace = true;
                    }
                }
            }
        } else if c == '>' {
            in_tag = false;
        } else if !in_tag {
            // Handle HTML entities
            if c == '&' {
                let mut entity = String::new();
                let start = i;
                while i < chars.len() && chars[i] != ';' && i - start < 10 {
                    entity.push(chars[i]);
                    i += 1;
                }
                if i < chars.len() && chars[i] == ';' {
                    entity.push(';');
                    let decoded = decode_entity(&entity);
                    if decoded != ' ' || !last_was_whitespace {
                        text.push(decoded);
                        last_was_whitespace = decoded.is_whitespace();
                    }
                    i += 1;
                    continue;
                } else {
                    i = start;
                    text.push(c);
                    last_was_whitespace = false;
                }
            } else if c.is_whitespace() {
                if !last_was_whitespace {
                    text.push(' ');
                    last_was_whitespace = true;
                }
            } else {
                text.push(c);
                last_was_whitespace = false;
            }
        }

        i += 1;
    }

    // Clean up multiple newlines
    let mut cleaned = String::new();
    let mut prev_newline_count = 0;
    for c in text.chars() {
        if c == '\n' {
            prev_newline_count += 1;
            if prev_newline_count <= 2 {
                cleaned.push(c);
            }
        } else {
            prev_newline_count = 0;
            cleaned.push(c);
        }
    }

    cleaned.trim().to_string()
}

/// Decode common HTML entities
fn decode_entity(entity: &str) -> char {
    match entity {
        "&amp;" => '&',
        "&lt;" => '<',
        "&gt;" => '>',
        "&quot;" => '"',
        "&apos;" => '\'',
        "&nbsp;" => ' ',
        "&ndash;" => '–',
        "&mdash;" => '—',
        "&copy;" => '©',
        "&reg;" => '®',
        "&trade;" => '™',
        _ => {
            // Try numeric entity
            if entity.starts_with("&#x") && entity.ends_with(';') {
                let hex = &entity[3..entity.len()-1];
                if let Ok(code) = u32::from_str_radix(hex, 16) {
                    if let Some(c) = char::from_u32(code) {
                        return c;
                    }
                }
            } else if entity.starts_with("&#") && entity.ends_with(';') {
                let num = &entity[2..entity.len()-1];
                if let Ok(code) = num.parse::<u32>() {
                    if let Some(c) = char::from_u32(code) {
                        return c;
                    }
                }
            }
            ' ' // Unknown entity becomes space
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_html_to_text_simple() {
        let html = "<html><body><p>Hello <b>World</b></p></body></html>";
        let text = html_to_text(html);
        assert!(text.contains("Hello"));
        assert!(text.contains("World"));
    }

    #[test]
    fn test_html_entities() {
        assert_eq!(decode_entity("&amp;"), '&');
        assert_eq!(decode_entity("&lt;"), '<');
        assert_eq!(decode_entity("&#65;"), 'A');
        assert_eq!(decode_entity("&#x41;"), 'A');
    }

    #[test]
    fn test_permission_required() {
        let temp = tempfile::TempDir::new().unwrap();
        let context = ToolContext::new(temp.path().to_path_buf(), "test".to_string());

        let tool = FetchTool;
        let level = tool.permission_level(None, &context);

        assert_eq!(level, PermissionLevel::Required);
    }
}
