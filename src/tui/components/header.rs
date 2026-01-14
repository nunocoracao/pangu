//! Animated header with PANGU logo and generating indicator

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Widget,
};

use crate::app::AppState;
use crate::tui::theme;

/// Pangu ASCII art logo (same as loading screen and welcome message)
const LOGO: &[&str] = &[
    "██████╗  █████╗ ███╗   ██╗ ██████╗ ██╗   ██╗",
    "██╔══██╗██╔══██╗████╗  ██║██╔════╝ ██║   ██║",
    "██████╔╝███████║██╔██╗ ██║██║  ███╗██║   ██║",
    "██╔═══╝ ██╔══██║██║╚██╗██║██║   ██║██║   ██║",
    "██║     ██║  ██║██║ ╚████║╚██████╔╝╚██████╔╝",
    "╚═╝     ╚═╝  ╚═╝╚═╝  ╚═══╝ ╚═════╝  ╚═════╝ ",
];

/// Wave characters for generating animation
const WAVE_CHARS: &[char] = &['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█', '▇', '▆', '▅', '▄', '▃', '▂'];

/// Animated header widget
pub struct Header<'a> {
    state: &'a AppState,
    tick: u64,
}

impl<'a> Header<'a> {
    pub fn new(state: &'a AppState, tick: u64) -> Self {
        Self { state, tick }
    }

    /// Get the pulsing color for the logo based on tick
    fn logo_color(&self) -> Color {
        // Cycle through colors: Cyan -> Blue -> Magenta -> Cyan
        let phase = (self.tick % 60) as f32 / 60.0;

        if phase < 0.33 {
            let t = phase / 0.33;
            interpolate_color(Color::Cyan, Color::Rgb(100, 100, 255), t)
        } else if phase < 0.66 {
            let t = (phase - 0.33) / 0.33;
            interpolate_color(Color::Rgb(100, 100, 255), Color::Magenta, t)
        } else {
            let t = (phase - 0.66) / 0.34;
            interpolate_color(Color::Magenta, Color::Cyan, t)
        }
    }

    /// Render the animated wave indicator for generating state
    fn render_wave(&self, buf: &mut Buffer, area: Rect) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let width = area.width as usize;

        // Create wave pattern that moves across the screen
        for x in 0..width {
            // Calculate wave position with phase shift based on position and tick
            let phase = (x as f32 / 3.0 + self.tick as f32 / 2.0) % WAVE_CHARS.len() as f32;
            let char_idx = phase as usize % WAVE_CHARS.len();
            let wave_char = WAVE_CHARS[char_idx];

            // Color gradient across the wave
            let color_phase = (x as f32 / width as f32 + self.tick as f32 / 30.0) % 1.0;
            let color = if color_phase < 0.33 {
                let t = color_phase / 0.33;
                interpolate_color(theme::PRIMARY, Color::Cyan, t)
            } else if color_phase < 0.66 {
                let t = (color_phase - 0.33) / 0.33;
                interpolate_color(Color::Cyan, Color::Magenta, t)
            } else {
                let t = (color_phase - 0.66) / 0.34;
                interpolate_color(Color::Magenta, theme::PRIMARY, t)
            };

            let cell_x = area.x + x as u16;
            if cell_x < area.x + area.width {
                if let Some(cell) = buf.cell_mut((cell_x, area.y)) {
                    cell.set_char(wave_char)
                        .set_style(Style::default().fg(color));
                }
            }
        }
    }

    /// Render the generating message with animation
    fn render_generating_message(&self, buf: &mut Buffer, area: Rect) {
        let message = "Generating... (Esc to cancel)";
        let msg_len = message.len();

        // Center the message
        let start_x = area.x + (area.width.saturating_sub(msg_len as u16)) / 2;

        // Animate each character with a wave of brightness
        for (i, ch) in message.chars().enumerate() {
            let x = start_x + i as u16;
            if x >= area.x + area.width {
                break;
            }

            // Calculate brightness wave
            let wave_phase = (i as f32 / 3.0 - self.tick as f32 / 4.0).sin();
            let brightness = ((wave_phase + 1.0) / 2.0 * 155.0 + 100.0) as u8;

            let color = Color::Rgb(brightness, brightness, brightness);
            if let Some(cell) = buf.cell_mut((x, area.y)) {
                cell.set_char(ch)
                    .set_style(Style::default().fg(color).add_modifier(Modifier::BOLD));
            }
        }
    }
}

impl Widget for Header<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 6 {
            return;
        }

        let logo_color = self.logo_color();
        let logo_width = LOGO[0].chars().count() as u16;
        let logo_height = LOGO.len() as u16;

        // Render the full logo centered
        for (i, line) in LOGO.iter().enumerate() {
            let y = area.y + i as u16;
            if y >= area.y + area.height {
                break;
            }

            let start_x = area.x + (area.width.saturating_sub(logo_width)) / 2;

            for (j, ch) in line.chars().enumerate() {
                let x = start_x + j as u16;
                if x >= area.x + area.width {
                    break;
                }

                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_char(ch)
                        .set_style(Style::default().fg(logo_color).add_modifier(Modifier::BOLD));
                }
            }
        }

        // If generating, show the wave animation below the logo
        if matches!(self.state, AppState::Generating) {
            let wave_y = area.y + logo_height;
            if wave_y < area.y + area.height {
                let wave_area = Rect::new(area.x, wave_y, area.width, 1);
                self.render_wave(buf, wave_area);
            }

            // Message below the wave
            let msg_y = area.y + logo_height + 1;
            if msg_y < area.y + area.height {
                let msg_area = Rect::new(area.x, msg_y, area.width, 1);
                self.render_generating_message(buf, msg_area);
            }
        }
    }
}

/// Interpolate between two colors
fn interpolate_color(from: Color, to: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);

    let (r1, g1, b1) = color_to_rgb(from);
    let (r2, g2, b2) = color_to_rgb(to);

    let r = (r1 as f32 + (r2 as f32 - r1 as f32) * t) as u8;
    let g = (g1 as f32 + (g2 as f32 - g1 as f32) * t) as u8;
    let b = (b1 as f32 + (b2 as f32 - b1 as f32) * t) as u8;

    Color::Rgb(r, g, b)
}

/// Convert a Color to RGB values
fn color_to_rgb(color: Color) -> (u8, u8, u8) {
    match color {
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Cyan => (0, 255, 255),
        Color::Blue => (0, 100, 255),
        Color::Magenta => (255, 0, 255),
        Color::Green => (0, 255, 0),
        Color::Yellow => (255, 255, 0),
        Color::White => (255, 255, 255),
        _ => (200, 200, 200),
    }
}

/// Height needed for the header
pub fn header_height(state: &AppState) -> u16 {
    if matches!(state, AppState::Generating) {
        8 // Logo (6) + wave (1) + message (1)
    } else {
        6 // Just the logo
    }
}
