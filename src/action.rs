/// Actions that can be performed on the application state
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum Action {
    /// Quit the application
    Quit,
    /// Submit user message
    SubmitMessage(String),
    /// Append streaming token
    AppendToken(String),
    /// Finish streaming
    FinishStreaming,
    /// Cancel current generation (Escape key)
    CancelGeneration,
    /// Set error state
    SetError(String),
    /// Scroll up
    ScrollUp(u16),
    /// Scroll down
    ScrollDown(u16),
    /// Model loaded successfully
    ModelLoaded,
    /// Start text selection at (col, row)
    SelectionStart(u16, u16),
    /// Update text selection to (col, row)
    SelectionUpdate(u16, u16),
    /// Finish text selection
    SelectionEnd,
    /// Copy selected text to clipboard
    CopySelection,
    /// Clear current selection
    ClearSelection,
    /// Add input tokens to session total
    AddInputTokens(usize),
    /// No-op (for unhandled events)
    None,
}
