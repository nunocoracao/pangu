use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};

/// Side pane showing tools and files info
pub struct SidePane;

impl SidePane {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SidePane {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for SidePane {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(" Context ");

        let inner = block.inner(area);
        block.render(area, buf);

        let lines = vec![
            Line::from(vec![
                Span::styled(
                    "Tools",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(Span::styled(
                "Coming soon...",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    "Files",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(Span::styled(
                "Coming soon...",
                Style::default().fg(Color::DarkGray),
            )),
        ];

        let paragraph = Paragraph::new(lines);
        paragraph.render(inner, buf);
    }
}
