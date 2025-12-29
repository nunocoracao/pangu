use std::process::Command;

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Widget},
};

use crate::app::AppState;

/// Spinner frames for loading animation
const SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Status bar showing app state, directory, and git info
pub struct StatusBar<'a> {
    state: &'a AppState,
    tick: u64,
}

impl<'a> StatusBar<'a> {
    pub fn new(state: &'a AppState, tick: u64) -> Self {
        Self { state, tick }
    }
}

fn get_working_dir() -> String {
    std::env::current_dir()
        .map(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| p.display().to_string())
        })
        .unwrap_or_else(|_| "?".to_string())
}

fn get_git_info() -> Option<(String, String)> {
    // Get branch
    let branch = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
            } else {
                None
            }
        })?;

    // Get status summary
    let status = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .map(|output| {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let mut modified = 0;
                let mut staged = 0;
                let mut untracked = 0;

                for line in stdout.lines() {
                    if line.len() >= 2 {
                        let chars: Vec<char> = line.chars().collect();
                        let index = chars[0];
                        let worktree = chars[1];

                        if index == '?' {
                            untracked += 1;
                        } else {
                            if index != ' ' && index != '?' {
                                staged += 1;
                            }
                            if worktree != ' ' && worktree != '?' {
                                modified += 1;
                            }
                        }
                    }
                }

                let mut parts = Vec::new();
                if staged > 0 {
                    parts.push(format!("+{}", staged));
                }
                if modified > 0 {
                    parts.push(format!("~{}", modified));
                }
                if untracked > 0 {
                    parts.push(format!("?{}", untracked));
                }
                parts.join(" ")
            } else {
                String::new()
            }
        })
        .unwrap_or_default();

    Some((branch, status))
}

impl Widget for StatusBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let spinner_frame = SPINNER[(self.tick as usize / 2) % SPINNER.len()];

        let (status_text, status_style) = match self.state {
            AppState::Idle => ("Ready".to_string(), Style::default().fg(Color::Green)),
            AppState::Generating => (
                format!("{} Generating...", spinner_frame),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            AppState::Loading => (
                format!("{} Loading model...", spinner_frame),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            AppState::Error(msg) => (
                msg.clone(),
                Style::default().fg(Color::Red),
            ),
        };

        // Left side: Pangu status
        let left_spans = vec![
            Span::styled(
                " Pangu ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(status_text, status_style),
            Span::styled(
                " │ ",
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                "Ctrl+C",
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                " quit",
                Style::default().fg(Color::Rgb(80, 80, 80)),
            ),
        ];

        // Right side: directory and git info
        let working_dir = get_working_dir();
        let git_info = get_git_info();

        let mut right_spans = vec![
            Span::styled("󰉋 ", Style::default().fg(Color::Yellow)),
            Span::styled(working_dir, Style::default().fg(Color::White)),
        ];

        if let Some((branch, status)) = git_info {
            right_spans.push(Span::styled(" │ ", Style::default().fg(Color::DarkGray)));
            right_spans.push(Span::styled(" ", Style::default().fg(Color::Magenta)));
            right_spans.push(Span::styled(
                branch,
                Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
            ));
            if !status.is_empty() {
                right_spans.push(Span::raw(" "));
                // Color the status parts
                for part in status.split_whitespace() {
                    let color = if part.starts_with('+') {
                        Color::Green
                    } else if part.starts_with('~') {
                        Color::Yellow
                    } else {
                        Color::Red
                    };
                    right_spans.push(Span::styled(format!("{} ", part), Style::default().fg(color)));
                }
            }
        }
        right_spans.push(Span::raw(" "));

        // Calculate widths
        let left_width: usize = left_spans.iter().map(|s| s.content.len()).sum();
        let right_width: usize = right_spans.iter().map(|s| s.content.len()).sum();
        let available = area.width as usize;

        // Add padding between left and right
        let padding = available.saturating_sub(left_width + right_width);

        let mut all_spans = left_spans;
        all_spans.push(Span::raw(" ".repeat(padding)));
        all_spans.extend(right_spans);

        let line = Line::from(all_spans);
        let paragraph = Paragraph::new(line).block(Block::default());
        paragraph.render(area, buf);
    }
}
