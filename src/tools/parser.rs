//! Tool call parser for extracting tool invocations from model output
//!
//! Supports multiple formats for maximum reliability with local models.
//!
//! Inline format (simplest):
//! ```xml
//! <tool_call> read_file src/main.rs </tool_call>
//! ```
//!
//! XML format with nested tags:
//! ```xml
//! <tool_call>
//! <name>read_file</name>
//! <path>src/main.rs</path>
//! </tool_call>
//! ```
//!
//! JSON format (fallback):
//! ```json
//! {"tool": "read_file", "params": {"path": "src/main.rs"}}
//! ```

use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;

use super::{ToolCall, ToolParams};

/// Regex for matching XML tool calls
static XML_TOOL_CALL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<tool_call>\s*(.*?)\s*</tool_call>").unwrap()
});

/// Regex for matching Mistral-style tool calls: [TOOL_CALLS]name[ARGS]{...}
static MISTRAL_TOOL_CALL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)\[TOOL_CALLS\]\s*(\w+)\s*\[ARGS\]\s*(\{.*?\})").unwrap()
});

/// Regex for extracting XML tag content (matches common tags)
/// Since regex crate doesn't support backreferences, we list common tag names
static XML_NAME_TAG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<name>\s*(.*?)\s*</name>").unwrap()
});

static XML_PATH_TAG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<path>\s*(.*?)\s*</path>").unwrap()
});

static XML_CONTENT_TAG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<content>\s*(.*?)\s*</content>").unwrap()
});

static XML_OLD_TEXT_TAG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<old_text>\s*(.*?)\s*</old_text>").unwrap()
});

static XML_NEW_TEXT_TAG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<new_text>\s*(.*?)\s*</new_text>").unwrap()
});

static XML_COMMAND_TAG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<command>\s*(.*?)\s*</command>").unwrap()
});

static XML_QUERY_TAG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<query>\s*(.*?)\s*</query>").unwrap()
});

static XML_URL_TAG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<url>\s*(.*?)\s*</url>").unwrap()
});

static XML_PATTERN_TAG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<pattern>\s*(.*?)\s*</pattern>").unwrap()
});

static XML_INCLUDE_TAG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<include>\s*(.*?)\s*</include>").unwrap()
});

static XML_TIMEOUT_TAG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<timeout>\s*(.*?)\s*</timeout>").unwrap()
});

static XML_NUM_RESULTS_TAG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<num_results>\s*(.*?)\s*</num_results>").unwrap()
});

/// Parser for extracting tool calls from model output
pub struct ToolCallParser;

impl ToolCallParser {
    /// Parse tool calls from assistant response
    ///
    /// Returns (clean_content, tool_calls) where clean_content has tool call
    /// blocks removed for display purposes.
    pub fn parse(content: &str) -> (String, Vec<ToolCall>) {
        let mut tool_calls = Vec::new();
        let mut clean_content = content.to_string();

        // Try Mistral format first: [TOOL_CALLS]name[ARGS]{...}
        let mistral_calls = Self::parse_mistral(&mut clean_content);
        tool_calls.extend(mistral_calls);

        // Try XML parsing (primary format for our prompts)
        if tool_calls.is_empty() {
            let xml_calls = Self::parse_xml(&mut clean_content);
            tool_calls.extend(xml_calls);
        }

        // Try JSON parsing as fallback if no XML calls found
        if tool_calls.is_empty() {
            let json_calls = Self::parse_json(&mut clean_content);
            tool_calls.extend(json_calls);
        }

        // Clean up extra whitespace
        let clean_content = clean_content
            .lines()
            .filter(|line| !line.trim().is_empty() || tool_calls.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string();

        (clean_content, tool_calls)
    }

    /// Parse Mistral-style tool calls: [TOOL_CALLS]name[ARGS]{...}
    fn parse_mistral(content: &mut String) -> Vec<ToolCall> {
        let mut tool_calls = Vec::new();

        // Find all [TOOL_CALLS]...[ARGS]{...} patterns
        let matches: Vec<_> = MISTRAL_TOOL_CALL_RE
            .captures_iter(content)
            .map(|cap| {
                let full = cap.get(0).unwrap();
                let name = cap.get(1).unwrap().as_str().to_string();
                let args_json = cap.get(2).unwrap().as_str().to_string();
                (full.start(), full.end(), full.as_str().to_string(), name, args_json)
            })
            .collect();

        // Process matches in reverse order to preserve indices when removing
        for (_start, _end, raw, name, args_json) in matches.iter().rev() {
            if let Some(tool_call) = Self::parse_mistral_call(name, args_json, raw) {
                tool_calls.push(tool_call);
            }

            // Remove the tool call from content
            *content = content.replace(raw.as_str(), "");
        }

        // Reverse to get correct order
        tool_calls.reverse();
        tool_calls
    }

    /// Parse a single Mistral-style tool call
    fn parse_mistral_call(name: &str, args_json: &str, raw: &str) -> Option<ToolCall> {
        let mut params = HashMap::new();

        // Try to parse the JSON args
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(args_json) {
            if let Some(obj) = value.as_object() {
                for (key, val) in obj {
                    if let Some(s) = val.as_str() {
                        params.insert(key.clone(), s.to_string());
                    } else {
                        // For non-string values, convert to string
                        params.insert(key.clone(), val.to_string());
                    }
                }
            }
        }

        Some(ToolCall::new(name.to_string(), ToolParams::from(params), raw.to_string()))
    }

    /// Parse XML-formatted tool calls
    fn parse_xml(content: &mut String) -> Vec<ToolCall> {
        let mut tool_calls = Vec::new();

        // Find all <tool_call>...</tool_call> blocks
        let matches: Vec<_> = XML_TOOL_CALL_RE
            .find_iter(content)
            .map(|m| (m.start(), m.end(), m.as_str().to_string()))
            .collect();

        // Process matches in reverse order to preserve indices when removing
        for (_start, _end, raw) in matches.iter().rev() {
            if let Some(tool_call) = Self::parse_xml_block(raw) {
                tool_calls.push(tool_call);
            }

            // Remove the tool call block from content
            *content = content.replace(raw.as_str(), "");
        }

        // Reverse to get correct order (we processed in reverse)
        tool_calls.reverse();
        tool_calls
    }

    /// Parse a single XML tool call block
    ///
    /// Supports two formats:
    /// 1. Inline: `<tool_call> tool_name arg1 arg2 </tool_call>`
    /// 2. Nested: `<tool_call><name>tool</name><path>arg</path></tool_call>`
    fn parse_xml_block(block: &str) -> Option<ToolCall> {
        // Extract the inner content
        let inner = XML_TOOL_CALL_RE
            .captures(block)?
            .get(1)?
            .as_str()
            .trim();

        // Check if it contains nested XML tags
        if inner.contains('<') {
            // Nested XML format
            Self::parse_nested_xml(inner, block)
        } else {
            // Inline format: "tool_name arg1 arg2 ..."
            Self::parse_inline_xml(inner, block)
        }
    }

    /// Parse nested XML format: `<name>tool</name><path>arg</path>`
    fn parse_nested_xml(inner: &str, raw: &str) -> Option<ToolCall> {
        let mut params = HashMap::new();

        // Extract tool name
        let name = XML_NAME_TAG_RE
            .captures(inner)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().trim().to_string())?;

        // Helper macro to extract optional parameter
        macro_rules! extract_param {
            ($regex:expr, $key:expr) => {
                if let Some(cap) = $regex.captures(inner) {
                    if let Some(m) = cap.get(1) {
                        params.insert($key.to_string(), m.as_str().trim().to_string());
                    }
                }
            };
        }

        // Extract all possible parameters
        extract_param!(XML_PATH_TAG_RE, "path");
        extract_param!(XML_CONTENT_TAG_RE, "content");
        extract_param!(XML_OLD_TEXT_TAG_RE, "old_text");
        extract_param!(XML_NEW_TEXT_TAG_RE, "new_text");
        extract_param!(XML_COMMAND_TAG_RE, "command");
        extract_param!(XML_QUERY_TAG_RE, "query");
        extract_param!(XML_URL_TAG_RE, "url");
        extract_param!(XML_PATTERN_TAG_RE, "pattern");
        extract_param!(XML_INCLUDE_TAG_RE, "include");
        extract_param!(XML_TIMEOUT_TAG_RE, "timeout");
        extract_param!(XML_NUM_RESULTS_TAG_RE, "num_results");

        Some(ToolCall::new(name, ToolParams::from(params), raw.to_string()))
    }

    /// Parse inline format: `tool_name arg1 arg2 ...`
    ///
    /// Maps positional arguments to named parameters based on tool:
    /// - read_file: first arg -> "path"
    /// - list_files: first arg -> "path"
    /// - grep: first arg -> "pattern", second -> "path"
    /// - write_file: first arg -> "path", rest -> "content"
    /// - edit_file: requires nested XML (too complex for inline)
    /// - bash: all args -> "command"
    /// - web_search: all args -> "query"
    /// - fetch: first arg -> "url"
    fn parse_inline_xml(inner: &str, raw: &str) -> Option<ToolCall> {
        let parts: Vec<&str> = inner.split_whitespace().collect();
        if parts.is_empty() {
            return None;
        }

        let name = parts[0].to_string();
        let args = &parts[1..];
        let mut params = HashMap::new();

        // Map positional args to named params based on tool
        match name.as_str() {
            "read_file" | "list_files" => {
                if let Some(path) = args.first() {
                    params.insert("path".to_string(), path.to_string());
                }
            }
            "grep" => {
                // grep pattern [path] [include]
                if let Some(pattern) = args.first() {
                    params.insert("pattern".to_string(), pattern.to_string());
                }
                if args.len() > 1 {
                    params.insert("path".to_string(), args[1].to_string());
                }
                if args.len() > 2 {
                    params.insert("include".to_string(), args[2].to_string());
                }
            }
            "write_file" => {
                if let Some(path) = args.first() {
                    params.insert("path".to_string(), path.to_string());
                }
                if args.len() > 1 {
                    // Join remaining args as content
                    params.insert("content".to_string(), args[1..].join(" "));
                }
            }
            "bash" => {
                // All args are the command
                if !args.is_empty() {
                    params.insert("command".to_string(), args.join(" "));
                }
            }
            "web_search" => {
                // All args are the query
                if !args.is_empty() {
                    params.insert("query".to_string(), args.join(" "));
                }
            }
            "fetch" => {
                if let Some(url) = args.first() {
                    params.insert("url".to_string(), url.to_string());
                }
            }
            _ => {
                // Generic handling: first arg is "path" or "arg1"
                for (i, arg) in args.iter().enumerate() {
                    let param_name = if i == 0 { "path".to_string() } else { format!("arg{}", i) };
                    params.insert(param_name, arg.to_string());
                }
            }
        }

        Some(ToolCall::new(name, ToolParams::from(params), raw.to_string()))
    }

    /// Parse JSON-formatted tool calls
    fn parse_json(content: &mut String) -> Vec<ToolCall> {
        let mut tool_calls = Vec::new();

        // Try to find JSON objects that look like tool calls
        // This is a simple heuristic - we look for {"tool": "..."}
        let lines: Vec<_> = content.lines().collect();
        let mut lines_to_remove = Vec::new();

        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // Check if line looks like a JSON tool call
            if trimmed.starts_with('{') && trimmed.contains("\"tool\"") {
                if let Some(tool_call) = Self::parse_json_line(trimmed) {
                    tool_calls.push(tool_call);
                    lines_to_remove.push(idx);
                }
            }
        }

        // Remove matched lines
        if !lines_to_remove.is_empty() {
            let new_content: Vec<_> = lines
                .iter()
                .enumerate()
                .filter(|(idx, _)| !lines_to_remove.contains(idx))
                .map(|(_, line)| *line)
                .collect();
            *content = new_content.join("\n");
        }

        tool_calls
    }

    /// Parse a single JSON tool call line
    fn parse_json_line(line: &str) -> Option<ToolCall> {
        // Try to parse as JSON
        let value: serde_json::Value = serde_json::from_str(line).ok()?;
        let obj = value.as_object()?;

        // Extract tool name
        let name = obj.get("tool")?.as_str()?.to_string();

        // Extract parameters
        let mut params = HashMap::new();

        if let Some(params_obj) = obj.get("params").and_then(|p| p.as_object()) {
            for (key, val) in params_obj {
                if let Some(s) = val.as_str() {
                    params.insert(key.clone(), s.to_string());
                } else {
                    params.insert(key.clone(), val.to_string());
                }
            }
        }

        // Also check for flat parameters (tool call without nested "params")
        for (key, val) in obj {
            if key != "tool" && key != "params" {
                if let Some(s) = val.as_str() {
                    params.insert(key.clone(), s.to_string());
                }
            }
        }

        Some(ToolCall::new(name, ToolParams::from(params), line.to_string()))
    }

    /// Check if content contains any tool calls (quick check without full parsing)
    pub fn contains_tool_call(content: &str) -> bool {
        content.contains("<tool_call>")
            || content.contains("[TOOL_CALLS]")
            || (content.contains("\"tool\"") && content.contains('{'))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_xml_simple() {
        let content = r#"Let me read that file for you.

<tool_call>
<name>read_file</name>
<path>src/main.rs</path>
</tool_call>

I'll analyze the contents."#;

        let (clean, calls) = ToolCallParser::parse(content);

        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "read_file");
        assert_eq!(calls[0].params.get("path"), Some("src/main.rs"));
        assert!(!clean.contains("<tool_call>"));
        assert!(clean.contains("Let me read that file"));
    }

    #[test]
    fn test_parse_xml_multiple() {
        let content = r#"<tool_call>
<name>read_file</name>
<path>file1.rs</path>
</tool_call>

<tool_call>
<name>read_file</name>
<path>file2.rs</path>
</tool_call>"#;

        let (_, calls) = ToolCallParser::parse(content);

        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].params.get("path"), Some("file1.rs"));
        assert_eq!(calls[1].params.get("path"), Some("file2.rs"));
    }

    #[test]
    fn test_parse_json() {
        let content = r#"Here's the tool call:
{"tool": "read_file", "params": {"path": "src/lib.rs"}}"#;

        let (_, calls) = ToolCallParser::parse(content);

        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "read_file");
        assert_eq!(calls[0].params.get("path"), Some("src/lib.rs"));
    }

    #[test]
    fn test_parse_no_tool_calls() {
        let content = "Just a regular message without any tool calls.";

        let (clean, calls) = ToolCallParser::parse(content);

        assert!(calls.is_empty());
        assert_eq!(clean, content);
    }

    #[test]
    fn test_contains_tool_call() {
        assert!(ToolCallParser::contains_tool_call("<tool_call>"));
        assert!(ToolCallParser::contains_tool_call(r#"{"tool": "x"}"#));
        assert!(!ToolCallParser::contains_tool_call("regular text"));
    }

    #[test]
    fn test_parse_inline_format() {
        let content = r#"Let me list the files.

<tool_call> list_files . </tool_call>

Here are the results."#;

        let (clean, calls) = ToolCallParser::parse(content);

        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "list_files");
        assert_eq!(calls[0].params.get("path"), Some("."));
        assert!(!clean.contains("<tool_call>"));
        assert!(clean.contains("Let me list the files"));
    }

    #[test]
    fn test_parse_inline_read_file() {
        let content = "<tool_call> read_file src/main.rs </tool_call>";

        let (_, calls) = ToolCallParser::parse(content);

        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "read_file");
        assert_eq!(calls[0].params.get("path"), Some("src/main.rs"));
    }

    #[test]
    fn test_parse_mixed_formats() {
        let content = r#"<tool_call> list_files . </tool_call>

<tool_call>
<name>read_file</name>
<path>Cargo.toml</path>
</tool_call>"#;

        let (_, calls) = ToolCallParser::parse(content);

        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "list_files");
        assert_eq!(calls[0].params.get("path"), Some("."));
        assert_eq!(calls[1].name, "read_file");
        assert_eq!(calls[1].params.get("path"), Some("Cargo.toml"));
    }

    #[test]
    fn test_parse_mistral_format() {
        let content = r#"Let me list the files.
[TOOL_CALLS]list_files[ARGS]{"path": "src"}
Here are the results."#;

        let (clean, calls) = ToolCallParser::parse(content);

        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "list_files");
        assert_eq!(calls[0].params.get("path"), Some("src"));
        assert!(!clean.contains("[TOOL_CALLS]"));
        assert!(clean.contains("Let me list the files"));
    }

    #[test]
    fn test_parse_mistral_write_file() {
        let content = r#"[TOOL_CALLS]write_file[ARGS]{"path": "test.txt", "content": "Hello World"}"#;

        let (_, calls) = ToolCallParser::parse(content);

        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "write_file");
        assert_eq!(calls[0].params.get("path"), Some("test.txt"));
        assert_eq!(calls[0].params.get("content"), Some("Hello World"));
    }

    #[test]
    fn test_contains_mistral_tool_call() {
        assert!(ToolCallParser::contains_tool_call("[TOOL_CALLS]list_files[ARGS]{}"));
    }
}
