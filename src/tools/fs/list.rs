//! List files tool for directory exploration

use std::fs;
use std::path::Path;

use crate::tools::{PermissionLevel, Tool, ToolContext, ToolError, ToolParameter, ToolParams, ToolResult};

/// Tool for listing files in a directory
pub struct ListFilesTool;

impl Tool for ListFilesTool {
    fn name(&self) -> &'static str {
        "list_files"
    }

    fn description(&self) -> &'static str {
        "List files and directories in a path. Use absolute paths or paths relative to the project root."
    }

    fn parameters(&self) -> &[ToolParameter] {
        &[ToolParameter {
            name: "path",
            description: "Path to the directory to list",
            required: true,
        }]
    }

    fn permission_level(&self, path: Option<&Path>, ctx: &ToolContext) -> PermissionLevel {
        match path {
            Some(p) => {
                if ctx.is_within_project(p) {
                    PermissionLevel::None
                } else {
                    PermissionLevel::Required
                }
            }
            None => PermissionLevel::Required,
        }
    }

    fn execute(&self, params: &ToolParams, ctx: &ToolContext) -> Result<ToolResult, ToolError> {
        let path_str = params.get("path").ok_or_else(|| {
            ToolError::MissingParameter("path".to_string())
        })?;

        // Resolve the path relative to project root
        let path = ctx.resolve_path(path_str)?;

        // Check if it's a directory
        if !path.is_dir() {
            return Ok(ToolResult::error(format!(
                "Not a directory: {}",
                path.display()
            )));
        }

        // List directory contents
        let mut entries = Vec::new();

        match fs::read_dir(&path) {
            Ok(dir) => {
                for entry in dir.flatten() {
                    let entry_path = entry.path();
                    let name = entry.file_name().to_string_lossy().to_string();

                    // Skip hidden files by default (but include them if listing hidden dirs)
                    if name.starts_with('.') && path_str != "." && !path_str.ends_with("/.") {
                        continue;
                    }

                    let entry_type = if entry_path.is_dir() {
                        "dir"
                    } else if entry_path.is_symlink() {
                        "link"
                    } else {
                        "file"
                    };

                    entries.push((name, entry_type));
                }
            }
            Err(e) => {
                return Ok(ToolResult::error(format!(
                    "Failed to read directory {}: {}",
                    path.display(),
                    e
                )));
            }
        }

        // Sort entries: directories first, then files, alphabetically
        entries.sort_by(|a, b| {
            match (a.1, b.1) {
                ("dir", "dir") | ("file", "file") | ("link", "link") => a.0.cmp(&b.0),
                ("dir", _) => std::cmp::Ordering::Less,
                (_, "dir") => std::cmp::Ordering::Greater,
                _ => a.0.cmp(&b.0),
            }
        });

        // Format output
        let output = if entries.is_empty() {
            format!("Directory {} is empty", path.display())
        } else {
            let formatted: Vec<String> = entries
                .iter()
                .map(|(name, entry_type)| {
                    let suffix = if *entry_type == "dir" { "/" } else { "" };
                    format!("{}{}", name, suffix)
                })
                .collect();

            format!(
                "Contents of {}:\n\n{}",
                path.display(),
                formatted.join("\n")
            )
        };

        Ok(ToolResult::success(output))
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_list_current_dir() {
        let tool = ListFilesTool;
        let ctx = ToolContext {
            project_root: env::current_dir().unwrap(),
            project_id: "test".to_string(),
        };

        let mut params = std::collections::HashMap::new();
        params.insert("path".to_string(), ".".to_string());

        let result = tool.execute(&ToolParams::from(params), &ctx);
        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.success);
        assert!(result.output.contains("Cargo.toml"));
    }
}
