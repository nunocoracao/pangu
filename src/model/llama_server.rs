use std::net::TcpListener;
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

const DEFAULT_SERVER_PORT: u16 = 8080;
const SERVER_HOST: &str = "127.0.0.1";

/// llama-server HTTP backend
#[allow(dead_code)]
pub struct LlamaServerBackend {
    /// Server process - only Some if we spawned it
    server_process: Option<Child>,
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
            // Tool results are sent as user messages in the chat API
            Role::Tool => "user".to_string(),
        }
    }

    fn to_api_messages(messages: &[ChatMessage]) -> Vec<ApiMessage> {
        // Convert messages and consolidate consecutive same-role messages
        // This prevents "roles must alternate" errors from llama-server
        let mut api_messages: Vec<ApiMessage> = Vec::new();

        for msg in messages {
            let role = Self::role_to_string(&msg.role);

            // Check if we should merge with the previous message
            if let Some(last) = api_messages.last_mut() {
                if last.role == role {
                    // Consolidate: append content to previous message
                    last.content.push_str("\n\n");
                    last.content.push_str(&msg.content);
                    continue;
                }
            }

            // Add as new message
            api_messages.push(ApiMessage {
                role,
                content: msg.content.clone(),
            });
        }

        api_messages
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

    /// Pick a server port.
    ///
    /// Priority:
    /// 1) `PANGU_LLAMA_PORT` env var (for debugging/pinning)
    /// 2) dynamic free port from OS
    /// 3) fallback to default
    fn pick_server_port() -> u16 {
        if let Ok(port_str) = std::env::var("PANGU_LLAMA_PORT") {
            if let Ok(port) = port_str.parse::<u16>() {
                if port > 0 {
                    return port;
                }
            }
        }

        if let Ok(listener) = TcpListener::bind((SERVER_HOST, 0)) {
            if let Ok(addr) = listener.local_addr() {
                return addr.port();
            }
        }

        DEFAULT_SERVER_PORT
    }

    /// Load with a specific llama-server binary path
    pub fn load_with_server(
        model_path: &Path,
        server_path: &Path,
        params: ModelParams,
    ) -> Result<Self, ModelError> {
        let model_path_str = model_path.to_string_lossy().to_string();
        let model_name = model_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let client = Client::new();
        let server_port = Self::pick_server_port();
        let base_url = format!("http://{}:{}", SERVER_HOST, server_port);
        let rt = tokio::runtime::Handle::current();
        if !server_path.exists() {
            return Err(ModelError::LoadError(format!(
                "llama-server not found at {:?}",
                server_path
            )));
        }

        // Build command arguments
        let mut cmd = Command::new(server_path);
        cmd.arg("--model").arg(&model_path_str)
            .arg("--host").arg(SERVER_HOST)
            .arg("--port").arg(server_port.to_string())
            .arg("--ctx-size").arg(params.context_size.to_string())
            .arg("--flash-attn").arg("on");

        // On Apple Silicon, explicitly target Metal to avoid backend ambiguity.
        #[cfg(target_os = "macos")]
        {
            cmd.arg("--device").arg("Metal");
        }

        // GPU layers
        if params.n_gpu_layers != 0 {
            let layers = if params.n_gpu_layers < 0 { 999 } else { params.n_gpu_layers };
            cmd.arg("--n-gpu-layers").arg(layers.to_string());
        }

        // Suppress server output
        cmd.stdout(Stdio::null()).stderr(Stdio::null());

        tracing::info!(
            "Starting llama-server with model: {} on {}:{}",
            model_path_str,
            SERVER_HOST,
            server_port
        );
        tracing::info!("llama-server command: {:?}", cmd);

        let process = cmd
            .spawn()
            .map_err(|e| ModelError::LoadError(format!("Failed to spawn llama-server: {}", e)))?;

        // Register PID for signal handler cleanup
        crate::set_llama_server_pid(process.id());

        // Wait for server to be ready
        rt.block_on(Self::wait_for_server(&client, &base_url))?;

        tracing::info!("llama-server started successfully");
        let server_process = Some(process);

        Ok(Self {
            server_process,
            client,
            base_url,
            info: ModelInfo {
                name: model_name,
                context_size: params.context_size as usize,
            },
        })
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

        let client = Client::new();
        let server_port = Self::pick_server_port();
        let base_url = format!("http://{}:{}", SERVER_HOST, server_port);
        let rt = tokio::runtime::Handle::current();
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
            .arg("--port").arg(server_port.to_string())
            .arg("--ctx-size").arg(params.context_size.to_string())
            .arg("--flash-attn").arg("on");

        // On Apple Silicon, explicitly target Metal to avoid backend ambiguity.
        #[cfg(target_os = "macos")]
        {
            cmd.arg("--device").arg("Metal");
        }

        // GPU layers
        if params.n_gpu_layers != 0 {
            let layers = if params.n_gpu_layers < 0 { 999 } else { params.n_gpu_layers };
            cmd.arg("--n-gpu-layers").arg(layers.to_string());
        }

        // Suppress server output
        cmd.stdout(Stdio::null()).stderr(Stdio::null());

        tracing::info!(
            "Starting llama-server with model: {} on {}:{}",
            model_path,
            SERVER_HOST,
            server_port
        );
        tracing::info!("llama-server command: {:?}", cmd);

        let process = cmd
            .spawn()
            .map_err(|e| ModelError::LoadError(format!("Failed to spawn llama-server: {}", e)))?;

        // Register PID for signal handler cleanup
        crate::set_llama_server_pid(process.id());

        // Wait for server to be ready
        rt.block_on(Self::wait_for_server(&client, &base_url))?;

        tracing::info!("llama-server started successfully");
        let server_process = Some(process);

        Ok(Self {
            server_process,
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

        // Log request info for debugging context overflow
        let msg_count = messages.len();
        let total_chars: usize = messages.iter().map(|m| m.content.len()).sum();
        let estimated_tokens = total_chars / 4;
        tracing::info!(
            "Sending request: {} messages, ~{} chars, ~{} tokens",
            msg_count, total_chars, estimated_tokens
        );

        tokio::spawn(async move {
            match client.post(&url).json(&request).send().await {
                Ok(response) => {
                    if !response.status().is_success() {
                        let status = response.status();
                        // Try to get error body for more details
                        let error_body = response.text().await.unwrap_or_default();
                        tracing::error!(
                            "llama-server error {}: {}",
                            status,
                            if error_body.len() > 200 { &error_body[..200] } else { &error_body }
                        );
                        let _ = tx.send(StreamEvent::Error(format!(
                            "Server error: {}",
                            status
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
                Role::Tool => prompt.push_str(&format!("[TOOL] {}\n", msg.content)),
            }
        }
        prompt
    }
}

impl Drop for LlamaServerBackend {
    fn drop(&mut self) {
        // Only kill the server if we spawned it (not if we connected to existing)
        if let Some(mut process) = self.server_process.take() {
            let pid = process.id();
            eprintln!("[pangu] Shutting down llama-server (pid: {})", pid);

            // Clear the global PID so signal handler doesn't try to kill it again
            crate::set_llama_server_pid(0);

            // Kill the process
            if let Err(e) = process.kill() {
                eprintln!("[pangu] Failed to kill llama-server: {}", e);
            }

            // Wait for the process to avoid zombies
            match process.wait() {
                Ok(status) => eprintln!("[pangu] llama-server exited: {}", status),
                Err(e) => eprintln!("[pangu] Failed to wait for llama-server: {}", e),
            }
        }
    }
}
