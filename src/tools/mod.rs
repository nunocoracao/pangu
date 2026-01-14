//! Tool system for agent capabilities
//!
//! Provides a trait-based tool system with:
//! - Tool trait for implementing new tools
//! - ToolRegistry for managing available tools
//! - ToolContext for execution environment
//! - Permission levels for security

pub mod fs;
pub mod parser;
pub mod permission;
pub mod shell;
pub mod web;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Permission level required for a tool operation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionLevel {
    /// Always allowed (e.g., read within project)
    None,
    /// Requires user permission (e.g., read outside project, all writes)
    Required,
}

/// Tool parameter definition
#[derive(Debug, Clone)]
pub struct ToolParameter {
    pub name: &'static str,
    pub description: &'static str,
    pub required: bool,
}

/// Parameters passed to tool execution
#[derive(Debug, Clone, Default)]
pub struct ToolParams {
    params: HashMap<String, String>,
}

impl ToolParams {
    pub fn new() -> Self {
        Self {
            params: HashMap::new(),
        }
    }

    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.params.insert(key.into(), value.into());
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.params.get(key).map(|s| s.as_str())
    }

    pub fn contains(&self, key: &str) -> bool {
        self.params.contains_key(key)
    }

    /// Iterate over all parameters
    pub fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.params.iter()
    }
}

impl From<HashMap<String, String>> for ToolParams {
    fn from(params: HashMap<String, String>) -> Self {
        Self { params }
    }
}

/// Result of tool execution
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub success: bool,
    pub output: String,
    pub truncated: bool,
}

impl ToolResult {
    pub fn success(output: impl Into<String>) -> Self {
        Self {
            success: true,
            output: output.into(),
            truncated: false,
        }
    }

    pub fn success_truncated(output: impl Into<String>) -> Self {
        Self {
            success: true,
            output: output.into(),
            truncated: true,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            output: message.into(),
            truncated: false,
        }
    }
}

/// Errors that can occur during tool execution
#[derive(Debug, Error)]
pub enum ToolError {
    #[error("Missing required parameter: {0}")]
    MissingParameter(String),

    #[error("Invalid parameter value for {0}: {1}")]
    InvalidParameter(String, String),

    #[error("Path error: {0}")]
    PathError(String),

    #[error("IO error: {0}")]
    IoError(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Tool not found: {0}")]
    NotFound(String),

    #[error("Execution error: {0}")]
    ExecutionError(String),
}

impl From<std::io::Error> for ToolError {
    fn from(e: std::io::Error) -> Self {
        ToolError::IoError(e.to_string())
    }
}

/// Context for tool execution
#[derive(Debug, Clone)]
pub struct ToolContext {
    /// Project root directory (working directory)
    pub project_root: PathBuf,
    /// Project ID for permission storage
    pub project_id: String,
}

impl ToolContext {
    pub fn new(project_root: PathBuf, project_id: String) -> Self {
        Self {
            project_root,
            project_id,
        }
    }

    /// Check if a path is within the project root
    pub fn is_within_project(&self, path: &Path) -> bool {
        // Canonicalize both paths to handle symlinks and relative paths
        let canonical_root = match self.project_root.canonicalize() {
            Ok(p) => p,
            Err(_) => return false,
        };

        let canonical_path = match path.canonicalize() {
            Ok(p) => p,
            Err(_) => {
                // Path doesn't exist yet, try to resolve parent
                if let Some(parent) = path.parent() {
                    match parent.canonicalize() {
                        Ok(p) => p.join(path.file_name().unwrap_or_default()),
                        Err(_) => return false,
                    }
                } else {
                    return false;
                }
            }
        };

        canonical_path.starts_with(&canonical_root)
    }

    /// Resolve a path relative to project root
    pub fn resolve_path(&self, path_str: &str) -> Result<PathBuf, ToolError> {
        let path = Path::new(path_str);

        let resolved = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.project_root.join(path)
        };

        // Canonicalize to prevent path traversal attacks
        resolved
            .canonicalize()
            .map_err(|e| ToolError::PathError(format!("Cannot resolve path '{}': {}", path_str, e)))
    }
}

/// Trait for implementing tools
pub trait Tool: Send + Sync {
    /// Tool name for invocation
    fn name(&self) -> &'static str;

    /// Human-readable description for LLM context
    fn description(&self) -> &'static str;

    /// Parameter definitions
    fn parameters(&self) -> &[ToolParameter];

    /// Determine permission level for this operation
    fn permission_level(&self, path: Option<&Path>, context: &ToolContext) -> PermissionLevel;

    /// Execute the tool with given parameters
    fn execute(&self, params: &ToolParams, context: &ToolContext) -> Result<ToolResult, ToolError>;

    /// Generate tool documentation for system prompt
    fn generate_docs(&self) -> String {
        let mut docs = format!("### {}\n{}\n", self.name(), self.description());

        let params = self.parameters();
        if !params.is_empty() {
            docs.push_str("Parameters:\n");
            for param in params {
                let required = if param.required { " (required)" } else { "" };
                docs.push_str(&format!("- `{}`: {}{}\n", param.name, param.description, required));
            }
        }

        docs
    }
}

/// Registry of available tools
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRegistry {
    /// Create a new registry with default tools
    pub fn new() -> Self {
        let mut registry = Self {
            tools: HashMap::new(),
        };

        // Register default tools
        // File operations
        registry.register(Box::new(fs::ReadFileTool));
        registry.register(Box::new(fs::ListFilesTool));
        registry.register(Box::new(fs::GrepTool));
        registry.register(Box::new(fs::WriteFileTool));
        registry.register(Box::new(fs::EditFileTool));
        // Shell operations
        registry.register(Box::new(shell::BashTool));
        // Web operations
        registry.register(Box::new(web::FetchTool));
        registry.register(Box::new(web::WebSearchTool));

        registry
    }

    /// Register a new tool
    pub fn register(&mut self, tool: Box<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    /// Get a tool by name
    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|t| t.as_ref())
    }

    /// Get all registered tool names
    pub fn tool_names(&self) -> Vec<&str> {
        self.tools.keys().map(|s| s.as_str()).collect()
    }

    /// Generate documentation for all tools (for system prompt)
    pub fn generate_all_docs(&self) -> String {
        let mut docs = String::from("## Available Tools\n\n");
        docs.push_str("You can use tools by including XML tags in your response:\n\n");

        for tool in self.tools.values() {
            docs.push_str(&tool.generate_docs());
            docs.push_str(&format!(
                "\nExample:\n```xml\n<tool_call>\n<name>{}</name>\n",
                tool.name()
            ));
            for param in tool.parameters() {
                docs.push_str(&format!("<{}>{}</{}>\n", param.name, "...", param.name));
            }
            docs.push_str("</tool_call>\n```\n\n");
        }

        docs
    }
}

/// Parsed tool call from model output
#[derive(Debug, Clone)]
pub struct ToolCall {
    pub name: String,
    pub params: ToolParams,
    pub raw: String,
}

impl ToolCall {
    pub fn new(name: impl Into<String>, params: ToolParams, raw: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            params,
            raw: raw.into(),
        }
    }
}
