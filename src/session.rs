//! Session persistence for saving/loading conversations
//!
//! Stores the conversation history in `.pangu/session.json` per project.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::model::ChatMessage;

/// Session data stored to disk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Version for future compatibility
    pub version: u32,
    /// Conversation messages (excludes system prompt)
    pub messages: Vec<ChatMessage>,
    /// Total input tokens from previous sessions
    #[serde(default)]
    pub total_input_tokens: usize,
    /// Total output tokens from previous sessions
    #[serde(default)]
    pub total_output_tokens: usize,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            version: 1,
            messages: Vec::new(),
            total_input_tokens: 0,
            total_output_tokens: 0,
        }
    }
}

/// Session manager for a project
pub struct SessionManager {
    /// Path to the session file
    session_path: PathBuf,
}

impl SessionManager {
    /// Create a new session manager for the given project root
    pub fn new(project_root: &Path) -> Self {
        let pangu_dir = project_root.join(".pangu");
        let session_path = pangu_dir.join("session.json");

        Self { session_path }
    }

    /// Load session from disk, returns empty session if not found
    pub fn load(&self) -> Session {
        if !self.session_path.exists() {
            debug!("No existing session found at {:?}", self.session_path);
            return Session::default();
        }

        match fs::read_to_string(&self.session_path) {
            Ok(content) => {
                match serde_json::from_str::<Session>(&content) {
                    Ok(session) => {
                        info!(
                            "Loaded session with {} messages from {:?}",
                            session.messages.len(),
                            self.session_path
                        );
                        session
                    }
                    Err(e) => {
                        warn!("Failed to parse session file: {}", e);
                        Session::default()
                    }
                }
            }
            Err(e) => {
                warn!("Failed to read session file: {}", e);
                Session::default()
            }
        }
    }

    /// Save session to disk
    pub fn save(&self, session: &Session) -> Result<(), std::io::Error> {
        // Ensure .pangu directory exists
        if let Some(parent) = self.session_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(session)?;
        fs::write(&self.session_path, content)?;

        debug!(
            "Saved session with {} messages to {:?}",
            session.messages.len(),
            self.session_path
        );

        Ok(())
    }

    /// Save messages to session (convenience method)
    pub fn save_messages(
        &self,
        messages: &[ChatMessage],
        input_tokens: usize,
        output_tokens: usize,
    ) -> Result<(), std::io::Error> {
        let session = Session {
            version: 1,
            messages: messages.to_vec(),
            total_input_tokens: input_tokens,
            total_output_tokens: output_tokens,
        };
        self.save(&session)
    }

    /// Clear the session (delete the file)
    pub fn clear(&self) -> Result<(), std::io::Error> {
        if self.session_path.exists() {
            fs::remove_file(&self.session_path)?;
            info!("Cleared session at {:?}", self.session_path);
        }
        Ok(())
    }

    /// Get the session file path
    #[allow(dead_code)]
    pub fn session_path(&self) -> &Path {
        &self.session_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_save_and_load_session() {
        let temp = TempDir::new().unwrap();
        let manager = SessionManager::new(temp.path());

        // Create some messages
        let messages = vec![
            ChatMessage::user("Hello"),
            ChatMessage::assistant("Hi there!"),
        ];

        // Save
        manager.save_messages(&messages, 10, 20).unwrap();

        // Load
        let loaded = manager.load();
        assert_eq!(loaded.messages.len(), 2);
        assert_eq!(loaded.messages[0].content, "Hello");
        assert_eq!(loaded.messages[1].content, "Hi there!");
        assert_eq!(loaded.total_input_tokens, 10);
        assert_eq!(loaded.total_output_tokens, 20);
    }

    #[test]
    fn test_load_nonexistent_session() {
        let temp = TempDir::new().unwrap();
        let manager = SessionManager::new(temp.path());

        let session = manager.load();
        assert!(session.messages.is_empty());
    }

    #[test]
    fn test_clear_session() {
        let temp = TempDir::new().unwrap();
        let manager = SessionManager::new(temp.path());

        // Save something
        manager.save_messages(&[ChatMessage::user("test")], 0, 0).unwrap();
        assert!(manager.session_path().exists());

        // Clear
        manager.clear().unwrap();
        assert!(!manager.session_path().exists());
    }
}
