//! Todo tool for task management
//!
//! This tool allows the agent to create and manage a task list.

use std::sync::{Arc, RwLock};

use super::{Tool, ToolError};

/// A single todo item
#[derive(Debug, Clone)]
pub struct TodoItem {
    /// Unique ID
    pub id: usize,
    /// Task description
    pub description: String,
    /// Whether the task is completed
    pub completed: bool,
}

/// Shared todo list state
#[derive(Debug, Default)]
pub struct TodoList {
    items: Vec<TodoItem>,
    next_id: usize,
}

impl TodoList {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            next_id: 1,
        }
    }

    /// Add a new todo item
    pub fn add(&mut self, description: String) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        self.items.push(TodoItem {
            id,
            description,
            completed: false,
        });
        id
    }

    /// Mark a todo as completed
    pub fn complete(&mut self, id: usize) -> bool {
        if let Some(item) = self.items.iter_mut().find(|i| i.id == id) {
            item.completed = true;
            true
        } else {
            false
        }
    }

    /// Remove a todo item
    pub fn remove(&mut self, id: usize) -> bool {
        let len_before = self.items.len();
        self.items.retain(|i| i.id != id);
        self.items.len() < len_before
    }

    /// Get all items
    pub fn items(&self) -> &[TodoItem] {
        &self.items
    }

    /// Clear all completed items
    pub fn clear_completed(&mut self) -> usize {
        let len_before = self.items.len();
        self.items.retain(|i| !i.completed);
        len_before - self.items.len()
    }

    /// Format the list for display
    pub fn format(&self) -> String {
        if self.items.is_empty() {
            return "No tasks in the todo list.".to_string();
        }

        let mut output = String::from("## Todo List\n\n");
        for item in &self.items {
            let status = if item.completed { "[x]" } else { "[ ]" };
            output.push_str(&format!("{} {} - {}\n", status, item.id, item.description));
        }

        let completed = self.items.iter().filter(|i| i.completed).count();
        let total = self.items.len();
        output.push_str(&format!("\n{}/{} tasks completed", completed, total));

        output
    }
}

/// Tool for managing a todo list
pub struct TodoTool {
    list: Arc<RwLock<TodoList>>,
}

impl TodoTool {
    /// Create a new TodoTool with shared state
    pub fn new(list: Arc<RwLock<TodoList>>) -> Self {
        Self { list }
    }

    /// Get a reference to the shared list
    pub fn list(&self) -> Arc<RwLock<TodoList>> {
        self.list.clone()
    }
}

#[async_trait::async_trait]
impl Tool for TodoTool {
    fn name(&self) -> &str {
        "todo"
    }

    fn description(&self) -> &str {
        "Manage a task list. Use this to track work items and their completion status.\n\
         \n\
         Commands:\n\
         - add <description>: Add a new task\n\
         - complete <id>: Mark a task as completed\n\
         - remove <id>: Remove a task\n\
         - list: Show all tasks\n\
         - clear: Remove all completed tasks\n\
         \n\
         Examples:\n\
         - add Implement user authentication\n\
         - complete 1\n\
         - list"
    }

    async fn execute(&self, params: &str) -> Result<String, ToolError> {
        let params = params.trim();
        let (cmd, args) = params.split_once(' ').unwrap_or((params, ""));
        let args = args.trim();

        let mut list = self.list.write().map_err(|_| {
            ToolError::ExecutionError("Failed to acquire todo list lock".to_string())
        })?;

        match cmd.to_lowercase().as_str() {
            "add" => {
                if args.is_empty() {
                    return Err(ToolError::ExecutionError(
                        "Usage: add <task description>".to_string(),
                    ));
                }
                let id = list.add(args.to_string());
                Ok(format!("Added task #{}: {}", id, args))
            }
            "complete" | "done" => {
                let id: usize = args.parse().map_err(|_| {
                    ToolError::ExecutionError(format!("Invalid task ID: {}", args))
                })?;
                if list.complete(id) {
                    Ok(format!("Marked task #{} as completed", id))
                } else {
                    Err(ToolError::ExecutionError(format!("Task #{} not found", id)))
                }
            }
            "remove" | "delete" => {
                let id: usize = args.parse().map_err(|_| {
                    ToolError::ExecutionError(format!("Invalid task ID: {}", args))
                })?;
                if list.remove(id) {
                    Ok(format!("Removed task #{}", id))
                } else {
                    Err(ToolError::ExecutionError(format!("Task #{} not found", id)))
                }
            }
            "list" | "show" | "" => {
                Ok(list.format())
            }
            "clear" => {
                let count = list.clear_completed();
                Ok(format!("Cleared {} completed tasks", count))
            }
            _ => {
                Err(ToolError::ExecutionError(format!(
                    "Unknown command: {}. Use: add, complete, remove, list, or clear",
                    cmd
                )))
            }
        }
    }
}
