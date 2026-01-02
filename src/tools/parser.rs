//! Parser for extracting tool calls and thinking blocks from model output

/// Represents a parsed thinking block from the model's output
#[derive(Debug, Clone)]
pub struct ThinkingBlock {
    /// The thinking content
    pub content: String,
    /// Start position in the original text
    pub start: usize,
    /// End position in the original text
    pub end: usize,
}

/// Represents a parsed tool call from the model's output
#[derive(Debug, Clone)]
pub struct ToolCall {
    /// Name of the tool to execute
    pub name: String,
    /// Parameters for the tool (e.g., URL for fetch)
    pub params: String,
    /// Start position in the original text
    pub start: usize,
    /// End position in the original text
    pub end: usize,
}

/// Parse tool calls from model output text
///
/// Looks for patterns like:
/// ```text
/// <tool_use>
/// <name>fetch</name>
/// <url>https://example.com</url>
/// </tool_use>
/// ```
///
/// Or simpler single-param format:
/// ```text
/// <tool_use>
/// <name>fetch</name>
/// <params>https://example.com</params>
/// </tool_use>
/// ```
pub fn parse_tool_calls(text: &str) -> Vec<ToolCall> {
    let mut calls = Vec::new();
    let mut search_start = 0;

    while let Some(start) = text[search_start..].find("<tool_use>") {
        let absolute_start = search_start + start;

        if let Some(end_offset) = text[absolute_start..].find("</tool_use>") {
            let absolute_end = absolute_start + end_offset + "</tool_use>".len();
            let block = &text[absolute_start..absolute_end];

            if let Some(tool_call) = parse_tool_block(block, absolute_start, absolute_end) {
                calls.push(tool_call);
            }

            search_start = absolute_end;
        } else {
            // No closing tag found, stop searching
            break;
        }
    }

    calls
}

/// Parse a single tool_use block
fn parse_tool_block(block: &str, start: usize, end: usize) -> Option<ToolCall> {
    // Extract name
    let name = extract_tag_content(block, "name")?;

    // Try different parameter tag names
    let params = extract_tag_content(block, "url")
        .or_else(|| extract_tag_content(block, "params"))
        .or_else(|| extract_tag_content(block, "param"))
        .or_else(|| extract_tag_content(block, "query"))
        .or_else(|| extract_tag_content(block, "path"))
        .unwrap_or_default();

    Some(ToolCall {
        name: name.trim().to_string(),
        params: params.trim().to_string(),
        start,
        end,
    })
}

/// Extract content between XML-style tags
fn extract_tag_content(text: &str, tag: &str) -> Option<String> {
    let open_tag = format!("<{}>", tag);
    let close_tag = format!("</{}>", tag);

    let start = text.find(&open_tag)?;
    let content_start = start + open_tag.len();
    let content_end = text[content_start..].find(&close_tag)?;

    Some(text[content_start..content_start + content_end].to_string())
}

/// Remove tool call blocks from text, returning the cleaned text
pub fn remove_tool_calls(text: &str) -> String {
    let calls = parse_tool_calls(text);
    if calls.is_empty() {
        return text.to_string();
    }

    let mut result = String::new();
    let mut last_end = 0;

    for call in &calls {
        result.push_str(&text[last_end..call.start]);
        last_end = call.end;
    }

    result.push_str(&text[last_end..]);
    result.trim().to_string()
}

/// Parse thinking blocks from model output text
///
/// Looks for patterns like:
/// ```text
/// <thinking>
/// My reasoning here...
/// </thinking>
/// ```
pub fn parse_thinking_blocks(text: &str) -> Vec<ThinkingBlock> {
    let mut blocks = Vec::new();
    let mut search_start = 0;

    while let Some(start) = text[search_start..].find("<thinking>") {
        let absolute_start = search_start + start;

        if let Some(end_offset) = text[absolute_start..].find("</thinking>") {
            let content_start = absolute_start + "<thinking>".len();
            let content_end = absolute_start + end_offset;
            let absolute_end = content_end + "</thinking>".len();

            let content = text[content_start..content_end].trim().to_string();

            blocks.push(ThinkingBlock {
                content,
                start: absolute_start,
                end: absolute_end,
            });

            search_start = absolute_end;
        } else {
            // No closing tag found, stop searching
            break;
        }
    }

    blocks
}

/// Remove thinking blocks from text, returning the cleaned text
pub fn remove_thinking_blocks(text: &str) -> String {
    let blocks = parse_thinking_blocks(text);
    if blocks.is_empty() {
        return text.to_string();
    }

    let mut result = String::new();
    let mut last_end = 0;

    for block in &blocks {
        result.push_str(&text[last_end..block.start]);
        last_end = block.end;
    }

    result.push_str(&text[last_end..]);
    result.trim().to_string()
}

/// Check if text contains a partial (unclosed) thinking block
pub fn has_partial_thinking(text: &str) -> bool {
    text.contains("<thinking>") && !text.contains("</thinking>")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_fetch_tool() {
        let text = r#"Let me look that up for you.

<tool_use>
<name>fetch</name>
<url>https://example.com</url>
</tool_use>

I'll analyze the results."#;

        let calls = parse_tool_calls(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "fetch");
        assert_eq!(calls[0].params, "https://example.com");
    }

    #[test]
    fn test_parse_multiple_tools() {
        let text = r#"<tool_use>
<name>fetch</name>
<url>https://first.com</url>
</tool_use>

Some text between.

<tool_use>
<name>fetch</name>
<url>https://second.com</url>
</tool_use>"#;

        let calls = parse_tool_calls(text);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].params, "https://first.com");
        assert_eq!(calls[1].params, "https://second.com");
    }

    #[test]
    fn test_remove_tool_calls() {
        let text = r#"Before.

<tool_use>
<name>fetch</name>
<url>https://example.com</url>
</tool_use>

After."#;

        let cleaned = remove_tool_calls(text);
        assert_eq!(cleaned, "Before.\n\n\n\nAfter.");
    }

    #[test]
    fn test_no_tool_calls() {
        let text = "Just regular text without any tool calls.";
        let calls = parse_tool_calls(text);
        assert!(calls.is_empty());
    }
}
