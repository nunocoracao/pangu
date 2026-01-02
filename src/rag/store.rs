//! Conversation storage for RAG
//!
//! Stores messages in `.pangu/history/` directory with UTC timestamps.

use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

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
    /// Optional conversation ID for grouping
    #[serde(default)]
    pub conversation_id: String,
}

impl StoredMessage {
    /// Create a new stored message from a ChatMessage
    pub fn from_chat_message(msg: &ChatMessage, conversation_id: &str) -> Self {
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

/// Manages conversation storage in `.pangu/history/`
pub struct ConversationStore {
    /// Base directory for storage (typically `.pangu/history/`)
    history_dir: PathBuf,
    /// Current conversation ID (based on session start time)
    conversation_id: String,
    /// Path to the current session file
    session_file: PathBuf,
}

impl ConversationStore {
    /// Create a new conversation store in the given directory
    pub fn new(base_dir: &std::path::Path) -> std::io::Result<Self> {
        let history_dir = base_dir.join(".pangu").join("history");
        fs::create_dir_all(&history_dir)?;

        // Create conversation ID from current timestamp
        let conversation_id = Utc::now().format("%Y%m%d_%H%M%S").to_string();
        let session_file = history_dir.join(format!("{}.jsonl", conversation_id));

        Ok(Self {
            history_dir,
            conversation_id,
            session_file,
        })
    }

    /// Get the conversation ID
    pub fn conversation_id(&self) -> &str {
        &self.conversation_id
    }

    /// Store a message
    pub fn store(&self, msg: &ChatMessage) -> std::io::Result<()> {
        // Skip system messages
        if msg.role == Role::System {
            return Ok(());
        }

        let stored = StoredMessage::from_chat_message(msg, &self.conversation_id);
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
    pub fn store_messages(&self, messages: &[ChatMessage]) -> std::io::Result<()> {
        for msg in messages {
            self.store(msg)?;
        }
        Ok(())
    }

    /// Load all messages from history
    pub fn load_all(&self) -> std::io::Result<Vec<StoredMessage>> {
        let mut all_messages = Vec::new();

        // Read all .jsonl files in history directory
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

            // Sort by filename (which is timestamp-based)
            files.sort_by_key(|e| e.path());

            for entry in files {
                if let Ok(messages) = self.load_file(&entry.path()) {
                    all_messages.extend(messages);
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

    /// Load messages from the last N sessions
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
                    all_messages.extend(messages);
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
    use tempfile::tempdir;

    #[test]
    fn test_store_and_load() {
        let dir = tempdir().unwrap();
        let store = ConversationStore::new(dir.path()).unwrap();

        // Store some messages
        let msg1 = ChatMessage::user("Hello");
        let msg2 = ChatMessage::assistant("Hi there!");

        store.store(&msg1).unwrap();
        store.store(&msg2).unwrap();

        // Load and verify
        let loaded = store.load_all().unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].role, "user");
        assert_eq!(loaded[0].content, "Hello");
        assert_eq!(loaded[1].role, "assistant");
        assert_eq!(loaded[1].content, "Hi there!");
    }

    #[test]
    fn test_skip_system_messages() {
        let dir = tempdir().unwrap();
        let store = ConversationStore::new(dir.path()).unwrap();

        // Store a system message (should be skipped)
        let system_msg = ChatMessage::system("You are a helpful assistant");
        store.store(&system_msg).unwrap();

        // Load and verify it was skipped
        let loaded = store.load_all().unwrap();
        assert_eq!(loaded.len(), 0);
    }
}
