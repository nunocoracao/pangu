use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use clap::Parser;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, Mutex};
use tokio::time::{sleep, Duration};

#[derive(Parser, Debug)]
#[command(name = "pangu-core")]
#[command(about = "Local daemon for task orchestration and agent execution")]
struct Cli {
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
    #[arg(long, default_value_t = 11435)]
    port: u16,
    #[arg(long)]
    data_dir: Option<PathBuf>,
    #[arg(long)]
    telegram_token: Option<String>,
    #[arg(long, default_value_t = 10)]
    telegram_poll_interval_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum TaskKind {
    Chat,
    Schedule,
    Browser,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum TaskStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum TaskPayload {
    Chat(ChatTask),
    Schedule(ScheduleTask),
    Browser(BrowserTask),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatTask {
    message: String,
    source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ScheduleTask {
    job_name: String,
    trigger: String,
    task_prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BrowserTask {
    headless: bool,
    steps: Vec<BrowserStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum BrowserStep {
    Navigate { url: String },
    Click { selector: String },
    Fill { selector: String, value: String },
    WaitFor { selector: String, timeout_ms: Option<u64> },
    ExtractText { selector: String },
    Screenshot { path: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TaskRecord {
    id: String,
    kind: TaskKind,
    status: TaskStatus,
    payload: TaskPayload,
    source: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    result: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CreateTaskRequest {
    kind: TaskKind,
    payload: TaskPayload,
    #[serde(default = "default_source")]
    source: String,
}

fn default_source() -> String {
    "local".to_string()
}

#[derive(Debug, Serialize)]
struct CreateTaskResponse {
    task_id: String,
    status: TaskStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ScheduleStatus {
    Active,
    Paused,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ScheduleRecord {
    id: String,
    name: String,
    interval_seconds: u64,
    task: CreateTaskRequest,
    status: ScheduleStatus,
    next_run_at: DateTime<Utc>,
    last_run_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct CreateScheduleRequest {
    name: String,
    interval_seconds: u64,
    task: CreateTaskRequest,
}

#[derive(Debug, Serialize)]
struct CreateScheduleResponse {
    schedule_id: String,
    next_run_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MemoryEvent {
    id: String,
    timestamp: DateTime<Utc>,
    kind: String,
    source: String,
    content: String,
    tags: Vec<String>,
    metadata: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct CreateMemoryEventRequest {
    kind: String,
    #[serde(default = "default_source")]
    source: String,
    content: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    metadata: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct MemorySearchRequest {
    query: String,
    #[serde(default = "default_search_limit")]
    limit: usize,
}

fn default_search_limit() -> usize {
    10
}

#[derive(Debug, Serialize)]
struct MemorySearchResponse {
    query: String,
    results: Vec<MemoryEvent>,
}

#[derive(Debug, Deserialize)]
struct LimitQuery {
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    queued_tasks: usize,
    total_tasks: usize,
    active_schedules: usize,
    memory_events: u64,
    telegram_enabled: bool,
}

struct MemoryStore {
    events_path: PathBuf,
    next_event_id: AtomicU64,
}

impl MemoryStore {
    fn new(data_dir: &Path) -> Result<Self, String> {
        let memory_dir = data_dir.join("memory");
        fs::create_dir_all(&memory_dir)
            .map_err(|e| format!("failed to create memory dir: {}", e))?;
        let events_path = memory_dir.join("events.jsonl");

        if !events_path.exists() {
            File::create(&events_path)
                .map_err(|e| format!("failed to create memory events file: {}", e))?;
        }

        let mut max_id = 0_u64;
        let file = File::open(&events_path)
            .map_err(|e| format!("failed to open memory events file: {}", e))?;
        let reader = BufReader::new(file);
        for line in reader.lines().map_while(Result::ok) {
            if let Ok(event) = serde_json::from_str::<MemoryEvent>(&line) {
                if let Some(num) = event.id.strip_prefix("mem-") {
                    if let Ok(parsed) = num.parse::<u64>() {
                        max_id = max_id.max(parsed);
                    }
                }
            }
        }

        Ok(Self {
            events_path,
            next_event_id: AtomicU64::new(max_id + 1),
        })
    }

    fn append(&self, req: CreateMemoryEventRequest) -> Result<MemoryEvent, String> {
        let event_id = self.next_event_id.fetch_add(1, Ordering::SeqCst);
        let event = MemoryEvent {
            id: format!("mem-{event_id:08}"),
            timestamp: Utc::now(),
            kind: req.kind,
            source: req.source,
            content: req.content,
            tags: req.tags,
            metadata: req.metadata,
        };

        let encoded = serde_json::to_string(&event)
            .map_err(|e| format!("failed to encode memory event: {}", e))?;
        let mut file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&self.events_path)
            .map_err(|e| format!("failed to open memory events for append: {}", e))?;
        writeln!(file, "{}", encoded)
            .map_err(|e| format!("failed to write memory event: {}", e))?;
        Ok(event)
    }

    fn list(&self, limit: usize) -> Result<Vec<MemoryEvent>, String> {
        let file = File::open(&self.events_path)
            .map_err(|e| format!("failed to open memory events: {}", e))?;
        let reader = BufReader::new(file);
        let mut events = Vec::new();
        for line in reader.lines().map_while(Result::ok) {
            if let Ok(event) = serde_json::from_str::<MemoryEvent>(&line) {
                events.push(event);
            }
        }
        if events.len() > limit {
            Ok(events[events.len() - limit..].to_vec())
        } else {
            Ok(events)
        }
    }

    fn search(&self, query: &str, limit: usize) -> Result<Vec<MemoryEvent>, String> {
        let q = query.to_lowercase();
        let file = File::open(&self.events_path)
            .map_err(|e| format!("failed to open memory events: {}", e))?;
        let reader = BufReader::new(file);
        let mut scored = Vec::<(i32, MemoryEvent)>::new();

        for line in reader.lines().map_while(Result::ok) {
            let Ok(event) = serde_json::from_str::<MemoryEvent>(&line) else {
                continue;
            };
            let mut score = 0_i32;
            let content = event.content.to_lowercase();
            let kind = event.kind.to_lowercase();
            let source = event.source.to_lowercase();

            if content.contains(&q) {
                score += 3;
            }
            if kind.contains(&q) || source.contains(&q) {
                score += 2;
            }
            for tag in &event.tags {
                if tag.to_lowercase().contains(&q) {
                    score += 1;
                }
            }

            if score > 0 {
                scored.push((score, event));
            }
        }

        scored.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| b.1.timestamp.cmp(&a.1.timestamp))
        });
        Ok(scored.into_iter().take(limit).map(|(_, event)| event).collect())
    }

    fn total_events(&self) -> u64 {
        self.next_event_id.load(Ordering::SeqCst).saturating_sub(1)
    }
}

#[derive(Clone)]
struct TelegramClient {
    token: String,
    http: Client,
}

impl TelegramClient {
    fn new(token: String) -> Self {
        Self {
            token,
            http: Client::new(),
        }
    }

    fn endpoint(&self, method: &str) -> String {
        format!("https://api.telegram.org/bot{}/{}", self.token, method)
    }

    async fn get_updates(&self, offset: Option<i64>, timeout: u64) -> Result<Vec<TelegramUpdate>, String> {
        let mut params = vec![
            ("timeout".to_string(), timeout.to_string()),
            ("allowed_updates".to_string(), r#"["message"]"#.to_string()),
        ];
        if let Some(offset) = offset {
            params.push(("offset".to_string(), offset.to_string()));
        }

        let resp = self
            .http
            .get(self.endpoint("getUpdates"))
            .query(&params)
            .send()
            .await
            .map_err(|e| format!("telegram getUpdates failed: {}", e))?;

        let parsed: TelegramEnvelope<Vec<TelegramUpdate>> = resp
            .json()
            .await
            .map_err(|e| format!("telegram getUpdates decode failed: {}", e))?;

        if !parsed.ok {
            return Err(parsed
                .description
                .unwrap_or_else(|| "telegram getUpdates failed".to_string()));
        }

        Ok(parsed.result)
    }

    async fn send_message(&self, chat_id: i64, text: &str) -> Result<(), String> {
        let payload = serde_json::json!({
            "chat_id": chat_id,
            "text": text,
        });

        let resp = self
            .http
            .post(self.endpoint("sendMessage"))
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("telegram sendMessage failed: {}", e))?;

        let parsed: TelegramEnvelope<serde_json::Value> = resp
            .json()
            .await
            .map_err(|e| format!("telegram sendMessage decode failed: {}", e))?;

        if parsed.ok {
            Ok(())
        } else {
            Err(parsed
                .description
                .unwrap_or_else(|| "telegram sendMessage failed".to_string()))
        }
    }
}

#[derive(Debug, Deserialize)]
struct TelegramEnvelope<T> {
    ok: bool,
    result: T,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TelegramUpdate {
    update_id: i64,
    message: Option<TelegramMessage>,
}

#[derive(Debug, Deserialize)]
struct TelegramMessage {
    chat: TelegramChat,
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TelegramChat {
    id: i64,
}

struct DaemonState {
    tasks: Mutex<HashMap<String, TaskRecord>>,
    schedules: Mutex<HashMap<String, ScheduleRecord>>,
    queue_tx: mpsc::Sender<String>,
    next_task_id: AtomicU64,
    next_schedule_id: AtomicU64,
    memory_store: MemoryStore,
    telegram: Option<TelegramClient>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let data_dir = resolve_data_dir(cli.data_dir.as_deref())?;
    fs::create_dir_all(&data_dir)?;

    let memory_store = MemoryStore::new(&data_dir)
        .map_err(|e| format!("failed to initialize memory store: {}", e))?;

    let (tx, rx) = mpsc::channel::<String>(1024);
    let telegram = cli.telegram_token.map(TelegramClient::new);

    let state = Arc::new(DaemonState {
        tasks: Mutex::new(HashMap::new()),
        schedules: Mutex::new(HashMap::new()),
        queue_tx: tx,
        next_task_id: AtomicU64::new(1),
        next_schedule_id: AtomicU64::new(1),
        memory_store,
        telegram,
    });

    tokio::spawn(worker_loop(state.clone(), rx));
    tokio::spawn(scheduler_loop(state.clone()));
    if state.telegram.is_some() {
        tokio::spawn(telegram_loop(
            state.clone(),
            cli.telegram_poll_interval_seconds.max(2),
        ));
    }

    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/tasks", post(create_task).get(list_tasks))
        .route("/v1/tasks/{task_id}", get(get_task))
        .route("/v1/tasks/{task_id}/cancel", post(cancel_task))
        .route("/v1/memory/events", post(create_memory_event).get(list_memory_events))
        .route("/v1/memory/search", post(search_memory))
        .route("/v1/schedules", post(create_schedule).get(list_schedules))
        .route("/v1/schedules/{schedule_id}/pause", post(pause_schedule))
        .route("/v1/schedules/{schedule_id}/resume", post(resume_schedule))
        .route("/v1/schedules/{schedule_id}/delete", post(delete_schedule))
        .with_state(state);

    let addr = format!("{}:{}", cli.host, cli.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!("pangu-core listening on http://{} (data_dir={})", addr, data_dir.display());
    axum::serve(listener, app).await?;
    Ok(())
}

fn resolve_data_dir(input: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(path) = input {
        return Ok(path.to_path_buf());
    }

    let home = dirs::home_dir().ok_or_else(|| "could not resolve home directory".to_string())?;
    Ok(home.join(".pangu-core"))
}

async fn health(State(state): State<Arc<DaemonState>>) -> Json<HealthResponse> {
    let tasks = state.tasks.lock().await;
    let queued_tasks = tasks
        .values()
        .filter(|t| t.status == TaskStatus::Queued || t.status == TaskStatus::Running)
        .count();
    drop(tasks);

    let schedules = state.schedules.lock().await;
    let active_schedules = schedules
        .values()
        .filter(|s| s.status == ScheduleStatus::Active)
        .count();

    Json(HealthResponse {
        status: "ok",
        queued_tasks,
        total_tasks: state.tasks.lock().await.len(),
        active_schedules,
        memory_events: state.memory_store.total_events(),
        telegram_enabled: state.telegram.is_some(),
    })
}

async fn create_task(
    State(state): State<Arc<DaemonState>>,
    Json(req): Json<CreateTaskRequest>,
) -> Result<(StatusCode, Json<CreateTaskResponse>), (StatusCode, String)> {
    let task_id = enqueue_task_from_request(&state, req).await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(CreateTaskResponse {
            task_id,
            status: TaskStatus::Queued,
        }),
    ))
}

async fn enqueue_task_from_request(
    state: &Arc<DaemonState>,
    mut req: CreateTaskRequest,
) -> Result<String, (StatusCode, String)> {
    validate_task_payload(&req.kind, &req.payload)?;
    if req.source.trim().is_empty() {
        req.source = default_source();
    }

    let id_num = state.next_task_id.fetch_add(1, Ordering::SeqCst);
    let task_id = format!("task-{id_num:08}");
    let now = Utc::now();
    let record = TaskRecord {
        id: task_id.clone(),
        kind: req.kind,
        status: TaskStatus::Queued,
        payload: req.payload,
        source: req.source,
        created_at: now,
        updated_at: now,
        result: None,
        error: None,
    };

    {
        let mut tasks = state.tasks.lock().await;
        tasks.insert(task_id.clone(), record);
    }

    if state.queue_tx.send(task_id.clone()).await.is_err() {
        return Err((StatusCode::INTERNAL_SERVER_ERROR, "queue is unavailable".to_string()));
    }

    Ok(task_id)
}

async fn list_tasks(
    State(state): State<Arc<DaemonState>>,
    Query(query): Query<LimitQuery>,
) -> Json<Vec<TaskRecord>> {
    let limit = query.limit.unwrap_or(100).clamp(1, 10_000);
    let tasks = state.tasks.lock().await;
    let mut list = tasks.values().cloned().collect::<Vec<_>>();
    list.sort_by_key(|t| t.created_at);
    if list.len() > limit {
        Json(list[list.len() - limit..].to_vec())
    } else {
        Json(list)
    }
}

async fn get_task(
    State(state): State<Arc<DaemonState>>,
    AxumPath(task_id): AxumPath<String>,
) -> Result<Json<TaskRecord>, (StatusCode, String)> {
    let tasks = state.tasks.lock().await;
    let Some(task) = tasks.get(&task_id) else {
        return Err((StatusCode::NOT_FOUND, format!("task not found: {}", task_id)));
    };
    Ok(Json(task.clone()))
}

async fn cancel_task(
    State(state): State<Arc<DaemonState>>,
    AxumPath(task_id): AxumPath<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let mut tasks = state.tasks.lock().await;
    let Some(task) = tasks.get_mut(&task_id) else {
        return Err((StatusCode::NOT_FOUND, format!("task not found: {}", task_id)));
    };

    if matches!(task.status, TaskStatus::Completed | TaskStatus::Failed) {
        return Ok(StatusCode::CONFLICT);
    }

    task.status = TaskStatus::Cancelled;
    task.updated_at = Utc::now();
    Ok(StatusCode::OK)
}

async fn create_memory_event(
    State(state): State<Arc<DaemonState>>,
    Json(req): Json<CreateMemoryEventRequest>,
) -> Result<(StatusCode, Json<MemoryEvent>), (StatusCode, String)> {
    if req.kind.trim().is_empty() || req.content.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "kind and content are required".to_string()));
    }
    let event = state.memory_store.append(req).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok((StatusCode::CREATED, Json(event)))
}

async fn list_memory_events(
    State(state): State<Arc<DaemonState>>,
    Query(query): Query<LimitQuery>,
) -> Result<Json<Vec<MemoryEvent>>, (StatusCode, String)> {
    let limit = query.limit.unwrap_or(100).clamp(1, 10_000);
    let events = state
        .memory_store
        .list(limit)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(events))
}

async fn search_memory(
    State(state): State<Arc<DaemonState>>,
    Json(req): Json<MemorySearchRequest>,
) -> Result<Json<MemorySearchResponse>, (StatusCode, String)> {
    if req.query.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "query cannot be empty".to_string()));
    }
    let limit = req.limit.clamp(1, 200);
    let results = state
        .memory_store
        .search(&req.query, limit)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(MemorySearchResponse {
        query: req.query,
        results,
    }))
}

async fn create_schedule(
    State(state): State<Arc<DaemonState>>,
    Json(req): Json<CreateScheduleRequest>,
) -> Result<(StatusCode, Json<CreateScheduleResponse>), (StatusCode, String)> {
    if req.name.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "name cannot be empty".to_string()));
    }
    if req.interval_seconds < 5 {
        return Err((StatusCode::BAD_REQUEST, "interval_seconds must be >= 5".to_string()));
    }
    validate_task_payload(&req.task.kind, &req.task.payload)?;

    let id = state.next_schedule_id.fetch_add(1, Ordering::SeqCst);
    let schedule_id = format!("sched-{id:08}");
    let now = Utc::now();
    let next_run_at = now + ChronoDuration::seconds(req.interval_seconds as i64);
    let record = ScheduleRecord {
        id: schedule_id.clone(),
        name: req.name,
        interval_seconds: req.interval_seconds,
        task: req.task,
        status: ScheduleStatus::Active,
        next_run_at,
        last_run_at: None,
        created_at: now,
        updated_at: now,
    };

    {
        let mut schedules = state.schedules.lock().await;
        schedules.insert(schedule_id.clone(), record);
    }

    Ok((
        StatusCode::CREATED,
        Json(CreateScheduleResponse {
            schedule_id,
            next_run_at,
        }),
    ))
}

async fn list_schedules(State(state): State<Arc<DaemonState>>) -> Json<Vec<ScheduleRecord>> {
    let schedules = state.schedules.lock().await;
    let mut list = schedules.values().cloned().collect::<Vec<_>>();
    list.sort_by_key(|s| s.created_at);
    Json(list)
}

async fn pause_schedule(
    State(state): State<Arc<DaemonState>>,
    AxumPath(schedule_id): AxumPath<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let mut schedules = state.schedules.lock().await;
    let Some(schedule) = schedules.get_mut(&schedule_id) else {
        return Err((StatusCode::NOT_FOUND, format!("schedule not found: {}", schedule_id)));
    };
    schedule.status = ScheduleStatus::Paused;
    schedule.updated_at = Utc::now();
    Ok(StatusCode::OK)
}

async fn resume_schedule(
    State(state): State<Arc<DaemonState>>,
    AxumPath(schedule_id): AxumPath<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let mut schedules = state.schedules.lock().await;
    let Some(schedule) = schedules.get_mut(&schedule_id) else {
        return Err((StatusCode::NOT_FOUND, format!("schedule not found: {}", schedule_id)));
    };
    schedule.status = ScheduleStatus::Active;
    schedule.next_run_at = Utc::now() + ChronoDuration::seconds(schedule.interval_seconds as i64);
    schedule.updated_at = Utc::now();
    Ok(StatusCode::OK)
}

async fn delete_schedule(
    State(state): State<Arc<DaemonState>>,
    AxumPath(schedule_id): AxumPath<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let mut schedules = state.schedules.lock().await;
    if schedules.remove(&schedule_id).is_none() {
        return Err((StatusCode::NOT_FOUND, format!("schedule not found: {}", schedule_id)));
    }
    Ok(StatusCode::OK)
}

async fn scheduler_loop(state: Arc<DaemonState>) {
    loop {
        let now = Utc::now();
        let mut due = Vec::<CreateTaskRequest>::new();
        {
            let mut schedules = state.schedules.lock().await;
            for schedule in schedules.values_mut() {
                if schedule.status != ScheduleStatus::Active {
                    continue;
                }
                if schedule.next_run_at <= now {
                    let mut task = schedule.task.clone();
                    task.source = format!("schedule:{}", schedule.id);
                    due.push(task);
                    schedule.last_run_at = Some(now);
                    schedule.next_run_at = now + ChronoDuration::seconds(schedule.interval_seconds as i64);
                    schedule.updated_at = now;
                }
            }
        }

        for request in due {
            let _ = enqueue_task_from_request(&state, request).await;
        }

        sleep(Duration::from_secs(1)).await;
    }
}

fn validate_task_payload(kind: &TaskKind, payload: &TaskPayload) -> Result<(), (StatusCode, String)> {
    let valid = matches!(
        (kind, payload),
        (TaskKind::Chat, TaskPayload::Chat(_))
            | (TaskKind::Schedule, TaskPayload::Schedule(_))
            | (TaskKind::Browser, TaskPayload::Browser(_))
    );

    if valid {
        Ok(())
    } else {
        Err((StatusCode::BAD_REQUEST, "kind does not match payload.type".to_string()))
    }
}

async fn worker_loop(state: Arc<DaemonState>, mut rx: mpsc::Receiver<String>) {
    while let Some(task_id) = rx.recv().await {
        let (payload, source, canceled) = {
            let mut tasks = state.tasks.lock().await;
            let Some(task) = tasks.get_mut(&task_id) else {
                continue;
            };

            if task.status == TaskStatus::Cancelled {
                (task.payload.clone(), task.source.clone(), true)
            } else {
                task.status = TaskStatus::Running;
                task.updated_at = Utc::now();
                (task.payload.clone(), task.source.clone(), false)
            }
        };

        if canceled {
            continue;
        }

        let run_result = execute_task(payload).await;

        let (status_text, response_text) = {
            let mut tasks = state.tasks.lock().await;
            let Some(task) = tasks.get_mut(&task_id) else {
                continue;
            };

            if task.status == TaskStatus::Cancelled {
                continue;
            }

            match run_result {
                Ok(output) => {
                    task.status = TaskStatus::Completed;
                    task.result = Some(output.clone());
                    task.error = None;
                    task.updated_at = Utc::now();
                    ("completed".to_string(), output)
                }
                Err(err) => {
                    task.status = TaskStatus::Failed;
                    task.error = Some(err.clone());
                    task.updated_at = Utc::now();
                    ("failed".to_string(), err)
                }
            }
        };

        let mut metadata = HashMap::new();
        metadata.insert("task_id".to_string(), task_id.clone());
        metadata.insert("status".to_string(), status_text);
        let _ = state.memory_store.append(CreateMemoryEventRequest {
            kind: "task_result".to_string(),
            source: source.clone(),
            content: response_text.clone(),
            tags: vec!["task".to_string()],
            metadata,
        });

        if let Some(chat_id) = parse_telegram_chat_source(&source) {
            if let Some(telegram) = &state.telegram {
                let _ = telegram
                    .send_message(chat_id, &format!("Task {}: {}", task_id, response_text))
                    .await;
            }
        }
    }
}

fn parse_telegram_chat_source(source: &str) -> Option<i64> {
    source.strip_prefix("telegram:")?.parse::<i64>().ok()
}

async fn telegram_loop(state: Arc<DaemonState>, poll_interval_seconds: u64) {
    let Some(client) = state.telegram.clone() else {
        return;
    };

    let mut offset = None::<i64>;
    loop {
        match client.get_updates(offset, poll_interval_seconds).await {
            Ok(updates) => {
                for update in updates {
                    offset = Some(update.update_id + 1);
                    let Some(msg) = update.message else {
                        continue;
                    };
                    let Some(text) = msg.text else {
                        continue;
                    };
                    let chat_id = msg.chat.id;
                    handle_telegram_text(&state, &client, chat_id, &text).await;
                }
            }
            Err(e) => {
                eprintln!("[pangu-core] telegram error: {}", e);
                sleep(Duration::from_secs(3)).await;
            }
        }
    }
}

async fn handle_telegram_text(
    state: &Arc<DaemonState>,
    client: &TelegramClient,
    chat_id: i64,
    text: &str,
) {
    let trimmed = text.trim();
    if trimmed.eq_ignore_ascii_case("/start") {
        let _ = client
            .send_message(chat_id, "Pangu is live locally. Send any message to enqueue a task.")
            .await;
        return;
    }

    if trimmed.eq_ignore_ascii_case("/health") {
        let tasks = state.tasks.lock().await;
        let running = tasks
            .values()
            .filter(|t| t.status == TaskStatus::Queued || t.status == TaskStatus::Running)
            .count();
        let total = tasks.len();
        drop(tasks);
        let _ = client
            .send_message(
                chat_id,
                &format!("pangu-core ok. queued/running={}, total_tasks={}", running, total),
            )
            .await;
        return;
    }

    let request = CreateTaskRequest {
        kind: TaskKind::Chat,
        payload: TaskPayload::Chat(ChatTask {
            message: trimmed.to_string(),
            source: "telegram".to_string(),
        }),
        source: format!("telegram:{}", chat_id),
    };

    match enqueue_task_from_request(state, request).await {
        Ok(task_id) => {
            let _ = client
                .send_message(chat_id, &format!("Queued: {}", task_id))
                .await;
        }
        Err((_, err)) => {
            let _ = client
                .send_message(chat_id, &format!("Failed to queue task: {}", err))
                .await;
        }
    }
}

async fn execute_task(payload: TaskPayload) -> Result<String, String> {
    match payload {
        TaskPayload::Chat(chat) => Ok(format!("chat accepted from {}: {}", chat.source, chat.message)),
        TaskPayload::Schedule(scheduled) => Ok(format!(
            "scheduled job '{}' with trigger '{}'",
            scheduled.job_name, scheduled.trigger
        )),
        TaskPayload::Browser(browser) => execute_browser_task(browser).await,
    }
}

async fn execute_browser_task(browser: BrowserTask) -> Result<String, String> {
    if browser.steps.is_empty() {
        return Err("browser task must include at least one step".to_string());
    }

    let mut summary = Vec::with_capacity(browser.steps.len());
    for step in browser.steps {
        let line = match step {
            BrowserStep::Navigate { url } => format!("navigate {}", url),
            BrowserStep::Click { selector } => format!("click {}", selector),
            BrowserStep::Fill { selector, .. } => format!("fill {}", selector),
            BrowserStep::WaitFor { selector, timeout_ms } => {
                format!("wait_for {} ({:?}ms)", selector, timeout_ms)
            }
            BrowserStep::ExtractText { selector } => format!("extract_text {}", selector),
            BrowserStep::Screenshot { path } => format!("screenshot {}", path),
        };
        summary.push(line);
    }

    Ok(format!(
        "browser task queued in {} mode: {}",
        if browser.headless { "headless" } else { "headed" },
        summary.join(" -> ")
    ))
}
