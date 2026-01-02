use crossterm::event::{KeyCode, KeyModifiers, MouseEventKind};

use crate::action::Action;
use crate::app::{App, AppState};
use crate::permissions::PermissionResponse;
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

            // Handle permission prompt input
            if matches!(app.state, AppState::AwaitingPermission) {
                return match key.code {
                    // Arrow keys for navigation
                    KeyCode::Up | KeyCode::Char('k') => Action::PermissionSelectPrev,
                    KeyCode::Down | KeyCode::Char('j') => Action::PermissionSelectNext,
                    // Enter to confirm selection
                    KeyCode::Enter => Action::PermissionConfirm,
                    // Direct keyboard shortcuts
                    KeyCode::Char('y') => Action::PermissionRespond(PermissionResponse::AllowOnce),
                    KeyCode::Char('a') => Action::PermissionRespond(PermissionResponse::AllowAlways),
                    KeyCode::Char('n') => Action::PermissionRespond(PermissionResponse::DenyOnce),
                    KeyCode::Char('!') => Action::PermissionRespond(PermissionResponse::DenyAlways),
                    // Escape to deny once
                    KeyCode::Esc => Action::PermissionRespond(PermissionResponse::DenyOnce),
                    _ => Action::None,
                };
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
/// Returns the permission response if a permission action was handled
pub fn apply_action(app: &mut App, action: Action) -> Option<PermissionResponse> {
    match action {
        Action::Quit => {
            app.should_quit = true;
            None
        }
        Action::SubmitMessage(text) => {
            app.add_user_message(text);
            app.start_generating();
            None
        }
        Action::AppendToken(token) => {
            app.append_token(token);
            None
        }
        Action::FinishStreaming => {
            app.finish_generating();
            None
        }
        Action::SetError(error) => {
            app.set_error(error);
            None
        }
        Action::ScrollUp(amount) => {
            app.scroll_up(amount);
            None
        }
        Action::ScrollDown(amount) => {
            app.scroll_down(amount);
            None
        }
        Action::ModelLoaded => {
            app.set_idle();
            None
        }
        Action::PermissionSelectPrev => {
            app.permission_select_prev();
            None
        }
        Action::PermissionSelectNext => {
            app.permission_select_next();
            None
        }
        Action::PermissionConfirm => {
            // Get the response from the current selection
            let response = app.get_permission_response();
            Some(response)
        }
        Action::PermissionRespond(response) => {
            // Direct response from keyboard shortcut
            Some(response)
        }
        Action::None => None,
    }
}
