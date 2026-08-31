//! Compact top header for coding workflow context.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Widget},
};

use crate::app::AppState;
use crate::tui::theme;

/// Spinner frames for active generation.
const SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub struct Header<'a> {
    state: &'a AppState,
    tick: u64,
}

impl<'a> Header<'a> {
    pub fn new(state: &'a AppState, tick: u64) -> Self {
        Self { state, tick }
    }
}

impl Widget for Header<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }

        let spinner_frame = SPINNER[(self.tick as usize / 2) % SPINNER.len()];
        let mode = match self.state {
            AppState::Idle => "Ready",
            AppState::Generating => "Generating",
            AppState::Downloading => "Downloading model",
            AppState::Loading => "Loading model",
            AppState::AwaitingPermission => "Awaiting permission",
            AppState::ExecutingTool(_) => "Executing tool",
            AppState::Error(_) => "Error",
        };

        let title = vec![
            Span::styled(
                " PANGU ",
                Style::default()
                    .fg(theme::BG)
                    .bg(theme::PRIMARY)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" local coding agent ", Style::default().fg(theme::MUTED)),
            Span::styled(" // ", Style::default().fg(theme::MUTED)),
            Span::styled(
                if matches!(self.state, AppState::Generating) {
                    format!("{spinner_frame} {mode}")
                } else {
                    mode.to_string()
                },
                Style::default().fg(theme::INFO),
            ),
        ];

        let subtitle = Line::from(vec![
            Span::styled("Phase 1", Style::default().fg(theme::SUCCESS).add_modifier(Modifier::BOLD)),
            Span::styled("  Read tools only  ", Style::default().fg(theme::MUTED)),
            Span::styled("Esc", Style::default().fg(theme::WARNING)),
            Span::styled(" cancel  ", Style::default().fg(theme::MUTED)),
            Span::styled("Tab", Style::default().fg(theme::WARNING)),
            Span::styled(" autocomplete", Style::default().fg(theme::MUTED)),
        ]);

        let paragraph = Paragraph::new(vec![Line::from(title), subtitle]).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(theme::MUTED))
                .title(" Workspace "),
        );

        paragraph.render(area, buf);
    }
}

pub fn header_height(_state: &AppState) -> u16 {
    3
}
