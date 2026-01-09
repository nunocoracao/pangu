use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{OnceLock, RwLock};
use std::time::{Duration, Instant};

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Widget},
};

use crate::app::AppState;

/// Spinner frames for loading animation
const SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Cached working directory (computed once)
static WORKING_DIR: OnceLock<String> = OnceLock::new();

/// Cached git info with refresh tracking
struct CachedGitInfo {
    info: Option<GitInfo>,
    last_refresh: Instant,
}

/// Global git cache using RwLock for safe concurrent access
static GIT_INFO_CACHE: OnceLock<RwLock<Option<CachedGitInfo>>> = OnceLock::new();

/// Flag to track if a background refresh is in progress
static GIT_REFRESH_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

/// Git info refresh interval (5 seconds)
const GIT_REFRESH_INTERVAL: Duration = Duration::from_secs(5);

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

fn get_working_dir() -> &'static str {
    WORKING_DIR.get_or_init(|| {
        std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "?".to_string())
    })
}

/// Git information: branch, status, and origin URL
#[derive(Clone)]
struct GitInfo {
    branch: String,
    status: String,
    origin_url: Option<String>,
}

fn fetch_git_info() -> Option<GitInfo> {
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

    // Get origin URL
    let origin_url = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !url.is_empty() {
                    // Clean up the URL for display
                    let display_url = url
                        .trim_start_matches("git@")
                        .trim_start_matches("https://")
                        .trim_start_matches("http://")
                        .trim_end_matches(".git")
                        .replace(':', "/");
                    Some(display_url)
                } else {
                    None
                }
            } else {
                None
            }
        });

    Some(GitInfo {
        branch,
        status,
        origin_url,
    })
}

/// Spawn a background thread to fetch git info
fn spawn_git_refresh() {
    // Only spawn if not already in progress
    if GIT_REFRESH_IN_PROGRESS
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
    {
        std::thread::spawn(|| {
            let info = fetch_git_info();
            let cache = GIT_INFO_CACHE.get_or_init(|| RwLock::new(None));

            if let Ok(mut guard) = cache.write() {
                *guard = Some(CachedGitInfo {
                    info,
                    last_refresh: Instant::now(),
                });
            }

            GIT_REFRESH_IN_PROGRESS.store(false, Ordering::SeqCst);
        });
    }
}

/// Get cached git info, spawning background refresh if stale
fn get_git_info() -> Option<GitInfo> {
    let cache = GIT_INFO_CACHE.get_or_init(|| RwLock::new(None));

    // Try to read from cache
    if let Ok(guard) = cache.read() {
        if let Some(ref cached) = *guard {
            let now = Instant::now();
            if now.duration_since(cached.last_refresh) < GIT_REFRESH_INTERVAL {
                return cached.info.clone();
            }
        }
    }

    // Cache is stale or empty - spawn background refresh
    spawn_git_refresh();

    // Return current cached value (or None if first call)
    if let Ok(guard) = cache.read() {
        if let Some(ref cached) = *guard {
            return cached.info.clone();
        }
    }

    None
}

impl Widget for StatusBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let spinner_frame = SPINNER[(self.tick as usize / 2) % SPINNER.len()];

        let (status_text, status_style) = match self.state {
            AppState::Idle => ("Ready".to_string(), Style::default().fg(Color::Green)),
            AppState::Generating => (
                format!("{} Generating... (Esc to cancel)", spinner_frame),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            AppState::Downloading => (
                format!("{} Downloading model...", spinner_frame),
                Style::default()
                    .fg(Color::Magenta)
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
        ];

        // Right side: path badge and git badge
        let working_dir = get_working_dir();
        let git_info = get_git_info();

        let mut right_spans = Vec::new();

        // Git info badge (if available)
        if let Some(info) = &git_info {
            // Origin URL badge
            if let Some(url) = &info.origin_url {
                right_spans.push(Span::styled(
                    format!(" {} ", url),
                    Style::default()
                        .fg(Color::White)
                        .bg(Color::Rgb(60, 60, 60)),
                ));
                right_spans.push(Span::raw(" "));
            }

            // Branch badge
            let mut branch_text = format!(" {} ", info.branch);
            if !info.status.is_empty() {
                branch_text = format!(" {} {} ", info.branch, info.status);
            }
            right_spans.push(Span::styled(
                branch_text,
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            ));
            right_spans.push(Span::raw(" "));
        }

        // Path badge
        right_spans.push(Span::styled(
            format!(" {} ", working_dir),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow),
        ));

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
