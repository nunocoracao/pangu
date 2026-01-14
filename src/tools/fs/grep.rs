//! grep tool implementation - search for patterns across files

use std::fs;
use std::path::Path;

use regex::Regex;
use walkdir::WalkDir;

use crate::tools::{
    PermissionLevel, Tool, ToolContext, ToolError, ToolParameter, ToolParams, ToolResult,
};

/// Maximum number of matches to return
const MAX_MATCHES: usize = 100;

/// Maximum output size in bytes
const MAX_OUTPUT_SIZE: usize = 50_000;

/// Tool for searching file contents with regex patterns
pub struct GrepTool;

impl Tool for GrepTool {
    fn name(&self) -> &'static str {
        "grep"
    }

    fn description(&self) -> &'static str {
        "Search for a pattern across files. Returns matching lines with file paths and line numbers."
    }

    fn parameters(&self) -> &[ToolParameter] {
        &[
            ToolParameter {
                name: "pattern",
                description: "Regex pattern to search for",
                required: true,
            },
            ToolParameter {
                name: "path",
                description: "Directory or file to search in (default: project root)",
                required: false,
            },
            ToolParameter {
                name: "include",
                description: "File glob pattern to include (e.g., *.rs, *.py)",
                required: false,
            },
        ]
    }

    fn permission_level(&self, path: Option<&Path>, context: &ToolContext) -> PermissionLevel {
        match path {
            Some(p) => {
                if context.is_within_project(p) {
                    PermissionLevel::None
                } else {
                    PermissionLevel::Required
                }
            }
            None => PermissionLevel::None, // Default to project root, which is always allowed
        }
    }

    fn execute(&self, params: &ToolParams, context: &ToolContext) -> Result<ToolResult, ToolError> {
        // Get pattern parameter
        let pattern_str = params
            .get("pattern")
            .ok_or_else(|| ToolError::MissingParameter("pattern".to_string()))?;

        // Compile regex
        let regex = Regex::new(pattern_str).map_err(|e| {
            ToolError::InvalidParameter("pattern".to_string(), format!("Invalid regex: {}", e))
        })?;

        // Get search path (default to project root)
        let search_path = match params.get("path") {
            Some(p) => context.resolve_path(p)?,
            None => context.project_root.clone(),
        };

        // Get include pattern for filtering files
        let include_pattern = params.get("include").map(|p| {
            // Convert glob pattern to regex
            let regex_str = glob_to_regex(p);
            Regex::new(&regex_str).ok()
        }).flatten();

        let mut matches = Vec::new();
        let mut total_output_size = 0;

        // Walk directory or check single file
        if search_path.is_file() {
            if should_search_file(&search_path, &include_pattern) {
                search_file(&search_path, &regex, &mut matches, &mut total_output_size)?;
            }
        } else if search_path.is_dir() {
            for entry in WalkDir::new(&search_path)
                .follow_links(true)
                .into_iter()
                .filter_entry(|e| !is_hidden(e))
                .filter_map(|e| e.ok())
            {
                if matches.len() >= MAX_MATCHES || total_output_size >= MAX_OUTPUT_SIZE {
                    break;
                }

                let path = entry.path();
                if path.is_file() && should_search_file(path, &include_pattern) {
                    search_file(path, &regex, &mut matches, &mut total_output_size)?;
                }
            }
        } else {
            return Err(ToolError::PathError(format!(
                "Path '{}' does not exist",
                search_path.display()
            )));
        }

        if matches.is_empty() {
            Ok(ToolResult::success("No matches found."))
        } else {
            let truncated = matches.len() >= MAX_MATCHES || total_output_size >= MAX_OUTPUT_SIZE;
            let output = matches.join("\n");

            if truncated {
                Ok(ToolResult::success_truncated(format!(
                    "{}\n\n[Truncated: showing first {} matches]",
                    output,
                    matches.len()
                )))
            } else {
                Ok(ToolResult::success(format!(
                    "{}\n\n[{} matches found]",
                    output,
                    matches.len()
                )))
            }
        }
    }
}

/// Search a single file for pattern matches
fn search_file(
    path: &Path,
    regex: &Regex,
    matches: &mut Vec<String>,
    total_size: &mut usize,
) -> Result<(), ToolError> {
    // Skip binary files
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Ok(()), // Skip files we can't read as text
    };

    for (line_num, line) in content.lines().enumerate() {
        if matches.len() >= MAX_MATCHES || *total_size >= MAX_OUTPUT_SIZE {
            break;
        }

        if regex.is_match(line) {
            let match_line = format!("{}:{}: {}", path.display(), line_num + 1, line.trim());
            *total_size += match_line.len();
            matches.push(match_line);
        }
    }

    Ok(())
}

/// Check if a file should be searched based on include pattern
fn should_search_file(path: &Path, include_pattern: &Option<Regex>) -> bool {
    // Skip common non-text files
    if let Some(ext) = path.extension() {
        let ext_str = ext.to_string_lossy().to_lowercase();
        let skip_extensions = [
            "png", "jpg", "jpeg", "gif", "bmp", "ico", "svg",
            "pdf", "doc", "docx", "xls", "xlsx",
            "zip", "tar", "gz", "rar", "7z",
            "exe", "dll", "so", "dylib", "o", "a",
            "wasm", "class", "pyc", "pyo",
            "db", "sqlite", "sqlite3",
            "lock", "gguf", "bin",
        ];
        if skip_extensions.contains(&ext_str.as_str()) {
            return false;
        }
    }

    // Check include pattern if specified
    if let Some(pattern) = include_pattern {
        if let Some(name) = path.file_name() {
            return pattern.is_match(&name.to_string_lossy());
        }
        return false;
    }

    true
}

/// Check if entry is hidden (starts with .)
/// We don't filter the root directory since that's our starting point
fn is_hidden(entry: &walkdir::DirEntry) -> bool {
    // Don't filter the root entry (depth 0)
    if entry.depth() == 0 {
        return false;
    }

    entry
        .file_name()
        .to_str()
        .map(|s| s.starts_with('.') && s != ".")
        .unwrap_or(false)
}

/// Convert a simple glob pattern to regex
fn glob_to_regex(glob: &str) -> String {
    let mut regex = String::from("^");
    for c in glob.chars() {
        match c {
            '*' => regex.push_str(".*"),
            '?' => regex.push('.'),
            '.' => regex.push_str("\\."),
            '[' | ']' | '(' | ')' | '{' | '}' | '^' | '$' | '|' | '+' | '\\' => {
                regex.push('\\');
                regex.push(c);
            }
            _ => regex.push(c),
        }
    }
    regex.push('$');
    regex
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_context(dir: &Path) -> ToolContext {
        ToolContext::new(dir.to_path_buf(), "test123".to_string())
    }

    #[test]
    fn test_grep_simple_pattern() {
        let temp = TempDir::new().unwrap();
        let context = create_test_context(temp.path());

        // Create test files
        fs::write(temp.path().join("file1.rs"), "fn main() {\n    println!(\"hello\");\n}").unwrap();
        fs::write(temp.path().join("file2.rs"), "fn helper() {\n    println!(\"world\");\n}").unwrap();

        let tool = GrepTool;
        let mut params = ToolParams::new();
        params.insert("pattern", "fn main");

        let result = tool.execute(&params, &context).unwrap();

        assert!(result.success);
        assert!(result.output.contains("fn main"));
        assert!(result.output.contains("file1.rs"));
    }

    #[test]
    fn test_grep_with_include() {
        let temp = TempDir::new().unwrap();
        let context = create_test_context(temp.path());

        // Create test files
        fs::write(temp.path().join("file1.rs"), "fn test()").unwrap();
        fs::write(temp.path().join("file2.py"), "def test():").unwrap();

        let tool = GrepTool;
        let mut params = ToolParams::new();
        params.insert("pattern", "test");
        params.insert("include", "*.rs");

        let result = tool.execute(&params, &context).unwrap();

        assert!(result.success);
        assert!(result.output.contains("file1.rs"));
        assert!(!result.output.contains("file2.py"));
    }

    #[test]
    fn test_grep_no_matches() {
        let temp = TempDir::new().unwrap();
        let context = create_test_context(temp.path());

        fs::write(temp.path().join("file.txt"), "hello world").unwrap();

        let tool = GrepTool;
        let mut params = ToolParams::new();
        params.insert("pattern", "xyz123notfound");

        let result = tool.execute(&params, &context).unwrap();

        assert!(result.success);
        assert!(result.output.contains("No matches found"));
    }

    #[test]
    fn test_glob_to_regex() {
        assert_eq!(glob_to_regex("*.rs"), "^.*\\.rs$");
        assert_eq!(glob_to_regex("test?.txt"), "^test.\\.txt$");
    }
}
