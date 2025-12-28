use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};

use crate::model::{ChatMessage, Role};

/// Widget for displaying chat messages
pub struct ChatView<'a> {
    messages: &'a [ChatMessage],
    current_response: &'a str,
    scroll: u16,
    is_generating: bool,
}

impl<'a> ChatView<'a> {
    pub fn new(
        messages: &'a [ChatMessage],
        current_response: &'a str,
        scroll: u16,
        is_generating: bool,
    ) -> Self {
        Self {
            messages,
            current_response,
            scroll,
            is_generating,
        }
    }

    fn format_messages(&self) -> Vec<Line<'a>> {
        let mut lines = Vec::new();

        for msg in self.messages.iter().filter(|m| m.role != Role::System) {
            // Add role header
            let (role_text, role_style) = match msg.role {
                Role::User => (
                    "You",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Role::Assistant => (
                    "Pangu",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Role::System => continue,
            };

            lines.push(Line::from(vec![Span::styled(
                format!("{}", role_text),
                role_style,
            )]));

            // Add message content
            for line in msg.content.lines() {
                lines.push(Line::from(line.to_string()));
            }

            // Add separator
            lines.push(Line::from(""));
        }

        // Add current streaming response if any
        if !self.current_response.is_empty() || self.is_generating {
            lines.push(Line::from(vec![Span::styled(
                "Pangu",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )]));

            for line in self.current_response.lines() {
                lines.push(Line::from(line.to_string()));
            }

            // Show cursor while generating
            if self.is_generating {
                if self.current_response.is_empty() || self.current_response.ends_with('\n') {
                    lines.push(Line::from(vec![Span::styled(
                        "...",
                        Style::default()
                            .fg(Color::DarkGray)
                            .add_modifier(Modifier::SLOW_BLINK),
                    )]));
                }
            }
        }

        lines
    }
}

impl Widget for ChatView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let lines = self.format_messages();

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(" Chat ");

        let paragraph = Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((self.scroll, 0));

        paragraph.render(area, buf);
    }
}
