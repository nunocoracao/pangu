mod settings;

pub use settings::{ModelSettings, Settings, SystemSettings, UiSettings};

use color_eyre::Result;
use directories::ProjectDirs;
use std::path::PathBuf;

/// Get the project directories for configuration and data storage
pub fn project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("", "", "pangu")
}

/// Get the config directory path
pub fn config_dir() -> PathBuf {
    project_dirs()
        .map(|d| d.config_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("./config"))
}

/// Get the data directory path (for models, etc.)
pub fn data_dir() -> PathBuf {
    project_dirs()
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("./data"))
}

/// Load settings from config file or use defaults
pub fn load_settings(config_path: Option<&PathBuf>) -> Result<Settings> {
    let mut settings = load_settings_from_file(config_path)?;

    // Load system prompt from file if specified
    if let Some(ref prompt_file) = settings.system.prompt_file {
        if prompt_file.exists() {
            settings.system.prompt = std::fs::read_to_string(prompt_file)?;
        } else if settings.system.prompt.is_empty() {
            // Fall back to default prompt if file doesn't exist and no inline prompt
            settings.system.prompt = default_system_prompt();
        }
    } else if settings.system.prompt.is_empty() {
        settings.system.prompt = default_system_prompt();
    }

    Ok(settings)
}

/// Load welcome message from file
pub fn load_welcome_message(settings: &Settings) -> String {
    if let Some(ref welcome_file) = settings.system.welcome_file {
        if welcome_file.exists() {
            if let Ok(content) = std::fs::read_to_string(welcome_file) {
                return content;
            }
        }
    }
    default_welcome_message()
}

fn default_welcome_message() -> String {
    r#"
    Welcome to Pangu!

    Your local AI coding assistant.
    Type a message below to start chatting.
"#.to_string()
}

/// Load settings from config file without processing prompt_file
fn load_settings_from_file(config_path: Option<&PathBuf>) -> Result<Settings> {
    // Try custom path first
    if let Some(path) = config_path {
        if path.exists() {
            let content = std::fs::read_to_string(path)?;
            return Ok(toml::from_str(&content)?);
        }
    }

    // Try user config directory
    let user_config = config_dir().join("config.toml");
    if user_config.exists() {
        let content = std::fs::read_to_string(&user_config)?;
        return Ok(toml::from_str(&content)?);
    }

    // Try local default config
    let local_config = PathBuf::from("./config/default.toml");
    if local_config.exists() {
        let content = std::fs::read_to_string(&local_config)?;
        return Ok(toml::from_str(&content)?);
    }

    // Fall back to defaults
    Ok(Settings::default())
}

/// Default system prompt
fn default_system_prompt() -> String {
    String::from(
        "You are Pangu, a helpful coding assistant running locally. \
        You assist with software engineering tasks including writing code, \
        debugging, explaining code, and file operations.\n\n\
        Guidelines:\n\
        - Be concise and direct\n\
        - Provide working code, not pseudocode\n\
        - Explain your reasoning when helpful\n\
        - Ask clarifying questions when requirements are ambiguous\n\
        - Prioritize correctness over cleverness",
    )
}
