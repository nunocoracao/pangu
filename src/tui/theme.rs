//! Centralized color theme for consistent UI styling

use ratatui::style::Color;

// Primary brand color - soft blue
pub const PRIMARY: Color = Color::Rgb(100, 180, 255);

// Accent color - soft green for assistant messages
pub const ACCENT: Color = Color::Rgb(130, 200, 130);

// User message color - lighter blue
pub const USER_COLOR: Color = Color::Rgb(140, 200, 250);

// Muted color - for borders, separators, secondary text
pub const MUTED: Color = Color::Rgb(90, 90, 100);

// Status colors
pub const SUCCESS: Color = Color::Rgb(120, 200, 120);
pub const WARNING: Color = Color::Rgb(230, 180, 80);
pub const ERROR: Color = Color::Rgb(220, 100, 100);
pub const INFO: Color = Color::Rgb(130, 170, 220);

// Background colors
pub const BG_BADGE: Color = Color::Rgb(50, 52, 60);

// Git status colors
pub const GIT_STAGED: Color = Color::Rgb(120, 200, 120);
pub const GIT_MODIFIED: Color = Color::Rgb(230, 180, 80);
pub const GIT_UNTRACKED: Color = Color::Rgb(140, 140, 150);

// Branch badge color
pub const BRANCH_BG: Color = Color::Rgb(140, 100, 180);
