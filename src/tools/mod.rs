//! Tool system for Pangu agent
//!
//! Tools allow the agent to interact with external systems like
//! fetching web content, reading files, etc.

mod fetch;
mod filesystem;
mod parser;
mod search;
mod think;
mod todo;

pub use fetch::FetchTool;
pub use filesystem::FilesystemTool;
pub use parser::{
    parse_thinking_blocks, parse_tool_calls, remove_thinking_blocks, remove_tool_calls,
    has_partial_thinking, ThinkingBlock, ToolCall,
};
pub use search::SearchTool;
pub use todo::{TodoItem, TodoList, TodoTool};

use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

/// Errors that can occur during tool execution
#[derive(Debug, Error)]
pub enum ToolError {
    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Timeout: request took too long")]
    Timeout,

    #[error("Invalid URL: {0}")]
    InvalidUrl(String),

    #[error("Response too large")]
    ResponseTooLarge,

    #[error("Tool not found: {0}")]
    NotFound(String),

    #[error("Execution error: {0}")]
    ExecutionError(String),
}

/// Trait for implementing tools
#[async_trait::async_trait]
pub trait Tool: Send + Sync {
    /// Get the tool name (used for matching tool calls)
    fn name(&self) -> &str;

    /// Get a description of what the tool does
    fn description(&self) -> &str;

    /// Execute the tool with the given parameters
    async fn execute(&self, params: &str) -> Result<String, ToolError>;
}

/// Registry of available tools
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
    /// Names of registered tools (for UI display)
    tool_names: Vec<String>,
}

impl ToolRegistry {
    /// Create a new tool registry with default tools
    pub fn new() -> Self {
        Self::with_todo_list(Arc::new(std::sync::RwLock::new(TodoList::new())))
    }

    /// Create a new tool registry with a shared todo list
    pub fn with_todo_list(todo_list: Arc<std::sync::RwLock<TodoList>>) -> Self {
        let mut registry = Self {
            tools: HashMap::new(),
            tool_names: Vec::new(),
        };

        // Register all tools
        // Note: ThinkTool is no longer registered as thinking is now automatic
        // via <thinking> blocks in the system prompt
        registry.register(Arc::new(FetchTool::new()));
        registry.register(Arc::new(SearchTool::new()));
        registry.register(Arc::new(FilesystemTool::new()));
        registry.register(Arc::new(TodoTool::new(todo_list)));

        registry
    }

    /// Get the list of tool names
    pub fn tool_names(&self) -> &[String] {
        &self.tool_names
    }

    /// Register a tool
    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        let name = tool.name().to_string();
        self.tool_names.push(name.clone());
        self.tools.insert(name, tool);
    }

    /// Get a tool by name
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    /// Execute a tool call
    pub async fn execute(&self, tool_call: &ToolCall) -> Result<String, ToolError> {
        let tool = self
            .get(&tool_call.name)
            .ok_or_else(|| ToolError::NotFound(tool_call.name.clone()))?;

        tool.execute(&tool_call.params).await
    }

    /// Get descriptions of all tools for the system prompt
    pub fn tool_descriptions(&self) -> String {
        let mut desc = String::from("## Available Tools\n\n");
        desc.push_str("You have access to the following tools:\n\n");

        for tool in self.tools.values() {
            desc.push_str(&format!("### {}\n", tool.name()));
            desc.push_str(&format!("{}\n\n", tool.description()));
        }

        desc.push_str("To use a tool, output:\n");
        desc.push_str("```\n<tool_use>\n<name>tool_name</name>\n<params>parameters</params>\n</tool_use>\n```\n\n");
        desc.push_str("Wait for the tool result before continuing your response.\n");

        desc
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
