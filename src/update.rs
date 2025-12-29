use crossterm::event::{KeyCode, KeyModifiers, MouseEventKind};

use crate::action::Action;
use crate::app::{App, AppState};
use crate::tui::Event;

/// Handle events and return an action
pub fn handle_event(app: &mut App, event: Event) -> Action {
    match event {
        Event::Quit => Action::Quit,

        Event::Key(key) => {
            // Block all input while generating (model is busy)
            if matches!(app.state, AppState::Generating) {
                return Action::None;
            }

            match key.code {
                // Scroll with Ctrl+U/D
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    Action::ScrollUp(10)
                }
                KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    Action::ScrollDown(10)
                }
                // Page up/down
                KeyCode::PageUp => Action::ScrollUp(20),
                KeyCode::PageDown => Action::ScrollDown(20),
                // Forward to input box
                _ => {
                    if let Some(text) = app.input_box.handle_input(key) {
                        // Only allow submission if model is ready
                        if app.model_ready {
                            Action::SubmitMessage(text)
                        } else {
                            // Put the text back since we can't submit
                            app.input_box.set_text(&text);
                            Action::None
                        }
                    } else {
                        Action::None
                    }
                }
            }
        }

        Event::StreamToken(token) => Action::AppendToken(token),
        Event::StreamDone => Action::FinishStreaming,
        Event::StreamError(error) => Action::SetError(error),

        Event::Mouse(mouse) => match mouse.kind {
            MouseEventKind::ScrollUp => Action::ScrollUp(3),
            MouseEventKind::ScrollDown => Action::ScrollDown(3),
            _ => Action::None,
        },

        _ => Action::None,
    }
}

/// Apply an action to the application state
pub fn apply_action(app: &mut App, action: Action) {
    match action {
        Action::Quit => {
            app.should_quit = true;
        }
        Action::SubmitMessage(text) => {
            app.add_user_message(text);
            app.start_generating();
        }
        Action::AppendToken(token) => {
            app.append_token(token);
        }
        Action::FinishStreaming => {
            app.finish_generating();
        }
        Action::SetError(error) => {
            app.set_error(error);
        }
        Action::ScrollUp(amount) => {
            app.scroll_up(amount);
        }
        Action::ScrollDown(amount) => {
            app.scroll_down(amount);
        }
        Action::ModelLoaded => {
            app.set_idle();
        }
        Action::None => {}
    }
}
