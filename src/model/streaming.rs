/// Events emitted during streaming inference
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// A new token was generated
    Token(String),
    /// Generation completed
    Done,
    /// An error occurred
    Error(String),
}
