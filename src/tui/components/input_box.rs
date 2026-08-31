use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders},
    Frame,
    widgets::Paragraph,
};
use tui_textarea::TextArea;

use crate::tui::theme;

#[derive(Debug, Clone, Copy)]
struct SlashCommand {
    name: &'static str,
    description: &'static str,
}

const SLASH_COMMANDS: &[SlashCommand] = &[
    SlashCommand {
        name: "/clear",
        description: "clear conversation state",
    },
    SlashCommand {
        name: "/new",
        description: "start a new conversation",
    },
    SlashCommand {
        name: "/help",
        description: "show command help in chat",
    },
];

/// Input widget for user messages
pub struct InputBox {
    textarea: TextArea<'static>,
    suggestions: Vec<usize>,
    selected_suggestion: usize,
}

impl Default for InputBox {
    fn default() -> Self {
        Self::new()
    }
}

impl InputBox {
    fn styled_textarea() -> TextArea<'static> {
        let mut textarea = TextArea::default();
        textarea.set_cursor_line_style(Style::default());
        textarea.set_placeholder_text("Ask anything...  Enter=send  Alt+Enter=newline  Tab=command autocomplete");
        textarea.set_placeholder_style(Style::default().fg(theme::MUTED));
        textarea.set_block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(theme::MUTED))
                .title(" Prompt "),
        );
        textarea
    }

    pub fn new() -> Self {
        Self {
            textarea: Self::styled_textarea(),
            suggestions: Vec::new(),
            selected_suggestion: 0,
        }
    }

    fn refresh_autocomplete(&mut self) {
        self.suggestions.clear();
        self.selected_suggestion = 0;

        let text = self.text();
        let trimmed_start = text.trim_start();

        if !trimmed_start.starts_with('/') || trimmed_start.contains('\n') {
            return;
        }

        let first = trimmed_start.split_whitespace().next().unwrap_or("");
        if first.is_empty() || !first.starts_with('/') {
            return;
        }

        let query = &first[1..].to_ascii_lowercase();
        for (idx, cmd) in SLASH_COMMANDS.iter().enumerate() {
            let candidate = cmd.name.trim_start_matches('/').to_ascii_lowercase();
            if candidate.starts_with(query) {
                self.suggestions.push(idx);
            }
        }
    }

    fn apply_selected_suggestion(&mut self) {
        if self.suggestions.is_empty() {
            return;
        }
        let idx = self.suggestions[self.selected_suggestion];
        let command = SLASH_COMMANDS[idx].name;
        self.set_text(&format!("{command} "));
    }

    fn clear_and_reset(&mut self) {
        self.textarea = Self::styled_textarea();
        self.suggestions.clear();
        self.selected_suggestion = 0;
    }

    /// Handle keyboard input
    ///
    /// Returns Some(text) if the user submitted the message (Enter without Shift)
    pub fn handle_input(&mut self, key: KeyEvent) -> Option<String> {
        match key.code {
            KeyCode::Tab => {
                if !self.suggestions.is_empty() {
                    self.apply_selected_suggestion();
                    return None;
                }
            }
            KeyCode::Up if !self.suggestions.is_empty() => {
                self.selected_suggestion = if self.selected_suggestion == 0 {
                    self.suggestions.len().saturating_sub(1)
                } else {
                    self.selected_suggestion - 1
                };
                return None;
            }
            KeyCode::Down if !self.suggestions.is_empty() => {
                self.selected_suggestion = (self.selected_suggestion + 1) % self.suggestions.len();
                return None;
            }
            _ => {}
        }

        match key.code {
            // Alt+Enter or Shift+Enter = insert newline
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::ALT) => {
                self.textarea.insert_newline();
                self.refresh_autocomplete();
                None
            }
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.textarea.insert_newline();
                self.refresh_autocomplete();
                None
            }
            // Some terminals send modified Enter as Char('\n') or Char('\r')
            KeyCode::Char('\n') | KeyCode::Char('\r')
                if key.modifiers.contains(KeyModifiers::SHIFT)
                || key.modifiers.contains(KeyModifiers::ALT) => {
                self.textarea.insert_newline();
                self.refresh_autocomplete();
                None
            }
            // Enter without modifiers = submit
            KeyCode::Enter => {
                if !self.suggestions.is_empty() {
                    self.apply_selected_suggestion();
                    return None;
                }
                let text = self.textarea.lines().join("\n");
                if text.trim().is_empty() {
                    return None;
                }
                // Clear the textarea
                self.clear_and_reset();
                Some(text)
            }
            // Everything else goes to textarea
            _ => {
                self.textarea.input(key);
                self.refresh_autocomplete();
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
        self.textarea
            .set_placeholder_text("Ask anything...  Enter=send  Alt+Enter=newline  Tab=command autocomplete");
        self.textarea.set_placeholder_style(Style::default().fg(theme::MUTED));
        self.textarea.set_block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(theme::MUTED))
                .title(" Prompt "),
        );
        self.refresh_autocomplete();
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
        self.refresh_autocomplete();
    }

    /// Check if the input is empty
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.textarea.lines().iter().all(|l| l.is_empty())
    }

    /// Render the input box
    pub fn render(&self, area: Rect, frame: &mut Frame) {
        if self.suggestions.is_empty() || area.height < 4 {
            frame.render_widget(&self.textarea, area);
            return;
        }

        let hint_height = (self.suggestions.len() as u16 + 1)
            .min(area.height.saturating_sub(3))
            .max(2);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(hint_height)])
            .split(area);

        frame.render_widget(&self.textarea, chunks[0]);

        let mut hint_lines = vec![Line::from(Span::styled(
            " Commands",
            Style::default().fg(theme::MUTED),
        ))];

        for (idx, cmd_idx) in self.suggestions.iter().enumerate() {
            let command = SLASH_COMMANDS[*cmd_idx];
            let (prefix, style) = if idx == self.selected_suggestion {
                ("> ", Style::default().fg(theme::PRIMARY))
            } else {
                ("  ", Style::default().fg(theme::INFO))
            };

            hint_lines.push(Line::from(vec![
                Span::styled(prefix, Style::default().fg(theme::PRIMARY)),
                Span::styled(command.name, style),
                Span::styled(
                    format!(" — {}", command.description),
                    Style::default().fg(theme::MUTED),
                ),
            ]));
        }

        let hint = Paragraph::new(hint_lines).block(
            Block::default()
                .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(theme::MUTED)),
        );
        frame.render_widget(hint, chunks[1]);
    }
}
