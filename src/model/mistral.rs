use std::path::Path;
use std::sync::Arc;

use mistralrs::{
    GgufModelBuilder, Model, PagedAttentionMetaBuilder, Response,
    TextMessageRole, TextMessages,
};
use tokio::sync::mpsc;

use super::{
    backend::{InferenceConfig, ModelBackend, ModelError, ModelInfo, ModelParams},
    message::{ChatMessage, Role},
    streaming::StreamEvent,
};

/// mistral.rs-based model backend
pub struct MistralBackend {
    model: Arc<Model>,
    info: ModelInfo,
}

impl MistralBackend {
    /// Convert our Role enum to mistralrs TextMessageRole
    fn to_mistral_role(role: &Role) -> TextMessageRole {
        match role {
            Role::System => TextMessageRole::System,
            Role::User => TextMessageRole::User,
            Role::Assistant => TextMessageRole::Assistant,
        }
    }

    /// Convert our ChatMessage vec to mistralrs TextMessages
    fn to_text_messages(messages: &[ChatMessage]) -> TextMessages {
        let mut text_messages = TextMessages::new();
        for msg in messages {
            text_messages = text_messages.add_message(
                Self::to_mistral_role(&msg.role),
                &msg.content,
            );
        }
        text_messages
    }
}

impl ModelBackend for MistralBackend {
    fn load(path: &Path, params: ModelParams) -> Result<Self, ModelError>
    where
        Self: Sized,
    {
        // mistral.rs uses async, so we need to block on it
        let rt = tokio::runtime::Handle::current();

        // GgufModelBuilder expects directory path and filename separately
        let dir_path = path
            .parent()
            .ok_or_else(|| ModelError::LoadError("Invalid model path".to_string()))?
            .to_string_lossy()
            .to_string();

        let model_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let model = rt.block_on(async {
            GgufModelBuilder::new(dir_path, vec![model_name.clone()])
                .with_logging()
                .with_paged_attn(|| PagedAttentionMetaBuilder::default().build())
                .map_err(|e| ModelError::LoadError(format!("Failed to configure paged attention: {}", e)))?
                .build()
                .await
                .map_err(|e| ModelError::LoadError(format!("Failed to load model: {}", e)))
        })?;

        Ok(Self {
            model: Arc::new(model),
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
        _config: &InferenceConfig,
        tx: mpsc::UnboundedSender<StreamEvent>,
    ) -> Result<(), ModelError> {
        let text_messages = Self::to_text_messages(messages);
        let model = Arc::clone(&self.model);

        // Spawn the streaming task
        tokio::spawn(async move {
            match model.stream_chat_request(text_messages).await {
                Ok(mut stream) => {
                    use futures::StreamExt;
                    while let Some(chunk) = stream.next().await {
                        match chunk {
                            Response::Chunk(chunk_response) => {
                                if let Some(choice) = chunk_response.choices.first() {
                                    if let Some(content) = &choice.delta.content {
                                        if tx.send(StreamEvent::Token(content.clone())).is_err() {
                                            break;
                                        }
                                    }
                                }
                            }
                            Response::Done(_) => {
                                let _ = tx.send(StreamEvent::Done);
                                break;
                            }
                            Response::InternalError(e) => {
                                let _ = tx.send(StreamEvent::Error(e.to_string()));
                                break;
                            }
                            _ => {}
                        }
                    }
                }
                Err(e) => {
                    let _ = tx.send(StreamEvent::Error(e.to_string()));
                }
            }
        });

        Ok(())
    }

    fn format_prompt(&self, messages: &[ChatMessage]) -> String {
        // mistral.rs handles formatting internally, but we provide this for compatibility
        let mut prompt = String::new();
        for msg in messages {
            match msg.role {
                Role::System => {
                    prompt.push_str(&format!("[SYSTEM] {}\n", msg.content));
                }
                Role::User => {
                    prompt.push_str(&format!("[USER] {}\n", msg.content));
                }
                Role::Assistant => {
                    prompt.push_str(&format!("[ASSISTANT] {}\n", msg.content));
                }
            }
        }
        prompt
    }
}
