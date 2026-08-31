//! Centralized color theme.

use ratatui::style::Color;

// Core palette
pub const BG: Color = Color::Rgb(16, 20, 28);
pub const PRIMARY: Color = Color::Rgb(104, 196, 255);
pub const ACCENT: Color = Color::Rgb(143, 233, 176);
pub const USER_COLOR: Color = Color::Rgb(244, 187, 107);
pub const MUTED: Color = Color::Rgb(120, 132, 148);

// Status colors
pub const SUCCESS: Color = Color::Rgb(110, 214, 148);
pub const WARNING: Color = Color::Rgb(255, 196, 102);
pub const ERROR: Color = Color::Rgb(245, 123, 123);
pub const INFO: Color = Color::Rgb(148, 186, 255);

// Badge backgrounds
pub const BG_BADGE: Color = Color::Rgb(44, 50, 65);
pub const BRANCH_BG: Color = Color::Rgb(88, 109, 196);

// Git status colors
pub const GIT_STAGED: Color = SUCCESS;
pub const GIT_MODIFIED: Color = WARNING;
pub const GIT_UNTRACKED: Color = Color::Rgb(171, 181, 199);
