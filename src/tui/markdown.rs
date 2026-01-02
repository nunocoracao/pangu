use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use syntect::{
    easy::HighlightLines,
    highlighting::{ThemeSet, Style as SyntectStyle},
    parsing::SyntaxSet,
    util::LinesWithEndings,
};

/// Markdown renderer that converts markdown text to ratatui Lines
pub struct MarkdownRenderer {
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
    /// Width for rendering (used for code block borders)
    width: Option<u16>,
}

impl Default for MarkdownRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl MarkdownRenderer {
    pub fn new() -> Self {
        Self {
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set: ThemeSet::load_defaults(),
            width: None,
        }
    }

    /// Set the rendering width (for code block borders)
    pub fn set_width(&mut self, width: u16) {
        self.width = Some(width);
    }

    /// Render markdown text to a vector of ratatui Lines
    pub fn render(&self, text: &str) -> Vec<Line<'static>> {
        let mut lines: Vec<Line<'static>> = Vec::new();
        let mut current_spans: Vec<Span<'static>> = Vec::new();

        // Style stack for nested formatting
        let mut style_stack: Vec<Style> = vec![Style::default()];

        // State tracking
        let mut in_code_block = false;
        let mut code_block_lang: Option<String> = None;
        let mut code_block_content = String::new();
        let mut list_depth: usize = 0;
        let mut ordered_list_indices: Vec<u64> = Vec::new();
        let mut in_blockquote = false;
        let mut in_table = false;
        let mut table_alignments: Vec<pulldown_cmark::Alignment> = Vec::new();
        let mut table_row: Vec<String> = Vec::new();
        let mut table_header = false;
        let mut link_url: Option<String> = None;

        let options = Options::ENABLE_TABLES
            | Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_TASKLISTS;
        let parser = Parser::new_ext(text, options);

        for event in parser {
            match event {
                Event::Start(tag) => match tag {
                    Tag::Paragraph => {}
                    Tag::Heading { level, .. } => {
                        let style = match level {
                            HeadingLevel::H1 => Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                            HeadingLevel::H2 => Style::default()
                                .fg(Color::Blue)
                                .add_modifier(Modifier::BOLD),
                            _ => Style::default()
                                .fg(Color::White)
                                .add_modifier(Modifier::BOLD),
                        };
                        style_stack.push(style);
                    }
                    Tag::BlockQuote => {
                        in_blockquote = true;
                    }
                    Tag::CodeBlock(kind) => {
                        in_code_block = true;
                        code_block_content.clear();
                        code_block_lang = match kind {
                            CodeBlockKind::Fenced(lang) => {
                                let lang_str = lang.to_string();
                                if lang_str.is_empty() {
                                    None
                                } else {
                                    Some(lang_str)
                                }
                            }
                            CodeBlockKind::Indented => None,
                        };
                    }
                    Tag::List(first_item) => {
                        list_depth += 1;
                        if let Some(start) = first_item {
                            ordered_list_indices.push(start);
                        } else {
                            ordered_list_indices.push(0); // 0 = unordered
                        }
                    }
                    Tag::Item => {}
                    Tag::Emphasis => {
                        let current = *style_stack.last().unwrap_or(&Style::default());
                        style_stack.push(current.add_modifier(Modifier::ITALIC));
                    }
                    Tag::Strong => {
                        let current = *style_stack.last().unwrap_or(&Style::default());
                        style_stack.push(current.add_modifier(Modifier::BOLD));
                    }
                    Tag::Strikethrough => {
                        let current = *style_stack.last().unwrap_or(&Style::default());
                        style_stack.push(current.add_modifier(Modifier::CROSSED_OUT));
                    }
                    Tag::Link { dest_url, .. } => {
                        link_url = Some(dest_url.to_string());
                        let current = *style_stack.last().unwrap_or(&Style::default());
                        style_stack.push(
                            current
                                .fg(Color::Blue)
                                .add_modifier(Modifier::UNDERLINED),
                        );
                    }
                    Tag::Table(alignments) => {
                        in_table = true;
                        table_alignments = alignments;
                        // Flush current line
                        if !current_spans.is_empty() {
                            lines.push(Line::from(std::mem::take(&mut current_spans)));
                        }
                    }
                    Tag::TableHead => {
                        table_header = true;
                    }
                    Tag::TableRow => {
                        table_row.clear();
                    }
                    Tag::TableCell => {}
                    _ => {}
                },

                Event::End(tag_end) => match tag_end {
                    TagEnd::Paragraph => {
                        if !current_spans.is_empty() {
                            let prefix = if in_blockquote {
                                vec![Span::styled("│ ", Style::default().fg(Color::DarkGray))]
                            } else {
                                vec![]
                            };
                            let mut line_spans = prefix;
                            line_spans.append(&mut current_spans);
                            lines.push(Line::from(line_spans));
                        }
                        lines.push(Line::from(""));
                    }
                    TagEnd::Heading(_) => {
                        style_stack.pop();
                        if !current_spans.is_empty() {
                            lines.push(Line::from(std::mem::take(&mut current_spans)));
                        }
                        lines.push(Line::from(""));
                    }
                    TagEnd::BlockQuote => {
                        in_blockquote = false;
                    }
                    TagEnd::CodeBlock => {
                        in_code_block = false;
                        // Render code block with syntax highlighting
                        let highlighted_lines =
                            self.highlight_code(&code_block_content, code_block_lang.as_deref());
                        lines.extend(highlighted_lines);
                        lines.push(Line::from(""));
                        code_block_lang = None;
                    }
                    TagEnd::List(_) => {
                        list_depth = list_depth.saturating_sub(1);
                        ordered_list_indices.pop();
                    }
                    TagEnd::Item => {
                        // Flush item content
                        if !current_spans.is_empty() {
                            let indent = "  ".repeat(list_depth.saturating_sub(1));
                            let bullet = if let Some(&idx) = ordered_list_indices.last() {
                                if idx == 0 {
                                    format!("{}• ", indent)
                                } else {
                                    let num = idx;
                                    // Increment for next item
                                    if let Some(last) = ordered_list_indices.last_mut() {
                                        *last += 1;
                                    }
                                    format!("{}{}. ", indent, num)
                                }
                            } else {
                                format!("{}• ", indent)
                            };

                            let mut item_spans =
                                vec![Span::styled(bullet, Style::default().fg(Color::DarkGray))];
                            item_spans.append(&mut current_spans);
                            lines.push(Line::from(item_spans));
                        }
                    }
                    TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => {
                        style_stack.pop();
                    }
                    TagEnd::Link => {
                        style_stack.pop();
                        // Append URL after link text
                        if let Some(url) = link_url.take() {
                            current_spans.push(Span::styled(
                                format!(" ({})", url),
                                Style::default().fg(Color::DarkGray),
                            ));
                        }
                    }
                    TagEnd::Table => {
                        in_table = false;
                        table_alignments.clear();
                        lines.push(Line::from(""));
                    }
                    TagEnd::TableHead => {
                        table_header = false;
                        // Render header row
                        let row_line = self.render_table_row(&table_row, &table_alignments, true);
                        lines.push(row_line);
                        // Add separator
                        let separator = self.render_table_separator(&table_row, &table_alignments);
                        lines.push(separator);
                    }
                    TagEnd::TableRow => {
                        if !table_header {
                            let row_line =
                                self.render_table_row(&table_row, &table_alignments, false);
                            lines.push(row_line);
                        }
                    }
                    TagEnd::TableCell => {
                        // Cell content captured separately
                    }
                    _ => {}
                },

                Event::Text(text) => {
                    if in_code_block {
                        code_block_content.push_str(&text);
                    } else if in_table {
                        table_row.push(text.to_string());
                    } else {
                        let style = *style_stack.last().unwrap_or(&Style::default());
                        current_spans.push(Span::styled(text.to_string(), style));
                    }
                }

                Event::Code(code) => {
                    let code_style = Style::default()
                        .fg(Color::Yellow)
                        .bg(Color::Rgb(40, 40, 40));
                    current_spans.push(Span::styled(format!(" {} ", code), code_style));
                }

                Event::Html(html) => {
                    // Render HTML as plain text
                    let style = Style::default().fg(Color::DarkGray);
                    current_spans.push(Span::styled(html.to_string(), style));
                }

                Event::SoftBreak => {
                    current_spans.push(Span::raw(" "));
                }

                Event::HardBreak => {
                    if !current_spans.is_empty() {
                        let prefix = if in_blockquote {
                            vec![Span::styled("│ ", Style::default().fg(Color::DarkGray))]
                        } else {
                            vec![]
                        };
                        let mut line_spans = prefix;
                        line_spans.append(&mut current_spans);
                        lines.push(Line::from(line_spans));
                    }
                }

                Event::Rule => {
                    lines.push(Line::from(Span::styled(
                        "─".repeat(40),
                        Style::default().fg(Color::DarkGray),
                    )));
                    lines.push(Line::from(""));
                }

                Event::TaskListMarker(checked) => {
                    let marker = if checked { "[x] " } else { "[ ] " };
                    current_spans.push(Span::styled(
                        marker,
                        Style::default().fg(Color::Magenta),
                    ));
                }

                _ => {}
            }
        }

        // Flush any remaining spans
        if !current_spans.is_empty() {
            lines.push(Line::from(current_spans));
        }

        lines
    }

    /// Highlight code with syntax highlighting
    fn highlight_code(&self, code: &str, lang: Option<&str>) -> Vec<Line<'static>> {
        let mut lines = Vec::new();

        // Calculate border width based on available width or default
        // Subtract 2 for the chat border, then use remaining space
        let border_width = self.width.map(|w| w.saturating_sub(4) as usize).unwrap_or(60);

        // Count lines for line number width calculation
        let line_count = code.lines().count();
        let line_num_width = if line_count == 0 { 1 } else { line_count.to_string().len() };

        // Add top border
        let lang_display = lang.unwrap_or("code");
        let header_content_len = 3 + lang_display.len() + 1; // "┌─ " + lang + " "
        let remaining_dashes = border_width.saturating_sub(header_content_len);
        lines.push(Line::from(vec![
            Span::styled("┌─ ", Style::default().fg(Color::DarkGray)),
            Span::styled(lang_display.to_string(), Style::default().fg(Color::Cyan)),
            Span::styled(" ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                "─".repeat(remaining_dashes),
                Style::default().fg(Color::DarkGray),
            ),
        ]));

        // Try to get syntax highlighting
        let syntax = lang
            .and_then(|l| self.syntax_set.find_syntax_by_token(l))
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        let theme = &self.theme_set.themes["base16-ocean.dark"];

        let mut highlighter = HighlightLines::new(syntax, theme);
        let mut line_num = 1;

        for line in LinesWithEndings::from(code) {
            // Format line number with padding
            let line_num_str = format!("{:>width$}", line_num, width = line_num_width);
            let mut spans = vec![
                Span::styled("│ ", Style::default().fg(Color::DarkGray)),
                Span::styled(line_num_str, Style::default().fg(Color::DarkGray)),
                Span::styled(" │ ", Style::default().fg(Color::DarkGray)),
            ];

            match highlighter.highlight_line(line, &self.syntax_set) {
                Ok(highlighted) => {
                    for (style, text) in highlighted {
                        spans.push(Span::styled(
                            text.trim_end_matches('\n').to_string(),
                            syntect_style_to_ratatui(style),
                        ));
                    }
                }
                Err(_) => {
                    // Fallback to plain text
                    spans.push(Span::styled(
                        line.trim_end_matches('\n').to_string(),
                        Style::default().fg(Color::White),
                    ));
                }
            }

            lines.push(Line::from(spans));
            line_num += 1;
        }

        // Add bottom border
        lines.push(Line::from(Span::styled(
            "└".to_string() + &"─".repeat(border_width.saturating_sub(1)),
            Style::default().fg(Color::DarkGray),
        )));

        lines
    }

    /// Render a table row
    fn render_table_row(
        &self,
        cells: &[String],
        _alignments: &[pulldown_cmark::Alignment],
        is_header: bool,
    ) -> Line<'static> {
        let mut spans = vec![Span::styled("│ ", Style::default().fg(Color::DarkGray))];

        let style = if is_header {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        for (i, cell) in cells.iter().enumerate() {
            spans.push(Span::styled(cell.clone(), style));
            if i < cells.len() - 1 {
                spans.push(Span::styled(" │ ", Style::default().fg(Color::DarkGray)));
            }
        }

        spans.push(Span::styled(" │", Style::default().fg(Color::DarkGray)));

        Line::from(spans)
    }

    /// Render table separator line
    fn render_table_separator(
        &self,
        cells: &[String],
        _alignments: &[pulldown_cmark::Alignment],
    ) -> Line<'static> {
        let mut separator = String::from("├─");

        for (i, cell) in cells.iter().enumerate() {
            separator.push_str(&"─".repeat(cell.len() + 2));
            if i < cells.len() - 1 {
                separator.push_str("┼");
            }
        }

        separator.push_str("─┤");

        Line::from(Span::styled(
            separator,
            Style::default().fg(Color::DarkGray),
        ))
    }
}

/// Convert syntect style to ratatui style
fn syntect_style_to_ratatui(style: SyntectStyle) -> Style {
    let fg = Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b);
    Style::default().fg(fg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_markdown() {
        let renderer = MarkdownRenderer::new();
        let lines = renderer.render("**bold** and *italic*");
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_code_block() {
        let renderer = MarkdownRenderer::new();
        let lines = renderer.render("```rust\nfn main() {}\n```");
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_list() {
        let renderer = MarkdownRenderer::new();
        let lines = renderer.render("- item 1\n- item 2");
        assert!(!lines.is_empty());
    }
}
