use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    process::Stdio,
    sync::Arc,
    time::{Duration, Instant},
};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::RngCore;
use tauri::{ipc::Channel, State};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, BufReader},
    net::TcpStream,
    process::Command,
    sync::mpsc,
};

use crate::{
    error::{AppError, CommandResult, Error, Result},
    models::{
        now_ms, CreateDatabaseInput, DatabaseEnvironment, DatabaseKind, DatabaseStatus,
        ManagedDatabase, OperationEvent,
    },
    operations,
    state::AppState,
};

type SharedState<'a> = State<'a, Arc<AppState>>;

struct DatabaseSpec {
    image: &'static str,
    internal_port: u16,
    volume_path: &'static str,
    username: &'static str,
    database: String,
}

#[derive(Default)]
struct PullLayer {
    status: String,
    current_bytes: Option<f64>,
    total_bytes: Option<f64>,
}

impl DatabaseSpec {
    fn for_kind(kind: &DatabaseKind, name: &str) -> Self {
        match kind {
            DatabaseKind::Mysql => Self {
                image: "mysql:8.4",
                internal_port: 3306,
                volume_path: "/var/lib/mysql",
                username: "nooki",
                database: name.into(),
            },
            DatabaseKind::Postgresql => Self {
                image: "postgres:17-alpine",
                internal_port: 5432,
                volume_path: "/var/lib/postgresql/data",
                username: "nooki",
                database: name.into(),
            },
            DatabaseKind::Mongodb => Self {
                image: "mongo:8",
                internal_port: 27017,
                volume_path: "/data/db",
                username: "nooki",
                database: name.into(),
            },
            DatabaseKind::Redis => Self {
                image: "redis:7-alpine",
                internal_port: 6379,
                volume_path: "/data",
                username: "default",
                database: "0".into(),
            },
        }
    }
}

#[tauri::command]
pub async fn database_environment() -> CommandResult<DatabaseEnvironment> {
    Ok(inspect_environment().await)
}

#[tauri::command]
pub async fn list_databases(
    state: SharedState<'_>,
    server_id: String,
) -> CommandResult<Vec<ManagedDatabase>> {
    state.server(&server_id).await.map_err(AppError::from)?;
    let mut databases = state
        .db
        .load_databases(&server_id)
        .await
        .map_err(AppError::from)?;
    if !inspect_environment().await.available {
        return Ok(databases);
    }

    for database in &mut databases {
        let args = strings(&[
            "inspect",
            "--format",
            "{{.State.Status}}",
            &database.container_name,
        ]);
        match run_docker(&args, None).await {
            Ok(output) => {
                database.status = match output.trim() {
                    "running" => DatabaseStatus::Running,
                    "created" | "exited" | "paused" | "restarting" => DatabaseStatus::Stopped,
                    _ => DatabaseStatus::Error,
                };
                database.last_error = None;
            }
            Err(error) => {
                let message = error.to_string();
                database.status = if message.to_ascii_lowercase().contains("no such object") {
                    DatabaseStatus::Missing
                } else {
                    DatabaseStatus::Error
                };
                database.last_error = Some(message);
            }
        }
        state
            .db
            .save_database(database)
            .await
            .map_err(AppError::from)?;
    }
    Ok(databases)
}

#[tauri::command]
pub async fn create_database(
    state: SharedState<'_>,
    server_id: String,
    input: CreateDatabaseInput,
    on_progress: Channel<OperationEvent>,
) -> CommandResult<ManagedDatabase> {
    create(state.inner().clone(), &server_id, input, &on_progress)
        .await
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn database_action(
    state: SharedState<'_>,
    id: String,
    action: String,
) -> CommandResult<ManagedDatabase> {
    let mut database = state
        .db
        .load_database(&id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::from(Error::NotFound("That database no longer exists.".into())))?;
    let _lock = state
        .operation_lock(&format!("database:{}", database.server_id))
        .lock_owned()
        .await;
    if !inspect_environment().await.available {
        return Err(AppError::from(Error::Conflict(
            "Start Docker Desktop before controlling this database.".into(),
        )));
    }
    let verb = match action.as_str() {
        "start" => "start",
        "stop" => "stop",
        "restart" => "restart",
        _ => {
            return Err(AppError::from(Error::Validation(
                "Unknown database action.".into(),
            )))
        }
    };
    run_docker(&strings(&[verb, &database.container_name]), None)
        .await
        .map_err(AppError::from)?;
    database.status = if action == "stop" {
        DatabaseStatus::Stopped
    } else {
        DatabaseStatus::Running
    };
    database.last_error = None;
    state
        .db
        .save_database(&database)
        .await
        .map_err(AppError::from)?;
    Ok(database)
}

#[tauri::command]
pub async fn delete_database(state: SharedState<'_>, id: String) -> CommandResult<()> {
    let database = state
        .db
        .load_database(&id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::from(Error::NotFound("That database no longer exists.".into())))?;
    if !inspect_environment().await.available {
        return Err(AppError::from(Error::Conflict(
            "Start Docker Desktop before deleting this database and its data volume.".into(),
        )));
    }
    let _lock = state
        .operation_lock(&format!("database:{}", database.server_id))
        .lock_owned()
        .await;
    let _ = run_docker(&strings(&["rm", "--force", &database.container_name]), None).await;
    run_docker(&strings(&["volume", "rm", &database.volume_name]), None)
        .await
        .map_err(AppError::from)?;
    state.db.delete_database(&id).await.map_err(AppError::from)
}

async fn create(
    state: Arc<AppState>,
    server_id: &str,
    input: CreateDatabaseInput,
    channel: &Channel<OperationEvent>,
) -> Result<ManagedDatabase> {
    state.server(server_id).await?;
    validate_name(&input.name)?;
    if state
        .db
        .load_databases(server_id)
        .await?
        .iter()
        .any(|database| database.name.eq_ignore_ascii_case(&input.name))
    {
        return Err(Error::Conflict(
            "This server already has a database with that name.".into(),
        ));
    }
    let environment = inspect_environment().await;
    if !environment.available {
        return Err(Error::Conflict(environment.message.unwrap_or_else(|| {
            "Install and start Docker Desktop before creating a database.".into()
        })));
    }

    let operation_id = uuid::Uuid::new_v4().to_string();
    let _operation = operations::begin(&operation_id);
    let _lock = state
        .operation_lock(&format!("database:{server_id}"))
        .lock_owned()
        .await;
    let _ = channel.send(OperationEvent::Started {
        operation_id: operation_id.clone(),
        message: "Preparing database".into(),
    });

    let spec = DatabaseSpec::for_kind(&input.kind, &input.name);
    progress(
        channel,
        &operation_id,
        "pull",
        5.0,
        "Starting image download",
    );
    pull_image(spec.image, &operation_id, channel).await?;
    operations::check(&operation_id)?;

    let id = uuid::Uuid::new_v4().to_string();
    let short_id = id.replace('-', "")[..12].to_owned();
    let container_name = format!("nooki-db-{short_id}");
    let volume_name = format!("nooki-db-{short_id}-data");
    let password = random_password();

    progress(
        channel,
        &operation_id,
        "storage",
        62.0,
        "Creating persistent storage",
    );
    run_docker(
        &strings(&["volume", "create", &volume_name]),
        Some(&operation_id),
    )
    .await?;

    let create_result = async {
        operations::check(&operation_id)?;
        progress(
            channel,
            &operation_id,
            "configure",
            72.0,
            "Configuring the database",
        );
        let mut args = vec![
            "create".into(),
            "--name".into(),
            container_name.clone(),
            "--label".into(),
            "nooki.managed=true".into(),
            "--label".into(),
            format!("nooki.database.id={id}"),
            "--label".into(),
            format!("nooki.server.id={server_id}"),
            "--restart".into(),
            "unless-stopped".into(),
            "-p".into(),
            format!("127.0.0.1::{}", spec.internal_port),
            "-v".into(),
            format!("{volume_name}:{}", spec.volume_path),
        ];
        match &input.kind {
            DatabaseKind::Mysql => {
                push_env(&mut args, "MYSQL_DATABASE", &spec.database);
                push_env(&mut args, "MYSQL_USER", spec.username);
                push_env(&mut args, "MYSQL_PASSWORD", &password);
                push_env(&mut args, "MYSQL_ROOT_PASSWORD", &password);
            }
            DatabaseKind::Postgresql => {
                push_env(&mut args, "POSTGRES_DB", &spec.database);
                push_env(&mut args, "POSTGRES_USER", spec.username);
                push_env(&mut args, "POSTGRES_PASSWORD", &password);
            }
            DatabaseKind::Mongodb => {
                push_env(&mut args, "MONGO_INITDB_ROOT_USERNAME", spec.username);
                push_env(&mut args, "MONGO_INITDB_ROOT_PASSWORD", &password);
            }
            DatabaseKind::Redis => {}
        }
        args.push(spec.image.into());
        if input.kind == DatabaseKind::Redis {
            args.extend(strings(&[
                "redis-server",
                "--appendonly",
                "yes",
                "--requirepass",
                &password,
            ]));
        }
        run_docker(&args, Some(&operation_id)).await?;

        progress(
            channel,
            &operation_id,
            "start",
            84.0,
            "Starting the database",
        );
        run_docker(&strings(&["start", &container_name]), Some(&operation_id)).await?;
        let port_output = run_docker(
            &strings(&[
                "port",
                &container_name,
                &format!("{}/tcp", spec.internal_port),
            ]),
            Some(&operation_id),
        )
        .await?;
        let port = parse_host_port(&port_output)?;

        progress(
            channel,
            &operation_id,
            "ready",
            91.0,
            "Waiting for the database to accept connections",
        );
        if let Err(error) = wait_for_port(port, &operation_id, channel).await {
            let logs = run_docker(&strings(&["logs", "--tail", "30", &container_name]), None)
                .await
                .unwrap_or_else(|log_error| format!("Could not read container logs: {log_error}"));
            return Err(Error::Process(format!(
                "{error} Recent container output: {}",
                truncate_diagnostic(&logs, 3000)
            )));
        }
        Ok::<u16, Error>(port)
    }
    .await;

    let port = match create_result {
        Ok(port) => port,
        Err(error) => {
            let _ = run_docker(&strings(&["rm", "--force", &container_name]), None).await;
            let _ = run_docker(&strings(&["volume", "rm", &volume_name]), None).await;
            return Err(error);
        }
    };

    let connection_uri =
        connection_uri(&input.kind, spec.username, &password, port, &spec.database);
    let database = ManagedDatabase {
        id,
        server_id: server_id.into(),
        kind: input.kind,
        name: input.name,
        status: DatabaseStatus::Running,
        host: "127.0.0.1".into(),
        port,
        username: spec.username.into(),
        password,
        database: spec.database,
        connection_uri,
        container_name,
        volume_name,
        created_at: now_ms(),
        last_error: None,
    };
    state.db.save_database(&database).await?;
    let _ = channel.send(OperationEvent::Finished {
        operation_id,
        message: "Database ready".into(),
    });
    Ok(database)
}

async fn inspect_environment() -> DatabaseEnvironment {
    let (docker, checked_paths) = find_docker();
    let Some(docker) = docker else {
        return DatabaseEnvironment {
            available: false,
            version: None,
            message: Some("Nooki could not find docker.exe in PATH or any supported Docker Desktop install location.".into()),
            code: Some("cli-not-found".into()),
            cli_path: None,
            context: None,
            details: vec![format!("Locations checked: {}", checked_paths.join(", "))],
            suggestions: vec![
                "Restart Nooki if Docker Desktop was installed while Nooki was open.".into(),
                "Repair or reinstall Docker Desktop if its command line tools are missing.".into(),
            ],
        };
    };
    let cli_path = docker.to_string_lossy().into_owned();
    let client_version = diagnostic_command(&docker, &["--version"]).await;
    let context = diagnostic_command(&docker, &["context", "show"])
        .await
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    let mut info_command = docker_command(&docker);
    let output = info_command
        .args(["info", "--format", "{{.ServerVersion}}"])
        .creation_flags_no_window()
        .output()
        .await;
    match output {
        Ok(output) if output.status.success() => DatabaseEnvironment {
            available: true,
            version: Some(String::from_utf8_lossy(&output.stdout).trim().into()),
            message: None,
            code: None,
            cli_path: Some(cli_path),
            context,
            details: client_version.into_iter().collect(),
            suggestions: Vec::new(),
        },
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            let raw = if stderr.is_empty() { stdout } else { stderr };
            let lower = raw.to_ascii_lowercase();
            let (code, message, suggestions) = if lower.contains("access is denied")
                || lower.contains("permission denied")
            {
                (
                    "permission-denied",
                    "Docker was found, but Windows denied access to its engine.",
                    vec![
                        "Wait for Docker Desktop to finish starting, then try again.".into(),
                        "Check that your Windows account can use Docker Desktop and restart it if needed.".into(),
                    ],
                )
            } else if lower.contains("cannot find the file")
                || lower.contains("system cannot find")
                || lower.contains("dockerdesktoplinuxengine")
                || lower.contains("docker_engine")
            {
                (
                    "daemon-not-ready",
                    "Docker Desktop is open, but its Linux container engine is not ready yet.",
                    vec![
                        "Wait until Docker Desktop reports that the engine is running, then try again.".into(),
                        "If it stays unavailable, restart Docker Desktop or its WSL backend.".into(),
                    ],
                )
            } else {
                (
                    "daemon-unavailable",
                    "Docker's command line tool was found, but the container engine did not respond.",
                    vec![
                        "Open or restart Docker Desktop, wait for startup to finish, then try again.".into(),
                        "Check Docker Desktop's engine and WSL status if the problem continues.".into(),
                    ],
                )
            };
            let mut details = Vec::new();
            if let Ok(version) = client_version {
                details.push(version);
            }
            if !raw.is_empty() {
                details.push(format!("Engine response: {raw}"));
            }
            DatabaseEnvironment {
                available: false,
                version: None,
                message: Some(message.into()),
                code: Some(code.into()),
                cli_path: Some(cli_path),
                context,
                details,
                suggestions,
            }
        }
        Err(error) => DatabaseEnvironment {
            available: false,
            version: None,
            message: Some(format!("Nooki could not start Docker: {error}")),
            code: Some("cli-launch-failed".into()),
            cli_path: Some(cli_path),
            context,
            details: client_version.into_iter().collect(),
            suggestions: vec![
                "Restart Nooki and Docker Desktop, then try again.".into(),
                "Repair Docker Desktop if docker.exe cannot be launched directly.".into(),
            ],
        },
    }
}

async fn run_docker(args: &[String], operation_id: Option<&str>) -> Result<String> {
    let (docker, _) = find_docker();
    let docker = docker.ok_or_else(|| {
        Error::NotFound("Docker's command line tools could not be found on this computer.".into())
    })?;
    let mut command = docker_command(&docker);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .creation_flags_no_window();
    let child = command.spawn()?;
    let output = if let Some(operation_id) = operation_id {
        tokio::select! {
            output = child.wait_with_output() => output?,
            _ = operations::cancelled(operation_id) => return Err(Error::Cancelled),
        }
    } else {
        child.wait_with_output().await?
    };
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let action = args.first().map(String::as_str).unwrap_or("command");
        let exit_code = output
            .status
            .code()
            .map(|code| code.to_string())
            .unwrap_or_else(|| "terminated".into());
        return Err(Error::Process(if message.is_empty() {
            format!("Docker {action} failed (exit {exit_code}) without an error message.")
        } else {
            format!("Docker {action} failed (exit {exit_code}): {message}")
        }));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().into())
}

async fn pull_image(
    image: &str,
    operation_id: &str,
    channel: &Channel<OperationEvent>,
) -> Result<()> {
    let (docker, _) = find_docker();
    let docker = docker.ok_or_else(|| {
        Error::NotFound("Docker's command line tools could not be found on this computer.".into())
    })?;
    let mut command = docker_command(&docker);
    command
        .args(["pull", image])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .creation_flags_no_window();
    let mut child = command.spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| Error::Internal("Docker pull output was unavailable.".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| Error::Internal("Docker pull error output was unavailable.".into()))?;
    let (sender, mut receiver) = mpsc::unbounded_channel();
    tokio::spawn(read_pull_stream(stdout, sender.clone()));
    tokio::spawn(read_pull_stream(stderr, sender));

    let started = Instant::now();
    let mut ticker = tokio::time::interval(Duration::from_secs(1));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut layers = HashMap::<String, PullLayer>::new();
    let mut recent = VecDeque::<String>::new();
    let mut latest = format!("Contacting the registry for {image}");
    let mut streams_open = true;

    loop {
        enum PullEvent {
            Exited(std::io::Result<std::process::ExitStatus>),
            Cancelled,
            Output(Option<String>),
            Tick,
        }
        let event = tokio::select! {
            status = child.wait() => PullEvent::Exited(status),
            _ = operations::cancelled(operation_id) => PullEvent::Cancelled,
            line = receiver.recv(), if streams_open => PullEvent::Output(line),
            _ = ticker.tick() => PullEvent::Tick,
        };
        match event {
            PullEvent::Exited(status) => {
                while let Ok(line) = receiver.try_recv() {
                    record_pull_line(&line, &mut layers, &mut latest, &mut recent);
                }
                let status = status?;
                if !status.success() {
                    let exit_code = status
                        .code()
                        .map(|code| code.to_string())
                        .unwrap_or_else(|| "terminated".into());
                    let detail = recent.into_iter().collect::<Vec<_>>().join(" | ");
                    return Err(Error::Process(format!(
                        "Docker pull failed (exit {exit_code}) while downloading {image}: {}",
                        if detail.is_empty() {
                            "Docker returned no diagnostic output."
                        } else {
                            detail.as_str()
                        }
                    )));
                }
                progress(
                    channel,
                    operation_id,
                    "pull",
                    60.0,
                    &format!("Image ready · {}", elapsed_label(started.elapsed())),
                );
                return Ok(());
            }
            PullEvent::Cancelled => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                return Err(Error::Cancelled);
            }
            PullEvent::Output(Some(line)) => {
                record_pull_line(&line, &mut layers, &mut latest, &mut recent);
                send_pull_progress(channel, operation_id, image, &layers, &latest, started);
            }
            PullEvent::Output(None) => streams_open = false,
            PullEvent::Tick => {
                send_pull_progress(channel, operation_id, image, &layers, &latest, started);
            }
        }
    }
}

async fn read_pull_stream<R>(reader: R, sender: mpsc::UnboundedSender<String>)
where
    R: AsyncRead + Unpin,
{
    let mut lines = BufReader::new(reader).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let line = line.trim().to_owned();
        if !line.is_empty() && sender.send(line).is_err() {
            break;
        }
    }
}

fn record_pull_line(
    line: &str,
    layers: &mut HashMap<String, PullLayer>,
    latest: &mut String,
    recent: &mut VecDeque<String>,
) {
    *latest = line.to_owned();
    recent.push_back(line.to_owned());
    while recent.len() > 8 {
        recent.pop_front();
    }
    let Some((candidate, status)) = line.split_once(':') else {
        return;
    };
    let candidate = candidate.trim();
    if candidate.len() >= 6
        && candidate
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        let status = status.trim();
        let layer = layers.entry(candidate.into()).or_default();
        layer.status = status.into();
        if let Some((current, total)) = parse_pull_bytes(status) {
            layer.current_bytes = Some(current);
            layer.total_bytes = Some(total);
        }
    }
}

fn send_pull_progress(
    channel: &Channel<OperationEvent>,
    operation_id: &str,
    image: &str,
    layers: &HashMap<String, PullLayer>,
    latest: &str,
    started: Instant,
) {
    let completed = layers
        .values()
        .filter(|layer| {
            let status = layer.status.to_ascii_lowercase();
            status.contains("pull complete") || status.contains("already exists")
        })
        .count();
    let layer_fraction = if layers.is_empty() {
        0.0
    } else {
        layers
            .values()
            .map(|layer| {
                let status = layer.status.to_ascii_lowercase();
                if status.contains("pull complete") || status.contains("already exists") {
                    1.0
                } else {
                    match (layer.current_bytes, layer.total_bytes) {
                        (Some(current), Some(total)) if total > 0.0 => (current / total).min(1.0),
                        _ => 0.0,
                    }
                }
            })
            .sum::<f64>()
            / layers.len() as f64
    };
    let progress_value = if layers.is_empty() {
        7.0
    } else {
        8.0 + layer_fraction as f32 * 50.0
    };
    let activity = latest
        .split_once(':')
        .map(|(_, status)| status.trim())
        .unwrap_or(latest);
    let transferred = layers
        .values()
        .filter_map(|layer| layer.current_bytes)
        .sum::<f64>();
    let transfer_total = layers
        .values()
        .filter_map(|layer| layer.total_bytes)
        .sum::<f64>();
    let byte_detail = (transfer_total > 0.0).then(|| {
        format!(
            " · {} / {}",
            format_transfer_bytes(transferred),
            format_transfer_bytes(transfer_total)
        )
    });
    let message = if layers.is_empty() {
        format!(
            "Downloading {image} · {activity} · {}",
            elapsed_label(started.elapsed())
        )
    } else {
        format!(
            "Downloading {image} · {completed}/{} layers complete{} · {activity} · {}",
            layers.len(),
            byte_detail.unwrap_or_default(),
            elapsed_label(started.elapsed())
        )
    };
    progress(
        channel,
        operation_id,
        "pull",
        progress_value.min(58.0),
        &message,
    );
}

fn parse_pull_bytes(status: &str) -> Option<(f64, f64)> {
    let pair = status.split_whitespace().find(|piece| {
        piece.contains('/') && piece.chars().any(|character| character.is_ascii_digit())
    })?;
    let (current, total) = pair.split_once('/')?;
    Some((parse_transfer_bytes(current)?, parse_transfer_bytes(total)?))
}

fn parse_transfer_bytes(value: &str) -> Option<f64> {
    let split = value
        .find(|character: char| !character.is_ascii_digit() && character != '.')
        .unwrap_or(value.len());
    let number = value[..split].parse::<f64>().ok()?;
    let unit = value[split..].trim().to_ascii_lowercase();
    let multiplier = match unit.as_str() {
        "" | "b" => 1.0,
        "kb" => 1_000.0,
        "mb" => 1_000_000.0,
        "gb" => 1_000_000_000.0,
        _ => return None,
    };
    Some(number * multiplier)
}

fn format_transfer_bytes(bytes: f64) -> String {
    if bytes >= 1_000_000_000.0 {
        format!("{:.1} GB", bytes / 1_000_000_000.0)
    } else if bytes >= 1_000_000.0 {
        format!("{:.1} MB", bytes / 1_000_000.0)
    } else if bytes >= 1_000.0 {
        format!("{:.1} kB", bytes / 1_000.0)
    } else {
        format!("{bytes:.0} B")
    }
}

fn elapsed_label(elapsed: Duration) -> String {
    let seconds = elapsed.as_secs();
    if seconds < 60 {
        format!("{seconds}s elapsed")
    } else {
        format!("{}m {:02}s elapsed", seconds / 60, seconds % 60)
    }
}

fn find_docker() -> (Option<PathBuf>, Vec<String>) {
    let mut candidates = Vec::new();
    if let Ok(path) = which::which("docker") {
        candidates.push(path);
    }
    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        let root = PathBuf::from(local_app_data);
        candidates.push(
            root.join("Programs")
                .join("DockerDesktop")
                .join("resources")
                .join("bin")
                .join("docker.exe"),
        );
        candidates.push(
            root.join("Docker")
                .join("resources")
                .join("bin")
                .join("docker.exe"),
        );
    }
    for environment_variable in ["ProgramFiles", "ProgramW6432"] {
        if let Some(program_files) = std::env::var_os(environment_variable) {
            candidates.push(
                PathBuf::from(program_files)
                    .join("Docker")
                    .join("Docker")
                    .join("resources")
                    .join("bin")
                    .join("docker.exe"),
            );
        }
    }
    let mut checked = Vec::new();
    for candidate in candidates {
        let display = candidate.to_string_lossy().into_owned();
        if checked.iter().any(|path| path == &display) {
            continue;
        }
        checked.push(display);
        if candidate.is_file() {
            return (Some(candidate), checked);
        }
    }
    (None, checked)
}

async fn diagnostic_command(
    docker: &PathBuf,
    args: &[&str],
) -> std::result::Result<String, String> {
    let mut command = docker_command(docker);
    match command.args(args).creation_flags_no_window().output().await {
        Ok(output) if output.status.success() => {
            Ok(String::from_utf8_lossy(&output.stdout).trim().into())
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            Err(if stderr.is_empty() {
                "Docker returned an unsuccessful response.".into()
            } else {
                stderr
            })
        }
        Err(error) => Err(error.to_string()),
    }
}

fn docker_command(docker: &PathBuf) -> Command {
    let mut command = Command::new(docker);
    if let Some(directory) = docker.parent() {
        let inherited = std::env::var_os("PATH").unwrap_or_default();
        let paths = std::iter::once(directory.to_path_buf())
            .chain(std::env::split_paths(&inherited))
            .collect::<Vec<_>>();
        if let Ok(path) = std::env::join_paths(paths) {
            command.env("PATH", path);
        }
    }
    command
}

async fn wait_for_port(
    port: u16,
    operation_id: &str,
    channel: &Channel<OperationEvent>,
) -> Result<()> {
    for attempt in 0..90 {
        operations::check(operation_id)?;
        if TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
            progress(
                channel,
                operation_id,
                "ready",
                99.0,
                &format!("Database accepted a connection on 127.0.0.1:{port}"),
            );
            return Ok(());
        }
        progress(
            channel,
            operation_id,
            "ready",
            91.0 + (attempt as f32 / 90.0) * 8.0,
            &format!(
                "Waiting for 127.0.0.1:{port} to accept connections · {}s elapsed",
                attempt + 1
            ),
        );
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    Err(Error::Process(
        "The database container started but did not become ready in time.".into(),
    ))
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 32
        || !name
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphabetic())
        || !name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err(Error::Validation(
            "Use 1–32 letters, numbers, or underscores, beginning with a letter.".into(),
        ));
    }
    Ok(())
}

fn parse_host_port(output: &str) -> Result<u16> {
    output
        .lines()
        .find_map(|line| line.trim().rsplit(':').next()?.parse().ok())
        .ok_or_else(|| Error::Process("Docker did not report the database port.".into()))
}

fn truncate_diagnostic(value: &str, max_chars: usize) -> String {
    let mut output = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        output.push('…');
    }
    output
}

fn connection_uri(
    kind: &DatabaseKind,
    username: &str,
    password: &str,
    port: u16,
    database: &str,
) -> String {
    match kind {
        DatabaseKind::Mysql => {
            format!("mysql://{username}:{password}@127.0.0.1:{port}/{database}")
        }
        DatabaseKind::Postgresql => {
            format!("postgresql://{username}:{password}@127.0.0.1:{port}/{database}")
        }
        DatabaseKind::Mongodb => {
            format!("mongodb://{username}:{password}@127.0.0.1:{port}/{database}?authSource=admin")
        }
        DatabaseKind::Redis => format!("redis://:{password}@127.0.0.1:{port}/0"),
    }
}

fn random_password() -> String {
    let mut bytes = [0_u8; 24];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn push_env(args: &mut Vec<String>, name: &str, value: &str) {
    args.push("-e".into());
    args.push(format!("{name}={value}"));
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).into()).collect()
}

fn progress(channel: &Channel<OperationEvent>, id: &str, phase: &str, value: f32, message: &str) {
    let _ = channel.send(OperationEvent::Progress {
        operation_id: id.into(),
        phase: phase.into(),
        progress: value,
        message: message.into(),
    });
}

#[cfg(windows)]
trait CommandExt {
    fn creation_flags_no_window(&mut self) -> &mut Self;
}

#[cfg(windows)]
impl CommandExt for Command {
    fn creation_flags_no_window(&mut self) -> &mut Self {
        use std::os::windows::process::CommandExt as _;
        self.as_std_mut().creation_flags(0x08000000);
        self
    }
}

#[cfg(not(windows))]
trait CommandExt {
    fn creation_flags_no_window(&mut self) -> &mut Self;
}

#[cfg(not(windows))]
impl CommandExt for Command {
    fn creation_flags_no_window(&mut self) -> &mut Self {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_portable_database_names() {
        assert!(validate_name("minecraft_main").is_ok());
        assert!(validate_name("9worlds").is_err());
        assert!(validate_name("world-name").is_err());
    }

    #[test]
    fn parses_docker_port_output() {
        assert_eq!(parse_host_port("127.0.0.1:49153\n").unwrap(), 49153);
    }

    #[test]
    fn parses_docker_layer_transfer_progress() {
        let (current, total) = parse_pull_bytes("Downloading [========>] 12.5MB/50MB").unwrap();
        assert_eq!(current, 12_500_000.0);
        assert_eq!(total, 50_000_000.0);
    }

    #[test]
    fn builds_local_connection_uris() {
        assert_eq!(
            connection_uri(&DatabaseKind::Redis, "default", "secret", 49153, "0"),
            "redis://:secret@127.0.0.1:49153/0"
        );
    }
}
