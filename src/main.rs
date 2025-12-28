mod action;
mod app;
mod config;
mod model;
mod tui;
mod update;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use clap::Parser;
use color_eyre::Result;
use tokio::sync::mpsc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use app::App;
use model::{ChatMessage, InferenceConfig, MistralBackend, ModelBackend, ModelParams, StreamEvent};
use tui::{ui, Event, EventHandler};
use update::{apply_action, handle_event};

/// Pangu - Terminal-based agentic coding assistant
#[derive(Parser, Debug)]
#[command(name = "pangu")]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Path to the GGUF model file
    #[arg(short, long)]
    model: Option<PathBuf>,

    /// Path to config file
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Context window size
    #[arg(long, default_value = "8192")]
    context_size: u32,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize error handling
    color_eyre::install()?;

    // Initialize logging (to file to not interfere with TUI)
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "pangu=info".into()),
        )
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();

    // Parse CLI arguments
    let cli = Cli::parse();

    // Load configuration
    let mut settings = config::load_settings(cli.config.as_ref())?;

    // Override model path if provided via CLI
    if let Some(model_path) = cli.model {
        settings.model.path = model_path;
    }
    settings.model.context_size = cli.context_size;

    // Initialize terminal
    let mut terminal = tui::init()?;

    // Create application state
    let mut app = App::new(&settings);

    // Create event handler
    let mut events = EventHandler::new(settings.ui.tick_rate, settings.ui.frame_rate);
    let event_tx = events.sender();

    // Load model in background using std::sync::Mutex for spawn_blocking compatibility
    let model_path = settings.model.path.clone();
    let model_params = ModelParams {
        n_gpu_layers: settings.model.n_gpu_layers,
        context_size: settings.model.context_size,
    };

    let model: Arc<Mutex<Option<MistralBackend>>> = Arc::new(Mutex::new(None));
    let model_clone = model.clone();
    let event_tx_clone = event_tx.clone();

    // Spawn model loading task
    tokio::task::spawn_blocking(move || {
        match MistralBackend::load(&model_path, model_params) {
            Ok(loaded_model) => {
                let mut guard = model_clone.lock().unwrap();
                *guard = Some(loaded_model);
                let _ = event_tx_clone.send(Event::Tick); // Trigger UI update
            }
            Err(e) => {
                let _ = event_tx_clone.send(Event::StreamError(format!("Failed to load model: {}", e)));
            }
        }
    });

    // Inference configuration
    let inference_config = InferenceConfig {
        max_tokens: 4096,
        temperature: settings.model.temperature,
        top_p: settings.model.top_p,
        stop_sequences: vec![],
    };

    // Main event loop
    while !app.should_quit {
        // Check if model is loaded
        {
            let guard = model.lock().unwrap();
            if guard.is_some() && matches!(app.state, app::AppState::Loading) {
                app.set_idle();
            }
        }

        // Get next event
        if let Some(event) = events.next().await {
            match event {
                Event::Render => {
                    terminal.draw(|frame| ui::draw(frame, &app))?;
                }
                Event::Tick => {
                    app.tick();
                }
                _ => {
                    let action = handle_event(&mut app, event.clone());

                    // Check if we need to start generation
                    let should_generate = matches!(action, action::Action::SubmitMessage(_));

                    apply_action(&mut app, action);

                    // Start generation if needed
                    if should_generate {
                        // Clone what we need for the generation task
                        let messages: Vec<ChatMessage> = app.messages.clone();
                        let config = inference_config.clone();
                        let model_arc = model.clone();
                        let event_tx_gen = event_tx.clone();

                        // Run generation in blocking task
                        tokio::task::spawn_blocking(move || {
                            // Take model out of mutex for generation
                            let mut guard = model_arc.lock().unwrap();
                            if let Some(ref mut backend) = *guard {
                                // Create channel for streaming
                                let (stream_tx, stream_rx) = mpsc::unbounded_channel::<StreamEvent>();

                                // Spawn a thread to forward events
                                let event_tx_forward = event_tx_gen.clone();
                                std::thread::spawn(move || {
                                    let rt = tokio::runtime::Builder::new_current_thread()
                                        .enable_all()
                                        .build()
                                        .unwrap();
                                    rt.block_on(async {
                                        let mut rx = stream_rx;
                                        while let Some(stream_event) = rx.recv().await {
                                            let event = match stream_event {
                                                StreamEvent::Token(token) => Event::StreamToken(token),
                                                StreamEvent::Done => Event::StreamDone,
                                                StreamEvent::Error(e) => Event::StreamError(e),
                                            };
                                            if event_tx_forward.send(event).is_err() {
                                                break;
                                            }
                                        }
                                    });
                                });

                                // Generate
                                if let Err(e) = backend.generate_stream(&messages, &config, stream_tx) {
                                    let _ = event_tx_gen.send(Event::StreamError(e.to_string()));
                                }
                            } else {
                                let _ = event_tx_gen.send(Event::StreamError("Model not loaded".to_string()));
                            }
                        });
                    }
                }
            }
        }
    }

    // Cleanup
    tui::restore()?;

    Ok(())
}
