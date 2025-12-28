use crate::config::Settings;
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
    /// Model name for display
    pub model_name: String,
    /// System prompt
    pub system_prompt: String,
    /// Tick counter for animations
    pub tick: u64,
}

impl App {
    /// Create a new application with the given settings
    pub fn new(settings: &Settings) -> Self {
        let model_name = settings
            .model
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        Self {
            state: AppState::Loading,
            messages: vec![ChatMessage::system(&settings.system.prompt)],
            current_response: String::new(),
            input_box: InputBox::new(),
            scroll_offset: 0,
            should_quit: false,
            model_name,
            system_prompt: settings.system.prompt.clone(),
            tick: 0,
        }
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
    }

    /// Scroll down in the chat view
    pub fn scroll_down(&mut self, amount: u16) {
        self.scroll_offset = self.scroll_offset.saturating_add(amount);
    }

    /// Reset scroll to bottom
    pub fn scroll_to_bottom(&mut self) {
        // For simplicity, we'll auto-scroll by setting a large offset
        // A more sophisticated implementation would calculate the actual content height
        self.scroll_offset = 0;
    }
}
