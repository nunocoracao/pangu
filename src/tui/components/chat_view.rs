use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};

use crate::model::{ChatMessage, Role};
use crate::tui::markdown::MarkdownRenderer;

/// Widget for displaying chat messages
pub struct ChatView<'a> {
    messages: &'a [ChatMessage],
    current_response: &'a str,
    scroll: u16,
    is_generating: bool,
    is_loading: bool,
    tick: u64,
    renderer: MarkdownRenderer,
    welcome_message: &'a str,
}

impl<'a> ChatView<'a> {
    pub fn new(
        messages: &'a [ChatMessage],
        current_response: &'a str,
        scroll: u16,
        is_generating: bool,
        is_loading: bool,
        tick: u64,
        welcome_message: &'a str,
    ) -> Self {
        Self {
            messages,
            current_response,
            scroll,
            is_generating,
            is_loading,
            tick,
            renderer: MarkdownRenderer::new(),
            welcome_message,
        }
    }

    /// Get the total content height in lines
    pub fn content_height(&self) -> u16 {
        self.format_messages().len() as u16
    }

    fn format_messages(&self) -> Vec<Line<'static>> {
        let mut lines = Vec::new();

        // Always show welcome message at the top
        for line in self.welcome_message.lines() {
            lines.push(Line::from(Span::styled(
                line.to_string(),
                Style::default().fg(Color::Cyan),
            )));
        }

        // Show loading animation if model is loading
        if self.is_loading {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "─".repeat(40),
                Style::default().fg(Color::DarkGray),
            )));
            lines.push(Line::from(""));

            // Animated loading indicator
            let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            let frame_idx = (self.tick as usize) % frames.len();
            let spinner = frames[frame_idx];

            lines.push(Line::from(vec![
                Span::styled(
                    format!("{} ", spinner),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "Loading model...".to_string(),
                    Style::default().fg(Color::Yellow),
                ),
            ]));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Please wait while the LLM initializes.".to_string(),
                Style::default().fg(Color::DarkGray),
            )));
            return lines;
        }

        // Add separator after welcome message if there are messages
        let has_messages = self.messages.iter().any(|m| m.role != Role::System)
            || !self.current_response.is_empty()
            || self.is_generating;

        if has_messages {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "─".repeat(40),
                Style::default().fg(Color::DarkGray),
            )));
            lines.push(Line::from(""));
        }

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
                role_text.to_string(),
                role_style,
            )]));

            // Add message content
            match msg.role {
                Role::User => {
                    // User messages: plain text
                    for line in msg.content.lines() {
                        lines.push(Line::from(line.to_string()));
                    }
                }
                Role::Assistant => {
                    // Assistant messages: render as markdown
                    let rendered = self.renderer.render(&msg.content);
                    lines.extend(rendered);
                }
                Role::System => {}
            }

            // Add separator
            lines.push(Line::from(""));
        }

        // Add current streaming response if any
        if !self.current_response.is_empty() || self.is_generating {
            lines.push(Line::from(vec![Span::styled(
                "Pangu".to_string(),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )]));

            // Render streaming response as markdown
            if !self.current_response.is_empty() {
                let rendered = self.renderer.render(self.current_response);
                lines.extend(rendered);
            }

            // Show cursor while generating
            if self.is_generating {
                if self.current_response.is_empty() || self.current_response.ends_with('\n') {
                    lines.push(Line::from(vec![Span::styled(
                        "...".to_string(),
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
