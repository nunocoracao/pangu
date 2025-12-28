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
