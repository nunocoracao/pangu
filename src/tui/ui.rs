use ratatui::{
    layout::{Constraint, Direction, Layout},
    Frame,
};

use super::components::{ChatView, SidePane, StatusBar};
use crate::app::App;

/// Render the application UI
pub fn draw(frame: &mut Frame, app: &mut App) {
    // Main vertical layout: content area and footer
    let main_vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),      // Content area (chat + side pane + input)
            Constraint::Length(1),   // Status bar (footer)
        ])
        .split(frame.area());

    // Content area: horizontal split for left (chat+input) and right (side pane)
    let content_horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(40),        // Left: chat + input
            Constraint::Length(28),     // Right: side pane
        ])
        .split(main_vertical[0]);

    // Left side: chat and input
    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),      // Chat area
            Constraint::Length(5),   // Input box
        ])
        .split(content_horizontal[0]);

    // Calculate view height (chat area minus borders)
    let view_height = left_chunks[0].height.saturating_sub(2);

    // Create chat view and get content height
    let chat = ChatView::new(
        &app.messages,
        &app.current_response,
        app.scroll_offset,
        app.state.is_generating(),
        app.state.is_loading(),
        app.tick,
        &app.welcome_message,
    );
    let content_height = chat.content_height();

    // Update app dimensions for auto-scroll
    app.update_dimensions(content_height, view_height);

    // Render chat view with updated scroll offset
    let chat = ChatView::new(
        &app.messages,
        &app.current_response,
        app.scroll_offset,
        app.state.is_generating(),
        app.state.is_loading(),
        app.tick,
        &app.welcome_message,
    );
    frame.render_widget(chat, left_chunks[0]);

    // Render input box
    app.input_box.render(left_chunks[1], frame);

    // Render side pane (tools, files)
    let side_pane = SidePane::new();
    frame.render_widget(side_pane, content_horizontal[1]);

    // Render status bar (spans full width, includes directory/git info)
    let status = StatusBar::new(&app.state, app.tick);
    frame.render_widget(status, main_vertical[1]);
}
