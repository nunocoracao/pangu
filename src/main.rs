mod action;
mod app;
mod config;
mod embedded;
mod model;
mod permissions;
mod rag;
mod tools;
mod tui;
mod update;

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use clap::Parser;
use color_eyre::Result;
use tokio::sync::mpsc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Global PID of llama-server process for signal handler cleanup
static LLAMA_SERVER_PID: AtomicU32 = AtomicU32::new(0);

/// Set the llama-server PID for signal handler cleanup
pub fn set_llama_server_pid(pid: u32) {
    LLAMA_SERVER_PID.store(pid, Ordering::SeqCst);
    tracing::info!("Registered llama-server PID: {}", pid);
}

/// Kill the llama-server process if running
fn kill_llama_server() {
    let pid = LLAMA_SERVER_PID.load(Ordering::SeqCst);
    if pid != 0 {
        eprintln!("[pangu] Killing llama-server (pid: {})", pid);
        #[cfg(unix)]
        {
            // Use libc to send signals directly
            unsafe {
                // SIGTERM (15) for graceful shutdown
                libc::kill(pid as i32, libc::SIGTERM);
                // Give it a moment to shut down
                std::thread::sleep(std::time::Duration::from_millis(100));
                // SIGKILL (9) to force kill
                libc::kill(pid as i32, libc::SIGKILL);
            }
        }
        #[cfg(not(unix))]
        {
            let _ = std::process::Command::new("taskkill")
                .args(["/F", "/PID", &pid.to_string()])
                .status();
        }
        LLAMA_SERVER_PID.store(0, Ordering::SeqCst);
    }
}

use std::sync::RwLock;

use app::App;
use embedded::SETTINGS;
use model::{ChatMessage, DownloadProgress, InferenceConfig, LlamaServerBackend, ModelBackend, ModelDownloader, ModelParams, StreamEvent};
use permissions::Permission;
use rag::{ConversationStore, Retriever};
use tools::{parse_tool_calls, TodoList, ToolRegistry};
use tui::{ui, Event, EventHandler};
use update::{apply_action, handle_event};

/// Pangu - Terminal-based agentic coding assistant
#[derive(Parser, Debug)]
#[command(name = "pangu")]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Path to the GGUF model file (overrides config)
    #[arg(short, long)]
    model: Option<PathBuf>,

    /// Context window size (overrides config)
    #[arg(long)]
    context_size: Option<u32>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize error handling
    color_eyre::install()?;

    // Initialize logging to file (stderr would corrupt TUI)
    let log_file = std::fs::File::create("pangu.log").ok();
    if let Some(file) = log_file {
        tracing_subscriber::registry()
            .with(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "pangu=info".into()),
            )
            .with(tracing_subscriber::fmt::layer().with_writer(std::sync::Mutex::new(file)))
            .init();
    }

    // Extract embedded llama-server to ~/.pangu
    let resources = embedded::EmbeddedResources::extract()
        .map_err(|e| color_eyre::eyre::eyre!("Failed to extract embedded resources: {}", e))?;

    // Parse CLI arguments
    let cli = Cli::parse();

    // Use embedded settings, with CLI overrides
    let model_path = cli.model.unwrap_or_else(|| SETTINGS.model.model_path());
    let context_size = cli.context_size.unwrap_or(SETTINGS.model.context_size);

    // Initialize terminal
    let mut terminal = tui::init()?;

    // Set up signal handlers for cleanup
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};

        // Clone terminal handle for cleanup in signal handler
        tokio::spawn(async move {
            let mut sigint = signal(SignalKind::interrupt()).expect("Failed to set up SIGINT handler");
            let mut sigterm = signal(SignalKind::terminate()).expect("Failed to set up SIGTERM handler");

            tokio::select! {
                _ = sigint.recv() => {
                    eprintln!("\n[pangu] Received SIGINT, cleaning up...");
                }
                _ = sigterm.recv() => {
                    eprintln!("\n[pangu] Received SIGTERM, cleaning up...");
                }
            }

            // Kill llama-server
            kill_llama_server();

            // Restore terminal
            let _ = tui::restore();

            // Exit
            std::process::exit(0);
        });
    }

    // Create shared todo list for app and tools
    let todo_list = Arc::new(RwLock::new(TodoList::new()));

    // Create application state with shared todo list
    let mut app = App::with_todo_list(todo_list.clone());

    // Set max context size for display
    app.set_max_context(context_size as usize);

    // Set the embedded welcome/system messages
    app.welcome_message = resources.welcome_message().to_string();
    app.set_system_prompt(resources.system_prompt());

    // Initialize RAG system
    let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let conversation_store = Arc::new(
        ConversationStore::new(&current_dir)
            .expect("Failed to initialize conversation store")
    );
    let retriever = Arc::new(Retriever::new());

    // Load history for RAG
    let rag_history = Arc::new(RwLock::new(
        conversation_store.load_all().unwrap_or_default()
    ));

    tracing::info!(
        "RAG initialized: {} historical messages from {} sessions",
        rag_history.read().unwrap().len(),
        conversation_store.session_count()
    );

    // Create event handler
    let mut events = EventHandler::new(SETTINGS.ui.tick_rate, SETTINGS.ui.frame_rate);
    let event_tx = events.sender();

    // Model backend (will be set after loading)
    let model: Arc<Mutex<Option<LlamaServerBackend>>> = Arc::new(Mutex::new(None));

    // Model params
    let server_path = resources.llama_server_path.clone();
    let model_params = ModelParams {
        n_gpu_layers: SETTINGS.model.n_gpu_layers,
        context_size,
    };

    // Keep copy for later use
    let model_path_for_load = model_path.clone();

    if !model_path.exists() {
        // Model doesn't exist - start downloading
        app.set_downloading();
        let download_url = SETTINGS.model.download_url();
        let dest_path = model_path.clone();
        let event_tx_download = event_tx.clone();

        tokio::spawn(async move {
            let downloader = ModelDownloader::new();
            let (progress_tx, mut progress_rx) = mpsc::unbounded_channel::<DownloadProgress>();

            // Forward progress to event system
            let event_tx_progress = event_tx_download.clone();
            tokio::spawn(async move {
                while let Some(progress) = progress_rx.recv().await {
                    let _ = event_tx_progress.send(Event::DownloadProgress(
                        progress.downloaded,
                        progress.total,
                        progress.speed,
                    ));
                }
            });

            // Start download
            match downloader.download(&download_url, &dest_path, progress_tx).await {
                Ok(()) => {
                    let _ = event_tx_download.send(Event::DownloadComplete);
                }
                Err(e) => {
                    let _ = event_tx_download.send(Event::DownloadError(e.to_string()));
                }
            }
        });
    } else {
        // Model exists - start loading immediately
        let model_clone = model.clone();
        let event_tx_clone = event_tx.clone();
        let params = model_params.clone();

        tokio::task::spawn_blocking(move || {
            match LlamaServerBackend::load_with_server(&model_path, &server_path, params) {
                Ok(loaded_model) => {
                    let mut guard = model_clone.lock().unwrap();
                    *guard = Some(loaded_model);
                    let _ = event_tx_clone.send(Event::Render);
                }
                Err(e) => {
                    let _ = event_tx_clone.send(Event::StreamError(format!("Failed to load model: {}", e)));
                }
            }
        });
    }

    // Inference configuration
    let inference_config = InferenceConfig {
        max_tokens: 4096,
        temperature: SETTINGS.model.temperature,
        top_p: SETTINGS.model.top_p,
        stop_sequences: vec![],
    };

    // Tool registry for agent capabilities (shares todo list with app)
    let tool_registry = Arc::new(ToolRegistry::with_todo_list(todo_list));

    // Set available tool names on app for UI display
    app.set_tool_names(tool_registry.tool_names().to_vec());

    // Keep copies for model loading after download
    let server_path_for_load = resources.llama_server_path.clone();

    // Main event loop
    while !app.should_quit {
        // Check if model is loaded
        {
            let guard = model.lock().unwrap();
            if guard.is_some() && !app.model_ready {
                app.set_model_ready();
                if matches!(app.state, app::AppState::Loading) {
                    app.set_idle();
                }
                // Force full terminal redraw after model loads
                terminal.clear()?;
            }
        }

        // Get next event
        if let Some(event) = events.next().await {
            match event {
                Event::Render => {
                    terminal.draw(|frame| ui::draw(frame, &mut app))?;
                }
                Event::Tick => {
                    app.tick();
                }
                Event::DownloadProgress(downloaded, total, speed) => {
                    app.update_download_progress(DownloadProgress {
                        downloaded,
                        total,
                        speed,
                    });
                }
                Event::DownloadComplete => {
                    // Download finished - start loading the model
                    app.set_loading();
                    terminal.clear()?;

                    let model_clone = model.clone();
                    let event_tx_clone = event_tx.clone();
                    let model_path = model_path_for_load.clone();
                    let server_path = server_path_for_load.clone();
                    let params = model_params.clone();

                    tokio::task::spawn_blocking(move || {
                        match LlamaServerBackend::load_with_server(&model_path, &server_path, params) {
                            Ok(loaded_model) => {
                                let mut guard = model_clone.lock().unwrap();
                                *guard = Some(loaded_model);
                                let _ = event_tx_clone.send(Event::Render);
                            }
                            Err(e) => {
                                let _ = event_tx_clone.send(Event::StreamError(format!("Failed to load model: {}", e)));
                            }
                        }
                    });
                }
                Event::DownloadError(e) => {
                    app.set_error(format!("Download failed: {}", e));
                }
                // Handle tool execution events
                Event::ToolExecutionStart(tool_name) => {
                    app.start_tool_execution(&tool_name);
                }
                Event::ToolExecutionDone(tool_name, result) => {
                    // Add tool result to messages
                    app.add_tool_result(&tool_name, result);
                    // Continue generation with tool results
                    app.start_generating();

                    // Clone what we need for the generation task
                    let messages: Vec<ChatMessage> = app.messages.clone();
                    let config = inference_config.clone();
                    let model_arc = model.clone();
                    let event_tx_gen = event_tx.clone();

                    // Run generation in blocking task
                    tokio::task::spawn_blocking(move || {
                        let mut guard = model_arc.lock().unwrap();
                        if let Some(ref mut backend) = *guard {
                            let (stream_tx, stream_rx) = mpsc::unbounded_channel::<StreamEvent>();

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

                            if let Err(e) = backend.generate_stream(&messages, &config, stream_tx) {
                                let _ = event_tx_gen.send(Event::StreamError(e.to_string()));
                            }
                        }
                    });
                }
                Event::ToolExecutionError(error) => {
                    app.set_error(format!("Tool execution failed: {}", error));
                }
                Event::PermissionGranted(tool_name, tool_params) => {
                    // Execute the tool now that permission is granted
                    let registry = tool_registry.clone();
                    let event_tx_tool = event_tx.clone();

                    let _ = event_tx_tool.send(Event::ToolExecutionStart(tool_name.clone()));

                    tokio::spawn(async move {
                        match registry.execute(&tools::ToolCall {
                            name: tool_name.clone(),
                            params: tool_params,
                            start: 0,
                            end: 0,
                        }).await {
                            Ok(result) => {
                                let _ = event_tx_tool.send(Event::ToolExecutionDone(tool_name, result));
                            }
                            Err(e) => {
                                let _ = event_tx_tool.send(Event::ToolExecutionError(e.to_string()));
                            }
                        }
                    });
                }
                Event::PermissionDenied(tool_name, _tool_params) => {
                    // Add a denial message to the chat
                    app.add_tool_result(&tool_name, format!("[Permission denied for {} tool]", tool_name));
                    // Continue generation with the denial message
                    app.start_generating();

                    let messages: Vec<ChatMessage> = app.messages.clone();
                    let config = inference_config.clone();
                    let model_arc = model.clone();
                    let event_tx_gen = event_tx.clone();

                    tokio::task::spawn_blocking(move || {
                        let mut guard = model_arc.lock().unwrap();
                        if let Some(ref mut backend) = *guard {
                            let (stream_tx, stream_rx) = mpsc::unbounded_channel::<StreamEvent>();

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

                            if let Err(e) = backend.generate_stream(&messages, &config, stream_tx) {
                                let _ = event_tx_gen.send(Event::StreamError(e.to_string()));
                            }
                        }
                    });
                }
                _ => {
                    let action = handle_event(&mut app, event.clone());

                    // Check if we need to start generation
                    let should_generate = matches!(action, action::Action::SubmitMessage(_));

                    // Check if this is a permission response
                    let is_permission_action = matches!(
                        action,
                        action::Action::PermissionConfirm
                            | action::Action::PermissionRespond(_)
                            | action::Action::PermissionSelectPrev
                            | action::Action::PermissionSelectNext
                    );

                    // Check if this is FinishStreaming - we need to check for tool calls first
                    if matches!(action, action::Action::FinishStreaming) {
                        let tool_calls = parse_tool_calls(&app.current_response);

                        if !tool_calls.is_empty() {
                            // Save the assistant's partial response before tool execution
                            if !app.current_response.is_empty() {
                                app.messages.push(ChatMessage::assistant(&app.current_response));
                                app.current_response.clear();
                            }

                            // Check permissions and execute tool calls
                            for tool_call in tool_calls {
                                let tool_name = tool_call.name.clone();
                                let tool_params = tool_call.params.clone();

                                // Skip permission check for safe tools (think, todo)
                                let safe_tools = ["think", "todo"];
                                let needs_permission = !safe_tools.contains(&tool_name.as_str());

                                // Check permission (or auto-allow for safe tools)
                                let (permission, _is_stored) = if needs_permission {
                                    app.permission_manager.check_permission(&tool_name, &tool_params)
                                } else {
                                    (Permission::Always, false)
                                };

                                match permission {
                                    Permission::Always => {
                                        // Permission granted - execute immediately
                                        let registry = tool_registry.clone();
                                        let event_tx_tool = event_tx.clone();
                                        let name = tool_name.clone();
                                        let params = tool_params.clone();

                                        let _ = event_tx_tool.send(Event::ToolExecutionStart(name.clone()));

                                        tokio::spawn(async move {
                                            match registry.execute(&tools::ToolCall {
                                                name: name.clone(),
                                                params,
                                                start: 0,
                                                end: 0,
                                            }).await {
                                                Ok(result) => {
                                                    let _ = event_tx_tool.send(Event::ToolExecutionDone(name, result));
                                                }
                                                Err(e) => {
                                                    let _ = event_tx_tool.send(Event::ToolExecutionError(e.to_string()));
                                                }
                                            }
                                        });
                                    }
                                    Permission::Never => {
                                        // Permission denied - add denial message
                                        let _ = event_tx.send(Event::PermissionDenied(tool_name, tool_params));
                                    }
                                    Permission::Ask => {
                                        // Need to ask user - show permission prompt
                                        app.request_permission(&tool_name, &tool_params);
                                        // Don't continue processing - wait for user response
                                        continue;
                                    }
                                }
                            }
                            // Don't apply FinishStreaming - we'll continue after tool execution
                            continue;
                        }
                    }

                    // Track if this is a FinishStreaming action (to store assistant message)
                    let is_finish_streaming = matches!(action, action::Action::FinishStreaming);

                    // Apply the action and check for permission response
                    let permission_response = apply_action(&mut app, action);

                    // Store assistant message to RAG after finishing
                    if is_finish_streaming {
                        if let Some(assistant_msg) = app.messages.last() {
                            if assistant_msg.role == model::Role::Assistant {
                                let _ = conversation_store.store(assistant_msg);
                                // Update context usage
                                app.update_context_usage(0);
                            }
                        }
                    }

                    // Handle permission response
                    if let Some(response) = permission_response {
                        if let Some(pending) = app.handle_permission_response(response) {
                            // Check if permission was granted or denied
                            let (permission, _persist) = response.to_permission_and_persist();
                            if permission == Permission::Always {
                                // Send permission granted event
                                let _ = event_tx.send(Event::PermissionGranted(
                                    pending.tool_name,
                                    pending.tool_params,
                                ));
                            } else {
                                // Send permission denied event
                                let _ = event_tx.send(Event::PermissionDenied(
                                    pending.tool_name,
                                    pending.tool_params,
                                ));
                            }
                        }
                        continue;
                    }

                    // Start generation if needed
                    if should_generate {
                        // Get the last user message as query for RAG
                        let query = app.messages.iter()
                            .rev()
                            .find(|m| m.role == model::Role::User)
                            .map(|m| m.content.clone())
                            .unwrap_or_default();

                        // Build context with RAG (max 10 RAG messages + last 30 recent)
                        let history = rag_history.read().unwrap().clone();
                        let system_prompt = resources.system_prompt();
                        let (context_messages, rag_count) = retriever.build_context(
                            &query,
                            &history,
                            &app.messages,
                            Some(system_prompt),
                            10,  // max RAG messages
                            30,  // max recent messages
                        );

                        // Update context usage display
                        app.update_context_usage(rag_count);

                        // Store the user message in RAG
                        if let Some(user_msg) = app.messages.iter().rev().find(|m| m.role == model::Role::User) {
                            let _ = conversation_store.store(user_msg);
                        }

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

                                // Generate with RAG-augmented context
                                if let Err(e) = backend.generate_stream(&context_messages, &config, stream_tx) {
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

    // Explicitly shutdown llama-server by dropping the backend
    {
        let mut guard = model.lock().unwrap();
        if let Some(backend) = guard.take() {
            drop(backend); // This triggers the Drop impl which kills llama-server
        }
    }

    Ok(())
}
