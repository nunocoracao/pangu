//! read_file tool implementation

use std::fs;
use std::path::Path;

use crate::tools::{
    PermissionLevel, Tool, ToolContext, ToolError, ToolParameter, ToolParams, ToolResult,
};

/// Maximum file size to read (50KB)
const MAX_FILE_SIZE: usize = 50_000;

/// Tool for reading file contents
pub struct ReadFileTool;

impl Tool for ReadFileTool {
    fn name(&self) -> &'static str {
        "read_file"
    }

    fn description(&self) -> &'static str {
        "Read the contents of a file. Use absolute paths or paths relative to the project root."
    }

    fn parameters(&self) -> &[ToolParameter] {
        &[ToolParameter {
            name: "path",
            description: "Path to the file to read",
            required: true,
        }]
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
            None => PermissionLevel::Required,
        }
    }

    fn execute(&self, params: &ToolParams, context: &ToolContext) -> Result<ToolResult, ToolError> {
        // Get path parameter
        let path_str = params
            .get("path")
            .ok_or_else(|| ToolError::MissingParameter("path".to_string()))?;

        // Resolve the path
        let path = context.resolve_path(path_str)?;

        // Check if it's a file
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

        // Check size and truncate if necessary
        if content.len() > MAX_FILE_SIZE {
            let truncated = &content[..MAX_FILE_SIZE];
            let result = format!(
                "{}\n\n[Truncated: showing first {} of {} bytes]",
                truncated,
                MAX_FILE_SIZE,
                content.len()
            );
            Ok(ToolResult::success_truncated(result))
        } else {
            Ok(ToolResult::success(content))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn create_test_context(dir: &Path) -> ToolContext {
        ToolContext::new(dir.to_path_buf(), "test123".to_string())
    }

    #[test]
    fn test_read_file() {
        let temp = TempDir::new().unwrap();
        let context = create_test_context(temp.path());

        // Create a test file
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "Hello, World!").unwrap();

        let tool = ReadFileTool;
        let mut params = ToolParams::new();
        params.insert("path", "test.txt");

        let result = tool.execute(&params, &context).unwrap();

        assert!(result.success);
        assert_eq!(result.output, "Hello, World!");
        assert!(!result.truncated);
    }

    #[test]
    fn test_read_file_absolute_path() {
        let temp = TempDir::new().unwrap();
        let context = create_test_context(temp.path());

        // Create a test file
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "Absolute path test").unwrap();

        let tool = ReadFileTool;
        let mut params = ToolParams::new();
        params.insert("path", file_path.to_str().unwrap());

        let result = tool.execute(&params, &context).unwrap();

        assert!(result.success);
        assert_eq!(result.output, "Absolute path test");
    }

    #[test]
    fn test_read_file_not_found() {
        let temp = TempDir::new().unwrap();
        let context = create_test_context(temp.path());

        let tool = ReadFileTool;
        let mut params = ToolParams::new();
        params.insert("path", "nonexistent.txt");

        let result = tool.execute(&params, &context);

        assert!(result.is_err());
    }

    #[test]
    fn test_permission_level_within_project() {
        let temp = TempDir::new().unwrap();
        let context = create_test_context(temp.path());

        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "test").unwrap();

        let tool = ReadFileTool;
        let level = tool.permission_level(Some(&file_path), &context);

        assert_eq!(level, PermissionLevel::None);
    }

    #[test]
    fn test_permission_level_outside_project() {
        let temp = TempDir::new().unwrap();
        let context = create_test_context(temp.path());

        let tool = ReadFileTool;
        let level = tool.permission_level(Some(Path::new("/etc/hosts")), &context);

        assert_eq!(level, PermissionLevel::Required);
    }

    #[test]
    fn test_truncation() {
        let temp = TempDir::new().unwrap();
        let context = create_test_context(temp.path());

        // Create a large file
        let file_path = temp.path().join("large.txt");
        let large_content = "x".repeat(MAX_FILE_SIZE + 1000);
        fs::write(&file_path, &large_content).unwrap();

        let tool = ReadFileTool;
        let mut params = ToolParams::new();
        params.insert("path", "large.txt");

        let result = tool.execute(&params, &context).unwrap();

        assert!(result.success);
        assert!(result.truncated);
        assert!(result.output.contains("[Truncated:"));
    }
}
