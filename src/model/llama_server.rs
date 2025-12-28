use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use super::{
    backend::{InferenceConfig, ModelBackend, ModelError, ModelInfo, ModelParams},
    message::{ChatMessage, Role},
    streaming::StreamEvent,
};

const SERVER_PORT: u16 = 8080;
const SERVER_HOST: &str = "127.0.0.1";

/// llama-server HTTP backend
pub struct LlamaServerBackend {
    _server_process: Child,
    client: Client,
    base_url: String,
    info: ModelInfo,
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    messages: Vec<ApiMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
}

#[derive(Debug, Serialize)]
struct ApiMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionChunk {
    choices: Vec<ChunkChoice>,
}

#[derive(Debug, Deserialize)]
struct ChunkChoice {
    delta: Delta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Delta {
    content: Option<String>,
}

impl LlamaServerBackend {
    fn role_to_string(role: &Role) -> String {
        match role {
            Role::System => "system".to_string(),
            Role::User => "user".to_string(),
            Role::Assistant => "assistant".to_string(),
        }
    }

    fn to_api_messages(messages: &[ChatMessage]) -> Vec<ApiMessage> {
        messages
            .iter()
            .map(|m| ApiMessage {
                role: Self::role_to_string(&m.role),
                content: m.content.clone(),
            })
            .collect()
    }

    /// Wait for server to be ready
    async fn wait_for_server(client: &Client, base_url: &str) -> Result<(), ModelError> {
        let health_url = format!("{}/health", base_url);

        for i in 0..60 {
            match client.get(&health_url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    return Ok(());
                }
                _ => {
                    if i % 10 == 0 {
                        tracing::info!("Waiting for llama-server to start... ({}s)", i);
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }

        Err(ModelError::LoadError("llama-server failed to start within 60 seconds".to_string()))
    }
}

impl ModelBackend for LlamaServerBackend {
    fn load(path: &Path, params: ModelParams) -> Result<Self, ModelError>
    where
        Self: Sized,
    {
        let model_path = path.to_string_lossy().to_string();
        let model_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        // Find llama-server binary - check multiple locations
        let possible_paths = [
            std::path::PathBuf::from("./bin/llama-server"),
            std::path::PathBuf::from("bin/llama-server"),
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|p| p.join("llama-server")))
                .unwrap_or_default(),
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().and_then(|p| p.parent()).map(|p| p.join("bin/llama-server")))
                .unwrap_or_default(),
        ];

        let server_path = possible_paths
            .iter()
            .find(|p| p.exists())
            .cloned()
            .ok_or_else(|| {
                ModelError::LoadError(format!(
                    "llama-server not found. Checked: {:?}. Please build llama.cpp and copy llama-server to bin/",
                    possible_paths
                ))
            })?;

        // Build command arguments
        let mut cmd = Command::new(&server_path);
        cmd.arg("--model").arg(&model_path)
           .arg("--host").arg(SERVER_HOST)
           .arg("--port").arg(SERVER_PORT.to_string())
           .arg("--ctx-size").arg(params.context_size.to_string());

        // GPU layers
        if params.n_gpu_layers != 0 {
            let layers = if params.n_gpu_layers < 0 { 999 } else { params.n_gpu_layers };
            cmd.arg("--n-gpu-layers").arg(layers.to_string());
        }

        // Suppress server output
        cmd.stdout(Stdio::null())
           .stderr(Stdio::null());

        tracing::info!("Starting llama-server with model: {}", model_path);

        let server_process = cmd
            .spawn()
            .map_err(|e| ModelError::LoadError(format!("Failed to spawn llama-server: {}", e)))?;

        let client = Client::new();
        let base_url = format!("http://{}:{}", SERVER_HOST, SERVER_PORT);

        // Wait for server to be ready (blocking)
        let rt = tokio::runtime::Handle::current();
        rt.block_on(Self::wait_for_server(&client, &base_url))?;

        tracing::info!("llama-server started successfully");

        Ok(Self {
            _server_process: server_process,
            client,
            base_url,
            info: ModelInfo {
                name: model_name,
                context_size: params.context_size as usize,
            },
        })
    }

    fn info(&self) -> &ModelInfo {
        &self.info
    }

    fn generate_stream(
        &mut self,
        messages: &[ChatMessage],
        config: &InferenceConfig,
        tx: mpsc::UnboundedSender<StreamEvent>,
    ) -> Result<(), ModelError> {
        let api_messages = Self::to_api_messages(messages);
        let client = self.client.clone();
        let url = format!("{}/v1/chat/completions", self.base_url);

        let request = ChatCompletionRequest {
            messages: api_messages,
            stream: true,
            max_tokens: Some(config.max_tokens),
            temperature: Some(config.temperature),
            top_p: Some(config.top_p),
        };

        tokio::spawn(async move {
            match client.post(&url).json(&request).send().await {
                Ok(response) => {
                    if !response.status().is_success() {
                        let _ = tx.send(StreamEvent::Error(format!(
                            "Server error: {}",
                            response.status()
                        )));
                        return;
                    }

                    use futures::StreamExt;
                    let mut stream = response.bytes_stream();
                    let mut buffer = String::new();

                    while let Some(chunk_result) = stream.next().await {
                        match chunk_result {
                            Ok(bytes) => {
                                buffer.push_str(&String::from_utf8_lossy(&bytes));

                                // Process complete SSE events
                                while let Some(pos) = buffer.find("\n\n") {
                                    let event = buffer[..pos].to_string();
                                    buffer = buffer[pos + 2..].to_string();

                                    // Parse SSE data
                                    for line in event.lines() {
                                        if let Some(data) = line.strip_prefix("data: ") {
                                            if data == "[DONE]" {
                                                let _ = tx.send(StreamEvent::Done);
                                                return;
                                            }

                                            if let Ok(chunk) = serde_json::from_str::<ChatCompletionChunk>(data) {
                                                if let Some(choice) = chunk.choices.first() {
                                                    if let Some(content) = &choice.delta.content {
                                                        if tx.send(StreamEvent::Token(content.clone())).is_err() {
                                                            return;
                                                        }
                                                    }
                                                    if choice.finish_reason.is_some() {
                                                        let _ = tx.send(StreamEvent::Done);
                                                        return;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                let _ = tx.send(StreamEvent::Error(e.to_string()));
                                return;
                            }
                        }
                    }

                    let _ = tx.send(StreamEvent::Done);
                }
                Err(e) => {
                    let _ = tx.send(StreamEvent::Error(e.to_string()));
                }
            }
        });

        Ok(())
    }

    fn format_prompt(&self, messages: &[ChatMessage]) -> String {
        // Not used with chat completions API, but implement for trait
        let mut prompt = String::new();
        for msg in messages {
            match msg.role {
                Role::System => prompt.push_str(&format!("[SYSTEM] {}\n", msg.content)),
                Role::User => prompt.push_str(&format!("[USER] {}\n", msg.content)),
                Role::Assistant => prompt.push_str(&format!("[ASSISTANT] {}\n", msg.content)),
            }
        }
        prompt
    }
}

impl Drop for LlamaServerBackend {
    fn drop(&mut self) {
        // Kill the server process when backend is dropped
        tracing::info!("Shutting down llama-server");
        let _ = self._server_process.kill();
    }
}
