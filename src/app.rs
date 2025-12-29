use crate::config::{load_welcome_message, Settings};
use crate::model::ChatMessage;
use crate::tui::components::InputBox;

/// Application state enum
#[derive(Debug, Clone)]
pub enum AppState {
    /// Waiting for user input
    Idle,
    /// Model is generating a response
    Generating,
    /// Loading the model
    Loading,
    /// Error state
    Error(String),
}

impl AppState {
    pub fn is_generating(&self) -> bool {
        matches!(self, AppState::Generating)
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, AppState::Loading)
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
}

impl App {
    /// Create a new application with the given settings
    pub fn new(settings: &Settings) -> Self {
        Self {
            state: AppState::Loading,
            messages: vec![ChatMessage::system(&settings.system.prompt)],
            current_response: String::new(),
            input_box: InputBox::new(),
            scroll_offset: 0,
            should_quit: false,
            tick: 0,
            auto_scroll: true,
            content_height: 0,
            view_height: 0,
            welcome_message: load_welcome_message(settings),
            model_ready: false,
        }
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
}
