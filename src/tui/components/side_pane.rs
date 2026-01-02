use std::sync::{Arc, RwLock};

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};

use crate::app::ContextInfo;
use crate::tools::TodoList;

/// Side pane showing context, tools, and todo list
pub struct SidePane<'a> {
    /// Context usage information
    context_info: &'a ContextInfo,
    /// Available tool names
    tool_names: &'a [String],
    /// Currently active tool (if any)
    active_tool: Option<&'a str>,
    /// Shared todo list
    todo_list: &'a Arc<RwLock<TodoList>>,
}

impl<'a> SidePane<'a> {
    pub fn new(
        context_info: &'a ContextInfo,
        tool_names: &'a [String],
        active_tool: Option<&'a str>,
        todo_list: &'a Arc<RwLock<TodoList>>,
    ) -> Self {
        Self {
            context_info,
            tool_names,
            active_tool,
            todo_list,
        }
    }

    /// Format a number with K/M suffix
    fn format_number(n: usize) -> String {
        if n >= 1_000_000 {
            format!("{:.1}M", n as f64 / 1_000_000.0)
        } else if n >= 1_000 {
            format!("{:.1}K", n as f64 / 1_000.0)
        } else {
            n.to_string()
        }
    }

    /// Create a simple progress bar
    fn progress_bar(width: usize, percent: f64) -> String {
        let filled = ((width as f64) * percent).round() as usize;
        let empty = width.saturating_sub(filled);
        format!("{}{}", "█".repeat(filled), "░".repeat(empty))
    }
}

impl Widget for SidePane<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(" Context ");

        let inner = block.inner(area);
        block.render(area, buf);

        let mut lines = Vec::new();

        // Context usage section
        lines.push(Line::from(vec![
            Span::styled(
                "\u{1F4CA} Usage",  // 📊
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        // Token usage with progress bar
        let percent = self.context_info.usage_percent();
        let bar_width = (inner.width as usize).saturating_sub(2).min(20);
        let bar = Self::progress_bar(bar_width, percent);

        // Color based on usage level
        let bar_color = if percent > 0.9 {
            Color::Red
        } else if percent > 0.7 {
            Color::Yellow
        } else {
            Color::Green
        };

        lines.push(Line::from(vec![
            Span::styled(bar, Style::default().fg(bar_color)),
        ]));

        // Token count
        let used = Self::format_number(self.context_info.tokens_used);
        let max = Self::format_number(self.context_info.max_tokens);
        lines.push(Line::from(vec![
            Span::styled(
                format!("{}/{} tokens", used, max),
                Style::default().fg(Color::DarkGray),
            ),
        ]));

        // Message count
        let msg_info = if self.context_info.rag_messages > 0 {
            format!(
                "{} msgs ({} RAG)",
                self.context_info.message_count,
                self.context_info.rag_messages
            )
        } else {
            format!("{} messages", self.context_info.message_count)
        };
        lines.push(Line::from(vec![
            Span::styled(msg_info, Style::default().fg(Color::DarkGray)),
        ]));

        lines.push(Line::from(""));

        // Tools section
        lines.push(Line::from(vec![
            Span::styled(
                "\u{2699} Tools",  // ⚙
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        for tool_name in self.tool_names {
            let is_active = self.active_tool == Some(tool_name.as_str());
            let (icon, style) = if is_active {
                ("\u{25B6} ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)) // ▶
            } else {
                ("  ", Style::default().fg(Color::DarkGray))
            };

            // Tool icons
            let tool_icon = match tool_name.as_str() {
                "fetch" => "\u{1F310} ", // 🌐
                "search" => "\u{1F50D} ", // 🔍
                "fs" => "\u{1F4C1} ", // 📁
                "todo" => "\u{2611} ", // ☑
                _ => "\u{2022} ", // •
            };

            lines.push(Line::from(vec![
                Span::styled(icon, style),
                Span::styled(tool_icon.to_string(), style),
                Span::styled(tool_name.clone(), style),
            ]));
        }

        lines.push(Line::from(""));

        // Todo section
        lines.push(Line::from(vec![
            Span::styled(
                "\u{2611} Tasks",  // ☑
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        // Read todo list
        if let Ok(list) = self.todo_list.read() {
            let items = list.items();
            if items.is_empty() {
                lines.push(Line::from(Span::styled(
                    "  No tasks yet",
                    Style::default().fg(Color::DarkGray),
                )));
            } else {
                let max_width = inner.width.saturating_sub(5) as usize;
                for item in items {
                    let (checkbox, style) = if item.completed {
                        ("[x] ", Style::default().fg(Color::Green))
                    } else {
                        ("[ ] ", Style::default().fg(Color::Yellow))
                    };

                    // Truncate description if too long
                    let desc = if item.description.len() > max_width {
                        format!("{}...", &item.description[..max_width.saturating_sub(3)])
                    } else {
                        item.description.clone()
                    };

                    lines.push(Line::from(vec![
                        Span::styled(checkbox, style),
                        Span::styled(
                            desc,
                            if item.completed {
                                Style::default().fg(Color::DarkGray).add_modifier(Modifier::CROSSED_OUT)
                            } else {
                                Style::default().fg(Color::White)
                            },
                        ),
                    ]));
                }

                // Summary
                let completed = items.iter().filter(|i| i.completed).count();
                let total = items.len();
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    format!("{}/{} done", completed, total),
                    Style::default().fg(Color::DarkGray),
                )));
            }
        } else {
            lines.push(Line::from(Span::styled(
                "  Loading...",
                Style::default().fg(Color::DarkGray),
            )));
        }

        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true });
        paragraph.render(inner, buf);
    }
}
