use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

use crate::permissions::PermissionResponse;

/// A modal prompt for tool permission requests
pub struct PermissionPrompt<'a> {
    /// Tool name
    tool_name: &'a str,
    /// Tool parameters (e.g., URL for fetch)
    tool_params: &'a str,
    /// The key being used for permission matching (e.g., domain)
    permission_key: &'a str,
    /// Currently selected option (0-3)
    selected: usize,
}

impl<'a> PermissionPrompt<'a> {
    pub fn new(tool_name: &'a str, tool_params: &'a str, permission_key: &'a str) -> Self {
        Self {
            tool_name,
            tool_params,
            permission_key,
            selected: 0,
        }
    }

    pub fn with_selected(mut self, selected: usize) -> Self {
        self.selected = selected.min(3);
        self
    }

    /// Get the permission response for the current selection
    pub fn get_selected_response(&self) -> PermissionResponse {
        match self.selected {
            0 => PermissionResponse::AllowOnce,
            1 => PermissionResponse::AllowAlways,
            2 => PermissionResponse::DenyOnce,
            3 => PermissionResponse::DenyAlways,
            _ => PermissionResponse::DenyOnce,
        }
    }
}

impl Widget for PermissionPrompt<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Calculate centered popup area
        let popup_width = 60.min(area.width.saturating_sub(4));
        let popup_height = 14.min(area.height.saturating_sub(2));

        let popup_x = (area.width.saturating_sub(popup_width)) / 2;
        let popup_y = (area.height.saturating_sub(popup_height)) / 2;

        let popup_area = Rect::new(
            area.x + popup_x,
            area.y + popup_y,
            popup_width,
            popup_height,
        );

        // Clear the area behind the popup
        Clear.render(popup_area, buf);

        // Draw border
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow))
            .title(" \u{26A0} Permission Required ")
            .title_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));

        let inner = block.inner(popup_area);
        block.render(popup_area, buf);

        // Build content
        let mut lines = Vec::new();

        // Tool info
        lines.push(Line::from(vec![
            Span::styled("Tool: ", Style::default().fg(Color::DarkGray)),
            Span::styled(self.tool_name, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        ]));

        // Truncate params if too long
        let max_param_len = (popup_width as usize).saturating_sub(10);
        let display_params = if self.tool_params.len() > max_param_len {
            format!("{}...", &self.tool_params[..max_param_len.saturating_sub(3)])
        } else {
            self.tool_params.to_string()
        };

        lines.push(Line::from(vec![
            Span::styled("Target: ", Style::default().fg(Color::DarkGray)),
            Span::styled(display_params, Style::default().fg(Color::White)),
        ]));

        lines.push(Line::from(""));

        // Permission key info
        lines.push(Line::from(vec![
            Span::styled("Permission for: ", Style::default().fg(Color::DarkGray)),
            Span::styled(self.permission_key, Style::default().fg(Color::Magenta)),
        ]));

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Select an option:",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(""));

        // Options
        let options = [
            ("Allow once", "y"),
            ("Always allow", "a"),
            ("Deny once", "n"),
            ("Never allow", "!"),
        ];

        for (i, (label, key)) in options.iter().enumerate() {
            let is_selected = i == self.selected;
            let prefix = if is_selected { "\u{25B6} " } else { "  " };
            let style = if is_selected {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            lines.push(Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(format!("[{}] ", key), Style::default().fg(Color::Yellow)),
                Span::styled(*label, style),
            ]));
        }

        // Render content
        let paragraph = Paragraph::new(lines).alignment(Alignment::Left);
        paragraph.render(inner, buf);
    }
}
