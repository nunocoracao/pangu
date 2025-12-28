use ratatui::{
    layout::{Constraint, Direction, Layout},
    Frame,
};

use super::components::{ChatView, StatusBar};
use crate::app::App;

/// Render the application UI
pub fn draw(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),      // Chat area
            Constraint::Length(5),   // Input box
            Constraint::Length(1),   // Status bar
        ])
        .split(frame.area());

    // Render chat view
    let chat = ChatView::new(
        &app.messages,
        &app.current_response,
        app.scroll_offset,
        app.state.is_generating(),
    );
    frame.render_widget(chat, chunks[0]);

    // Render input box
    app.input_box.render(chunks[1], frame);

    // Render status bar
    let status = StatusBar::new(&app.model_name, &app.state, app.tick);
    frame.render_widget(status, chunks[2]);
}
