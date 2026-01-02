//! Think tool for reasoning and reflection
//!
//! This tool provides a dedicated space for the model to reason through
//! complex problems step by step before providing answers.

use super::{Tool, ToolError};

/// Tool for structured thinking and reasoning
pub struct ThinkTool;

impl ThinkTool {
    /// Create a new ThinkTool
    pub fn new() -> Self {
        Self
    }
}

impl Default for ThinkTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Tool for ThinkTool {
    fn name(&self) -> &str {
        "think"
    }

    fn description(&self) -> &str {
        "Use this tool to think through complex problems step by step.\n\
         This is a scratchpad for reasoning - use it when you need to:\n\
         - Break down a complex problem\n\
         - Plan a multi-step solution\n\
         - Consider different approaches\n\
         - Verify your reasoning\n\
         Parameters: Your thoughts and reasoning process"
    }

    async fn execute(&self, params: &str) -> Result<String, ToolError> {
        let thought = params.trim();

        if thought.is_empty() {
            return Ok("[Empty thought - consider what you need to reason about]".to_string());
        }

        // The think tool just acknowledges the thought
        // This gives the model a dedicated space to reason
        Ok(format!(
            "[Thought recorded]\n\nYou reasoned through:\n{}\n\n[Continue with your response based on this reasoning]",
            thought
        ))
    }
}
