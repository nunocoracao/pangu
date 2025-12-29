use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Application settings loaded from config file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub model: ModelSettings,
    pub ui: UiSettings,
    pub system: SystemSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            model: ModelSettings::default(),
            ui: UiSettings::default(),
            system: SystemSettings::default(),
        }
    }
}

/// Model-related settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSettings {
    /// Path to the GGUF model file
    pub path: PathBuf,
    /// Number of layers to offload to GPU (-1 = all)
    pub n_gpu_layers: i32,
    /// Context window size
    pub context_size: u32,
    /// Temperature for sampling
    pub temperature: f32,
    /// Top-p sampling parameter
    pub top_p: f32,
}

impl Default for ModelSettings {
    fn default() -> Self {
        Self {
            path: PathBuf::from("./models/devstral-small-2-q4.gguf"),
            n_gpu_layers: -1,
            context_size: 8192,
            temperature: 0.15,
            top_p: 0.95,
        }
    }
}

/// UI-related settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiSettings {
    /// Frames per second for rendering
    pub frame_rate: f64,
    /// Ticks per second for updates
    pub tick_rate: f64,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            frame_rate: 30.0,
            tick_rate: 4.0,
        }
    }
}

/// System prompt settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemSettings {
    /// System prompt for the assistant (used if prompt_file is not set)
    #[serde(default)]
    pub prompt: String,
    /// Path to a file containing the system prompt (takes precedence over prompt)
    #[serde(default)]
    pub prompt_file: Option<PathBuf>,
    /// Path to a file containing the welcome message
    #[serde(default)]
    pub welcome_file: Option<PathBuf>,
}

impl Default for SystemSettings {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            prompt_file: Some(PathBuf::from("./config/system_prompt.txt")),
            welcome_file: Some(PathBuf::from("./config/welcome.txt")),
        }
    }
}
