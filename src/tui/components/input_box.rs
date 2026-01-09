use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders},
    Frame,
};
use tui_textarea::TextArea;

/// Input widget for user messages
pub struct InputBox {
    textarea: TextArea<'static>,
}

impl Default for InputBox {
    fn default() -> Self {
        Self::new()
    }
}

impl InputBox {
    pub fn new() -> Self {
        let mut textarea = TextArea::default();
        textarea.set_cursor_line_style(Style::default());
        textarea.set_block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(" Message (Enter to send, Alt+Enter for newline) "),
        );
        Self { textarea }
    }

    /// Handle keyboard input
    ///
    /// Returns Some(text) if the user submitted the message (Enter without Shift)
    pub fn handle_input(&mut self, key: KeyEvent) -> Option<String> {
        match key.code {
            // Alt+Enter or Shift+Enter = insert newline
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::ALT) => {
                self.textarea.insert_newline();
                None
            }
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.textarea.insert_newline();
                None
            }
            // Some terminals send modified Enter as Char('\n') or Char('\r')
            KeyCode::Char('\n') | KeyCode::Char('\r')
                if key.modifiers.contains(KeyModifiers::SHIFT)
                || key.modifiers.contains(KeyModifiers::ALT) => {
                self.textarea.insert_newline();
                None
            }
            // Enter without modifiers = submit
            KeyCode::Enter => {
                let text = self.textarea.lines().join("\n");
                if text.trim().is_empty() {
                    return None;
                }
                // Clear the textarea
                self.textarea = TextArea::default();
                self.textarea.set_cursor_line_style(Style::default());
                self.textarea.set_block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::DarkGray))
                        .title(" Message (Enter to send, Alt+Enter for newline) "),
                );
                Some(text)
            }
            // Everything else goes to textarea
            _ => {
                self.textarea.input(key);
                None
            }
        }
    }

    /// Get the current input text
    #[allow(dead_code)]
    pub fn text(&self) -> String {
        self.textarea.lines().join("\n")
    }

    /// Set the input text
    pub fn set_text(&mut self, text: &str) {
        // Clear and set new text
        self.textarea = TextArea::from(text.lines().collect::<Vec<_>>());
        self.textarea.set_cursor_line_style(Style::default());
        self.textarea.set_block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(" Message (Enter to send, Alt+Enter for newline) "),
        );
    }

    /// Insert text at cursor position (used for paste)
    pub fn insert_text(&mut self, text: &str) {
        // Insert each character, handling newlines properly
        for c in text.chars() {
            if c == '\n' {
                self.textarea.insert_newline();
            } else {
                self.textarea.insert_char(c);
            }
        }
    }

    /// Check if the input is empty
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.textarea.lines().iter().all(|l| l.is_empty())
    }

    /// Render the input box
    pub fn render(&self, area: Rect, frame: &mut Frame) {
        frame.render_widget(&self.textarea, area);
    }
}
