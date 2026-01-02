//! Filesystem tool for reading, writing, listing, and searching files

use std::fs;
use std::path::{Path, PathBuf};

use super::{Tool, ToolError};

/// Maximum file size to read (1MB)
const MAX_READ_SIZE: u64 = 1024 * 1024;

/// Maximum content length to return (truncate if larger)
const MAX_CONTENT_LENGTH: usize = 32000;

/// Filesystem tool for local file operations
pub struct FilesystemTool;

impl FilesystemTool {
    pub fn new() -> Self {
        Self
    }

    /// Parse the command from params
    fn parse_command(params: &str) -> Result<(String, String), ToolError> {
        let params = params.trim();

        // Split on first whitespace to get command
        let (cmd, rest) = if let Some(idx) = params.find(char::is_whitespace) {
            (params[..idx].to_string(), params[idx..].trim().to_string())
        } else {
            (params.to_string(), String::new())
        };

        Ok((cmd.to_lowercase(), rest))
    }

    /// Resolve and validate a path
    fn resolve_path(path_str: &str) -> Result<PathBuf, ToolError> {
        let path_str = path_str.trim();

        if path_str.is_empty() {
            return Err(ToolError::ExecutionError("Path cannot be empty".to_string()));
        }

        // Expand ~ to home directory
        let expanded = if path_str.starts_with("~/") {
            if let Some(home) = dirs::home_dir() {
                home.join(&path_str[2..])
            } else {
                PathBuf::from(path_str)
            }
        } else {
            PathBuf::from(path_str)
        };

        // Canonicalize if the path exists, otherwise just use the expanded path
        if expanded.exists() {
            expanded.canonicalize().map_err(|e| {
                ToolError::ExecutionError(format!("Failed to resolve path: {}", e))
            })
        } else {
            Ok(expanded)
        }
    }

    /// Read a file's contents
    fn read_file(path: &Path) -> Result<String, ToolError> {
        // Check if path exists
        if !path.exists() {
            return Err(ToolError::ExecutionError(format!(
                "File not found: {}",
                path.display()
            )));
        }

        // Check if it's a file
        if !path.is_file() {
            return Err(ToolError::ExecutionError(format!(
                "Not a file: {}",
                path.display()
            )));
        }

        // Check file size
        let metadata = fs::metadata(path).map_err(|e| {
            ToolError::ExecutionError(format!("Failed to read metadata: {}", e))
        })?;

        if metadata.len() > MAX_READ_SIZE {
            return Err(ToolError::ExecutionError(format!(
                "File too large: {} bytes (max {} bytes)",
                metadata.len(),
                MAX_READ_SIZE
            )));
        }

        // Read file
        let content = fs::read_to_string(path).map_err(|e| {
            // Try reading as binary if text fails
            if e.kind() == std::io::ErrorKind::InvalidData {
                return ToolError::ExecutionError(
                    "File appears to be binary, cannot read as text".to_string(),
                );
            }
            ToolError::ExecutionError(format!("Failed to read file: {}", e))
        })?;

        // Truncate if too long
        if content.len() > MAX_CONTENT_LENGTH {
            let truncated = &content[..MAX_CONTENT_LENGTH];
            Ok(format!(
                "{}\n\n[... truncated, showing first {} of {} bytes ...]",
                truncated,
                MAX_CONTENT_LENGTH,
                content.len()
            ))
        } else {
            Ok(content)
        }
    }

    /// Write content to a file
    fn write_file(path: &Path, content: &str) -> Result<String, ToolError> {
        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(|e| {
                    ToolError::ExecutionError(format!("Failed to create directories: {}", e))
                })?;
            }
        }

        // Write file
        fs::write(path, content).map_err(|e| {
            ToolError::ExecutionError(format!("Failed to write file: {}", e))
        })?;

        Ok(format!(
            "Successfully wrote {} bytes to {}",
            content.len(),
            path.display()
        ))
    }

    /// List directory contents
    fn list_dir(path: &Path) -> Result<String, ToolError> {
        if !path.exists() {
            return Err(ToolError::ExecutionError(format!(
                "Directory not found: {}",
                path.display()
            )));
        }

        if !path.is_dir() {
            return Err(ToolError::ExecutionError(format!(
                "Not a directory: {}",
                path.display()
            )));
        }

        let mut entries: Vec<String> = Vec::new();
        let mut dirs: Vec<String> = Vec::new();
        let mut files: Vec<String> = Vec::new();

        let read_dir = fs::read_dir(path).map_err(|e| {
            ToolError::ExecutionError(format!("Failed to read directory: {}", e))
        })?;

        for entry in read_dir {
            let entry = entry.map_err(|e| {
                ToolError::ExecutionError(format!("Failed to read entry: {}", e))
            })?;

            let name = entry.file_name().to_string_lossy().to_string();
            let file_type = entry.file_type().map_err(|e| {
                ToolError::ExecutionError(format!("Failed to get file type: {}", e))
            })?;

            if file_type.is_dir() {
                dirs.push(format!("{}/", name));
            } else if file_type.is_file() {
                // Get file size
                if let Ok(meta) = entry.metadata() {
                    let size = meta.len();
                    let size_str = if size >= 1024 * 1024 {
                        format!("{:.1}M", size as f64 / (1024.0 * 1024.0))
                    } else if size >= 1024 {
                        format!("{:.1}K", size as f64 / 1024.0)
                    } else {
                        format!("{}B", size)
                    };
                    files.push(format!("{} ({})", name, size_str));
                } else {
                    files.push(name);
                }
            } else if file_type.is_symlink() {
                files.push(format!("{} -> (symlink)", name));
            }
        }

        // Sort entries
        dirs.sort();
        files.sort();

        // Format output
        entries.push(format!("Directory: {}\n", path.display()));

        if !dirs.is_empty() {
            entries.push("Directories:".to_string());
            for dir in &dirs {
                entries.push(format!("  {}", dir));
            }
            entries.push(String::new());
        }

        if !files.is_empty() {
            entries.push("Files:".to_string());
            for file in &files {
                entries.push(format!("  {}", file));
            }
        }

        if dirs.is_empty() && files.is_empty() {
            entries.push("(empty directory)".to_string());
        }

        Ok(entries.join("\n"))
    }

    /// Search for files matching a pattern
    fn search_files(pattern: &str, search_path: &Path) -> Result<String, ToolError> {
        if !search_path.exists() {
            return Err(ToolError::ExecutionError(format!(
                "Search path not found: {}",
                search_path.display()
            )));
        }

        if !search_path.is_dir() {
            return Err(ToolError::ExecutionError(format!(
                "Search path is not a directory: {}",
                search_path.display()
            )));
        }

        let pattern_lower = pattern.to_lowercase();
        let mut matches: Vec<String> = Vec::new();
        let mut searched = 0;
        const MAX_RESULTS: usize = 50;
        const MAX_DEPTH: usize = 10;

        fn search_recursive(
            dir: &Path,
            pattern: &str,
            matches: &mut Vec<String>,
            searched: &mut usize,
            depth: usize,
            max_depth: usize,
            max_results: usize,
        ) -> Result<(), ToolError> {
            if depth > max_depth || matches.len() >= max_results {
                return Ok(());
            }

            let entries = match fs::read_dir(dir) {
                Ok(e) => e,
                Err(_) => return Ok(()), // Skip directories we can't read
            };

            for entry in entries {
                if matches.len() >= max_results {
                    break;
                }

                let entry = match entry {
                    Ok(e) => e,
                    Err(_) => continue,
                };

                *searched += 1;
                let name = entry.file_name().to_string_lossy().to_string();
                let name_lower = name.to_lowercase();

                // Check if name matches pattern (simple substring match)
                if name_lower.contains(pattern) {
                    let path = entry.path();
                    let is_dir = path.is_dir();
                    matches.push(if is_dir {
                        format!("{}/", path.display())
                    } else {
                        path.display().to_string()
                    });
                }

                // Recurse into directories
                if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    // Skip hidden directories and common large directories
                    if !name.starts_with('.')
                        && name != "node_modules"
                        && name != "target"
                        && name != "build"
                        && name != "dist"
                    {
                        search_recursive(
                            &entry.path(),
                            pattern,
                            matches,
                            searched,
                            depth + 1,
                            max_depth,
                            max_results,
                        )?;
                    }
                }
            }

            Ok(())
        }

        search_recursive(
            search_path,
            &pattern_lower,
            &mut matches,
            &mut searched,
            0,
            MAX_DEPTH,
            MAX_RESULTS,
        )?;

        if matches.is_empty() {
            Ok(format!(
                "No files matching '{}' found in {} (searched {} entries)",
                pattern,
                search_path.display(),
                searched
            ))
        } else {
            let mut result = format!(
                "Found {} matches for '{}' in {}:\n\n",
                matches.len(),
                pattern,
                search_path.display()
            );
            for m in &matches {
                result.push_str(&format!("  {}\n", m));
            }
            if matches.len() >= MAX_RESULTS {
                result.push_str(&format!("\n(showing first {} results)", MAX_RESULTS));
            }
            Ok(result)
        }
    }

    /// Get the permission key for a filesystem operation
    pub fn permission_key(command: &str, path: &Path) -> String {
        format!("fs:{}:{}", command, path.display())
    }
}

impl Default for FilesystemTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Tool for FilesystemTool {
    fn name(&self) -> &str {
        "fs"
    }

    fn description(&self) -> &str {
        "Filesystem operations: read, write, list, and search files and directories"
    }

    async fn execute(&self, params: &str) -> Result<String, ToolError> {
        let (cmd, args) = Self::parse_command(params)?;

        match cmd.as_str() {
            "read" => {
                let path = Self::resolve_path(&args)?;
                Self::read_file(&path)
            }
            "write" => {
                // Parse: write <path> <content>
                // Content can contain spaces, so we need to be careful
                // Format: write /path/to/file.txt
                // Content follows on next lines or after a separator

                // Find the path (first argument) and content (rest)
                let (path_str, content) = if let Some(idx) = args.find('\n') {
                    // If there's a newline, path is before it, content after
                    (args[..idx].trim().to_string(), args[idx + 1..].to_string())
                } else if args.contains("<<<") {
                    // Support <<< as content delimiter
                    let parts: Vec<&str> = args.splitn(2, "<<<").collect();
                    if parts.len() == 2 {
                        (parts[0].trim().to_string(), parts[1].trim().to_string())
                    } else {
                        return Err(ToolError::ExecutionError(
                            "Write command requires: write <path>\\n<content> or write <path> <<< <content>".to_string()
                        ));
                    }
                } else {
                    return Err(ToolError::ExecutionError(
                        "Write command requires content. Use: write <path>\\n<content>".to_string()
                    ));
                };

                let path = Self::resolve_path(&path_str)?;
                Self::write_file(&path, &content)
            }
            "list" | "ls" => {
                let path = if args.is_empty() {
                    std::env::current_dir().map_err(|e| {
                        ToolError::ExecutionError(format!("Failed to get current directory: {}", e))
                    })?
                } else {
                    Self::resolve_path(&args)?
                };
                Self::list_dir(&path)
            }
            "search" | "find" => {
                // Parse: search <pattern> [path]
                let parts: Vec<&str> = args.splitn(2, char::is_whitespace).collect();

                if parts.is_empty() || parts[0].is_empty() {
                    return Err(ToolError::ExecutionError(
                        "Search command requires a pattern: search <pattern> [path]".to_string()
                    ));
                }

                let pattern = parts[0];
                let search_path = if parts.len() > 1 && !parts[1].trim().is_empty() {
                    Self::resolve_path(parts[1].trim())?
                } else {
                    std::env::current_dir().map_err(|e| {
                        ToolError::ExecutionError(format!("Failed to get current directory: {}", e))
                    })?
                };

                Self::search_files(pattern, &search_path)
            }
            "mkdir" => {
                let path = Self::resolve_path(&args)?;
                fs::create_dir_all(&path).map_err(|e| {
                    ToolError::ExecutionError(format!("Failed to create directory: {}", e))
                })?;
                Ok(format!("Created directory: {}", path.display()))
            }
            "rm" | "delete" => {
                let path = Self::resolve_path(&args)?;

                if !path.exists() {
                    return Err(ToolError::ExecutionError(format!(
                        "Path not found: {}", path.display()
                    )));
                }

                if path.is_dir() {
                    // Only allow removing empty directories for safety
                    fs::remove_dir(&path).map_err(|e| {
                        ToolError::ExecutionError(format!(
                            "Failed to remove directory (must be empty): {}", e
                        ))
                    })?;
                    Ok(format!("Removed directory: {}", path.display()))
                } else {
                    fs::remove_file(&path).map_err(|e| {
                        ToolError::ExecutionError(format!("Failed to remove file: {}", e))
                    })?;
                    Ok(format!("Removed file: {}", path.display()))
                }
            }
            _ => Err(ToolError::ExecutionError(format!(
                "Unknown fs command: '{}'. Available: read, write, list, search, mkdir, rm",
                cmd
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_list_directory() {
        let tool = FilesystemTool::new();
        let dir = tempdir().unwrap();

        // Create some test files
        fs::write(dir.path().join("test.txt"), "hello").unwrap();
        fs::create_dir(dir.path().join("subdir")).unwrap();

        let result = tool.execute(&format!("list {}", dir.path().display())).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("test.txt"));
        assert!(output.contains("subdir/"));
    }

    #[tokio::test]
    async fn test_read_file() {
        let tool = FilesystemTool::new();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");

        fs::write(&file_path, "Hello, world!").unwrap();

        let result = tool.execute(&format!("read {}", file_path.display())).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Hello, world!");
    }

    #[tokio::test]
    async fn test_write_file() {
        let tool = FilesystemTool::new();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("output.txt");

        let result = tool.execute(&format!("write {}\nTest content", file_path.display())).await;
        assert!(result.is_ok());

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "Test content");
    }

    #[tokio::test]
    async fn test_search_files() {
        let tool = FilesystemTool::new();
        let dir = tempdir().unwrap();

        fs::write(dir.path().join("hello.txt"), "").unwrap();
        fs::write(dir.path().join("world.txt"), "").unwrap();
        fs::create_dir(dir.path().join("subdir")).unwrap();
        fs::write(dir.path().join("subdir").join("hello2.txt"), "").unwrap();

        let result = tool.execute(&format!("search hello {}", dir.path().display())).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("hello.txt"));
        assert!(output.contains("hello2.txt"));
        assert!(!output.contains("world.txt"));
    }
}
