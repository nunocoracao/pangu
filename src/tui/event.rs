use std::time::Duration;

use crossterm::event::{Event as CrosstermEvent, KeyEvent, KeyEventKind, MouseEvent, MouseEventKind};
use futures::{FutureExt, StreamExt};
use tokio::{
    sync::mpsc::{self, UnboundedReceiver, UnboundedSender},
    task::JoinHandle,
};

/// Application events
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum Event {
    /// Terminal tick for periodic updates
    Tick,
    /// Render frame
    Render,
    /// Key press
    Key(KeyEvent),
    /// Pasted text (from bracketed paste)
    Paste(String),
    /// Mouse event (for scroll)
    Mouse(MouseEvent),
    /// Mouse button pressed at (column, row)
    MouseDown(u16, u16),
    /// Mouse dragged to (column, row)
    MouseDrag(u16, u16),
    /// Mouse button released at (column, row)
    MouseUp(u16, u16),
    /// Terminal resize
    Resize(u16, u16),
    /// Model download progress (downloaded, total, speed in bytes/sec)
    DownloadProgress(u64, u64, f64),
    /// Model download completed
    DownloadComplete,
    /// Model download error
    DownloadError(String),
    /// Streaming token from model
    StreamToken(String),
    /// Streaming completed
    StreamDone,
    /// Streaming error
    StreamError(String),
    /// Input tokens sent to model (for tracking)
    InputTokens(usize),
    /// Tool execution started
    ToolExecutionStart(String),
    /// Tool execution completed with result
    ToolExecutionDone(String, String), // (tool_name, result)
    /// Tool execution failed
    ToolExecutionError(String),
    /// Permission granted - execute the pending tool
    PermissionGranted(String, String), // (tool_name, tool_params)
    /// Permission denied - add denial message to chat
    PermissionDenied(String, String), // (tool_name, tool_params)
    /// Quit the application
    Quit,
}

/// Handles terminal events and provides an async stream of Events
pub struct EventHandler {
    /// Event receiver
    rx: UnboundedReceiver<Event>,
    /// Event sender (for external events like streaming)
    tx: UnboundedSender<Event>,
    /// Background task handle
    _task: JoinHandle<()>,
}

impl EventHandler {
    /// Create a new event handler
    pub fn new(tick_rate: f64, frame_rate: f64) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let tx_clone = tx.clone();

        let tick_duration = Duration::from_secs_f64(1.0 / tick_rate);
        let render_duration = Duration::from_secs_f64(1.0 / frame_rate);

        let task = tokio::spawn(async move {
            let mut reader = crossterm::event::EventStream::new();
            let mut tick_interval = tokio::time::interval(tick_duration);
            let mut render_interval = tokio::time::interval(render_duration);

            loop {
                let tick_delay = tick_interval.tick();
                let render_delay = render_interval.tick();
                let crossterm_event = reader.next().fuse();

                tokio::select! {
                    _ = tick_delay => {
                        if tx_clone.send(Event::Tick).is_err() {
                            break;
                        }
                    }
                    _ = render_delay => {
                        if tx_clone.send(Event::Render).is_err() {
                            break;
                        }
                    }
                    maybe_event = crossterm_event => {
                        match maybe_event {
                            Some(Ok(evt)) => {
                                let event = match evt {
                                    CrosstermEvent::Key(key) => {
                                        // Only handle key press events, ignore release/repeat
                                        // (keyboard enhancement flags cause release events)
                                        if key.kind != KeyEventKind::Press {
                                            continue;
                                        }
                                        // Pass all key events through - Ctrl+C is handled in update.rs
                                        // to support copy when text is selected
                                        Event::Key(key)
                                    }
                                    CrosstermEvent::Paste(text) => {
                                        // Bracketed paste - multiline text pasted at once
                                        Event::Paste(text)
                                    }
                                    CrosstermEvent::Resize(w, h) => Event::Resize(w, h),
                                    CrosstermEvent::Mouse(mouse) => {
                                        match mouse.kind {
                                            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                                                Event::Mouse(mouse)
                                            }
                                            MouseEventKind::Down(_) => {
                                                Event::MouseDown(mouse.column, mouse.row)
                                            }
                                            MouseEventKind::Drag(_) => {
                                                Event::MouseDrag(mouse.column, mouse.row)
                                            }
                                            MouseEventKind::Up(_) => {
                                                Event::MouseUp(mouse.column, mouse.row)
                                            }
                                            _ => continue,
                                        }
                                    }
                                    _ => continue,
                                };
                                if tx_clone.send(event).is_err() {
                                    break;
                                }
                            }
                            Some(Err(_)) => {}
                            None => break,
                        }
                    }
                }
            }
        });

        Self { rx, tx, _task: task }
    }

    /// Get the next event
    pub async fn next(&mut self) -> Option<Event> {
        self.rx.recv().await
    }

    /// Get a sender for external events (e.g., streaming tokens)
    pub fn sender(&self) -> UnboundedSender<Event> {
        self.tx.clone()
    }
}
