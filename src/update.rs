use crossterm::event::{KeyCode, KeyModifiers, MouseEventKind};

use crate::action::Action;
use crate::app::{App, AppState};
use crate::tui::Event;
use crate::tui::components::chat_view::extract_selected_text;

/// Handle events and return an action
pub fn handle_event(app: &mut App, event: Event) -> Action {
    match event {
        Event::Quit => Action::Quit,

        Event::Key(key) => {
            // Handle Ctrl+C: copy if selection, else quit
            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                if app.has_selection() {
                    return Action::CopySelection;
                } else {
                    return Action::Quit;
                }
            }

            match key.code {
                // Scroll with Ctrl+U/D (always allowed, even during generation)
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    Action::ScrollUp(10)
                }
                KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    Action::ScrollDown(10)
                }
                // Page up/down (always allowed)
                KeyCode::PageUp => Action::ScrollUp(20),
                KeyCode::PageDown => Action::ScrollDown(20),
                // Arrow key scrolling when generating (since input is blocked)
                KeyCode::Up if matches!(app.state, AppState::Generating) => Action::ScrollUp(1),
                KeyCode::Down if matches!(app.state, AppState::Generating) => Action::ScrollDown(1),
                // Escape: cancel generation, deny permission, or clear selection
                KeyCode::Esc => {
                    if matches!(app.state, AppState::Generating) {
                        Action::CancelGeneration
                    } else if matches!(app.state, AppState::AwaitingPermission) {
                        Action::HandlePermissionResponse { granted: false, always: false }
                    } else if app.has_selection() {
                        Action::ClearSelection
                    } else {
                        Action::None
                    }
                }
                // Permission responses when awaiting permission
                KeyCode::Char('1') if matches!(app.state, AppState::AwaitingPermission) => {
                    Action::HandlePermissionResponse { granted: true, always: false }
                }
                KeyCode::Char('2') if matches!(app.state, AppState::AwaitingPermission) => {
                    Action::HandlePermissionResponse { granted: true, always: true }
                }
                KeyCode::Char('3') if matches!(app.state, AppState::AwaitingPermission) => {
                    Action::HandlePermissionResponse { granted: false, always: false }
                }
                KeyCode::Enter if matches!(app.state, AppState::AwaitingPermission) => {
                    // Enter defaults to Allow Once
                    Action::HandlePermissionResponse { granted: true, always: false }
                }
                // Block other input while generating (model is busy)
                _ if matches!(app.state, AppState::Generating) => Action::None,
                // Block input while awaiting permission
                _ if matches!(app.state, AppState::AwaitingPermission) => Action::None,
                // Forward to input box
                _ => {
                    if let Some(text) = app.input_box.handle_input(key) {
                        // Check for special commands
                        let trimmed = text.trim();
                        if trimmed == "/clear" || trimmed == "/new" {
                            return Action::ClearSession;
                        }
                        if trimmed == "/help" {
                            return Action::ShowHelp;
                        }
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

        // Handle pasted text (from bracketed paste mode)
        Event::Paste(text) => {
            // Don't allow paste while generating
            if matches!(app.state, AppState::Generating) {
                return Action::None;
            }
            // Insert pasted text into input box (preserves newlines)
            app.input_box.insert_text(&text);
            Action::None
        }

        Event::StreamToken(token) => Action::AppendToken(token),
        Event::StreamDone => Action::FinishStreaming,
        Event::StreamError(error) => Action::SetError(error),
        Event::InputTokens(count) => Action::AddInputTokens(count),

        // Handle mouse scroll events (always allowed)
        Event::Mouse(mouse) => {
            match mouse.kind {
                MouseEventKind::ScrollUp => Action::ScrollUp(3),
                MouseEventKind::ScrollDown => Action::ScrollDown(3),
                _ => Action::None,
            }
        }

        // Handle mouse selection events
        Event::MouseDown(col, row) => Action::SelectionStart(col, row),
        Event::MouseDrag(col, row) => Action::SelectionUpdate(col, row),
        Event::MouseUp(_col, _row) => Action::SelectionEnd,

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
        Action::CancelGeneration => {
            app.cancel_generation();
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
        Action::SelectionStart(col, row) => {
            app.start_selection(col, row);
        }
        Action::SelectionUpdate(col, row) => {
            app.update_selection(col, row);
        }
        Action::SelectionEnd => {
            app.finish_selection();
        }
        Action::CopySelection => {
            // Extract selected text and copy to clipboard
            if let Some(ref selection) = app.selection {
                let text = extract_selected_text(&app.chat_cache.lines, selection, app.scroll_offset);
                if !text.is_empty() {
                    if let Ok(mut clipboard) = arboard::Clipboard::new() {
                        let _ = clipboard.set_text(&text);
                        tracing::info!("Copied {} chars to clipboard", text.len());
                    }
                }
            }
            app.clear_selection();
        }
        Action::ClearSelection => {
            app.clear_selection();
        }
        Action::AddInputTokens(count) => {
            app.add_input_tokens(count);
        }
        Action::ProcessToolCall { name, params, raw: _ } => {
            // This is handled in main.rs event loop for now
            // The event loop will check permissions and execute
            tracing::info!("Processing tool call: {} with params: {:?}", name, params);
        }
        Action::ShowPermissionPrompt { tool_name, path, is_write } => {
            tracing::info!(
                "Permission requested for tool={}, path={}, is_write={}",
                tool_name,
                path,
                is_write
            );
            app.set_awaiting_permission();
        }
        Action::HandlePermissionResponse { granted, always } => {
            // Handled in main.rs - this just logs for now
            tracing::info!("Permission response: granted={}, always={}", granted, always);
            app.set_idle();
        }
        Action::ExecuteTool { name, params: _ } => {
            app.set_executing_tool(&name);
        }
        Action::AppendToolResult { tool_name, result, success } => {
            app.add_tool_result(&tool_name, &result, success);
            app.set_idle();
        }
        Action::ClearSession => {
            // Clear messages and reset state
            app.messages.clear();
            app.total_input_tokens = 0;
            app.total_output_tokens = 0;
            app.scroll_offset = 0;
            app.current_response.clear();
            app.set_idle();
            tracing::info!("Session cleared by user");
        }
        Action::ShowHelp => {
            app.messages.push(crate::model::ChatMessage::assistant(
                "Slash commands:\n- /clear: clear the current session\n- /new: same as /clear\n- /help: show this help\n\nPhase 1 tools are read-only and run via model tool calls: list_files, read_file, grep."
            ));
        }
        Action::None => {}
    }
}
