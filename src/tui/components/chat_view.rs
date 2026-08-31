use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Widget, Wrap},
};

use crate::app::TextSelection;
use crate::model::{ChatMessage, Role};
use crate::tui::markdown::MarkdownRenderer;
use crate::tui::theme;

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
    /// Width for rendering (set during render)
    render_width: Option<u16>,
    /// Optional pre-computed cached lines (skips format_messages if present)
    cached_lines: Option<&'a [Line<'static>]>,
    /// Optional text selection for highlighting
    selection: Option<&'a TextSelection>,
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
            render_width: None,
            cached_lines: None,
            selection: None,
        }
    }

    /// Set the text selection for highlighting
    pub fn with_selection(mut self, selection: Option<&'a TextSelection>) -> Self {
        self.selection = selection;
        self
    }

    /// Set the rendering width (for code blocks to extend to full width)
    pub fn with_width(mut self, width: u16) -> Self {
        self.render_width = Some(width);
        self
    }

    /// Use pre-computed cached lines instead of formatting
    pub fn with_cached_lines(mut self, lines: &'a [Line<'static>]) -> Self {
        self.cached_lines = Some(lines);
        self
    }

    /// Get the total content height in lines from cached lines
    pub fn content_height_from_lines(lines: &[Line<'static>], area_width: u16) -> u16 {
        let mut total_lines: u16 = 0;

        // Account for text wrapping - estimate wrapped lines
        let usable_width = area_width.saturating_sub(2) as usize; // minus borders
        for line in lines {
            // Use character count, not byte length (important for Unicode like ─)
            let line_len: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
            if usable_width > 0 && line_len > 0 {
                // Ceiling division to get number of wrapped lines
                let wrapped = ((line_len + usable_width - 1) / usable_width) as u16;
                total_lines += wrapped.max(1);
            } else {
                total_lines += 1;
            }
        }

        total_lines
    }

    /// Get the total content height in lines
    /// Note: This is an estimate since we can't know exact wrapped height without rendering
    #[allow(dead_code)]
    pub fn content_height(&mut self, area_width: u16) -> u16 {
        self.render_width = Some(area_width);

        // Use cached lines if available
        if let Some(cached) = self.cached_lines {
            return Self::content_height_from_lines(cached, area_width);
        }

        let lines = self.format_messages();
        Self::content_height_from_lines(&lines, area_width)
    }

    /// Format all messages into renderable lines
    /// This is the expensive operation that should be cached
    pub fn format_messages(&mut self) -> Vec<Line<'static>> {
        let mut lines = Vec::new();

        // Set width on renderer if available
        if let Some(width) = self.render_width {
            self.renderer.set_width(width);
        }

        // Always show welcome message at the top
        for line in self.welcome_message.lines() {
            lines.push(Line::from(Span::styled(
                line.to_string(),
                Style::default().fg(theme::MUTED),
            )));
        }

        // Show loading animation if model is loading
        if self.is_loading {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "─".repeat(40),
                Style::default().fg(theme::MUTED),
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
                        .fg(theme::PRIMARY)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "Loading model...".to_string(),
                    Style::default().fg(theme::WARNING),
                ),
            ]));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Please wait while the LLM initializes.".to_string(),
                Style::default().fg(theme::MUTED),
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
                Style::default().fg(theme::MUTED),
            )));
            lines.push(Line::from(""));
        }

        for msg in self.messages.iter().filter(|m| m.role != Role::System) {
            // Skip todo tool messages entirely - they're visible in the side pane
            if msg.role == Role::Tool && msg.content.starts_with("[Tool: todo]") {
                continue;
            }

            // Add role header with icons
            let (role_text, role_style) = match msg.role {
                Role::User => (
                    " USER ",
                    Style::default()
                        .fg(theme::USER_COLOR)
                        .add_modifier(Modifier::BOLD),
                ),
                Role::Assistant => (
                    " PANGU ",
                    Style::default()
                        .fg(theme::ACCENT)
                        .add_modifier(Modifier::BOLD),
                ),
                Role::Tool => (
                    " TOOL ",
                    Style::default()
                        .fg(theme::WARNING)
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
                        lines.push(Line::from(Span::styled(
                            format!("› {line}"),
                            Style::default().fg(theme::USER_COLOR),
                        )));
                    }
                }
                Role::Assistant => {
                    // Render assistant message as markdown
                    if !msg.content.is_empty() {
                        let rendered = self.renderer.render(&msg.content);
                        lines.extend(rendered);
                    }
                }
                Role::Tool => {
                    // Tool messages with special formatting
                    if msg.content.starts_with("[Permission Required]") {
                        // Permission request - show with warning styling
                        for line in msg.content.lines() {
                            if line.contains("Press 1") || line.contains("Press 2") || line.contains("Press 3") {
                                // Keyboard hints line
                                lines.push(Line::from(vec![
                                    Span::styled("1", Style::default().fg(theme::SUCCESS)),
                                    Span::raw(" Allow Once  "),
                                    Span::styled("2", Style::default().fg(theme::INFO)),
                                    Span::raw(" Always Allow  "),
                                    Span::styled("3/Esc", Style::default().fg(theme::ERROR)),
                                    Span::raw(" Deny"),
                                ]));
                            } else if line.starts_with("[Permission Required]") {
                                lines.push(Line::from(Span::styled(
                                    line.to_string(),
                                    Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD),
                                )));
                            } else if line.contains("wants to access") {
                                lines.push(Line::from(Span::styled(
                                    line.to_string(),
                                    Style::default().fg(theme::PRIMARY),
                                )));
                            } else {
                                lines.push(Line::from(line.to_string()));
                            }
                        }
                    } else if msg.content.contains("Error:") {
                        // Error result
                        lines.push(Line::from(Span::styled(
                            "┌─ tool error",
                            Style::default().fg(theme::ERROR).add_modifier(Modifier::BOLD),
                        )));
                        for line in msg.content.lines() {
                            lines.push(Line::from(Span::styled(
                                format!("│ {line}"),
                                Style::default().fg(theme::ERROR),
                            )));
                        }
                        lines.push(Line::from(Span::styled(
                            "└─",
                            Style::default().fg(theme::ERROR),
                        )));
                    } else {
                        // Normal tool result
                        lines.push(Line::from(Span::styled(
                            "┌─ tool output",
                            Style::default().fg(theme::SUCCESS).add_modifier(Modifier::BOLD),
                        )));
                        for line in msg.content.lines() {
                            if line.starts_with("[Tool:") {
                                lines.push(Line::from(Span::styled(
                                    format!("│ {line}"),
                                    Style::default().fg(theme::SUCCESS),
                                )));
                            } else {
                                lines.push(Line::from(Span::styled(
                                    format!("│ {line}"),
                                    Style::default().fg(theme::MUTED),
                                )));
                            }
                        }
                        lines.push(Line::from(Span::styled(
                            "└─",
                            Style::default().fg(theme::SUCCESS),
                        )));
                    }
                }
                Role::System => {}
            }

            // Add separator
            lines.push(Line::from(""));
        }

        // Add current streaming response if any
        if !self.current_response.is_empty() || self.is_generating {
            lines.push(Line::from(vec![Span::styled(
                " PANGU ".to_string(),
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD),
            )]));

            // Render streaming response. While generating, use a lightweight plain
            // renderer to keep token updates smooth and defer markdown parsing.
            if !self.current_response.is_empty() {
                if self.is_generating {
                    for line in self.current_response.lines() {
                        lines.push(Line::from(Span::styled(
                            line.to_string(),
                            Style::default().fg(theme::ACCENT),
                        )));
                    }
                } else {
                    let rendered = self.renderer.render(self.current_response);
                    lines.extend(rendered);
                }
            }

            // Show animated cursor while generating
            if self.is_generating {
                let cursor_frames = ["▏", "▎", "▍", "▌"];
                let cursor_idx = (self.tick as usize / 2) % cursor_frames.len();
                lines.push(Line::from(vec![Span::styled(
                    cursor_frames[cursor_idx].to_string(),
                    Style::default().fg(theme::ACCENT),
                )]));
            }
        }

        // Add minimal padding at the bottom
        lines.push(Line::from(""));

        lines
    }
}

impl Widget for ChatView<'_> {
    fn render(mut self, area: Rect, buf: &mut Buffer) {
        // Set width for code blocks to extend to full chat width
        self.render_width = Some(area.width);

        // Use cached lines if available, otherwise format
        let lines: Vec<Line<'static>> = if let Some(cached) = self.cached_lines {
            cached.to_vec()
        } else {
            self.format_messages()
        };

        // Apply selection highlighting if active
        let lines = if let Some(selection) = self.selection {
            apply_selection_highlight(lines, selection, self.scroll)
        } else {
            lines
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme::MUTED))
            .title(" Conversation ");

        let paragraph = Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((self.scroll, 0));

        paragraph.render(area, buf);
    }
}

/// Apply selection highlighting to lines
fn apply_selection_highlight(
    lines: Vec<Line<'static>>,
    selection: &TextSelection,
    _scroll: u16,
) -> Vec<Line<'static>> {
    let ((start_row, start_col), (end_row, end_col)) = selection.normalized();
    let highlight_style = Style::default().bg(theme::PRIMARY).fg(Color::Black);

    lines
        .into_iter()
        .enumerate()
        .map(|(row_idx, line)| {
            let row = row_idx as u16;

            // Check if this line intersects with selection
            if row < start_row || row > end_row {
                return line;
            }

            // Determine selection bounds for this line
            let line_start = if row == start_row { start_col as usize } else { 0 };
            let line_end = if row == end_row {
                end_col as usize
            } else {
                usize::MAX
            };

            // Apply highlighting to spans
            let mut new_spans: Vec<Span<'static>> = Vec::new();
            let mut char_pos = 0usize;

            for span in line.spans {
                let span_len = span.content.chars().count();
                let span_start = char_pos;
                let span_end = char_pos + span_len;

                if span_end <= line_start || span_start >= line_end {
                    // Span is entirely outside selection
                    new_spans.push(span);
                } else if span_start >= line_start && span_end <= line_end {
                    // Span is entirely inside selection
                    new_spans.push(Span::styled(span.content, highlight_style));
                } else {
                    // Span partially overlaps - split it
                    let content: String = span.content.to_string();
                    let chars: Vec<char> = content.chars().collect();

                    // Before selection
                    if span_start < line_start {
                        let before_len = line_start - span_start;
                        let before: String = chars[..before_len].iter().collect();
                        new_spans.push(Span::styled(before, span.style));
                    }

                    // Selected portion
                    let sel_start = line_start.saturating_sub(span_start);
                    let sel_end = (line_end - span_start).min(span_len);
                    if sel_start < sel_end {
                        let selected: String = chars[sel_start..sel_end].iter().collect();
                        new_spans.push(Span::styled(selected, highlight_style));
                    }

                    // After selection
                    if span_end > line_end {
                        let after_start = line_end - span_start;
                        let after: String = chars[after_start..].iter().collect();
                        new_spans.push(Span::styled(after, span.style));
                    }
                }

                char_pos = span_end;
            }

            Line::from(new_spans)
        })
        .collect()
}

/// Extract text from selection
pub fn extract_selected_text(
    lines: &[Line<'static>],
    selection: &TextSelection,
    _scroll: u16,
) -> String {
    let ((start_row, start_col), (end_row, end_col)) = selection.normalized();
    let mut result = String::new();

    for (row_idx, line) in lines.iter().enumerate() {
        let row = row_idx as u16;

        // Skip lines outside selection
        if row < start_row || row > end_row {
            continue;
        }

        // Get full line text
        let line_text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();

        // Determine selection bounds for this line
        let line_start = if row == start_row {
            start_col as usize
        } else {
            0
        };
        let line_end = if row == end_row {
            end_col as usize
        } else {
            line_text.chars().count()
        };

        // Extract selected portion
        let chars: Vec<char> = line_text.chars().collect();
        let selected: String = chars
            .get(line_start..line_end.min(chars.len()))
            .unwrap_or(&[])
            .iter()
            .collect();

        if !result.is_empty() && row > start_row {
            result.push('\n');
        }
        result.push_str(&selected);
    }

    result
}
