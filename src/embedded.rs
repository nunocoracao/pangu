//! Embedded resources for standalone binary distribution
//!
//! This module embeds the llama-server binary and config data into
//! the Pangu executable. Only llama-server is extracted to disk.
//! Configs are kept in memory.

use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::sync::LazyLock;

use crate::config::Settings;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// Embedded llama-server binary (~6MB)
const LLAMA_SERVER: &[u8] = include_bytes!("../bin/llama-server");

/// Embedded system prompt (kept in memory, not extracted)
const SYSTEM_PROMPT: &str = include_str!("../config/system_prompt.txt");

/// Embedded welcome message (kept in memory, not extracted)
const WELCOME: &str = include_str!("../config/welcome.txt");

/// Embedded configuration (parsed at runtime from config/default.toml)
const CONFIG_TOML: &str = include_str!("../config/default.toml");

/// Settings loaded from embedded config/default.toml
pub static SETTINGS: LazyLock<Settings> = LazyLock::new(|| {
    toml::from_str(CONFIG_TOML).expect("Failed to parse embedded config/default.toml")
});

/// Embedded resources - llama-server extracted, configs in memory
pub struct EmbeddedResources {
    /// Base directory for Pangu data (~/.pangu)
    pub base_dir: PathBuf,
    /// Path to extracted llama-server
    pub llama_server_path: PathBuf,
    /// Path where model should be stored
    pub model_dir: PathBuf,
}

impl EmbeddedResources {
    /// Extract llama-server to ~/.pangu directory
    pub fn extract() -> std::io::Result<Self> {
        // Use ~/.pangu for persistent storage
        let base_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".pangu");

        // Create directory structure (only bin and models)
        fs::create_dir_all(&base_dir)?;
        fs::create_dir_all(base_dir.join("bin"))?;
        fs::create_dir_all(base_dir.join("models"))?;

        let llama_server_path = base_dir.join("bin/llama-server");
        let model_dir = base_dir.join("models");

        // Extract llama-server if not present or different size
        if should_extract(&llama_server_path, LLAMA_SERVER.len()) {
            tracing::info!("Extracting llama-server ({} bytes)...", LLAMA_SERVER.len());
            let mut file = File::create(&llama_server_path)?;
            file.write_all(LLAMA_SERVER)?;

            // Make executable on Unix
            #[cfg(unix)]
            {
                let mut perms = file.metadata()?.permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&llama_server_path, perms)?;
            }
        }

        tracing::info!("Embedded resources ready at {:?}", base_dir);

        Ok(Self {
            base_dir,
            llama_server_path,
            model_dir,
        })
    }

    /// Get the expected model path
    pub fn model_path(&self) -> PathBuf {
        self.model_dir.join(&SETTINGS.model.filename)
    }

    /// Check if the model exists
    pub fn model_exists(&self) -> bool {
        self.model_path().exists()
    }

    /// Get the embedded system prompt
    pub fn system_prompt(&self) -> &'static str {
        SYSTEM_PROMPT
    }

    /// Get the embedded welcome message
    pub fn welcome_message(&self) -> &'static str {
        WELCOME
    }
}

/// Check if we need to extract a file (doesn't exist or wrong size)
fn should_extract(path: &PathBuf, expected_size: usize) -> bool {
    match fs::metadata(path) {
        Ok(meta) => meta.len() != expected_size as u64,
        Err(_) => true,
    }
}
