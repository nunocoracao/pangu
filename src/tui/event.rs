use std::time::Duration;

use crossterm::event::{self, Event as CrosstermEvent, KeyEvent, KeyEventKind, MouseEvent};
use futures::{FutureExt, StreamExt};
use tokio::{
    sync::mpsc::{self, UnboundedReceiver, UnboundedSender},
    task::JoinHandle,
};

/// Application events
#[derive(Debug, Clone)]
pub enum Event {
    /// Terminal tick for periodic updates
    Tick,
    /// Render frame
    Render,
    /// Key press
    Key(KeyEvent),
    /// Mouse event
    Mouse(MouseEvent),
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
                                        // Handle Ctrl+C for quit
                                        if key.code == event::KeyCode::Char('c')
                                            && key.modifiers.contains(event::KeyModifiers::CONTROL)
                                        {
                                            Event::Quit
                                        } else {
                                            Event::Key(key)
                                        }
                                    }
                                    CrosstermEvent::Mouse(mouse) => Event::Mouse(mouse),
                                    CrosstermEvent::Resize(w, h) => Event::Resize(w, h),
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
