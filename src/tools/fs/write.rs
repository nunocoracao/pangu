//! write_file tool implementation - create or overwrite files

use std::fs;
use std::path::Path;

use crate::tools::{
    PermissionLevel, Tool, ToolContext, ToolError, ToolParameter, ToolParams, ToolResult,
};

/// Maximum content size to write (1MB)
const MAX_CONTENT_SIZE: usize = 1_000_000;

/// Tool for writing file contents
pub struct WriteFileTool;

impl Tool for WriteFileTool {
    fn name(&self) -> &'static str {
        "write_file"
    }

    fn description(&self) -> &'static str {
        "Create a new file or overwrite an existing file with the specified content."
    }

    fn parameters(&self) -> &[ToolParameter] {
        &[
            ToolParameter {
                name: "path",
                description: "Path to the file to write",
                required: true,
            },
            ToolParameter {
                name: "content",
                description: "Content to write to the file",
                required: true,
            },
        ]
    }

    fn permission_level(&self, path: Option<&Path>, context: &ToolContext) -> PermissionLevel {
        match path {
            Some(p) => {
                // Check if file exists (overwrite requires permission)
                if p.exists() {
                    return PermissionLevel::Required;
                }
                // Check if within project
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

        // Get content parameter
        let content = params
            .get("content")
            .ok_or_else(|| ToolError::MissingParameter("content".to_string()))?;

        // Check content size
        if content.len() > MAX_CONTENT_SIZE {
            return Err(ToolError::InvalidParameter(
                "content".to_string(),
                format!("Content too large: {} bytes (max {})", content.len(), MAX_CONTENT_SIZE),
            ));
        }

        // Resolve the path
        let path = if Path::new(path_str).is_absolute() {
            Path::new(path_str).to_path_buf()
        } else {
            context.project_root.join(path_str)
        };

        // Security check: ensure path is within project for new files
        // For relative paths joined with project root, we're always within project
        // For absolute paths, check they start with project root
        let is_relative_path = !Path::new(path_str).is_absolute();

        if !is_relative_path {
            // Absolute path - check it's within project
            let canonical_root = context.project_root.canonicalize().map_err(|e| {
                ToolError::PathError(format!("Cannot resolve project root: {}", e))
            })?;

            // For new files with absolute paths, check if target would be in project
            if !path.starts_with(&canonical_root) {
                return Err(ToolError::PermissionDenied(format!(
                    "Cannot create files outside project: {}",
                    path_str
                )));
            }
        }
        // Relative paths are always within project since we join with project_root

        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(|e| {
                    ToolError::IoError(format!("Failed to create directories: {}", e))
                })?;
            }
        }

        // Check if file exists (for messaging)
        let existed = path.exists();

        // Write the file
        fs::write(&path, content).map_err(|e| {
            ToolError::IoError(format!("Failed to write '{}': {}", path_str, e))
        })?;

        let action = if existed { "Updated" } else { "Created" };
        Ok(ToolResult::success(format!(
            "{} file: {} ({} bytes)",
            action,
            path_str,
            content.len()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_context(dir: &Path) -> ToolContext {
        ToolContext::new(dir.to_path_buf(), "test123".to_string())
    }

    #[test]
    fn test_write_new_file() {
        let temp = TempDir::new().unwrap();
        let context = create_test_context(temp.path());

        let tool = WriteFileTool;
        let mut params = ToolParams::new();
        params.insert("path", "test.txt");
        params.insert("content", "Hello, World!");

        let result = tool.execute(&params, &context).unwrap();

        assert!(result.success);
        assert!(result.output.contains("Created"));

        // Verify file contents
        let written = fs::read_to_string(temp.path().join("test.txt")).unwrap();
        assert_eq!(written, "Hello, World!");
    }

    #[test]
    fn test_write_creates_directories() {
        let temp = TempDir::new().unwrap();
        let context = create_test_context(temp.path());

        let tool = WriteFileTool;
        let mut params = ToolParams::new();
        params.insert("path", "nested/dir/test.txt");
        params.insert("content", "Nested content");

        let result = tool.execute(&params, &context).unwrap();

        assert!(result.success);
        assert!(temp.path().join("nested/dir/test.txt").exists());
    }

    #[test]
    fn test_write_overwrite_existing() {
        let temp = TempDir::new().unwrap();
        let context = create_test_context(temp.path());

        // Create initial file
        let file_path = temp.path().join("existing.txt");
        fs::write(&file_path, "Original content").unwrap();

        let tool = WriteFileTool;
        let mut params = ToolParams::new();
        params.insert("path", "existing.txt");
        params.insert("content", "New content");

        let result = tool.execute(&params, &context).unwrap();

        assert!(result.success);
        assert!(result.output.contains("Updated"));

        let written = fs::read_to_string(file_path).unwrap();
        assert_eq!(written, "New content");
    }

    #[test]
    fn test_permission_new_file_in_project() {
        let temp = TempDir::new().unwrap();
        let context = create_test_context(temp.path());

        let new_file = temp.path().join("new.txt");
        let tool = WriteFileTool;
        let level = tool.permission_level(Some(&new_file), &context);

        assert_eq!(level, PermissionLevel::None);
    }

    #[test]
    fn test_permission_existing_file() {
        let temp = TempDir::new().unwrap();
        let context = create_test_context(temp.path());

        let existing = temp.path().join("existing.txt");
        fs::write(&existing, "test").unwrap();

        let tool = WriteFileTool;
        let level = tool.permission_level(Some(&existing), &context);

        assert_eq!(level, PermissionLevel::Required);
    }

    #[test]
    fn test_permission_outside_project() {
        let temp = TempDir::new().unwrap();
        let context = create_test_context(temp.path());

        let tool = WriteFileTool;
        let level = tool.permission_level(Some(Path::new("/tmp/outside.txt")), &context);

        assert_eq!(level, PermissionLevel::Required);
    }
}
