use crate::permissions::PermissionResponse;

/// Actions that can be performed on the application state
#[derive(Debug, Clone)]
pub enum Action {
    /// Quit the application
    Quit,
    /// Submit user message
    SubmitMessage(String),
    /// Append streaming token
    AppendToken(String),
    /// Finish streaming
    FinishStreaming,
    /// Set error state
    SetError(String),
    /// Scroll up
    ScrollUp(u16),
    /// Scroll down
    ScrollDown(u16),
    /// Model loaded successfully
    ModelLoaded,
    /// Permission prompt navigation - move up
    PermissionSelectPrev,
    /// Permission prompt navigation - move down
    PermissionSelectNext,
    /// Confirm permission selection
    PermissionConfirm,
    /// Direct permission response (from keyboard shortcut)
    PermissionRespond(PermissionResponse),
    /// No-op (for unhandled events)
    None,
}
