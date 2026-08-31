use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Widget, Wrap},
};

use crate::app::{App, AppState};
use crate::tui::theme;

/// Context panel inspired by coding-agent sidebars.
pub struct ActivityPanel<'a> {
    app: &'a App,
}

impl<'a> ActivityPanel<'a> {
    pub fn new(app: &'a App) -> Self {
        Self { app }
    }
}

impl Widget for ActivityPanel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let state = match &self.app.state {
            AppState::Idle => "idle",
            AppState::Generating => "thinking",
            AppState::Downloading => "downloading",
            AppState::Loading => "loading",
            AppState::AwaitingPermission => "permission",
            AppState::ExecutingTool(_) => "tool",
            AppState::Error(_) => "error",
        };

        let lines = vec![
            Line::from(vec![
                Span::styled("PHASE ", Style::default().fg(theme::MUTED)),
                Span::styled("1", Style::default().fg(theme::PRIMARY).add_modifier(Modifier::BOLD)),
                Span::styled("  READ TOOLS", Style::default().fg(theme::INFO)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("State: ", Style::default().fg(theme::MUTED)),
                Span::styled(state, Style::default().fg(theme::SUCCESS)),
            ]),
            Line::from(vec![
                Span::styled("Messages: ", Style::default().fg(theme::MUTED)),
                Span::styled(self.app.messages.len().to_string(), Style::default().fg(theme::INFO)),
            ]),
            Line::from(vec![
                Span::styled("Context: ", Style::default().fg(theme::MUTED)),
                Span::styled(
                    format!("{}/{}", self.app.context_info.tokens_used, self.app.context_info.max_tokens),
                    Style::default().fg(theme::INFO),
                ),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "Read tools enabled",
                Style::default().fg(theme::PRIMARY).add_modifier(Modifier::BOLD),
            )),
            Line::from("• list_files"),
            Line::from("• read_file"),
            Line::from("• grep"),
            Line::from(""),
            Line::from(Span::styled(
                "Slash commands",
                Style::default().fg(theme::PRIMARY).add_modifier(Modifier::BOLD),
            )),
            Line::from("• /clear"),
            Line::from("• /new"),
            Line::from("• /help"),
            Line::from(""),
            Line::from(Span::styled(
                "Tips",
                Style::default().fg(theme::PRIMARY).add_modifier(Modifier::BOLD),
            )),
            Line::from("• Tab to autocomplete /commands"),
            Line::from("• Esc to cancel generation"),
            Line::from("• Ctrl+U / Ctrl+D to scroll"),
        ];

        let panel = Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(" Activity ")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(theme::MUTED)),
            );

        panel.render(area, buf);
    }
}
