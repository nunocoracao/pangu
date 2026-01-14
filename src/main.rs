mod action;
mod app;
mod config;
mod embedded;
mod model;
mod rag;
mod session;
mod tools;
mod tui;
mod update;

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, RwLock};

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

use app::App;
use embedded::SETTINGS;
use model::{DownloadProgress, InferenceConfig, LlamaServerBackend, ModelBackend, ModelDownloader, ModelParams, StreamEvent, truncate_to_context};
use rag::{ConversationStore, Retriever};
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

    // Initialize logging to file in ~/.pangu/logs/ (stderr would corrupt TUI)
    let log_dir = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".pangu")
        .join("logs");
    let _ = std::fs::create_dir_all(&log_dir);
    let log_path = log_dir.join("pangu.log");
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .ok();
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

    // Create application state
    let mut app = App::new();

    // Set max context size for display
    app.set_max_context(context_size as usize);

    // Set the embedded welcome/system messages
    app.welcome_message = resources.welcome_message().to_string();

    // Set system prompt
    app.set_system_prompt(resources.system_prompt());

    // Initialize session manager and load previous session
    let project_root = std::env::current_dir().unwrap_or_default();
    let session_manager = session::SessionManager::new(&project_root);
    let loaded_session = session_manager.load();

    // Restore messages from previous session (excluding system messages)
    if !loaded_session.messages.is_empty() {
        app.messages = loaded_session.messages;
        app.total_input_tokens = loaded_session.total_input_tokens;
        app.total_output_tokens = loaded_session.total_output_tokens;
        tracing::info!(
            "Restored session with {} messages",
            app.messages.len()
        );
    }

    // Initialize RAG system (stores in ~/.pangu/history/)
    let conversation_store = Arc::new(
        ConversationStore::new()
            .expect("Failed to initialize conversation store")
    );
    let retriever = Arc::new(Retriever::new());

    // Start with empty history, load in background
    let rag_history = Arc::new(RwLock::new(Vec::new()));

    // Spawn background task to load RAG history
    {
        let rag_history = rag_history.clone();
        let conversation_store = conversation_store.clone();
        std::thread::spawn(move || {
            let loaded = conversation_store.load_all().unwrap_or_default();
            let count = loaded.len();
            let session_count = conversation_store.session_count();
            let project_id = conversation_store.project_id().to_string();
            let branch = conversation_store.branch().to_string();

            // Update the shared history
            if let Ok(mut history) = rag_history.write() {
                *history = loaded;
            }

            tracing::info!(
                "RAG loaded: {} historical messages from {} sessions (project: {}, branch: {})",
                count,
                session_count,
                project_id,
                branch
            );
        });
    }

    tracing::info!("RAG initialization started in background");

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
                _ => {
                    let action = handle_event(&mut app, event.clone());

                    // Check if we need to start generation
                    let should_generate = matches!(action, action::Action::SubmitMessage(_));

                    // Track if this is a FinishStreaming action (to store assistant message)
                    let is_finish_streaming = matches!(action, action::Action::FinishStreaming);

                    // Handle permission response - execute tool if granted
                    if let action::Action::HandlePermissionResponse { granted, always } = &action {
                        if let app::AppState::AwaitingPermission(ref pending) = app.state {
                            let pending_clone = pending.clone();

                            if *granted {
                                // Execute the pending tool
                                let tool_context = tools::ToolContext::new(
                                    std::env::current_dir().unwrap_or_default(),
                                    "default".to_string(),
                                );
                                let tool_registry = tools::ToolRegistry::new();

                                if let Some(tool) = tool_registry.get(&pending_clone.name) {
                                    // Build params from pending
                                    let mut params = tools::ToolParams::new();
                                    for (k, v) in &pending_clone.params {
                                        params.insert(k.clone(), v.clone());
                                    }

                                    match tool.execute(&params, &tool_context) {
                                        Ok(result) => {
                                            // Update the permission message to show result
                                            if let Some(last_msg) = app.messages.last_mut() {
                                                if last_msg.content.contains("[Permission Required]") {
                                                    *last_msg = model::ChatMessage::tool_result(
                                                        &pending_clone.name,
                                                        &result.output,
                                                    );
                                                }
                                            }
                                            tracing::info!("Tool {} executed after permission", pending_clone.name);

                                            // Store permission if "always"
                                            if *always {
                                                tracing::info!("Storing 'always allow' for {}", pending_clone.path);
                                                // TODO: Store in permission manager
                                            }
                                        }
                                        Err(e) => {
                                            if let Some(last_msg) = app.messages.last_mut() {
                                                if last_msg.content.contains("[Permission Required]") {
                                                    *last_msg = model::ChatMessage::tool_result(
                                                        &pending_clone.name,
                                                        format!("Error: {}", e),
                                                    );
                                                }
                                            }
                                            tracing::error!("Tool {} failed: {}", pending_clone.name, e);
                                        }
                                    }
                                }
                            } else {
                                // Permission denied - update message
                                if let Some(last_msg) = app.messages.last_mut() {
                                    if last_msg.content.contains("[Permission Required]") {
                                        *last_msg = model::ChatMessage::tool_result(
                                            &pending_clone.name,
                                            "Permission denied by user",
                                        );
                                    }
                                }
                                tracing::info!("Permission denied for tool {}", pending_clone.name);
                            }

                            // Save session after permission decision
                            if let Err(e) = session_manager.save_messages(
                                &app.messages,
                                app.total_input_tokens,
                                app.total_output_tokens,
                            ) {
                                tracing::warn!("Failed to save session after permission: {}", e);
                            }
                        }
                    }

                    // Handle clear session command
                    let is_clear_session = matches!(action, action::Action::ClearSession);

                    // Apply the action
                    apply_action(&mut app, action);

                    // Clear session file after /clear command
                    if is_clear_session {
                        if let Err(e) = session_manager.clear() {
                            tracing::warn!("Failed to clear session file: {}", e);
                        } else {
                            tracing::info!("Session file cleared");
                        }
                        // Skip the rest of generation logic
                        continue;
                    }

                    // Store assistant message to RAG after finishing
                    if is_finish_streaming {
                        // Get assistant message content and store to RAG
                        let assistant_content = app.messages.last()
                            .filter(|m| m.role == model::Role::Assistant)
                            .map(|m| m.content.clone());

                        if let Some(ref content) = assistant_content {
                            if let Some(assistant_msg) = app.messages.last() {
                                let _ = conversation_store.store(assistant_msg);
                            }
                            // Update context usage
                            app.update_context_usage(0);

                            // Parse for tool calls in the assistant's response
                            let (clean_content, tool_calls) = tools::parser::ToolCallParser::parse(content);

                            if !tool_calls.is_empty() {
                                tracing::info!("Detected {} tool call(s)", tool_calls.len());

                                // Update the assistant message - remove raw tool XML
                                // If clean_content is empty, remove the message entirely
                                if clean_content.trim().is_empty() {
                                    app.messages.pop(); // Remove the tool-call-only message
                                } else if let Some(last_msg) = app.messages.last_mut() {
                                    last_msg.content = clean_content;
                                }

                                // Track if we need to trigger follow-up generation
                                let mut should_continue_generation = false;

                                // Process tool calls
                                let tool_context = tools::ToolContext::new(
                                    std::env::current_dir().unwrap_or_default(),
                                    "default".to_string(),
                                );

                                for tool_call in tool_calls {
                                    let tool_name = &tool_call.name;
                                    let tool_registry = tools::ToolRegistry::new();

                                    if let Some(tool) = tool_registry.get(tool_name) {
                                        // Get path for permission check
                                        let path_str = tool_call.params.get("path").unwrap_or(".");
                                        let resolved_path = tool_context.resolve_path(path_str).ok();

                                        let perm_level = tool.permission_level(
                                            resolved_path.as_deref(),
                                            &tool_context,
                                        );

                                        if perm_level == tools::PermissionLevel::None {
                                            // Execute immediately - no permission needed
                                            match tool.execute(&tool_call.params, &tool_context) {
                                                Ok(result) => {
                                                    // Add tool result as a message
                                                    let result_msg = model::ChatMessage::tool_result(
                                                        tool_name,
                                                        &result.output,
                                                    );
                                                    app.messages.push(result_msg);
                                                    should_continue_generation = true;
                                                    tracing::info!("Tool {} executed successfully", tool_name);
                                                }
                                                Err(e) => {
                                                    let error_msg = model::ChatMessage::tool_result(
                                                        tool_name,
                                                        format!("Error: {}", e),
                                                    );
                                                    app.messages.push(error_msg);
                                                    should_continue_generation = true;
                                                    tracing::error!("Tool {} failed: {}", tool_name, e);
                                                }
                                            }
                                        } else {
                                            // Need permission - show inline prompt
                                            tracing::info!(
                                                "Tool {} needs permission for path: {}",
                                                tool_name,
                                                path_str
                                            );
                                            // Show permission request inline in chat
                                            let perm_msg = model::ChatMessage {
                                                role: model::Role::Tool,
                                                content: format!(
                                                    "[Permission Required]\nTool '{}' wants to access: {}\n\nPress 1 to Allow Once, 2 to Always Allow, 3 or Esc to Deny",
                                                    tool_name,
                                                    path_str
                                                ),
                                            };
                                            app.messages.push(perm_msg);

                                            // Store pending tool call
                                            let pending = app::PendingToolCall {
                                                name: tool_name.clone(),
                                                params: tool_call.params.iter()
                                                    .map(|(k, v)| (k.clone(), v.clone()))
                                                    .collect(),
                                                path: path_str.to_string(),
                                                is_write: false, // reads for now
                                            };
                                            app.set_awaiting_permission(pending);
                                        }
                                    } else {
                                        // Unknown tool
                                        let error_msg = model::ChatMessage::tool_result(
                                            tool_name,
                                            format!("Unknown tool: {}", tool_name),
                                        );
                                        app.messages.push(error_msg);
                                        should_continue_generation = true;
                                        tracing::warn!("Unknown tool requested: {}", tool_name);
                                    }
                                }

                                // After tool execution, trigger follow-up generation
                                // so the model can respond to the tool results
                                if should_continue_generation {
                                    tracing::info!("Triggering follow-up generation after tool execution");
                                    app.start_generating();

                                    // Get query for RAG (use original user message)
                                    let query = app.messages.iter()
                                        .rev()
                                        .find(|m| m.role == model::Role::User)
                                        .map(|m| m.content.clone())
                                        .unwrap_or_default();

                                    // Build context with tool results
                                    let history_guard = rag_history.read().unwrap();
                                    let system_prompt = resources.system_prompt();
                                    let (context_messages, rag_count) = retriever.build_context(
                                        &query,
                                        &history_guard,
                                        &app.messages,
                                        Some(system_prompt),
                                        SETTINGS.rag.max_rag_messages,
                                        SETTINGS.rag.max_recent_messages,
                                    );
                                    drop(history_guard);

                                    let truncated_context = truncate_to_context(
                                        &context_messages,
                                        context_size as usize,
                                        inference_config.max_tokens as usize,
                                    );

                                    app.update_context_usage_from_context(&truncated_context, rag_count);

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

                                            if let Err(e) = backend.generate_stream(&truncated_context, &config, stream_tx) {
                                                let _ = event_tx_gen.send(Event::StreamError(e.to_string()));
                                            }
                                        }
                                    });
                                }
                            }
                        }

                        // Save session after assistant response (even if no tool calls)
                        if let Err(e) = session_manager.save_messages(
                            &app.messages,
                            app.total_input_tokens,
                            app.total_output_tokens,
                        ) {
                            tracing::warn!("Failed to save session: {}", e);
                        }
                    }

                    // Start generation if needed
                    if should_generate {
                        // Get the last user message as query for RAG
                        let query = app.messages.iter()
                            .rev()
                            .find(|m| m.role == model::Role::User)
                            .map(|m| m.content.clone())
                            .unwrap_or_default();

                        // Build context with RAG (using config limits)
                        let history_guard = rag_history.read().unwrap();
                        let system_prompt = resources.system_prompt();
                        let (context_messages, rag_count) = retriever.build_context(
                            &query,
                            &history_guard,
                            &app.messages,
                            Some(system_prompt),
                            SETTINGS.rag.max_rag_messages,
                            SETTINGS.rag.max_recent_messages,
                        );
                        drop(history_guard); // Release lock early

                        // Truncate context to prevent overflow
                        let truncated_context = truncate_to_context(
                            &context_messages,
                            context_size as usize,
                            inference_config.max_tokens as usize,
                        );

                        // Update context usage display with actual context being sent
                        app.update_context_usage_from_context(&truncated_context, rag_count);

                        // Estimate input tokens and track them
                        let input_chars: usize = truncated_context.iter().map(|m| m.content.len()).sum();
                        let input_tokens = input_chars / 3; // ~3 chars per token (conservative)
                        app.add_input_tokens(input_tokens);

                        // Store the user message in RAG
                        if let Some(user_msg) = app.messages.iter().rev().find(|m| m.role == model::Role::User) {
                            let _ = conversation_store.store(user_msg);
                        }

                        // Save session after user message (in case generation fails)
                        if let Err(e) = session_manager.save_messages(
                            &app.messages,
                            app.total_input_tokens,
                            app.total_output_tokens,
                        ) {
                            tracing::warn!("Failed to save session after user message: {}", e);
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

                                // Generate with RAG-augmented context (truncated to fit)
                                if let Err(e) = backend.generate_stream(&truncated_context, &config, stream_tx) {
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

    // Save session before exit
    if let Err(e) = session_manager.save_messages(
        &app.messages,
        app.total_input_tokens,
        app.total_output_tokens,
    ) {
        tracing::warn!("Failed to save session on exit: {}", e);
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
