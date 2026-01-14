//! bash tool implementation - execute shell commands

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::tools::{
    PermissionLevel, Tool, ToolContext, ToolError, ToolParameter, ToolParams, ToolResult,
};

/// Default timeout in seconds
const DEFAULT_TIMEOUT: u64 = 30;

/// Maximum timeout in seconds
const MAX_TIMEOUT: u64 = 300;

/// Maximum output size in bytes
const MAX_OUTPUT_SIZE: usize = 100_000;

/// Dangerous command patterns that should be blocked
const BLOCKED_PATTERNS: &[&str] = &[
    "rm -rf /",
    "rm -rf /*",
    "rm -rf ~",
    "rm -rf ~/*",
    "sudo rm",
    "sudo dd",
    "> /dev/",
    "mkfs",
    ":(){:|:&};:",  // Fork bomb
    "chmod -R 777 /",
    "chown -R",
];

/// Tool for executing shell commands
pub struct BashTool;

impl Tool for BashTool {
    fn name(&self) -> &'static str {
        "bash"
    }

    fn description(&self) -> &'static str {
        "Execute a shell command. Use for building, testing, git operations, and other shell tasks."
    }

    fn parameters(&self) -> &[ToolParameter] {
        &[
            ToolParameter {
                name: "command",
                description: "The command to execute",
                required: true,
            },
            ToolParameter {
                name: "timeout",
                description: "Maximum execution time in seconds (default: 30, max: 300)",
                required: false,
            },
        ]
    }

    fn permission_level(&self, _path: Option<&Path>, _context: &ToolContext) -> PermissionLevel {
        // All command execution requires permission
        PermissionLevel::Required
    }

    fn execute(&self, params: &ToolParams, context: &ToolContext) -> Result<ToolResult, ToolError> {
        // Get command parameter
        let command = params
            .get("command")
            .ok_or_else(|| ToolError::MissingParameter("command".to_string()))?;

        // Check for blocked patterns
        if is_dangerous_command(command) {
            return Err(ToolError::PermissionDenied(
                "Command blocked for safety reasons".to_string(),
            ));
        }

        // Get timeout
        let timeout_secs = params
            .get("timeout")
            .and_then(|t| t.parse::<u64>().ok())
            .unwrap_or(DEFAULT_TIMEOUT)
            .min(MAX_TIMEOUT);

        // Execute the command
        let output = execute_command(command, &context.project_root, timeout_secs)?;

        // Format output
        let mut result = String::new();

        if !output.stdout.is_empty() {
            result.push_str(&output.stdout);
        }

        if !output.stderr.is_empty() {
            if !result.is_empty() {
                result.push_str("\n");
            }
            result.push_str("[stderr]\n");
            result.push_str(&output.stderr);
        }

        if result.is_empty() {
            result = "(no output)".to_string();
        }

        // Add exit code
        let exit_info = if output.success {
            format!("\n[exit code: 0]")
        } else {
            format!("\n[exit code: {}]", output.exit_code)
        };
        result.push_str(&exit_info);

        // Truncate if necessary
        if result.len() > MAX_OUTPUT_SIZE {
            let truncated = &result[..MAX_OUTPUT_SIZE];
            Ok(ToolResult::success_truncated(format!(
                "{}\n\n[Truncated: showing first {} of {} bytes]",
                truncated,
                MAX_OUTPUT_SIZE,
                result.len()
            )))
        } else if output.success {
            Ok(ToolResult::success(result))
        } else {
            // Command failed but we still return success=true for the tool
            // (the command ran, it just had a non-zero exit)
            Ok(ToolResult::success(result))
        }
    }
}

/// Output from command execution
struct CommandOutput {
    stdout: String,
    stderr: String,
    exit_code: i32,
    success: bool,
}

/// Execute a command with timeout
fn execute_command(
    command: &str,
    working_dir: &Path,
    timeout_secs: u64,
) -> Result<CommandOutput, ToolError> {
    let shell = if cfg!(windows) { "cmd" } else { "sh" };
    let shell_arg = if cfg!(windows) { "/C" } else { "-c" };

    let mut child = Command::new(shell)
        .arg(shell_arg)
        .arg(command)
        .current_dir(working_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| ToolError::ExecutionError(format!("Failed to spawn command: {}", e)))?;

    // Wait with timeout
    let timeout = Duration::from_secs(timeout_secs);

    // Simple timeout implementation using try_wait in a loop
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                // Process finished
                let output = child.wait_with_output().map_err(|e| {
                    ToolError::ExecutionError(format!("Failed to read output: {}", e))
                })?;

                return Ok(CommandOutput {
                    stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                    stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                    exit_code: status.code().unwrap_or(-1),
                    success: status.success(),
                });
            }
            Ok(None) => {
                // Still running, check timeout
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    return Err(ToolError::ExecutionError(format!(
                        "Command timed out after {} seconds",
                        timeout_secs
                    )));
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => {
                return Err(ToolError::ExecutionError(format!(
                    "Failed to check process status: {}",
                    e
                )));
            }
        }
    }
}

/// Check if a command matches any dangerous patterns
fn is_dangerous_command(command: &str) -> bool {
    let normalized = command.to_lowercase();

    for pattern in BLOCKED_PATTERNS {
        if normalized.contains(&pattern.to_lowercase()) {
            return true;
        }
    }

    // Check for sudo at the start
    if normalized.trim_start().starts_with("sudo ") {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_context(dir: &Path) -> ToolContext {
        ToolContext::new(dir.to_path_buf(), "test123".to_string())
    }

    #[test]
    fn test_simple_command() {
        let temp = TempDir::new().unwrap();
        let context = create_test_context(temp.path());

        let tool = BashTool;
        let mut params = ToolParams::new();
        params.insert("command", "echo hello");

        let result = tool.execute(&params, &context).unwrap();

        assert!(result.success);
        assert!(result.output.contains("hello"));
        assert!(result.output.contains("[exit code: 0]"));
    }

    #[test]
    fn test_command_with_working_dir() {
        let temp = TempDir::new().unwrap();
        let context = create_test_context(temp.path());

        // Create a file in the temp dir
        std::fs::write(temp.path().join("test.txt"), "content").unwrap();

        let tool = BashTool;
        let mut params = ToolParams::new();
        params.insert("command", "ls");

        let result = tool.execute(&params, &context).unwrap();

        assert!(result.success);
        assert!(result.output.contains("test.txt"));
    }

    #[test]
    fn test_command_failure() {
        let temp = TempDir::new().unwrap();
        let context = create_test_context(temp.path());

        let tool = BashTool;
        let mut params = ToolParams::new();
        params.insert("command", "exit 1");

        let result = tool.execute(&params, &context).unwrap();

        // Tool execution succeeds, but exit code is non-zero
        assert!(result.success);
        assert!(result.output.contains("[exit code: 1]"));
    }

    #[test]
    fn test_blocked_command() {
        let temp = TempDir::new().unwrap();
        let context = create_test_context(temp.path());

        let tool = BashTool;
        let mut params = ToolParams::new();
        params.insert("command", "sudo rm -rf /");

        let result = tool.execute(&params, &context);

        assert!(result.is_err());
        if let Err(ToolError::PermissionDenied(_)) = result {
            // Expected
        } else {
            panic!("Expected PermissionDenied error");
        }
    }

    #[test]
    fn test_dangerous_patterns() {
        assert!(is_dangerous_command("rm -rf /"));
        assert!(is_dangerous_command("sudo apt install"));
        assert!(is_dangerous_command("echo hello > /dev/sda"));
        assert!(!is_dangerous_command("cargo build"));
        assert!(!is_dangerous_command("npm test"));
    }

    #[test]
    fn test_permission_always_required() {
        let temp = TempDir::new().unwrap();
        let context = create_test_context(temp.path());

        let tool = BashTool;
        let level = tool.permission_level(None, &context);

        assert_eq!(level, PermissionLevel::Required);
    }
}
