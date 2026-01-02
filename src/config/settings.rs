use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Application settings loaded from embedded config file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub model: ModelSettings,
    pub ui: UiSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            model: ModelSettings::default(),
            ui: UiSettings::default(),
        }
    }
}

/// Model-related settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSettings {
    /// Hugging Face repository (e.g., "nunocoracao/pangu")
    pub hf_repo: String,
    /// Model filename to download
    pub filename: String,
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
            hf_repo: "nunocoracao/pangu".to_string(),
            filename: "devstral-small-2-q4.gguf".to_string(),
            n_gpu_layers: -1,
            context_size: 8192, // Model supports up to 131K, but 8K is memory-friendly
            temperature: 0.15,
            top_p: 0.95,
        }
    }
}

impl ModelSettings {
    /// Get the direct download URL for the model
    pub fn download_url(&self) -> String {
        format!(
            "https://huggingface.co/{}/resolve/main/{}",
            self.hf_repo, self.filename
        )
    }

    /// Get the local model path (in ~/.pangu/models/)
    pub fn model_path(&self) -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".pangu")
            .join("models")
            .join(&self.filename)
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
