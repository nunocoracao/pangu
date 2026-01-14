//! edit_file tool implementation - targeted find/replace edits

use std::fs;
use std::path::Path;

use crate::tools::{
    PermissionLevel, Tool, ToolContext, ToolError, ToolParameter, ToolParams, ToolResult,
};

/// Tool for making targeted edits to files
pub struct EditFileTool;

impl Tool for EditFileTool {
    fn name(&self) -> &'static str {
        "edit_file"
    }

    fn description(&self) -> &'static str {
        "Make a targeted edit to a file by replacing exact text. The old_text must match exactly."
    }

    fn parameters(&self) -> &[ToolParameter] {
        &[
            ToolParameter {
                name: "path",
                description: "Path to the file to edit",
                required: true,
            },
            ToolParameter {
                name: "old_text",
                description: "Exact text to find and replace",
                required: true,
            },
            ToolParameter {
                name: "new_text",
                description: "Text to replace it with",
                required: true,
            },
        ]
    }

    fn permission_level(&self, _path: Option<&Path>, _context: &ToolContext) -> PermissionLevel {
        // All edits require permission - destructive operation
        PermissionLevel::Required
    }

    fn execute(&self, params: &ToolParams, context: &ToolContext) -> Result<ToolResult, ToolError> {
        // Get parameters
        let path_str = params
            .get("path")
            .ok_or_else(|| ToolError::MissingParameter("path".to_string()))?;

        let old_text = params
            .get("old_text")
            .ok_or_else(|| ToolError::MissingParameter("old_text".to_string()))?;

        let new_text = params
            .get("new_text")
            .ok_or_else(|| ToolError::MissingParameter("new_text".to_string()))?;

        // Resolve the path
        let path = context.resolve_path(path_str)?;

        // Check if file exists
        if !path.is_file() {
            return Err(ToolError::PathError(format!(
                "'{}' is not a file or does not exist",
                path_str
            )));
        }

        // Read the file
        let content = fs::read_to_string(&path).map_err(|e| {
            ToolError::IoError(format!("Failed to read '{}': {}", path_str, e))
        })?;

        // Count occurrences
        let matches: Vec<_> = content.match_indices(old_text).collect();

        if matches.is_empty() {
            return Err(ToolError::InvalidParameter(
                "old_text".to_string(),
                format!("Text not found in file. The old_text must match exactly, including whitespace and indentation."),
            ));
        }

        if matches.len() > 1 {
            return Err(ToolError::InvalidParameter(
                "old_text".to_string(),
                format!(
                    "Found {} occurrences of the text. Please provide more context to make the match unique.",
                    matches.len()
                ),
            ));
        }

        // Perform the replacement
        let new_content = content.replacen(old_text, new_text, 1);

        // Write the file
        fs::write(&path, &new_content).map_err(|e| {
            ToolError::IoError(format!("Failed to write '{}': {}", path_str, e))
        })?;

        // Generate a simple diff summary
        let old_lines = old_text.lines().count();
        let new_lines = new_text.lines().count();
        let line_diff = new_lines as i32 - old_lines as i32;
        let line_change = if line_diff > 0 {
            format!("+{} lines", line_diff)
        } else if line_diff < 0 {
            format!("{} lines", line_diff)
        } else {
            "same line count".to_string()
        };

        Ok(ToolResult::success(format!(
            "Edited {}: replaced {} chars with {} chars ({})",
            path_str,
            old_text.len(),
            new_text.len(),
            line_change
        )))
    }
}

/// Generate a unified diff between old and new text (for permission prompt display)
#[allow(dead_code)]
pub fn generate_diff(old_text: &str, new_text: &str, path: &str) -> String {
    let mut diff = String::new();
    diff.push_str(&format!("--- a/{}\n", path));
    diff.push_str(&format!("+++ b/{}\n", path));

    // Simple line-by-line diff
    let old_lines: Vec<&str> = old_text.lines().collect();
    let new_lines: Vec<&str> = new_text.lines().collect();

    for line in &old_lines {
        diff.push_str(&format!("-{}\n", line));
    }
    for line in &new_lines {
        diff.push_str(&format!("+{}\n", line));
    }

    diff
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_context(dir: &Path) -> ToolContext {
        ToolContext::new(dir.to_path_buf(), "test123".to_string())
    }

    #[test]
    fn test_edit_simple_replacement() {
        let temp = TempDir::new().unwrap();
        let context = create_test_context(temp.path());

        // Create test file
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "Hello, World!").unwrap();

        let tool = EditFileTool;
        let mut params = ToolParams::new();
        params.insert("path", "test.txt");
        params.insert("old_text", "World");
        params.insert("new_text", "Rust");

        let result = tool.execute(&params, &context).unwrap();

        assert!(result.success);

        let content = fs::read_to_string(file_path).unwrap();
        assert_eq!(content, "Hello, Rust!");
    }

    #[test]
    fn test_edit_multiline() {
        let temp = TempDir::new().unwrap();
        let context = create_test_context(temp.path());

        let file_path = temp.path().join("code.rs");
        fs::write(&file_path, "fn main() {\n    println!(\"hello\");\n}").unwrap();

        let tool = EditFileTool;
        let mut params = ToolParams::new();
        params.insert("path", "code.rs");
        params.insert("old_text", "fn main() {\n    println!(\"hello\");\n}");
        params.insert("new_text", "fn main() {\n    println!(\"goodbye\");\n    println!(\"world\");\n}");

        let result = tool.execute(&params, &context).unwrap();

        assert!(result.success);
        assert!(result.output.contains("+1 lines"));

        let content = fs::read_to_string(file_path).unwrap();
        assert!(content.contains("goodbye"));
        assert!(content.contains("world"));
    }

    #[test]
    fn test_edit_not_found() {
        let temp = TempDir::new().unwrap();
        let context = create_test_context(temp.path());

        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "Hello, World!").unwrap();

        let tool = EditFileTool;
        let mut params = ToolParams::new();
        params.insert("path", "test.txt");
        params.insert("old_text", "not found text");
        params.insert("new_text", "replacement");

        let result = tool.execute(&params, &context);

        assert!(result.is_err());
        if let Err(ToolError::InvalidParameter(_, msg)) = result {
            assert!(msg.contains("not found"));
        } else {
            panic!("Expected InvalidParameter error");
        }
    }

    #[test]
    fn test_edit_ambiguous_match() {
        let temp = TempDir::new().unwrap();
        let context = create_test_context(temp.path());

        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "hello hello hello").unwrap();

        let tool = EditFileTool;
        let mut params = ToolParams::new();
        params.insert("path", "test.txt");
        params.insert("old_text", "hello");
        params.insert("new_text", "hi");

        let result = tool.execute(&params, &context);

        assert!(result.is_err());
        if let Err(ToolError::InvalidParameter(_, msg)) = result {
            assert!(msg.contains("3 occurrences"));
        } else {
            panic!("Expected InvalidParameter error");
        }
    }

    #[test]
    fn test_permission_always_required() {
        let temp = TempDir::new().unwrap();
        let context = create_test_context(temp.path());

        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "test").unwrap();

        let tool = EditFileTool;
        let level = tool.permission_level(Some(&file_path), &context);

        assert_eq!(level, PermissionLevel::Required);
    }
}
