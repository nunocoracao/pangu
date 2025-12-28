use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Widget},
};

use crate::app::AppState;

/// Spinner frames for loading animation
const SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Status bar showing model info and current state
pub struct StatusBar<'a> {
    model_name: &'a str,
    state: &'a AppState,
    tick: u64,
}

impl<'a> StatusBar<'a> {
    pub fn new(model_name: &'a str, state: &'a AppState, tick: u64) -> Self {
        Self { model_name, state, tick }
    }
}

impl Widget for StatusBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let spinner_frame = SPINNER[(self.tick as usize / 2) % SPINNER.len()];

        let (status_text, status_style) = match self.state {
            AppState::Idle => ("Ready".to_string(), Style::default().fg(Color::Green)),
            AppState::Generating => (
                format!("{} Generating...", spinner_frame),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            AppState::Loading => (
                format!("{} Loading model (~14GB, please wait)...", spinner_frame),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            AppState::Error(msg) => (
                msg.clone(),
                Style::default().fg(Color::Red),
            ),
        };

        let line = Line::from(vec![
            Span::styled(
                format!(" {} ", self.model_name),
                Style::default()
                    .fg(Color::White)
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(status_text, status_style),
            Span::raw(" | "),
            Span::styled(
                "Ctrl+C: quit",
                Style::default().fg(Color::DarkGray),
            ),
            Span::raw(" | "),
            Span::styled(
                "Ctrl+U/D: scroll",
                Style::default().fg(Color::DarkGray),
            ),
        ]);

        let paragraph = Paragraph::new(line).block(Block::default());
        paragraph.render(area, buf);
    }
}
