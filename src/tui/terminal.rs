use std::io::{self, stderr, Stderr};

use color_eyre::Result;
use crossterm::{
    event::{
        DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

/// Type alias for our terminal
pub type Tui = Terminal<CrosstermBackend<Stderr>>;

/// Initialize the terminal for TUI mode
pub fn init() -> Result<Tui> {
    // Enable raw mode for direct input handling
    enable_raw_mode()?;

    // Enter alternate screen buffer and enable keyboard/mouse enhancements
    // The keyboard enhancement enables proper detection of Shift+Enter
    // Mouse capture enabled for scroll wheel support
    // Bracketed paste enables proper handling of pasted multiline text
    execute!(
        stderr(),
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste,
        PushKeyboardEnhancementFlags(
            KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
        )
    )?;

    // Create the terminal
    let backend = CrosstermBackend::new(stderr());
    let terminal = Terminal::new(backend)?;

    // Install panic hook to restore terminal on panic
    let panic_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = restore();
        panic_hook(info);
    }));

    Ok(terminal)
}

/// Restore the terminal to its original state
pub fn restore() -> io::Result<()> {
    disable_raw_mode()?;
    // Pop keyboard enhancements, disable mouse/paste, and leave alternate screen
    execute!(
        stderr(),
        PopKeyboardEnhancementFlags,
        DisableBracketedPaste,
        DisableMouseCapture,
        LeaveAlternateScreen
    )?;
    Ok(())
}
