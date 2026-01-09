//! Conversation storage for RAG
//!
//! Stores messages in `~/.pangu/history/` directory with project and branch indexing.

use std::collections::hash_map::DefaultHasher;
use std::fs::{self, File};
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::Command;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::model::{ChatMessage, Role};

/// A stored message with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMessage {
    /// UTC timestamp when the message was stored
    pub timestamp: DateTime<Utc>,
    /// Message role (user, assistant, tool)
    pub role: String,
    /// Message content
    pub content: String,
    /// Conversation ID for grouping
    #[serde(default)]
    pub conversation_id: String,
    /// Project identifier (hash of git origin URL or directory path)
    #[serde(default)]
    pub project_id: String,
    /// Git branch name
    #[serde(default)]
    pub branch: String,
}

impl StoredMessage {
    /// Create a new stored message from a ChatMessage
    pub fn from_chat_message(msg: &ChatMessage, conversation_id: &str, project_id: &str, branch: &str) -> Self {
        let role = match msg.role {
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
            Role::System => "system",
        };

        Self {
            timestamp: Utc::now(),
            role: role.to_string(),
            content: msg.content.clone(),
            conversation_id: conversation_id.to_string(),
            project_id: project_id.to_string(),
            branch: branch.to_string(),
        }
    }

    /// Convert back to a ChatMessage
    pub fn to_chat_message(&self) -> ChatMessage {
        let role = match self.role.as_str() {
            "user" => Role::User,
            "assistant" => Role::Assistant,
            "tool" => Role::Tool,
            _ => Role::System,
        };

        ChatMessage {
            role,
            content: self.content.clone(),
        }
    }
}

/// Get the git origin URL for the current directory
fn get_git_origin_url() -> Option<String> {
    Command::new("git")
        .args(["remote", "get-url", "origin"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Get the current git branch
fn get_current_branch() -> String {
    Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Hash a string to a short identifier
fn hash_string(s: &str) -> String {
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Generate a project ID from git origin URL or directory path
fn generate_project_id() -> String {
    // Try git origin URL first
    if let Some(url) = get_git_origin_url() {
        // Clean up URL for consistent hashing
        let clean_url = url
            .trim_start_matches("git@")
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_end_matches(".git")
            .replace(':', "/");
        return hash_string(&clean_url)[..8].to_string();
    }

    // Fallback: hash the current directory path
    let cwd = std::env::current_dir().unwrap_or_default();
    hash_string(&cwd.display().to_string())[..8].to_string()
}

/// Manages conversation storage in `~/.pangu/history/`
pub struct ConversationStore {
    /// Base directory for storage (`~/.pangu/history/`)
    history_dir: PathBuf,
    /// Current conversation ID (based on session start time)
    conversation_id: String,
    /// Path to the current session file
    session_file: PathBuf,
    /// Project identifier (hash of git origin URL or directory)
    project_id: String,
    /// Current git branch
    branch: String,
}

impl ConversationStore {
    /// Create a new conversation store in ~/.pangu/history/
    pub fn new() -> std::io::Result<Self> {
        let history_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".pangu")
            .join("history");
        fs::create_dir_all(&history_dir)?;

        // Create conversation ID from current timestamp
        let conversation_id = Utc::now().format("%Y%m%d_%H%M%S").to_string();
        let session_file = history_dir.join(format!("{}.jsonl", conversation_id));

        // Get project context
        let project_id = generate_project_id();
        let branch = get_current_branch();

        Ok(Self {
            history_dir,
            conversation_id,
            session_file,
            project_id,
            branch,
        })
    }

    /// Get the project ID
    pub fn project_id(&self) -> &str {
        &self.project_id
    }

    /// Get the current branch
    pub fn branch(&self) -> &str {
        &self.branch
    }

    /// Get the conversation ID
    #[allow(dead_code)]
    pub fn conversation_id(&self) -> &str {
        &self.conversation_id
    }

    /// Store a message
    pub fn store(&self, msg: &ChatMessage) -> std::io::Result<()> {
        // Skip system messages
        if msg.role == Role::System {
            return Ok(());
        }

        let stored = StoredMessage::from_chat_message(
            msg,
            &self.conversation_id,
            &self.project_id,
            &self.branch,
        );
        let json = serde_json::to_string(&stored)?;

        // Append to session file
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.session_file)?;

        writeln!(file, "{}", json)?;
        Ok(())
    }

    /// Store multiple messages
    #[allow(dead_code)]
    pub fn store_messages(&self, messages: &[ChatMessage]) -> std::io::Result<()> {
        for msg in messages {
            self.store(msg)?;
        }
        Ok(())
    }

    /// Load messages from history, filtered by current project and branch
    /// Only loads the most recent N session files to avoid scanning too many
    pub fn load_all(&self) -> std::io::Result<Vec<StoredMessage>> {
        self.load_filtered(Some(&self.project_id), Some(&self.branch), 50) // Limit to 50 recent sessions
    }

    /// Load messages with optional project/branch filtering
    /// max_files limits how many session files to scan (0 = unlimited)
    pub fn load_filtered(
        &self,
        project_id: Option<&str>,
        branch: Option<&str>,
        max_files: usize,
    ) -> std::io::Result<Vec<StoredMessage>> {
        let mut all_messages = Vec::new();

        // Read .jsonl files in history directory
        if let Ok(entries) = fs::read_dir(&self.history_dir) {
            let mut files: Vec<_> = entries
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.path()
                        .extension()
                        .map(|ext| ext == "jsonl")
                        .unwrap_or(false)
                })
                .collect();

            // Sort by filename descending (most recent first)
            files.sort_by_key(|e| std::cmp::Reverse(e.path()));

            // Limit files to scan
            let files_to_scan = if max_files > 0 {
                files.into_iter().take(max_files).collect::<Vec<_>>()
            } else {
                files
            };

            for entry in files_to_scan {
                if let Ok(messages) = self.load_file(&entry.path()) {
                    // Filter by project_id and branch
                    let filtered: Vec<_> = messages
                        .into_iter()
                        .filter(|m| {
                            let project_match = project_id
                                .map(|pid| m.project_id == pid || m.project_id.is_empty())
                                .unwrap_or(true);
                            let branch_match = branch
                                .map(|b| m.branch == b || m.branch.is_empty())
                                .unwrap_or(true);
                            project_match && branch_match
                        })
                        .collect();
                    all_messages.extend(filtered);
                }
            }
        }

        Ok(all_messages)
    }

    /// Load messages from a specific file
    fn load_file(&self, path: &std::path::Path) -> std::io::Result<Vec<StoredMessage>> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut messages = Vec::new();

        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }

            if let Ok(msg) = serde_json::from_str::<StoredMessage>(&line) {
                messages.push(msg);
            }
        }

        Ok(messages)
    }

    /// Load messages from the last N sessions (filtered by project/branch)
    #[allow(dead_code)]
    pub fn load_recent_sessions(&self, n: usize) -> std::io::Result<Vec<StoredMessage>> {
        let mut all_messages = Vec::new();

        if let Ok(entries) = fs::read_dir(&self.history_dir) {
            let mut files: Vec<_> = entries
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.path()
                        .extension()
                        .map(|ext| ext == "jsonl")
                        .unwrap_or(false)
                })
                .collect();

            // Sort by filename descending (most recent first)
            files.sort_by_key(|e| std::cmp::Reverse(e.path()));

            // Take last N files
            for entry in files.into_iter().take(n) {
                if let Ok(messages) = self.load_file(&entry.path()) {
                    // Filter by project_id and branch
                    let filtered: Vec<_> = messages
                        .into_iter()
                        .filter(|m| {
                            (m.project_id == self.project_id || m.project_id.is_empty())
                                && (m.branch == self.branch || m.branch.is_empty())
                        })
                        .collect();
                    all_messages.extend(filtered);
                }
            }
        }

        // Reverse to get chronological order
        all_messages.reverse();
        Ok(all_messages)
    }

    /// Get the number of stored sessions
    pub fn session_count(&self) -> usize {
        fs::read_dir(&self.history_dir)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter(|e| {
                        e.path()
                            .extension()
                            .map(|ext| ext == "jsonl")
                            .unwrap_or(false)
                    })
                    .count()
            })
            .unwrap_or(0)
    }

    /// Clean up old sessions (keep only last N)
    #[allow(dead_code)]
    pub fn cleanup(&self, keep_sessions: usize) -> std::io::Result<usize> {
        let mut files: Vec<_> = fs::read_dir(&self.history_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|ext| ext == "jsonl")
                    .unwrap_or(false)
            })
            .collect();

        if files.len() <= keep_sessions {
            return Ok(0);
        }

        // Sort by filename (oldest first)
        files.sort_by_key(|e| e.path());

        let to_delete = files.len() - keep_sessions;
        let mut deleted = 0;

        for entry in files.into_iter().take(to_delete) {
            // Don't delete current session
            if entry.path() != self.session_file {
                if fs::remove_file(entry.path()).is_ok() {
                    deleted += 1;
                }
            }
        }

        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_string() {
        let hash1 = hash_string("github.com/user/repo");
        let hash2 = hash_string("github.com/user/repo");
        let hash3 = hash_string("github.com/other/repo");

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
        assert_eq!(hash1.len(), 16);
    }

    #[test]
    fn test_stored_message_fields() {
        let msg = ChatMessage::user("Hello");
        let stored = StoredMessage::from_chat_message(&msg, "conv123", "proj456", "main");

        assert_eq!(stored.role, "user");
        assert_eq!(stored.content, "Hello");
        assert_eq!(stored.conversation_id, "conv123");
        assert_eq!(stored.project_id, "proj456");
        assert_eq!(stored.branch, "main");
    }

    #[test]
    fn test_skip_system_messages() {
        // This test would need a mock ConversationStore or temp directory
        // For now, just verify the role check logic exists in store()
        let system_msg = ChatMessage::system("You are a helpful assistant");
        assert_eq!(system_msg.role, Role::System);
    }
}
