use ratatui::{
    layout::{Constraint, Direction, Layout},
    Frame,
};

use super::components::{ChatView, Header, LoadingScreen, LoadingState, StatusBar, header_height};
use crate::app::App;

/// Render the application UI
pub fn draw(frame: &mut Frame, app: &mut App) {
    // Show full-screen loading animation while downloading or loading model
    if app.state.is_downloading() {
        let (downloaded, total, speed) = app.download_progress
            .as_ref()
            .map(|p| (p.downloaded, p.total, p.speed))
            .unwrap_or((0, 0, 0.0));
        let state = LoadingState::Downloading { downloaded, total, speed };
        let loading = LoadingScreen::new(app.tick, &mut app.matrix_columns, state);
        frame.render_widget(loading, frame.area());
        return;
    }

    if app.state.is_loading() {
        let loading = LoadingScreen::new(app.tick, &mut app.matrix_columns, LoadingState::Loading);
        frame.render_widget(loading, frame.area());
        return;
    }

    // Calculate header height based on state
    let h_height = header_height(&app.state);

    // Main vertical layout: header, content area, and footer
    let main_vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(h_height), // Header with logo
            Constraint::Min(1),           // Content area (chat + input)
            Constraint::Length(1),        // Status bar (footer)
        ])
        .split(frame.area());

    // Render the animated header
    let header = Header::new(&app.state, app.tick);
    frame.render_widget(header, main_vertical[0]);

    // Content area: chat view and input box
    let content_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),      // Chat area
            Constraint::Length(5),   // Input box
        ])
        .split(main_vertical[1]);

    // Calculate view height (chat area minus borders)
    let view_height = content_chunks[0].height.saturating_sub(2);
    let chat_width = content_chunks[0].width;

    // Check if chat cache needs updating
    let is_generating = app.state.is_generating();
    let is_loading = app.state.is_loading();
    let needs_update = app.chat_cache.needs_update(
        &app.messages,
        &app.current_response,
        app.tick,
        chat_width,
        is_generating,
        is_loading,
    );

    if needs_update {
        // Format messages and update cache
        let mut chat = ChatView::new(
            &app.messages,
            &app.current_response,
            app.scroll_offset,
            is_generating,
            is_loading,
            app.tick,
            &app.welcome_message,
        ).with_width(chat_width);

        app.chat_cache.lines = chat.format_messages();
        app.chat_cache.update_metadata(
            &app.messages,
            &app.current_response,
            app.tick,
            chat_width,
            is_generating,
            is_loading,
        );
    }

    // Calculate content height from cached lines
    let content_height = ChatView::content_height_from_lines(&app.chat_cache.lines, chat_width);

    // Update app dimensions for auto-scroll
    app.update_dimensions(content_height, view_height);

    // Store chat area for mouse coordinate translation
    app.set_chat_area(content_chunks[0]);

    // Render chat view with cached lines and selection
    let chat = ChatView::new(
        &app.messages,
        &app.current_response,
        app.scroll_offset,
        is_generating,
        is_loading,
        app.tick,
        &app.welcome_message,
    )
    .with_cached_lines(&app.chat_cache.lines)
    .with_selection(app.selection.as_ref());

    frame.render_widget(chat, content_chunks[0]);

    // Render input box
    app.input_box.render(content_chunks[1], frame);

    // Render status bar (spans full width)
    let status = StatusBar::new(&app.state, app.tick);
    frame.render_widget(status, main_vertical[2]);
}
