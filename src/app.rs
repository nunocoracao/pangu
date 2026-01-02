use std::sync::{Arc, RwLock};

use crate::config::Settings;
use crate::model::{ChatMessage, DownloadProgress};
use crate::permissions::{PermissionManager, PermissionResponse};
use crate::tools::TodoList;
use crate::tui::components::{InputBox, MatrixColumn};

/// Application state enum
#[derive(Debug, Clone)]
pub enum AppState {
    /// Waiting for user input
    Idle,
    /// Model is generating a response
    Generating,
    /// Downloading the model from Hugging Face
    Downloading,
    /// Loading the model into llama-server
    Loading,
    /// Executing a tool
    ExecutingTool(String),
    /// Awaiting permission from user
    AwaitingPermission,
    /// Error state
    Error(String),
}

impl AppState {
    pub fn is_generating(&self) -> bool {
        matches!(self, AppState::Generating)
    }

    pub fn is_downloading(&self) -> bool {
        matches!(self, AppState::Downloading)
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, AppState::Loading)
    }

    pub fn is_executing_tool(&self) -> bool {
        matches!(self, AppState::ExecutingTool(_))
    }

    pub fn is_awaiting_permission(&self) -> bool {
        matches!(self, AppState::AwaitingPermission)
    }
}

/// Pending permission request
#[derive(Debug, Clone)]
pub struct PendingPermission {
    /// Tool name
    pub tool_name: String,
    /// Tool parameters
    pub tool_params: String,
    /// Permission key (e.g., domain for fetch)
    pub permission_key: String,
}

/// Context usage information
#[derive(Debug, Clone, Default)]
pub struct ContextInfo {
    /// Approximate tokens used in current context
    pub tokens_used: usize,
    /// Maximum context size
    pub max_tokens: usize,
    /// Number of messages in context
    pub message_count: usize,
    /// Number of messages from RAG retrieval
    pub rag_messages: usize,
}

impl ContextInfo {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            tokens_used: 0,
            max_tokens,
            message_count: 0,
            rag_messages: 0,
        }
    }

    /// Get usage as a percentage (0.0 to 1.0)
    pub fn usage_percent(&self) -> f64 {
        if self.max_tokens == 0 {
            0.0
        } else {
            (self.tokens_used as f64 / self.max_tokens as f64).min(1.0)
        }
    }
}

/// Main application state (TEA Model)
pub struct App {
    /// Current application state
    pub state: AppState,
    /// Conversation messages
    pub messages: Vec<ChatMessage>,
    /// Buffer for streaming response
    pub current_response: String,
    /// Input box component
    pub input_box: InputBox,
    /// Scroll offset for chat view
    pub scroll_offset: u16,
    /// Whether to quit the application
    pub should_quit: bool,
    /// Tick counter for animations
    pub tick: u64,
    /// Auto-scroll to bottom when new content arrives
    pub auto_scroll: bool,
    /// Total content height (for scroll calculations)
    pub content_height: u16,
    /// Visible area height
    pub view_height: u16,
    /// Welcome message to display when chat is empty
    pub welcome_message: String,
    /// Whether the model is ready for inference
    pub model_ready: bool,
    /// Matrix rain columns for loading animation
    pub matrix_columns: Vec<MatrixColumn>,
    /// Current download progress (if downloading)
    pub download_progress: Option<DownloadProgress>,
    /// Permission manager
    pub permission_manager: PermissionManager,
    /// Pending permission request (if awaiting user response)
    pub pending_permission: Option<PendingPermission>,
    /// Selected option in permission prompt (0-3)
    pub permission_selection: usize,
    /// Shared todo list
    pub todo_list: Arc<RwLock<TodoList>>,
    /// List of available tool names
    pub tool_names: Vec<String>,
    /// Currently active tool (if any)
    pub active_tool: Option<String>,
    /// Context usage information
    pub context_info: ContextInfo,
}

impl App {
    /// Create a new application with the given settings
    pub fn new(_settings: &Settings) -> Self {
        Self::with_todo_list(Arc::new(RwLock::new(TodoList::new())))
    }

    /// Create a new application with a shared todo list
    pub fn with_todo_list(todo_list: Arc<RwLock<TodoList>>) -> Self {
        Self {
            state: AppState::Loading,
            messages: Vec::new(), // System message added via set_system_prompt
            current_response: String::new(),
            input_box: InputBox::new(),
            scroll_offset: 0,
            should_quit: false,
            tick: 0,
            auto_scroll: true,
            content_height: 0,
            view_height: 0,
            welcome_message: String::new(), // Set via welcome_message field
            model_ready: false,
            matrix_columns: Vec::new(), // Initialized dynamically based on screen width
            download_progress: None,
            permission_manager: PermissionManager::new(),
            pending_permission: None,
            permission_selection: 0,
            todo_list,
            tool_names: Vec::new(),
            active_tool: None,
            context_info: ContextInfo::default(),
        }
    }

    /// Set the maximum context size
    pub fn set_max_context(&mut self, max_tokens: usize) {
        self.context_info.max_tokens = max_tokens;
    }

    /// Update context usage (approximate token count)
    /// Uses a simple heuristic: ~4 chars per token
    pub fn update_context_usage(&mut self, rag_messages: usize) {
        // Count only non-system messages for the display
        // (system prompt is counted separately in actual context sent)
        let mut total_chars = 0;
        let mut msg_count = 0;
        for msg in &self.messages {
            if msg.role != crate::model::Role::System {
                total_chars += msg.content.len();
                msg_count += 1;
            }
        }
        total_chars += self.current_response.len();

        // Approximate tokens (roughly 4 chars per token for English)
        self.context_info.tokens_used = total_chars / 4;
        self.context_info.message_count = msg_count;
        self.context_info.rag_messages = rag_messages;
    }

    /// Set the available tool names
    pub fn set_tool_names(&mut self, names: Vec<String>) {
        self.tool_names = names;
    }

    /// Set the system prompt (adds as first message)
    pub fn set_system_prompt(&mut self, prompt: &str) {
        // Remove existing system message if any
        self.messages.retain(|m| m.role != crate::model::Role::System);
        // Add new system message at the beginning
        self.messages.insert(0, ChatMessage::system(prompt));
    }

    /// Set to downloading state
    pub fn set_downloading(&mut self) {
        self.state = AppState::Downloading;
    }

    /// Update download progress
    pub fn update_download_progress(&mut self, progress: DownloadProgress) {
        self.download_progress = Some(progress);
    }

    /// Set to loading state (after download completes)
    pub fn set_loading(&mut self) {
        self.state = AppState::Loading;
        self.download_progress = None;
    }

    /// Mark the model as ready
    pub fn set_model_ready(&mut self) {
        self.model_ready = true;
    }

    /// Increment tick counter (for animations)
    pub fn tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
    }

    /// Add a user message
    pub fn add_user_message(&mut self, content: String) {
        self.messages.push(ChatMessage::user(content));
    }

    /// Start generating a response
    pub fn start_generating(&mut self) {
        self.state = AppState::Generating;
        self.current_response.clear();
    }

    /// Append a token to the current response
    pub fn append_token(&mut self, token: String) {
        self.current_response.push_str(&token);
    }

    /// Finish generating and commit the response
    pub fn finish_generating(&mut self) {
        if !self.current_response.is_empty() {
            self.messages
                .push(ChatMessage::assistant(&self.current_response));
        }
        self.current_response.clear();
        self.state = AppState::Idle;
    }

    /// Set error state
    pub fn set_error(&mut self, error: String) {
        self.state = AppState::Error(error);
    }

    /// Set to idle state
    pub fn set_idle(&mut self) {
        self.state = AppState::Idle;
    }

    /// Start tool execution
    pub fn start_tool_execution(&mut self, tool_name: &str) {
        self.state = AppState::ExecutingTool(tool_name.to_string());
        self.active_tool = Some(tool_name.to_string());
    }

    /// Add a tool result message
    pub fn add_tool_result(&mut self, tool_name: &str, result: String) {
        self.messages.push(ChatMessage::tool_result(tool_name, result));
        self.active_tool = None;
    }

    /// Scroll up in the chat view
    pub fn scroll_up(&mut self, amount: u16) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
        // Disable auto-scroll when user scrolls up
        self.auto_scroll = false;
    }

    /// Scroll down in the chat view
    pub fn scroll_down(&mut self, amount: u16) {
        self.scroll_offset = self.scroll_offset.saturating_add(amount);
        // Re-enable auto-scroll if we're at the bottom
        let max_scroll = self.content_height.saturating_sub(self.view_height);
        if self.scroll_offset >= max_scroll {
            self.scroll_offset = max_scroll;
            self.auto_scroll = true;
        }
    }

    /// Update content and view dimensions
    pub fn update_dimensions(&mut self, content_height: u16, view_height: u16) {
        self.content_height = content_height;
        self.view_height = view_height;

        // Auto-scroll to bottom if enabled
        if self.auto_scroll {
            let max_scroll = content_height.saturating_sub(view_height);
            self.scroll_offset = max_scroll;
        } else {
            // Clamp scroll offset to valid range
            let max_scroll = content_height.saturating_sub(view_height);
            self.scroll_offset = self.scroll_offset.min(max_scroll);
        }
    }

    /// Reset scroll to bottom
    pub fn scroll_to_bottom(&mut self) {
        let max_scroll = self.content_height.saturating_sub(self.view_height);
        self.scroll_offset = max_scroll;
        self.auto_scroll = true;
    }

    /// Request permission for a tool call
    pub fn request_permission(&mut self, tool_name: &str, tool_params: &str) {
        let permission_key = self.permission_manager.get_display_key(tool_name, tool_params);
        self.pending_permission = Some(PendingPermission {
            tool_name: tool_name.to_string(),
            tool_params: tool_params.to_string(),
            permission_key,
        });
        self.permission_selection = 0;
        self.state = AppState::AwaitingPermission;
    }

    /// Handle permission response from user
    pub fn handle_permission_response(&mut self, response: PermissionResponse) -> Option<PendingPermission> {
        let pending = self.pending_permission.take()?;
        let (permission, persist) = response.to_permission_and_persist();

        if persist {
            self.permission_manager.set_permission(
                &pending.tool_name,
                &pending.permission_key,
                permission,
            );
        } else {
            self.permission_manager.set_session_permission(
                &pending.tool_name,
                &pending.permission_key,
                permission,
            );
        }

        self.state = AppState::Idle;
        Some(pending)
    }

    /// Move permission selection up
    pub fn permission_select_prev(&mut self) {
        if self.permission_selection > 0 {
            self.permission_selection -= 1;
        } else {
            self.permission_selection = 3; // Wrap to bottom
        }
    }

    /// Move permission selection down
    pub fn permission_select_next(&mut self) {
        if self.permission_selection < 3 {
            self.permission_selection += 1;
        } else {
            self.permission_selection = 0; // Wrap to top
        }
    }

    /// Get the current permission response based on selection
    pub fn get_permission_response(&self) -> PermissionResponse {
        match self.permission_selection {
            0 => PermissionResponse::AllowOnce,
            1 => PermissionResponse::AllowAlways,
            2 => PermissionResponse::DenyOnce,
            3 => PermissionResponse::DenyAlways,
            _ => PermissionResponse::DenyOnce,
        }
    }
}
