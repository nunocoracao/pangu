use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};

use crate::model::{ChatMessage, Role};
use crate::tools::{
    has_partial_thinking, parse_thinking_blocks, parse_tool_calls, remove_thinking_blocks,
    remove_tool_calls,
};
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
    /// Width for rendering (set during render)
    render_width: Option<u16>,
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
        }
    }

    /// Set the rendering width (for code blocks to extend to full width)
    pub fn with_width(mut self, width: u16) -> Self {
        self.render_width = Some(width);
        self
    }

    /// Get the total content height in lines
    /// Note: This is an estimate since we can't know exact wrapped height without rendering
    pub fn content_height(&mut self, area_width: u16) -> u16 {
        self.render_width = Some(area_width);
        let lines = self.format_messages();
        let mut total_lines: u16 = 0;

        // Account for text wrapping - estimate wrapped lines
        let usable_width = area_width.saturating_sub(2) as usize; // minus borders
        for line in &lines {
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

    /// Format a tool call as a nice UI box
    fn format_tool_call(&self, tool_name: &str, params: &str) -> Vec<Line<'static>> {
        let width = self.render_width.unwrap_or(60) as usize;
        let box_width = width.saturating_sub(4).min(70); // Max 70 chars for the box

        let mut lines = Vec::new();

        // Truncate URL/params if too long
        let display_params = if params.len() > box_width - 10 {
            format!("{}...", &params[..box_width - 13])
        } else {
            params.to_string()
        };

        // Top border with tool name
        let header = format!("┌─ {} ", tool_name);
        let header_dashes = "─".repeat(box_width.saturating_sub(header.chars().count() + 1));
        lines.push(Line::from(vec![
            Span::styled(header, Style::default().fg(Color::Magenta)),
            Span::styled(header_dashes + "┐", Style::default().fg(Color::DarkGray)),
        ]));

        // Content line
        let content = format!("│ {} ", display_params);
        let padding = " ".repeat(box_width.saturating_sub(content.chars().count()));
        lines.push(Line::from(vec![
            Span::styled("│ ", Style::default().fg(Color::DarkGray)),
            Span::styled(display_params, Style::default().fg(Color::Cyan)),
            Span::styled(padding + "│", Style::default().fg(Color::DarkGray)),
        ]));

        // Bottom border
        lines.push(Line::from(Span::styled(
            format!("└{}┘", "─".repeat(box_width.saturating_sub(2))),
            Style::default().fg(Color::DarkGray),
        )));

        lines
    }

    /// Format an animated tool loading indicator
    fn format_tool_loading(&self) -> Vec<Line<'static>> {
        let width = self.render_width.unwrap_or(60) as usize;
        let box_width = width.saturating_sub(4).min(70);

        let mut lines = Vec::new();

        // Animated spinner
        let spinner_frames = ["\u{25D0}", "\u{25D3}", "\u{25D1}", "\u{25D2}"];
        let frame = spinner_frames[(self.tick as usize / 2) % spinner_frames.len()];

        // Top border with spinner
        let header = format!("\u{250C}\u{2500} {} Preparing tool call ", frame);
        let header_dashes = "\u{2500}".repeat(box_width.saturating_sub(header.chars().count() + 1));
        lines.push(Line::from(vec![
            Span::styled(header, Style::default().fg(Color::Magenta)),
            Span::styled(header_dashes + "\u{2510}", Style::default().fg(Color::DarkGray)),
        ]));

        // Content line with animated dots
        let dots = ".".repeat(((self.tick as usize / 3) % 4) + 1);
        let padding = " ".repeat(4 - dots.len());
        lines.push(Line::from(vec![
            Span::styled("\u{2502} ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("Reading parameters{}{}", dots, padding),
                Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
            ),
        ]));

        // Bottom border
        lines.push(Line::from(Span::styled(
            format!("\u{2514}{}\u{2518}", "\u{2500}".repeat(box_width.saturating_sub(2))),
            Style::default().fg(Color::DarkGray),
        )));

        lines
    }

    /// Format an animated thinking indicator
    fn format_thinking_indicator(&self) -> Vec<Line<'static>> {
        // Fun thinking messages that rotate
        const THINKING_MESSAGES: &[&str] = &[
            "Pondering the possibilities",
            "Consulting the neural pathways",
            "Assembling thoughts",
            "Weaving ideas together",
            "Processing your request",
            "Exploring solution space",
            "Connecting the dots",
            "Synthesizing response",
            "Diving into the problem",
            "Crafting an answer",
            "Analyzing the question",
            "Gathering insights",
        ];

        // Brainwave animation frames
        const BRAIN_FRAMES: &[&str] = &[
            "🧠 ∿∿∿",
            "🧠 ∿∿∿∿",
            "🧠 ~∿∿∿",
            "🧠 ~~∿∿",
            "🧠 ~~~∿",
            "🧠 ~~~~",
            "🧠 ∿~~~",
            "🧠 ∿∿~~",
            "🧠 ∿∿∿~",
        ];

        let message_idx = (self.tick as usize / 8) % THINKING_MESSAGES.len();
        let frame_idx = (self.tick as usize) % BRAIN_FRAMES.len();

        let message = THINKING_MESSAGES[message_idx];
        let brain = BRAIN_FRAMES[frame_idx];

        // Typewriter effect for the message
        let chars_to_show = ((self.tick % 32) as usize * 2).min(message.len());
        let displayed_message = if chars_to_show < message.len() {
            format!("{}▊", &message[..chars_to_show])
        } else {
            format!("{}...", message)
        };

        let mut lines = Vec::new();

        lines.push(Line::from(vec![
            Span::styled(
                brain.to_string(),
                Style::default().fg(Color::Magenta),
            ),
        ]));

        lines.push(Line::from(vec![
            Span::styled(
                displayed_message,
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
            ),
        ]));

        lines
    }

    /// Format a thinking block with a collapsible appearance
    fn format_thinking_block(&self, content: &str) -> Vec<Line<'static>> {
        let width = self.render_width.unwrap_or(60) as usize;
        let box_width = width.saturating_sub(4).min(70);

        let mut lines = Vec::new();

        // Header with collapse indicator (visual only - not interactive yet)
        let header = "▶ Thinking";
        lines.push(Line::from(vec![
            Span::styled(
                header.to_string(),
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::DIM),
            ),
            Span::styled(
                format!(" {} ", "─".repeat(box_width.saturating_sub(header.chars().count() + 3))),
                Style::default().fg(Color::DarkGray).add_modifier(Modifier::DIM),
            ),
        ]));

        // Show a preview of the thinking content (first 2 lines, dimmed)
        let preview_lines: Vec<&str> = content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .take(2)
            .collect();

        for preview_line in preview_lines {
            let truncated = if preview_line.len() > box_width - 4 {
                format!("{}...", &preview_line[..box_width - 7])
            } else {
                preview_line.to_string()
            };
            lines.push(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(
                    truncated,
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::ITALIC | Modifier::DIM),
                ),
            ]));
        }

        // Show ellipsis if there's more content
        let total_lines = content.lines().filter(|l| !l.trim().is_empty()).count();
        if total_lines > 2 {
            lines.push(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(
                    format!("... ({} more lines)", total_lines - 2),
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::DIM),
                ),
            ]));
        }

        lines.push(Line::from("")); // Add spacing after thinking block

        lines
    }

    /// Format an active thinking animation during streaming
    fn format_thinking_streaming(&self) -> Vec<Line<'static>> {
        let width = self.render_width.unwrap_or(60) as usize;
        let box_width = width.saturating_sub(4).min(70);

        let mut lines = Vec::new();

        // Animated brain icon
        let brain_frames = ["🧠", "🧠", "💭", "💭"];
        let frame = brain_frames[(self.tick as usize / 3) % brain_frames.len()];

        // Animated dots
        let dots = ".".repeat(((self.tick as usize / 2) % 4) + 1);
        let padding = " ".repeat(4 - dots.len());

        let header = format!("{} Thinking{}{}", frame, dots, padding);
        lines.push(Line::from(vec![
            Span::styled(
                header,
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" {}", "─".repeat(box_width.saturating_sub(20))),
                Style::default().fg(Color::DarkGray),
            ),
        ]));

        lines
    }

    /// Format a tool result as a summary box
    fn format_tool_result(&self, content: &str) -> Vec<Line<'static>> {
        let width = self.render_width.unwrap_or(60) as usize;
        let box_width = width.saturating_sub(4).min(70);

        let mut lines = Vec::new();

        // Parse the tool result header (e.g., "[Tool: fetch]")
        let (tool_name, result_content) = if content.starts_with("[Tool: ") {
            if let Some(end) = content.find("]\n") {
                let name = &content[7..end];
                let rest = &content[end + 2..];
                (name.to_string(), rest.to_string())
            } else {
                ("fetch".to_string(), content.to_string())
            }
        } else {
            ("result".to_string(), content.to_string())
        };

        // Calculate content size
        let content_bytes = result_content.len();
        let size_str = if content_bytes > 1024 * 1024 {
            format!("{:.1} MB", content_bytes as f64 / (1024.0 * 1024.0))
        } else if content_bytes > 1024 {
            format!("{:.1} KB", content_bytes as f64 / 1024.0)
        } else {
            format!("{} bytes", content_bytes)
        };

        // Top border
        let header = format!("┌─ {} result ", tool_name);
        let header_dashes = "─".repeat(box_width.saturating_sub(header.chars().count() + 1));
        lines.push(Line::from(vec![
            Span::styled(header, Style::default().fg(Color::Yellow)),
            Span::styled(header_dashes + "┐", Style::default().fg(Color::DarkGray)),
        ]));

        // Size info line
        lines.push(Line::from(vec![
            Span::styled("│ ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("Received {} of data", size_str), Style::default().fg(Color::DarkGray)),
        ]));

        // Show first few meaningful lines as preview
        let preview_lines: Vec<&str> = result_content
            .lines()
            .filter(|l| !l.trim().is_empty() && !l.starts_with("<?xml") && !l.starts_with("<!"))
            .take(3)
            .collect();

        for preview_line in preview_lines {
            let truncated = if preview_line.len() > box_width - 4 {
                format!("{}...", &preview_line[..box_width - 7])
            } else {
                preview_line.to_string()
            };
            lines.push(Line::from(vec![
                Span::styled("│ ", Style::default().fg(Color::DarkGray)),
                Span::styled(truncated, Style::default().fg(Color::White)),
            ]));
        }

        if result_content.lines().count() > 3 {
            lines.push(Line::from(vec![
                Span::styled("│ ", Style::default().fg(Color::DarkGray)),
                Span::styled("...", Style::default().fg(Color::DarkGray)),
            ]));
        }

        // Bottom border
        lines.push(Line::from(Span::styled(
            format!("└{}┘", "─".repeat(box_width.saturating_sub(2))),
            Style::default().fg(Color::DarkGray),
        )));

        lines
    }

    fn format_messages(&mut self) -> Vec<Line<'static>> {
        let mut lines = Vec::new();

        // Set width on renderer if available
        if let Some(width) = self.render_width {
            self.renderer.set_width(width);
        }

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
                Role::Tool => (
                    "Tool",
                    Style::default()
                        .fg(Color::Yellow)
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
                    // First, handle thinking blocks
                    let thinking_blocks = parse_thinking_blocks(&msg.content);
                    for thinking in &thinking_blocks {
                        lines.extend(self.format_thinking_block(&thinking.content));
                    }

                    // Remove thinking blocks from content for further processing
                    let content_without_thinking = remove_thinking_blocks(&msg.content);

                    // Check for tool calls and render them specially
                    let tool_calls = parse_tool_calls(&content_without_thinking);
                    if !tool_calls.is_empty() {
                        // Render tool calls as nice boxes
                        for tool_call in &tool_calls {
                            lines.extend(self.format_tool_call(&tool_call.name, &tool_call.params));
                        }
                        // Render remaining text (without tool call XML)
                        let cleaned = remove_tool_calls(&content_without_thinking);
                        if !cleaned.is_empty() {
                            let rendered = self.renderer.render(&cleaned);
                            lines.extend(rendered);
                        }
                    } else if !content_without_thinking.is_empty() {
                        // Normal assistant message (without thinking blocks)
                        let rendered = self.renderer.render(&content_without_thinking);
                        lines.extend(rendered);
                    }
                }
                Role::Tool => {
                    // Tool results: show summary box
                    lines.extend(self.format_tool_result(&msg.content));
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

            // Render streaming response - handle thinking and tool calls specially
            if !self.current_response.is_empty() {
                // First, check for partial thinking block (show animation)
                if has_partial_thinking(self.current_response) {
                    // Show "Thinking..." animation for partial thinking block
                    lines.extend(self.format_thinking_streaming());
                } else {
                    // Handle completed thinking blocks
                    let thinking_blocks = parse_thinking_blocks(self.current_response);
                    for thinking in &thinking_blocks {
                        lines.extend(self.format_thinking_block(&thinking.content));
                    }

                    // Get content without thinking blocks
                    let content_without_thinking = remove_thinking_blocks(self.current_response);

                    // Check for tool calls in streaming content
                    let tool_calls = parse_tool_calls(&content_without_thinking);
                    if !tool_calls.is_empty() {
                        // Render completed tool calls as nice boxes
                        for tool_call in &tool_calls {
                            lines.extend(self.format_tool_call(&tool_call.name, &tool_call.params));
                        }
                        // Render remaining text (without tool call XML)
                        let cleaned = remove_tool_calls(&content_without_thinking);
                        if !cleaned.trim().is_empty() {
                            let rendered = self.renderer.render(&cleaned);
                            lines.extend(rendered);
                        }
                    } else {
                        // Check for partial tool call being typed (hide it)
                        let has_partial_tool_call = content_without_thinking.contains("<tool_use>")
                            && !content_without_thinking.contains("</tool_use>");

                        if has_partial_tool_call {
                            // Split at <tool_use> and only show content before it
                            if let Some(idx) = content_without_thinking.find("<tool_use>") {
                                let before_tool = &content_without_thinking[..idx];
                                if !before_tool.trim().is_empty() {
                                    let rendered = self.renderer.render(before_tool);
                                    lines.extend(rendered);
                                }
                                // Show "preparing tool call" indicator
                                lines.extend(self.format_tool_loading());
                            }
                        } else if !content_without_thinking.trim().is_empty() {
                            // Normal rendering
                            let rendered = self.renderer.render(&content_without_thinking);
                            lines.extend(rendered);
                        }
                    }
                }
            }

            // Show animated cursor/indicator while generating
            if self.is_generating {
                let is_in_thinking = has_partial_thinking(self.current_response);
                let is_in_tool_call = self.current_response.contains("<tool_use>")
                    && !self.current_response.contains("</tool_use>");

                if self.current_response.is_empty() {
                    // Show thinking animation when no response yet (waiting for thinking block)
                    lines.extend(self.format_thinking_streaming());
                } else if !is_in_thinking && !is_in_tool_call {
                    // Show simple cursor when mid-response (but not during thinking or tool call)
                    lines.push(Line::from(vec![Span::styled(
                        "▊".to_string(),
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::SLOW_BLINK),
                    )]));
                }
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
