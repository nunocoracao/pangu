use std::path::Path;
use thiserror::Error;
use tokio::sync::mpsc;

use super::{ChatMessage, StreamEvent};

/// Errors that can occur during model operations
#[derive(Debug, Error)]
pub enum ModelError {
    #[error("Failed to load model: {0}")]
    LoadError(String),

    #[error("Inference error: {0}")]
    InferenceError(String),

    #[error("Context overflow: context window exceeded")]
    ContextOverflow,

    #[error("Model not loaded")]
    NotLoaded,
}

/// Parameters for model loading
#[derive(Debug, Clone)]
pub struct ModelParams {
    /// Number of layers to offload to GPU (-1 = all, 0 = CPU only)
    pub n_gpu_layers: i32,
    /// Context window size
    pub context_size: u32,
}

impl Default for ModelParams {
    fn default() -> Self {
        Self {
            n_gpu_layers: -1,
            context_size: 8192,
        }
    }
}

/// Configuration for inference
#[derive(Debug, Clone)]
pub struct InferenceConfig {
    /// Maximum tokens to generate
    pub max_tokens: usize,
    /// Temperature for sampling
    pub temperature: f32,
    /// Top-p sampling parameter
    pub top_p: f32,
    /// Stop sequences
    pub stop_sequences: Vec<String>,
}

impl Default for InferenceConfig {
    fn default() -> Self {
        Self {
            max_tokens: 4096,
            temperature: 0.15,
            top_p: 0.95,
            stop_sequences: vec![],
        }
    }
}

/// Information about a loaded model
#[derive(Debug, Clone)]
pub struct ModelInfo {
    /// Model name (usually the filename)
    pub name: String,
    /// Context window size
    pub context_size: usize,
}

/// Trait for LLM backends
///
/// This abstraction allows swapping between different inference backends
/// (llama.cpp, llamafile, API-based, etc.)
pub trait ModelBackend: Send + Sync {
    /// Load a model from the given path
    fn load(path: &Path, params: ModelParams) -> Result<Self, ModelError>
    where
        Self: Sized;

    /// Get information about the loaded model
    fn info(&self) -> &ModelInfo;

    /// Generate a streaming response
    ///
    /// Sends tokens through the provided channel as they are generated.
    fn generate_stream(
        &mut self,
        messages: &[ChatMessage],
        config: &InferenceConfig,
        tx: mpsc::UnboundedSender<StreamEvent>,
    ) -> Result<(), ModelError>;

    /// Format messages into a prompt string
    fn format_prompt(&self, messages: &[ChatMessage]) -> String;
}
