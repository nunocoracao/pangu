use std::time::Instant;

use ratatui::text::Line;

use crate::model::{ChatMessage, DownloadProgress};
use crate::tui::components::{InputBox, MatrixColumn};

/// Cache for formatted chat lines to avoid re-rendering every frame
#[derive(Debug, Default)]
pub struct ChatViewCache {
    /// Cached formatted lines
    pub lines: Vec<Line<'static>>,
    /// Number of messages when cache was built
    message_count: usize,
    /// Length of last message content when cache was built
    last_message_len: usize,
    /// Length of current_response when cache was built
    current_response_len: usize,
    /// Tick value when cache was built (for animations)
    cached_tick: u64,
    /// Width used for rendering
    cached_width: u16,
    /// Whether generating state
    cached_is_generating: bool,
    /// Whether loading state
    cached_is_loading: bool,
}

impl ChatViewCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if cache needs to be invalidated
    pub fn needs_update(
        &self,
        messages: &[ChatMessage],
        current_response: &str,
        tick: u64,
        width: u16,
        is_generating: bool,
        is_loading: bool,
    ) -> bool {
        // Always invalidate if dimensions changed
        if width != self.cached_width {
            return true;
        }

        // Check if loading state changed
        if is_loading != self.cached_is_loading {
            return true;
        }

        // Check if message count changed
        if messages.len() != self.message_count {
            return true;
        }

        // Check if last message content changed
        let last_len = messages.last().map(|m| m.content.len()).unwrap_or(0);
        if last_len != self.last_message_len {
            return true;
        }

        // Check if current response changed
        if current_response.len() != self.current_response_len {
            return true;
        }

        // Check if generating state changed
        if is_generating != self.cached_is_generating {
            return true;
        }

        // During generation or loading, update periodically for animations (every 2 ticks)
        if (is_generating || is_loading) && tick.wrapping_sub(self.cached_tick) >= 2 {
            return true;
        }

        false
    }

    /// Update cache metadata after reformatting
    pub fn update_metadata(
        &mut self,
        messages: &[ChatMessage],
        current_response: &str,
        tick: u64,
        width: u16,
        is_generating: bool,
        is_loading: bool,
    ) {
        self.message_count = messages.len();
        self.last_message_len = messages.last().map(|m| m.content.len()).unwrap_or(0);
        self.current_response_len = current_response.len();
        self.cached_tick = tick;
        self.cached_width = width;
        self.cached_is_generating = is_generating;
        self.cached_is_loading = is_loading;
    }
}

/// Pending tool call awaiting permission
#[derive(Debug, Clone)]
pub struct PendingToolCall {
    pub name: String,
    pub params: std::collections::HashMap<String, String>,
    pub path: String,
    pub is_write: bool,
}

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
    /// Awaiting user permission for a tool call
    AwaitingPermission(PendingToolCall),
    /// Executing a tool
    ExecutingTool(String),
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
}

/// Text selection state for copy/paste
#[derive(Debug, Clone)]
pub struct TextSelection {
    /// Start position (row, col) in visual coordinates relative to chat area
    pub start: (u16, u16),
    /// End position (row, col) in visual coordinates relative to chat area
    pub end: (u16, u16),
    /// Whether user is currently dragging to select
    pub is_selecting: bool,
}

impl TextSelection {
    /// Create a new selection starting at the given position
    pub fn new(row: u16, col: u16) -> Self {
        Self {
            start: (row, col),
            end: (row, col),
            is_selecting: true,
        }
    }

    /// Update the end position of the selection
    pub fn update(&mut self, row: u16, col: u16) {
        self.end = (row, col);
    }

    /// Finish selection (stop dragging)
    pub fn finish(&mut self) {
        self.is_selecting = false;
    }

    /// Get normalized start and end (start <= end)
    pub fn normalized(&self) -> ((u16, u16), (u16, u16)) {
        let (start_row, start_col) = self.start;
        let (end_row, end_col) = self.end;

        if start_row < end_row || (start_row == end_row && start_col <= end_col) {
            (self.start, self.end)
        } else {
            (self.end, self.start)
        }
    }

    /// Check if a position is within the selection
    #[allow(dead_code)]
    pub fn contains(&self, row: u16, col: u16) -> bool {
        let ((start_row, start_col), (end_row, end_col)) = self.normalized();

        if row < start_row || row > end_row {
            return false;
        }

        if start_row == end_row {
            // Single line selection
            col >= start_col && col < end_col
        } else if row == start_row {
            // First line of multi-line selection
            col >= start_col
        } else if row == end_row {
            // Last line of multi-line selection
            col < end_col
        } else {
            // Middle lines are fully selected
            true
        }
    }
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
    #[allow(dead_code)]
    pub fn new(max_tokens: usize) -> Self {
        Self {
            tokens_used: 0,
            max_tokens,
            message_count: 0,
            rag_messages: 0,
        }
    }

    /// Get usage as a percentage (0.0 to 1.0)
    #[allow(dead_code)]
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
    /// Context usage information
    pub context_info: ContextInfo,
    /// Cache for chat view formatted lines
    pub chat_cache: ChatViewCache,
    /// Time when generation started (for elapsed time display)
    pub generation_start: Option<Instant>,
    /// Number of tokens received in current generation
    pub token_count: usize,
    /// Current text selection (for copy/paste)
    pub selection: Option<TextSelection>,
    /// Bounds of the chat area (for coordinate translation)
    pub chat_area: Option<ratatui::layout::Rect>,
    /// Session start time (for session timer)
    #[allow(dead_code)]
    pub session_start: Instant,
    /// Total input tokens sent this session
    pub total_input_tokens: usize,
    /// Total output tokens received this session
    pub total_output_tokens: usize,
    /// Queue of pending tool calls to execute
    pub pending_tool_calls: std::collections::VecDeque<crate::tools::ToolCall>,
    /// Selected permission option (0=Allow Once, 1=Always, 2=Deny)
    pub permission_selection: usize,
}

impl App {
    /// Create a new application
    pub fn new() -> Self {
        Self {
            state: AppState::Loading,
            messages: Vec::new(),
            current_response: String::new(),
            input_box: InputBox::new(),
            scroll_offset: 0,
            should_quit: false,
            tick: 0,
            auto_scroll: true,
            content_height: 0,
            view_height: 0,
            welcome_message: String::new(),
            model_ready: false,
            matrix_columns: Vec::new(),
            download_progress: None,
            context_info: ContextInfo::default(),
            chat_cache: ChatViewCache::new(),
            generation_start: None,
            token_count: 0,
            selection: None,
            chat_area: None,
            session_start: Instant::now(),
            total_input_tokens: 0,
            total_output_tokens: 0,
            pending_tool_calls: std::collections::VecDeque::new(),
            permission_selection: 0,
        }
    }

    /// Set the maximum context size
    pub fn set_max_context(&mut self, max_tokens: usize) {
        self.context_info.max_tokens = max_tokens;
    }

    /// Update context usage based on actual context messages sent to model
    /// Uses a simple heuristic: ~4 chars per token
    pub fn update_context_usage_from_context(&mut self, context_messages: &[crate::model::ChatMessage], rag_messages: usize) {
        let mut total_chars = 0;
        let mut msg_count = 0;
        for msg in context_messages {
            total_chars += msg.content.len();
            if msg.role != crate::model::Role::System {
                msg_count += 1;
            }
        }

        // Approximate tokens (roughly 4 chars per token for English)
        self.context_info.tokens_used = total_chars / 4;
        self.context_info.message_count = msg_count;
        self.context_info.rag_messages = rag_messages;
    }

    /// Update context usage (simple version, estimates from app.messages)
    pub fn update_context_usage(&mut self, rag_messages: usize) {
        // Count only non-system messages for the display
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
        self.generation_start = Some(Instant::now());
        self.token_count = 0;
    }

    /// Append a token to the current response
    pub fn append_token(&mut self, token: String) {
        self.current_response.push_str(&token);
        self.token_count += 1;
    }

    /// Finish generating and commit the response
    pub fn finish_generating(&mut self) {
        if !self.current_response.is_empty() {
            self.messages
                .push(ChatMessage::assistant(&self.current_response));
        }
        self.current_response.clear();
        self.generation_start = None;
        // Add output tokens to session total before resetting
        self.total_output_tokens += self.token_count;
        self.token_count = 0;
        self.state = AppState::Idle;
    }

    /// Cancel the current generation (Escape key)
    pub fn cancel_generation(&mut self) {
        if !matches!(self.state, AppState::Generating) {
            return;
        }
        // Commit partial response with cancellation marker
        if !self.current_response.is_empty() {
            let cancelled_response = format!("{}\n\n[Cancelled]", self.current_response);
            self.messages.push(ChatMessage::assistant(&cancelled_response));
        }
        self.current_response.clear();
        self.generation_start = None;
        // Add output tokens to session total before resetting
        self.total_output_tokens += self.token_count;
        self.token_count = 0;
        self.state = AppState::Idle;
    }

    /// Get elapsed time since generation started (in seconds)
    #[allow(dead_code)]
    pub fn generation_elapsed_secs(&self) -> u64 {
        self.generation_start
            .map(|t| t.elapsed().as_secs())
            .unwrap_or(0)
    }

    /// Get session elapsed time in seconds
    #[allow(dead_code)]
    pub fn session_elapsed_secs(&self) -> u64 {
        self.session_start.elapsed().as_secs()
    }

    /// Add input tokens to session total
    pub fn add_input_tokens(&mut self, tokens: usize) {
        self.total_input_tokens += tokens;
    }

    /// Set error state
    pub fn set_error(&mut self, error: String) {
        self.state = AppState::Error(error);
    }

    /// Set to idle state
    pub fn set_idle(&mut self) {
        self.state = AppState::Idle;
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
    #[allow(dead_code)]
    pub fn scroll_to_bottom(&mut self) {
        let max_scroll = self.content_height.saturating_sub(self.view_height);
        self.scroll_offset = max_scroll;
        self.auto_scroll = true;
    }

    /// Start a new text selection at the given terminal coordinates
    pub fn start_selection(&mut self, col: u16, row: u16) {
        // Translate terminal coordinates to chat area coordinates
        if let Some(area) = self.chat_area {
            if col >= area.x && col < area.x + area.width
                && row >= area.y && row < area.y + area.height
            {
                let rel_col = col - area.x;
                let rel_row = row - area.y + self.scroll_offset;
                self.selection = Some(TextSelection::new(rel_row, rel_col));
            }
        }
    }

    /// Update the current text selection
    pub fn update_selection(&mut self, col: u16, row: u16) {
        if let (Some(ref mut sel), Some(area)) = (&mut self.selection, self.chat_area) {
            if col >= area.x && row >= area.y {
                let rel_col = col.saturating_sub(area.x);
                let rel_row = row.saturating_sub(area.y) + self.scroll_offset;
                sel.update(rel_row, rel_col);
            }
        }
    }

    /// Finish the current selection
    pub fn finish_selection(&mut self) {
        if let Some(ref mut sel) = self.selection {
            sel.finish();
        }
    }

    /// Clear the current selection
    pub fn clear_selection(&mut self) {
        self.selection = None;
    }

    /// Check if there is an active selection with content
    pub fn has_selection(&self) -> bool {
        if let Some(ref sel) = self.selection {
            let ((start_row, start_col), (end_row, end_col)) = sel.normalized();
            // Has content if not a zero-width selection
            start_row != end_row || start_col != end_col
        } else {
            false
        }
    }

    /// Set the chat area bounds (called during rendering)
    pub fn set_chat_area(&mut self, area: ratatui::layout::Rect) {
        self.chat_area = Some(area);
    }

    /// Set awaiting permission state
    pub fn set_awaiting_permission(&mut self, pending: PendingToolCall) {
        self.permission_selection = 0; // Reset to "Allow Once"
        self.state = AppState::AwaitingPermission(pending);
    }

    /// Set executing tool state
    pub fn set_executing_tool(&mut self, tool_name: &str) {
        self.state = AppState::ExecutingTool(tool_name.to_string());
    }

    /// Add a tool result message
    pub fn add_tool_result(&mut self, tool_name: &str, result: &str, success: bool) {
        let content = if success {
            format!("[Tool: {}]\n{}", tool_name, result)
        } else {
            format!("[Tool: {} - Error]\n{}", tool_name, result)
        };
        self.messages.push(ChatMessage::tool_result(tool_name, content));
    }

    /// Cycle permission selection (for keyboard navigation)
    pub fn next_permission_option(&mut self) {
        self.permission_selection = (self.permission_selection + 1) % 3;
    }

    /// Cycle permission selection backwards
    pub fn prev_permission_option(&mut self) {
        self.permission_selection = if self.permission_selection == 0 {
            2
        } else {
            self.permission_selection - 1
        };
    }

    /// Check if awaiting permission
    pub fn is_awaiting_permission(&self) -> bool {
        matches!(self.state, AppState::AwaitingPermission(_))
    }

    /// Check if executing tool
    pub fn is_executing_tool(&self) -> bool {
        matches!(self.state, AppState::ExecutingTool(_))
    }
}
