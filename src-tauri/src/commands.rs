use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use tauri::{ipc::Channel, AppHandle, State};

use crate::{
    backups,
    error::{AppError, CommandResult, Error, Result},
    java,
    models::{
        now_ms, AppEvent, AppSettings, AppSnapshot, Backup, BackupSchedule, ChangeSoftwareInput,
        CreateServerInput, ImportScan, ImportServerInput, JarCandidate, JavaRuntime, LogLevel,
        LogLine, LogSession, OperationEvent, PlayerActionInput, Server, ServerAlert,
        ServerSettingsInput, ServerStatus, ServerType, VersionCatalog,
    },
    paths::{normalize_path, normalize_path_string, path_string},
    process::parse_jvm_args,
    properties::PropertiesFile,
    state::{directory_size, AppState},
};

type SharedState<'a> = State<'a, Arc<AppState>>;
const MAX_SERVER_ICON_BYTES: u64 = 1024 * 1024;

#[tauri::command]
pub async fn initialize(
    state: SharedState<'_>,
    on_event: Channel<AppEvent>,
) -> CommandResult<AppSnapshot> {
    state.subscribe(on_event);
    Ok(state.snapshot().await)
}

#[tauri::command]
pub async fn cancel_operation(operation_id: String) -> CommandResult<()> {
    if crate::operations::cancel(&operation_id) {
        Ok(())
    } else {
        Err(AppError::from(Error::NotFound(
            "That operation has already finished.".into(),
        )))
    }
}

#[tauri::command]
pub async fn load_server_icon(path: String) -> CommandResult<String> {
    encode_local_server_icon(Path::new(&path))
        .await
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn list_software_versions(
    state: SharedState<'_>,
    server_type: ServerType,
    include_experimental: bool,
) -> CommandResult<VersionCatalog> {
    state
        .catalog
        .list_versions(server_type, include_experimental)
        .await
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn scan_server_folder(path: String) -> CommandResult<ImportScan> {
    scan_folder(PathBuf::from(path))
        .await
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn create_server(
    state: SharedState<'_>,
    input: CreateServerInput,
    on_progress: Channel<OperationEvent>,
) -> CommandResult<Server> {
    create(state.inner().clone(), input, Some(&on_progress))
        .await
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn import_server(
    state: SharedState<'_>,
    input: ImportServerInput,
    on_progress: Channel<OperationEvent>,
) -> CommandResult<Server> {
    import(state.inner().clone(), input, Some(&on_progress))
        .await
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn server_action(
    state: SharedState<'_>,
    id: String,
    action: String,
) -> CommandResult<()> {
    let state = state.inner().clone();
    let lock = state.operation_lock(&id);
    let _guard = lock.try_lock().map_err(|_| {
        AppError::from(Error::Conflict(
            "Another operation is already using this server.".into(),
        ))
    })?;
    match action.as_str() {
        "start" => state.processes.start(state.clone(), &id).await,
        "stop" => state.processes.stop(state.clone(), &id).await,
        "forceStop" => state.processes.force_stop(&id).await,
        "restart" => {
            let mut server = state.server(&id).await.map_err(AppError::from)?;
            server.status = ServerStatus::Restarting;
            state.save_server(server).await.map_err(AppError::from)?;
            state
                .processes
                .stop(state.clone(), &id)
                .await
                .map_err(AppError::from)?;
            if !state
                .processes
                .wait_for_exit(&id, Duration::from_secs(60))
                .await
            {
                return Err(Error::Process(
                    "Minecraft did not stop in time. Force stop it before restarting.".into(),
                )
                .into());
            }
            state.processes.start(state.clone(), &id).await
        }
        _ => Err(Error::Validation("Unknown server action.".into())),
    }
    .map_err(AppError::from)
}

#[tauri::command]
pub async fn send_console_command(
    state: SharedState<'_>,
    id: String,
    command: String,
) -> CommandResult<()> {
    if command.trim_start_matches('/').eq_ignore_ascii_case("stop") {
        return Err(Error::Validation(
            "Use the Stop button so Nooki can track shutdown correctly.".into(),
        )
        .into());
    }
    state
        .append_console(
            &id,
            LogLine {
                id: uuid::Uuid::new_v4().to_string(),
                at: now_ms(),
                level: LogLevel::Info,
                source: "Nooki".into(),
                text: format!("> {command}"),
            },
        )
        .await;
    state
        .processes
        .send(&id, &command)
        .await
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn save_server_settings(
    state: SharedState<'_>,
    id: String,
    input: ServerSettingsInput,
) -> CommandResult<Server> {
    save_settings(state.inner().clone(), &id, input)
        .await
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn dismiss_server_alert(
    state: SharedState<'_>,
    id: String,
    alert_id: String,
) -> CommandResult<Server> {
    let mut server = state.server(&id).await.map_err(AppError::from)?;
    server.alerts.retain(|alert| alert.id != alert_id);
    state
        .save_server(server.clone())
        .await
        .map_err(AppError::from)?;
    Ok(server)
}

#[tauri::command]
pub async fn player_action(
    state: SharedState<'_>,
    id: String,
    input: PlayerActionInput,
) -> CommandResult<()> {
    let state = state.inner().clone();
    if !state.processes.is_running(&id).await {
        return Err(Error::Conflict(
            "Start the server before changing players or access lists.".into(),
        )
        .into());
    }
    validate_username(&input.username).map_err(AppError::from)?;
    let command = match input.action.as_str() {
        "kick" => format!(
            "kick {} {}",
            input.username,
            input
                .reason
                .unwrap_or_else(|| "Kicked by an operator".into())
        ),
        "ban" => format!(
            "ban {} {}",
            input.username,
            input.reason.unwrap_or_else(|| "No reason given".into())
        ),
        "unban" => format!("pardon {}", input.username),
        "whitelistAdd" => format!("whitelist add {}", input.username),
        "whitelistRemove" => format!("whitelist remove {}", input.username),
        "op" => format!("op {}", input.username),
        "deop" => format!("deop {}", input.username),
        _ => return Err(Error::Validation("Unknown player action.".into()).into()),
    };
    state
        .processes
        .send(&id, &command)
        .await
        .map_err(AppError::from)?;
    tokio::time::sleep(Duration::from_millis(400)).await;
    state.refresh_roster(&id).await.map_err(AppError::from)?;
    Ok(())
}

#[tauri::command]
pub async fn create_backup(
    state: SharedState<'_>,
    server_id: String,
    notes: Option<String>,
    backup_type: Option<String>,
    on_progress: Channel<OperationEvent>,
) -> CommandResult<Backup> {
    let state = state.inner().clone();
    let lock = state.operation_lock(&server_id);
    let _guard = lock.try_lock().map_err(|_| {
        AppError::from(Error::Conflict(
            "Another operation is already using this server.".into(),
        ))
    })?;
    backups::create_backup(
        state,
        &server_id,
        backup_type.as_deref().unwrap_or("manual"),
        notes,
        Some(&on_progress),
    )
    .await
    .map_err(AppError::from)
}

#[tauri::command]
pub async fn restore_backup(
    state: SharedState<'_>,
    backup_id: String,
    on_progress: Channel<OperationEvent>,
) -> CommandResult<()> {
    let state = state.inner().clone();
    let backup = state
        .backups
        .read()
        .await
        .get(&backup_id)
        .cloned()
        .ok_or_else(|| AppError::from(Error::NotFound("That backup no longer exists.".into())))?;
    let lock = state.operation_lock(&backup.server_id);
    let _guard = lock.try_lock().map_err(|_| {
        AppError::from(Error::Conflict(
            "Another operation is already using this server.".into(),
        ))
    })?;
    backups::restore_backup(state, &backup_id, Some(&on_progress))
        .await
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn delete_backup(state: SharedState<'_>, backup_id: String) -> CommandResult<()> {
    backups::delete_backup(state.inner().clone(), &backup_id)
        .await
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn save_backup_schedule(
    state: SharedState<'_>,
    server_id: String,
    mut schedule: BackupSchedule,
) -> CommandResult<BackupSchedule> {
    validate_schedule(&mut schedule).map_err(AppError::from)?;
    state
        .db
        .save_schedule(&server_id, &schedule)
        .await
        .map_err(AppError::from)?;
    state
        .schedules
        .write()
        .await
        .insert(server_id.clone(), schedule.clone());
    state.emit(AppEvent::ScheduleChanged {
        server_id,
        schedule: schedule.clone(),
    });
    Ok(schedule)
}

#[tauri::command]
pub async fn remove_server(
    state: SharedState<'_>,
    id: String,
    mode: String,
    confirmation: Option<String>,
) -> CommandResult<()> {
    let state = state.inner().clone();
    let lock = state.operation_lock(&id);
    let _guard = lock.try_lock().map_err(|_| {
        AppError::from(Error::Conflict(
            "Another operation is already using this server.".into(),
        ))
    })?;
    remove(state, &id, &mode, confirmation.as_deref())
        .await
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn detect_java_runtimes(state: SharedState<'_>) -> CommandResult<Vec<JavaRuntime>> {
    let current = state
        .runtimes
        .read()
        .await
        .values()
        .cloned()
        .collect::<Vec<_>>();
    let mut runtimes = java::detect_runtimes(&state.runtime_dir, &current)
        .await
        .map_err(AppError::from)?;
    let servers = state.servers.read().await;
    for runtime in &mut runtimes {
        runtime.used_by = servers
            .values()
            .filter(|server| server.java_runtime_id == runtime.id)
            .count() as u32;
        state
            .db
            .save_runtime(runtime)
            .await
            .map_err(AppError::from)?;
    }
    drop(servers);
    *state.runtimes.write().await = runtimes
        .iter()
        .cloned()
        .map(|runtime| (runtime.id.clone(), runtime))
        .collect();
    state.emit(AppEvent::RuntimesChanged(runtimes.clone()));
    Ok(runtimes)
}

#[tauri::command]
pub async fn install_java_runtime(
    state: SharedState<'_>,
    major: u32,
    on_progress: Channel<OperationEvent>,
) -> CommandResult<JavaRuntime> {
    let id = uuid::Uuid::new_v4().to_string();
    let _operation = crate::operations::begin(&id);
    let runtime = java::install_temurin(
        major,
        &state.runtime_dir,
        Some(&on_progress),
        &id,
        (2.0, 94.0),
    )
    .await
    .map_err(AppError::from)?;
    state
        .db
        .save_runtime(&runtime)
        .await
        .map_err(AppError::from)?;
    state
        .runtimes
        .write()
        .await
        .insert(runtime.id.clone(), runtime.clone());
    state.emit(AppEvent::RuntimesChanged(
        state.runtimes.read().await.values().cloned().collect(),
    ));
    Ok(runtime)
}

#[tauri::command]
pub async fn remove_java_runtime(state: SharedState<'_>, id: String) -> CommandResult<()> {
    let runtime = state
        .runtimes
        .read()
        .await
        .get(&id)
        .cloned()
        .ok_or_else(|| {
            AppError::from(Error::NotFound(
                "That Java runtime no longer exists.".into(),
            ))
        })?;
    if !runtime.bundled {
        return Err(
            Error::Validation("Nooki cannot remove a system Java installation.".into()).into(),
        );
    }
    if state
        .servers
        .read()
        .await
        .values()
        .any(|server| server.java_runtime_id == id)
    {
        return Err(
            Error::Conflict("This Java runtime is still assigned to a server.".into()).into(),
        );
    }
    let root = Path::new(&runtime.path)
        .ancestors()
        .find(|path| path.parent() == Some(state.runtime_dir.as_path()))
        .ok_or_else(|| {
            AppError::from(Error::Validation(
                "The managed Java path is unsafe to remove.".into(),
            ))
        })?;
    tokio::fs::remove_dir_all(root)
        .await
        .map_err(Error::from)
        .map_err(AppError::from)?;
    state.db.delete_runtime(&id).await.map_err(AppError::from)?;
    state.runtimes.write().await.remove(&id);
    state.emit(AppEvent::RuntimesChanged(
        state.runtimes.read().await.values().cloned().collect(),
    ));
    Ok(())
}

#[tauri::command]
pub async fn list_log_sessions(
    state: SharedState<'_>,
    server_id: String,
) -> CommandResult<Vec<LogSession>> {
    let mut sessions = state
        .sessions
        .read()
        .await
        .values()
        .filter(|session| session.server_id == server_id)
        .cloned()
        .collect::<Vec<_>>();
    sessions.sort_by_key(|session| std::cmp::Reverse(session.started_at));
    Ok(sessions)
}

#[tauri::command]
pub async fn read_log_session(
    state: SharedState<'_>,
    session_id: String,
) -> CommandResult<Vec<LogLine>> {
    let session = state
        .sessions
        .read()
        .await
        .get(&session_id)
        .cloned()
        .ok_or_else(|| {
            AppError::from(Error::NotFound("That log session no longer exists.".into()))
        })?;
    read_log(Path::new(&session.path))
        .await
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn export_log(
    state: SharedState<'_>,
    session_id: String,
    destination: String,
) -> CommandResult<()> {
    let session = state
        .sessions
        .read()
        .await
        .get(&session_id)
        .cloned()
        .ok_or_else(|| {
            AppError::from(Error::NotFound("That log session no longer exists.".into()))
        })?;
    tokio::fs::copy(&session.path, destination)
        .await
        .map(|_| ())
        .map_err(Error::from)
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn save_app_settings(
    app: AppHandle,
    state: SharedState<'_>,
    mut settings: AppSettings,
) -> CommandResult<AppSettings> {
    settings.server_folder = normalize_path_string(&settings.server_folder);
    settings.backup_folder = normalize_path_string(&settings.backup_folder);
    tokio::fs::create_dir_all(&settings.server_folder)
        .await
        .map_err(Error::from)
        .map_err(AppError::from)?;
    tokio::fs::create_dir_all(&settings.backup_folder)
        .await
        .map_err(Error::from)
        .map_err(AppError::from)?;
    state
        .db
        .save_settings(&settings)
        .await
        .map_err(AppError::from)?;
    *state.settings.write().await = settings.clone();
    apply_autostart(&app, settings.launch_on_login).map_err(AppError::from)?;
    Ok(settings)
}

#[tauri::command]
pub async fn activate_relay(
    state: SharedState<'_>,
    activation_key: String,
) -> CommandResult<crate::models::RelayAccess> {
    let access = state
        .shares
        .activate(&activation_key)
        .await
        .map_err(AppError::from)?;
    let running = state
        .servers
        .read()
        .await
        .values()
        .filter(|server| matches!(server.status, ServerStatus::Running))
        .map(|server| server.id.clone())
        .collect::<Vec<_>>();
    for server_id in running {
        state.shares.start(state.inner().clone(), &server_id).await;
    }
    Ok(access)
}

#[tauri::command]
pub async fn reveal_path(path: String) -> CommandResult<()> {
    let target = PathBuf::from(path);
    if !target.exists() {
        return Err(Error::NotFound("That file or folder no longer exists.".into()).into());
    }
    let mut command = tokio::process::Command::new("explorer.exe");
    if target.is_file() {
        command.arg(format!("/select,{}", target.display()));
    } else {
        command.arg(target);
    }
    command
        .spawn()
        .map(|_| ())
        .map_err(Error::from)
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn check_server_updates(state: SharedState<'_>) -> CommandResult<Vec<Server>> {
    check_updates(state.inner().clone())
        .await
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn change_server_software(
    state: SharedState<'_>,
    id: String,
    input: ChangeSoftwareInput,
    on_progress: Channel<OperationEvent>,
) -> CommandResult<Server> {
    change_software(state.inner().clone(), &id, input, Some(&on_progress))
        .await
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn quit_application(
    app: AppHandle,
    state: SharedState<'_>,
    force: bool,
) -> CommandResult<bool> {
    let state = state.inner().clone();
    if force {
        let ids = state
            .servers
            .read()
            .await
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for id in ids {
            if state.processes.is_running(&id).await {
                let _ = state.processes.force_stop(&id).await;
            }
        }
        app.exit(0);
        return Ok(true);
    }
    if state.processes.stop_all(state.clone()).await {
        app.exit(0);
        Ok(true)
    } else {
        Ok(false)
    }
}

async fn create(
    state: Arc<AppState>,
    input: CreateServerInput,
    channel: Option<&Channel<OperationEvent>>,
) -> Result<Server> {
    create_with_overlay(state, input, channel, None, None).await
}

pub(crate) struct CreationProgress {
    pub operation_id: String,
    pub start: f32,
    pub end: f32,
}

impl CreationProgress {
    fn map(&self, value: f32) -> f32 {
        self.start + (self.end - self.start) * value.clamp(0.0, 100.0) / 100.0
    }
}

pub(crate) async fn create_with_overlay(
    state: Arc<AppState>,
    input: CreateServerInput,
    channel: Option<&Channel<OperationEvent>>,
    overlay: Option<PathBuf>,
    creation_progress: Option<CreationProgress>,
) -> Result<Server> {
    let icon_data = validate_server_icon_data(input.icon_data.as_deref())?;
    validate_new_server(
        &state,
        &input.name,
        input.port,
        input.min_memory,
        input.max_memory,
        input.eula,
    )
    .await?;
    let operation_id = creation_progress
        .as_ref()
        .map(|progress| progress.operation_id.clone())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let _operation = creation_progress
        .is_none()
        .then(|| crate::operations::begin(&operation_id));
    let map_progress = |value| {
        creation_progress
            .as_ref()
            .map_or(value, |progress| progress.map(value))
    };
    progress(
        channel,
        &operation_id,
        "resolve",
        map_progress(3.0),
        "Resolving server software",
    );
    let resolved = state
        .catalog
        .resolve(
            input.server_type.clone(),
            &input.version,
            input.build.as_deref(),
            input.experimental,
        )
        .await?;
    let runtime = ensure_runtime(
        state.clone(),
        resolved.java_major,
        input.java_runtime_id.as_deref(),
        channel,
        &operation_id,
        (map_progress(5.0), map_progress(18.0)),
    )
    .await?;
    let parent = canonical_or_original(PathBuf::from(&input.parent_folder))?;
    tokio::fs::create_dir_all(&parent).await.map_err(|error| {
        path_io_error(
            "Nooki could not create the server storage folder",
            &parent,
            error,
        )
    })?;
    let folder_name = unique_folder_name(&input.name);
    let final_folder = parent.join(folder_name);
    if final_folder.exists() {
        return Err(Error::Conflict(
            "A folder with this server name already exists.".into(),
        ));
    }
    let staging = parent.join(format!(".nooki-create-{operation_id}"));
    tokio::fs::create_dir_all(&staging).await.map_err(|error| {
        path_io_error(
            "Nooki cannot write to the selected server folder",
            &parent,
            error,
        )
    })?;
    let jar_staging = staging.join("server.jar");
    let result = async {
        state
            .catalog
            .download(
                &resolved,
                &jar_staging,
                channel,
                &operation_id,
                (map_progress(20.0), map_progress(68.0)),
            )
            .await?;
        crate::operations::check(&operation_id)?;
        let launch_target = if matches!(input.server_type, ServerType::Forge | ServerType::NeoForge)
        {
            progress(
                channel,
                &operation_id,
                "install",
                map_progress(72.0),
                if input.server_type == ServerType::NeoForge {
                    "Installing NeoForge libraries"
                } else {
                    "Installing Forge libraries"
                },
            );
            install_mod_loader(
                Path::new(&runtime.path),
                &jar_staging,
                &staging,
                &resolved.version,
                &resolved.build,
                input.server_type.clone(),
                &operation_id,
            )
            .await?
        } else {
            PathBuf::from("server.jar")
        };
        if let Some(overlay) = overlay.as_ref() {
            progress(
                channel,
                &operation_id,
                "modpack",
                map_progress(80.0),
                "Applying modpack server files",
            );
            let source = overlay.clone();
            let destination = staging.clone();
            tokio::task::spawn_blocking(move || merge_server_overlay(&source, &destination))
                .await
                .map_err(|error| Error::Internal(error.to_string()))??;
        }
        progress(
            channel,
            &operation_id,
            "configure",
            map_progress(84.0),
            "Writing server settings",
        );
        tokio::fs::write(
            staging.join("eula.txt"),
            "# Accepted through Nooki\neula=true\n",
        )
        .await?;
        let mut properties = PropertiesFile::parse("");
        properties.update(&HashMap::from([
            ("server-port", input.port.to_string()),
            ("motd", format!("{} - managed by Nooki", input.name.trim())),
            ("max-players", "20".into()),
            ("gamemode", "survival".into()),
            ("difficulty", "normal".into()),
            ("pvp", "true".into()),
            ("white-list", "false".into()),
            ("online-mode", "true".into()),
        ]));
        properties
            .write_atomic(&staging.join("server.properties"))
            .await?;
        if matches!(
            input.server_type,
            ServerType::Forge | ServerType::NeoForge | ServerType::Fabric
        ) {
            tokio::fs::create_dir_all(staging.join("mods")).await?;
        }
        progress(
            channel,
            &operation_id,
            "finalize",
            map_progress(94.0),
            "Finalizing server files",
        );
        crate::operations::check(&operation_id)?;
        rename_directory_with_retry(&staging, &final_folder).await?;
        Ok::<_, Error>(launch_target)
    }
    .await;
    let launch_target = match result {
        Ok(target) => target,
        Err(error) => {
            let _ = tokio::fs::remove_dir_all(&staging).await;
            return Err(error);
        }
    };
    let id = uuid::Uuid::new_v4().to_string();
    let rich_management = modern_management(&input.version);
    let server = Server {
        id: id.clone(),
        name: input.name.trim().into(),
        server_type: input.server_type,
        version: resolved.version,
        build: resolved.build,
        status: ServerStatus::Stopped,
        players: 0,
        max_players: 20,
        started_at: None,
        memory: 0.0,
        min_memory: input.min_memory,
        max_memory: input.max_memory,
        cpu: 0.0,
        disk_used: directory_size(&final_folder).await,
        port: input.port,
        folder: path_string(&final_folder),
        jar_path: path_string(&final_folder.join(launch_target)),
        accent: accent_for(&id),
        icon_data,
        motd: format!("{} - managed by Nooki", input.name.trim()),
        game_mode: "survival".into(),
        difficulty: "normal".into(),
        pvp: true,
        whitelist_enabled: false,
        online_mode: true,
        java_runtime_id: runtime.id.clone(),
        java_runtime: runtime.label.clone(),
        jvm_args: "-XX:+UseG1GC".into(),
        history: Vec::new(),
        alerts: Vec::new(),
        update_available: None,
        last_exit: None,
        active_operation: None,
        rich_management,
        sharing: Default::default(),
        ephemeral: false,
    };
    state.save_server(server.clone()).await?;
    let schedule = BackupSchedule::default();
    state.db.save_schedule(&id, &schedule).await?;
    state.schedules.write().await.insert(id.clone(), schedule);
    state
        .rosters
        .write()
        .await
        .insert(id.clone(), Default::default());
    state
        .activity("settings", Some(&server), "Server added to Nooki")
        .await?;
    progress(
        channel,
        &operation_id,
        "done",
        map_progress(100.0),
        "Server is ready",
    );
    Ok(server)
}

fn merge_server_overlay(source: &Path, destination: &Path) -> Result<()> {
    for entry in walkdir::WalkDir::new(source).follow_links(false) {
        let entry = entry.map_err(|error| Error::Io(error.into()))?;
        if entry.file_type().is_symlink() {
            return Err(Error::Archive(
                "Modpacks containing symbolic links are not supported.".into(),
            ));
        }
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(|error| Error::Internal(error.to_string()))?;
        if relative.as_os_str().is_empty() || protected_pack_path(relative) {
            continue;
        }
        let target = destination.join(relative);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(target)?;
        } else {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn protected_pack_path(path: &Path) -> bool {
    let first = path
        .components()
        .next()
        .and_then(|component| component.as_os_str().to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(first.as_str(), "libraries" | "versions") {
        return true;
    }
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    name == "server.jar"
        || name == "user_jvm_args.txt"
        || name.ends_with(".bat")
        || name.ends_with(".cmd")
        || name.ends_with(".ps1")
        || name.ends_with(".sh")
        || name.ends_with(".exe")
}

async fn import(
    state: Arc<AppState>,
    input: ImportServerInput,
    channel: Option<&Channel<OperationEvent>>,
) -> Result<Server> {
    let icon_data = validate_server_icon_data(input.icon_data.as_deref())?;
    validate_new_server(
        &state,
        &input.name,
        input.port,
        input.min_memory,
        input.max_memory,
        input.eula,
    )
    .await?;
    let operation_id = uuid::Uuid::new_v4().to_string();
    let _operation = crate::operations::begin(&operation_id);
    progress(
        channel,
        &operation_id,
        "scan",
        12.0,
        "Checking existing server files",
    );
    let folder = canonical_or_original(PathBuf::from(&input.folder))?;
    let jar = canonical_or_original(PathBuf::from(&input.jar_path))?;
    if !folder.is_dir() || !jar.is_file() || !jar.starts_with(&folder) {
        return Err(Error::Validation(
            "Choose a server launch file inside the selected server folder.".into(),
        ));
    }
    if state
        .servers
        .read()
        .await
        .values()
        .any(|server| Path::new(&server.folder) == folder)
    {
        return Err(Error::Conflict(
            "This folder is already registered in Nooki.".into(),
        ));
    }
    let java_major = match input.server_type {
        ServerType::Paper => crate::catalog::paper_java_major(&input.version),
        ServerType::Vanilla | ServerType::Forge | ServerType::NeoForge | ServerType::Fabric => {
            state
                .catalog
                .resolve(ServerType::Vanilla, &input.version, None, false)
                .await?
                .java_major
        }
    };
    let runtime = ensure_runtime(
        state.clone(),
        java_major,
        input.java_runtime_id.as_deref(),
        channel,
        &operation_id,
        (16.0, 72.0),
    )
    .await?;
    tokio::fs::write(
        folder.join("eula.txt"),
        "# Accepted through Nooki\neula=true\n",
    )
    .await?;
    let properties = PropertiesFile::read(&folder.join("server.properties")).await?;
    let id = uuid::Uuid::new_v4().to_string();
    let rich_management = modern_management(&input.version);
    let server = Server {
        id: id.clone(),
        name: input.name.trim().into(),
        server_type: input.server_type,
        version: input.version,
        build: input.build,
        status: ServerStatus::Stopped,
        players: 0,
        max_players: properties
            .get("max-players")
            .and_then(|value| value.parse().ok())
            .unwrap_or(20),
        started_at: None,
        memory: 0.0,
        min_memory: input.min_memory,
        max_memory: input.max_memory,
        cpu: 0.0,
        disk_used: directory_size(&folder).await,
        port: input.port,
        folder: path_string(&folder),
        jar_path: path_string(&jar),
        accent: accent_for(&id),
        icon_data,
        motd: crate::properties::unescape_motd(
            properties.get("motd").unwrap_or("A Minecraft Server"),
        ),
        game_mode: properties.get("gamemode").unwrap_or("survival").into(),
        difficulty: properties.get("difficulty").unwrap_or("normal").into(),
        pvp: properties.get("pvp").unwrap_or("true") == "true",
        whitelist_enabled: properties
            .get("white-list")
            .or_else(|| properties.get("whitelist"))
            .unwrap_or("false")
            == "true",
        online_mode: properties.get("online-mode").unwrap_or("true") == "true",
        java_runtime_id: runtime.id.clone(),
        java_runtime: runtime.label.clone(),
        jvm_args: String::new(),
        history: Vec::new(),
        alerts: Vec::new(),
        update_available: None,
        last_exit: None,
        active_operation: None,
        rich_management,
        sharing: Default::default(),
        ephemeral: false,
    };
    state.save_server(server.clone()).await?;
    let schedule = BackupSchedule::default();
    state.db.save_schedule(&id, &schedule).await?;
    state.schedules.write().await.insert(id.clone(), schedule);
    state.refresh_roster(&id).await?;
    state.hydrate_console_from_disk(&server).await;
    state
        .activity("settings", Some(&server), "Existing server imported")
        .await?;
    progress(channel, &operation_id, "done", 100.0, "Server imported");
    Ok(server)
}

async fn install_mod_loader(
    java: &Path,
    installer: &Path,
    folder: &Path,
    minecraft_version: &str,
    forge_version: &str,
    server_type: ServerType,
    operation_id: &str,
) -> Result<PathBuf> {
    let loader_name = if server_type == ServerType::NeoForge {
        "NeoForge"
    } else {
        "Forge"
    };
    let mut command = tokio::process::Command::new(java);
    command
        .current_dir(folder)
        .arg("-jar")
        .arg(installer)
        .arg("--installServer")
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let install = tokio::time::timeout(Duration::from_secs(600), command.output());
    tokio::pin!(install);
    let output = tokio::select! {
        result = &mut install => result.map_err(|_| Error::Process(format!("{loader_name} installation timed out after ten minutes.")))??,
        _ = crate::operations::cancelled(operation_id) => return Err(Error::Cancelled),
    };
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        let detail = detail.chars().rev().take(2_000).collect::<String>();
        let detail = detail.chars().rev().collect::<String>();
        return Err(Error::Process(format!(
            "{loader_name} could not install its server files. {}",
            detail.trim()
        )));
    }

    let coordinate = if server_type == ServerType::NeoForge {
        forge_version.to_owned()
    } else {
        format!("{minecraft_version}-{forge_version}")
    };
    let argument_file = if server_type == ServerType::NeoForge {
        let artifact = if minecraft_version == "1.20.1" {
            "forge"
        } else {
            "neoforge"
        };
        folder
            .join("libraries")
            .join("net")
            .join("neoforged")
            .join(artifact)
            .join(&coordinate)
            .join("win_args.txt")
    } else {
        folder
            .join("libraries")
            .join("net")
            .join("minecraftforge")
            .join("forge")
            .join(&coordinate)
            .join("win_args.txt")
    };
    let target = if argument_file.is_file() {
        argument_file
    } else {
        walkdir::WalkDir::new(folder.join("libraries"))
            .into_iter()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.into_path())
            .find(|path| {
                path.is_file()
                    && path
                        .file_name()
                        .is_some_and(|name| name.eq_ignore_ascii_case("win_args.txt"))
            })
            .ok_or_else(|| {
                Error::Process(format!(
                    "{loader_name} finished installing but did not create a launch target."
                ))
            })?
    };
    let relative = target
        .strip_prefix(folder)
        .map_err(|error| Error::Internal(error.to_string()))?
        .to_path_buf();
    // The installer is no longer needed, but Windows Defender and other file
    // scanners may briefly retain a handle after Java exits. Leaving the file
    // behind is harmless and must not turn a successful Forge install into a
    // failed server setup.
    let _ = tokio::fs::remove_file(installer).await;
    Ok(relative)
}

async fn rename_directory_with_retry(source: &Path, destination: &Path) -> Result<()> {
    const RETRIES: usize = 20;
    for attempt in 0..=RETRIES {
        match tokio::fs::rename(source, destination).await {
            Ok(()) => return Ok(()),
            Err(error)
                if error.kind() == std::io::ErrorKind::PermissionDenied && attempt < RETRIES =>
            {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                return copy_directory_commit(source, destination).await;
            }
            Err(error) => {
                return Err(path_io_error(
                    "Nooki could not finalize the server folder",
                    destination,
                    error,
                ));
            }
        }
    }
    unreachable!("the retry loop always returns on its last attempt")
}

async fn copy_directory_commit(source: &Path, destination: &Path) -> Result<()> {
    tokio::fs::create_dir(destination).await.map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            Error::Conflict(format!(
                "The server folder {} appeared while setup was running. Nooki did not overwrite it.",
                path_string(destination)
            ))
        } else {
            path_io_error(
                "Nooki could not create the final server folder",
                destination,
                error,
            )
        }
    })?;

    let copy_result = async {
        for entry in walkdir::WalkDir::new(source).follow_links(false) {
            let entry = entry.map_err(|error| {
                Error::Io(std::io::Error::other(format!(
                    "Nooki could not read the completed setup files: {error}"
                )))
            })?;
            let relative = entry
                .path()
                .strip_prefix(source)
                .map_err(|error| Error::Internal(error.to_string()))?;
            if relative.as_os_str().is_empty() {
                continue;
            }
            let target = destination.join(relative);
            if entry.file_type().is_dir() {
                tokio::fs::create_dir_all(&target).await.map_err(|error| {
                    path_io_error("Nooki could not create a server subfolder", &target, error)
                })?;
            } else if entry.file_type().is_file() {
                tokio::fs::copy(entry.path(), &target)
                    .await
                    .map_err(|error| {
                        path_io_error(
                            "Nooki could not copy a completed server file",
                            &target,
                            error,
                        )
                    })?;
            } else {
                return Err(Error::Unsupported(format!(
                    "Setup produced an unsupported filesystem entry at {}.",
                    path_string(entry.path())
                )));
            }
        }
        Ok::<_, Error>(())
    }
    .await;

    if let Err(error) = copy_result {
        let _ = tokio::fs::remove_dir_all(destination).await;
        return Err(error);
    }

    // The final folder is now complete. Failure to remove the hidden staging
    // copy must not invalidate it; that Nooki-created copy is safe to remove later.
    let _ = tokio::fs::remove_dir_all(source).await;
    Ok(())
}

fn path_io_error(action: &str, path: &Path, error: std::io::Error) -> Error {
    Error::Io(std::io::Error::new(
        error.kind(),
        format!("{action} at {}: {error}", path_string(path)),
    ))
}

async fn scan_folder(folder: PathBuf) -> Result<ImportScan> {
    let folder = canonical_or_original(folder)?;
    if !folder.is_dir() {
        return Ok(ImportScan {
            folder: path_string(&folder),
            valid: false,
            detected_name: String::new(),
            detected_type: None,
            detected_version: None,
            port: None,
            eula_accepted: false,
            candidates: Vec::new(),
            warnings: vec!["The selected path is not a folder.".into()],
        });
    }
    let properties_path = folder.join("server.properties");
    let properties = PropertiesFile::read(&properties_path).await?;
    let port = properties
        .get("server-port")
        .and_then(|value| value.parse().ok());
    let eula = tokio::fs::read_to_string(folder.join("eula.txt"))
        .await
        .unwrap_or_default()
        .lines()
        .any(|line| line.trim().eq_ignore_ascii_case("eula=true"));
    let mut candidates = Vec::new();
    let mut entries = tokio::fs::read_dir(&folder).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("jar"))
        {
            let (server_type, version, build) = inspect_server_jar(path.clone())
                .await
                .unwrap_or((None, None, None));
            candidates.push(JarCandidate {
                path: path_string(&path),
                file_name: entry.file_name().to_string_lossy().into_owned(),
                server_type,
                version,
                build,
            });
        }
    }
    for entry in walkdir::WalkDir::new(folder.join("libraries"))
        .min_depth(1)
        .max_depth(8)
        .into_iter()
        .filter_map(|entry| entry.ok())
    {
        let path = entry.path();
        if entry.file_type().is_file() && entry.file_name() == "win_args.txt" {
            let coordinate = path
                .parent()
                .and_then(Path::file_name)
                .and_then(|value| value.to_str())
                .unwrap_or_default();
            let normalized_path = path_string(path).replace('\\', "/").to_ascii_lowercase();
            let neoforge = normalized_path.contains("/net/neoforged/");
            let (version, build) = if neoforge {
                (
                    crate::catalog::neoforge_minecraft_version(coordinate),
                    Some(coordinate.into()),
                )
            } else {
                coordinate
                    .rsplit_once('-')
                    .map(|(version, build)| (Some(version.into()), Some(build.into())))
                    .unwrap_or((None, None))
            };
            candidates.push(JarCandidate {
                path: path_string(path),
                file_name: format!(
                    "{} launch arguments ({coordinate})",
                    if neoforge { "NeoForge" } else { "Forge" }
                ),
                server_type: Some(if neoforge {
                    ServerType::NeoForge
                } else {
                    ServerType::Forge
                }),
                version,
                build,
            });
        }
    }
    candidates.sort_by(|a, b| a.file_name.cmp(&b.file_name));
    let detected = candidates
        .iter()
        .find(|candidate| candidate.server_type.is_some())
        .or_else(|| candidates.first());
    let valid = properties_path.is_file() && !candidates.is_empty();
    let mut warnings = Vec::new();
    if !properties_path.is_file() {
        warnings.push("server.properties is missing.".into());
    }
    if candidates.is_empty() {
        warnings.push("No server JAR was found in this folder.".into());
    }
    if candidates.len() > 1 {
        warnings.push(
            "Several JAR files were found. Choose the one normally used to start the server."
                .into(),
        );
    }
    if detected
        .and_then(|candidate| candidate.version.as_ref())
        .is_none()
    {
        warnings.push("Nooki could not determine the exact Minecraft version.".into());
    }
    Ok(ImportScan {
        folder: path_string(&folder),
        valid,
        detected_name: folder
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Imported server".into()),
        detected_type: detected.and_then(|candidate| candidate.server_type.clone()),
        detected_version: detected.and_then(|candidate| candidate.version.clone()),
        port,
        eula_accepted: eula,
        candidates,
        warnings,
    })
}

async fn inspect_server_jar(
    path: PathBuf,
) -> Result<(Option<ServerType>, Option<String>, Option<String>)> {
    tokio::task::spawn_blocking(move || {
        let file = std::fs::File::open(&path)?;
        let mut archive = zip::ZipArchive::new(file)?;
        let mut version = None;
        if let Ok(mut entry) = archive.by_name("version.json") {
            use std::io::Read;
            let mut text = String::new();
            entry.read_to_string(&mut text)?;
            let value: serde_json::Value = serde_json::from_str(&text)?;
            version = value["id"]
                .as_str()
                .or_else(|| value["name"].as_str())
                .map(str::to_owned);
        }
        let file_name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();
        let archive_names = archive.file_names().map(str::to_owned).collect::<Vec<_>>();
        let server_type = if file_name.contains("paper")
            || archive_names.iter().any(|name| name.contains("io/papermc"))
        {
            Some(ServerType::Paper)
        } else if file_name.contains("fabric")
            || archive_names
                .iter()
                .any(|name| name.contains("net/fabricmc/loader"))
        {
            Some(ServerType::Fabric)
        } else if file_name.contains("neoforge")
            || archive_names
                .iter()
                .any(|name| name.contains("net/neoforged/neoforge"))
        {
            Some(ServerType::NeoForge)
        } else if file_name.contains("forge")
            || archive_names
                .iter()
                .any(|name| name.contains("net/minecraftforge"))
        {
            Some(ServerType::Forge)
        } else {
            Some(ServerType::Vanilla)
        };
        let build = match server_type {
            Some(ServerType::Paper) => RegexNumber::paper_build(&file_name),
            Some(ServerType::Fabric) => RegexNumber::fabric_loader(&file_name),
            Some(ServerType::Forge) => RegexNumber::forge_build(&file_name),
            Some(ServerType::NeoForge) => RegexNumber::neoforge_build(&file_name),
            _ => Some("release".into()),
        };
        Ok((server_type, version, build))
    })
    .await
    .map_err(|error| Error::Internal(error.to_string()))?
}

struct RegexNumber;
impl RegexNumber {
    fn paper_build(value: &str) -> Option<String> {
        regex::Regex::new(r"(?:paper|build)[-_]?(?:[0-9.]+-)?([0-9]+)")
            .ok()?
            .captures(value)
            .and_then(|capture| capture.get(1))
            .map(|value| value.as_str().into())
    }

    fn fabric_loader(value: &str) -> Option<String> {
        regex::Regex::new(r"fabric(?:-server)?(?:-mc\.[^-]+)?-loader[.-]([0-9.]+)")
            .ok()?
            .captures(value)
            .and_then(|capture| capture.get(1))
            .map(|value| value.as_str().into())
    }

    fn forge_build(value: &str) -> Option<String> {
        regex::Regex::new(r"forge-[^-]+-([0-9]+(?:\.[0-9]+){1,3})")
            .ok()?
            .captures(value)
            .and_then(|capture| capture.get(1))
            .map(|value| value.as_str().into())
    }

    fn neoforge_build(value: &str) -> Option<String> {
        regex::Regex::new(r"neoforge-([0-9]+(?:\.[0-9]+){2,3}(?:-(?:beta|alpha))?)")
            .ok()?
            .captures(value)
            .and_then(|capture| capture.get(1))
            .map(|value| value.as_str().into())
    }
}

async fn save_settings(
    state: Arc<AppState>,
    id: &str,
    input: ServerSettingsInput,
) -> Result<Server> {
    if input.name.trim().is_empty() {
        return Err(Error::Validation("A server needs a name.".into()));
    }
    if input.port < 1024 {
        return Err(Error::Validation(
            "Pick a port between 1024 and 65535.".into(),
        ));
    }
    if input.min_memory >= input.max_memory {
        return Err(Error::Validation(
            "Maximum memory must be higher than minimum memory.".into(),
        ));
    }
    parse_jvm_args(&input.jvm_args)?;
    let vanity = crate::sharing::normalize_vanity(input.vanity.as_deref())?;
    if state
        .servers
        .read()
        .await
        .values()
        .any(|server| server.id != id && server.name.eq_ignore_ascii_case(input.name.trim()))
    {
        return Err(Error::Conflict(
            "Another server already uses this name.".into(),
        ));
    }
    let runtime = state
        .runtimes
        .read()
        .await
        .get(&input.java_runtime_id)
        .cloned()
        .ok_or_else(|| Error::NotFound("The selected Java runtime is unavailable.".into()))?;
    let mut server = state.server(id).await?;
    let vanity_changed = server.sharing.vanity != vanity;
    let restart_required = state.processes.is_running(id).await
        && (server.port != input.port
            || server.min_memory != input.min_memory
            || server.max_memory != input.max_memory
            || server.java_runtime_id != input.java_runtime_id
            || server.jvm_args != input.jvm_args
            || server.motd != input.motd
            || server.game_mode != input.game_mode
            || server.difficulty != input.difficulty
            || server.max_players != input.max_players
            || server.pvp != input.pvp
            || server.whitelist_enabled != input.whitelist_enabled
            || server.online_mode != input.online_mode);
    let properties_path = Path::new(&server.folder).join("server.properties");
    let mut properties = PropertiesFile::read(&properties_path).await?;
    let original_properties = properties.clone();
    properties.update(&HashMap::from([
        ("server-port", input.port.to_string()),
        ("motd", input.motd.clone()),
        ("gamemode", input.game_mode.clone()),
        ("difficulty", input.difficulty.clone()),
        ("max-players", input.max_players.to_string()),
        ("pvp", input.pvp.to_string()),
        ("white-list", input.whitelist_enabled.to_string()),
        ("online-mode", input.online_mode.to_string()),
    ]));
    properties.write_atomic(&properties_path).await?;
    server.name = input.name.trim().into();
    server.motd = input.motd;
    server.game_mode = input.game_mode;
    server.difficulty = input.difficulty;
    server.max_players = input.max_players;
    server.pvp = input.pvp;
    server.whitelist_enabled = input.whitelist_enabled;
    server.online_mode = input.online_mode;
    server.port = input.port;
    server.min_memory = input.min_memory;
    server.max_memory = input.max_memory;
    server.java_runtime_id = runtime.id;
    server.java_runtime = runtime.label;
    server.jvm_args = input.jvm_args;
    server.sharing.vanity = vanity;
    if vanity_changed {
        server.sharing.address = None;
        server.sharing.last_error = None;
        server.sharing.status = if state.processes.is_running(id).await {
            crate::models::SharingStatus::Connecting
        } else {
            crate::models::SharingStatus::Offline
        };
    }
    if restart_required
        && !server
            .alerts
            .iter()
            .any(|alert| alert.kind == "restart-required")
    {
        server.alerts.push(ServerAlert {
            id: uuid::Uuid::new_v4().to_string(),
            kind: "restart-required".into(),
            title: "Restart required".into(),
            detail: "Restart the server to apply the changed settings.".into(),
            severity: "info".into(),
        });
    }
    if let Err(error) = state.save_server(server.clone()).await {
        let _ = original_properties.write_atomic(&properties_path).await;
        return Err(error);
    }
    state
        .activity("settings", Some(&server), "Settings updated")
        .await?;
    if vanity_changed && state.processes.is_running(id).await {
        state.shares.reconnect(state.clone(), id).await;
    }
    state.server(id).await
}

async fn remove(
    state: Arc<AppState>,
    id: &str,
    mode: &str,
    confirmation: Option<&str>,
) -> Result<()> {
    let server = state.server(id).await?;
    let database_count = state.db.count_databases(id).await?;
    if database_count > 0 {
        return Err(Error::Conflict(format!(
            "Delete this server's {database_count} managed database{} first.",
            if database_count == 1 { "" } else { "s" }
        )));
    }
    if state.processes.is_running(id).await {
        if server.status != ServerStatus::Crashed {
            return Err(Error::Conflict(
                "Stop the server before removing it.".into(),
            ));
        }

        // A fatal startup line can mark a server as crashed just before Java fully exits.
        // Finish terminating that crashed process so its files can be safely removed.
        // It may have exited between the status check and this call; waiting for the
        // monitor to finish is what makes deletion safe, so a redundant kill is harmless.
        let _ = state.processes.force_stop(id).await;
        if !state
            .processes
            .wait_for_exit(id, Duration::from_secs(30))
            .await
        {
            return Err(Error::Process(
                "The crashed Java process did not exit in time. Try again after it closes.".into(),
            ));
        }
    }
    if mode == "recycle" {
        if confirmation != Some(server.name.as_str()) {
            return Err(Error::Validation(
                "Type the server name exactly to recycle its files.".into(),
            ));
        }
        let folder = canonical_or_original(PathBuf::from(&server.folder))?;
        let settings = state.settings.read().await.clone();
        let default_root = canonical_or_original(PathBuf::from(settings.server_folder))?;
        if folder == default_root
            || default_root.starts_with(&folder)
            || folder.parent().is_none()
            || folder.parent().and_then(|path| path.parent()).is_none()
        {
            return Err(Error::Validation(
                "Nooki refused to recycle an unsafe folder path.".into(),
            ));
        }
        if state
            .servers
            .read()
            .await
            .values()
            .any(|other| other.id != id && Path::new(&other.folder).starts_with(&folder))
        {
            return Err(Error::Conflict(
                "Another registered server is inside this folder.".into(),
            ));
        }
        let folder_for_task = folder.clone();
        tokio::task::spawn_blocking(move || crate::recycle::move_to_recycle_bin(&folder_for_task))
            .await
            .map_err(|error| Error::Internal(error.to_string()))??;
    } else if mode != "forget" {
        return Err(Error::Validation("Unknown server removal mode.".into()));
    }
    state.db.delete_server(id).await?;
    state.servers.write().await.remove(id);
    state.players.write().await.remove(id);
    state.rosters.write().await.remove(id);
    state.schedules.write().await.remove(id);
    state.emit(AppEvent::ServerRemoved {
        server_id: id.into(),
    });
    state
        .activity(
            "settings",
            None,
            format!("Removed {} from Nooki", server.name),
        )
        .await?;
    Ok(())
}

pub(crate) async fn ensure_runtime(
    state: Arc<AppState>,
    major: u32,
    requested_id: Option<&str>,
    channel: Option<&Channel<OperationEvent>>,
    operation_id: &str,
    progress_range: (f32, f32),
) -> Result<JavaRuntime> {
    if let Some(id) = requested_id {
        let runtime = state
            .runtimes
            .read()
            .await
            .get(id)
            .cloned()
            .ok_or_else(|| Error::NotFound("The selected Java runtime is unavailable.".into()))?;
        if runtime.major != major {
            return Err(Error::Validation(format!(
                "This server needs Java {major}, but {} is Java {}.",
                runtime.label, runtime.major
            )));
        }
        return Ok(runtime);
    }
    if let Some(runtime) = state
        .runtimes
        .read()
        .await
        .values()
        .find(|runtime| runtime.major == major)
        .cloned()
    {
        return Ok(runtime);
    }
    progress(
        channel,
        operation_id,
        "java",
        progress_range.0,
        &format!("Installing Java {major}"),
    );
    let runtime = java::install_temurin(
        major,
        &state.runtime_dir,
        channel,
        operation_id,
        progress_range,
    )
    .await?;
    state.db.save_runtime(&runtime).await?;
    state
        .runtimes
        .write()
        .await
        .insert(runtime.id.clone(), runtime.clone());
    state.emit(AppEvent::RuntimesChanged(
        state.runtimes.read().await.values().cloned().collect(),
    ));
    Ok(runtime)
}

async fn validate_new_server(
    state: &AppState,
    name: &str,
    port: u16,
    min_memory: u32,
    max_memory: u32,
    eula: bool,
) -> Result<()> {
    if name.trim().is_empty() {
        return Err(Error::Validation("A server needs a name.".into()));
    }
    if !eula {
        return Err(Error::Validation(
            "Accept the Minecraft EULA before continuing.".into(),
        ));
    }
    if port < 1024 {
        return Err(Error::Validation(
            "Pick a port between 1024 and 65535.".into(),
        ));
    }
    if min_memory >= max_memory {
        return Err(Error::Validation(
            "Maximum memory must be higher than minimum memory.".into(),
        ));
    }
    let servers = state.servers.read().await;
    if servers
        .values()
        .any(|server| server.name.eq_ignore_ascii_case(name.trim()))
    {
        return Err(Error::Conflict(
            "Another server already uses this name.".into(),
        ));
    }
    Ok(())
}

fn validate_username(username: &str) -> Result<()> {
    if !(3..=16).contains(&username.len())
        || !username
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err(Error::Validation(
            "Enter a valid Minecraft username.".into(),
        ));
    }
    Ok(())
}

fn validate_schedule(schedule: &mut BackupSchedule) -> Result<()> {
    if !["hourly", "daily", "weekly"].contains(&schedule.frequency.as_str()) {
        return Err(Error::Validation("Choose a valid backup frequency.".into()));
    }
    if !(1..=100).contains(&schedule.keep) {
        return Err(Error::Validation(
            "Keep between 1 and 100 scheduled backups.".into(),
        ));
    }
    if schedule.frequency == "weekly" && schedule.weekday.is_none_or(|day| day > 6) {
        return Err(Error::Validation(
            "Choose a weekday for weekly backups.".into(),
        ));
    }
    if schedule.frequency != "weekly" {
        schedule.weekday = None;
    }
    schedule.next_run_at = Some(next_schedule_time(schedule)?);
    Ok(())
}

pub fn next_schedule_time(schedule: &BackupSchedule) -> Result<i64> {
    use chrono::{Datelike, Duration as ChronoDuration, Local, NaiveTime, TimeZone};
    let now = Local::now();
    if schedule.frequency == "hourly" {
        return Ok((now + ChronoDuration::hours(1)).timestamp_millis());
    }
    let time = NaiveTime::parse_from_str(&schedule.time, "%H:%M")
        .map_err(|_| Error::Validation("Enter a valid backup time.".into()))?;
    let mut candidate = Local
        .from_local_datetime(&now.date_naive().and_time(time))
        .single()
        .unwrap_or(now);
    if schedule.frequency == "daily" {
        if candidate <= now {
            candidate += ChronoDuration::days(1);
        }
    } else {
        // The shared contract uses JavaScript weekday numbering (Sunday = 0).
        let target = ((schedule.weekday.unwrap_or(0) + 6) % 7) as i64;
        let current = now.weekday().num_days_from_monday() as i64;
        let mut delta = (target - current + 7) % 7;
        if delta == 0 && candidate <= now {
            delta = 7;
        }
        candidate += ChronoDuration::days(delta);
    }
    Ok(candidate.timestamp_millis())
}

async fn read_log(path: &Path) -> Result<Vec<LogLine>> {
    let text = tokio::fs::read_to_string(path).await?;
    Ok(text
        .lines()
        .enumerate()
        .map(|(index, text)| LogLine {
            id: format!("{}-{index}", path.display()),
            at: 0,
            level: if text.contains("ERROR") || text.contains("SEVERE") {
                LogLevel::Error
            } else if text.contains("WARN") {
                LogLevel::Warn
            } else {
                LogLevel::Info
            },
            source: "Server".into(),
            text: text.into(),
        })
        .collect())
}

async fn check_updates(state: Arc<AppState>) -> Result<Vec<Server>> {
    let server_list = state
        .servers
        .read()
        .await
        .values()
        .cloned()
        .collect::<Vec<_>>();
    let mut changed = Vec::new();
    for mut server in server_list
        .into_iter()
        .filter(|server| server.server_type == ServerType::Paper)
    {
        if let Ok(resolved) = state
            .catalog
            .resolve(ServerType::Paper, &server.version, None, false)
            .await
        {
            if resolved.build != server.build {
                server.update_available = Some(crate::models::UpdateAvailable {
                    version: server.version.clone(),
                    build: resolved.build,
                    notes: "A newer stable Paper build is available for this Minecraft version."
                        .into(),
                    experimental: false,
                });
                state.save_server(server.clone()).await?;
                changed.push(server);
            }
        }
    }
    Ok(changed)
}

async fn change_software(
    state: Arc<AppState>,
    id: &str,
    input: ChangeSoftwareInput,
    channel: Option<&Channel<OperationEvent>>,
) -> Result<Server> {
    let lock = state.operation_lock(id);
    let _guard = lock
        .try_lock()
        .map_err(|_| Error::Conflict("Another operation is already using this server.".into()))?;
    let original = state.server(id).await?;
    if matches!(
        original.server_type,
        ServerType::Forge | ServerType::NeoForge
    ) {
        return Err(Error::Unsupported(
            "Forge and NeoForge version changes are not available yet. Create a new mod-loader server or update its installation manually, then re-import it."
                .into(),
        ));
    }
    let downgrade = crate::catalog::version_cmp(&input.version, &original.version).is_lt();
    if (input.experimental || downgrade)
        && input.confirmation.as_deref() != Some(original.name.as_str())
    {
        return Err(Error::Validation(if downgrade {
            "Type the server name exactly to downgrade Minecraft.".into()
        } else {
            "Type the server name exactly to use an experimental version.".into()
        }));
    }
    let operation_id = uuid::Uuid::new_v4().to_string();
    let _operation = crate::operations::begin(&operation_id);
    let resolved = state
        .catalog
        .resolve(
            original.server_type.clone(),
            &input.version,
            input.build.as_deref(),
            input.experimental,
        )
        .await?;
    let backup = backups::create_backup(
        state.clone(),
        id,
        "pre-update",
        Some(format!("Before changing to {}", input.version)),
        channel,
    )
    .await?;
    let was_running = state.processes.is_running(id).await;
    if was_running {
        progress(channel, &operation_id, "stop", 22.0, "Stopping the server");
        state.processes.stop(state.clone(), id).await?;
        if !state
            .processes
            .wait_for_exit(id, Duration::from_secs(60))
            .await
        {
            return Err(Error::Process("Minecraft did not stop in time.".into()));
        }
    }
    let jar = PathBuf::from(&original.jar_path);
    let download = jar.with_extension("jar.nooki-new");
    state
        .catalog
        .download(&resolved, &download, channel, &operation_id, (28.0, 68.0))
        .await?;
    if let Err(error) = crate::operations::check(&operation_id) {
        let _ = tokio::fs::remove_file(&download).await;
        return Err(error);
    }
    let previous = jar.with_extension("jar.nooki-prev");
    if previous.exists() {
        tokio::fs::remove_file(&previous).await?;
    }
    tokio::fs::rename(&jar, &previous).await?;
    if let Err(error) = tokio::fs::rename(&download, &jar).await {
        let _ = tokio::fs::rename(&previous, &jar).await;
        return Err(error.into());
    }
    let runtime = match ensure_runtime(
        state.clone(),
        resolved.java_major,
        None,
        channel,
        &operation_id,
        (70.0, 82.0),
    )
    .await
    {
        Ok(runtime) => runtime,
        Err(error) => {
            rollback_jar(&state, &original, &jar, &previous).await?;
            return Err(error);
        }
    };
    let mut changed = original.clone();
    changed.version = resolved.version;
    changed.build = resolved.build;
    changed.java_runtime_id = runtime.id;
    changed.java_runtime = runtime.label;
    changed.update_available = None;
    changed.rich_management = modern_management(&changed.version);
    changed.status = ServerStatus::Stopped;
    if let Err(error) = state.save_server(changed.clone()).await {
        rollback_jar(&state, &original, &jar, &previous).await?;
        return Err(error);
    }
    progress(
        channel,
        &operation_id,
        "restart",
        86.0,
        "Validating the new server software",
    );
    if let Err(error) = state.processes.start(state.clone(), id).await {
        rollback_software_change(state.clone(), &original, &jar, &previous, &backup.id).await?;
        return Err(Error::Process(format!(
            "The new version could not start and was rolled back: {error}"
        )));
    }
    let started = tokio::time::Instant::now();
    loop {
        tokio::time::sleep(Duration::from_millis(500)).await;
        if crate::operations::check(&operation_id).is_err() {
            if state.processes.is_running(id).await {
                let _ = state.processes.force_stop(id).await;
                let _ = state
                    .processes
                    .wait_for_exit(id, Duration::from_secs(10))
                    .await;
            }
            rollback_software_change(state.clone(), &original, &jar, &previous, &backup.id).await?;
            return Err(Error::Cancelled);
        }
        let current = state.server(id).await?;
        if current.status == ServerStatus::Running {
            break;
        }
        if current.status == ServerStatus::Crashed || started.elapsed() > Duration::from_secs(120) {
            if state.processes.is_running(id).await {
                let _ = state.processes.force_stop(id).await;
            }
            let _ = state
                .processes
                .wait_for_exit(id, Duration::from_secs(10))
                .await;
            rollback_software_change(state.clone(), &original, &jar, &previous, &backup.id).await?;
            return Err(Error::Process(
                "The new version failed readiness checks and was rolled back.".into(),
            ));
        }
    }
    if !was_running {
        state.processes.stop(state.clone(), id).await?;
        if !state
            .processes
            .wait_for_exit(id, Duration::from_secs(60))
            .await
        {
            let _ = state.processes.force_stop(id).await;
            let _ = state
                .processes
                .wait_for_exit(id, Duration::from_secs(10))
                .await;
        }
    }
    let final_server = state.server(id).await?;
    state
        .activity(
            "update",
            Some(&final_server),
            format!(
                "Updated to {} build {}",
                final_server.version, final_server.build
            ),
        )
        .await?;
    progress(channel, &operation_id, "done", 100.0, "Update finished");
    Ok(final_server)
}

async fn rollback_software_change(
    state: Arc<AppState>,
    original: &Server,
    jar: &Path,
    previous: &Path,
    backup_id: &str,
) -> Result<()> {
    rollback_jar(&state, original, jar, previous).await?;
    backups::restore_backup(state, backup_id, None).await
}

async fn rollback_jar(
    state: &AppState,
    original: &Server,
    jar: &Path,
    previous: &Path,
) -> Result<()> {
    if jar.exists() {
        tokio::fs::remove_file(jar).await?;
    }
    tokio::fs::rename(previous, jar).await?;
    state.save_server(original.clone()).await
}

fn apply_autostart(app: &AppHandle, enabled: bool) -> Result<()> {
    use tauri_plugin_autostart::ManagerExt;
    if enabled {
        app.autolaunch().enable()
    } else {
        app.autolaunch().disable()
    }
    .map_err(|error| Error::Internal(error.to_string()))
}

fn canonical_or_original(path: PathBuf) -> Result<PathBuf> {
    if path.exists() {
        Ok(normalize_path(std::fs::canonicalize(path)?))
    } else {
        Ok(normalize_path(path))
    }
}

fn unique_folder_name(name: &str) -> String {
    let value = name
        .chars()
        .map(|character| {
            if r#"<>:"/\|?*"#.contains(character) {
                '-'
            } else {
                character
            }
        })
        .collect::<String>();
    let value = value.trim_matches([' ', '.']);
    if value.is_empty() {
        "Minecraft Server".into()
    } else {
        value.into()
    }
}

fn accent_for(id: &str) -> String {
    let palette = [
        "#5fb87f", "#6ba8e6", "#a78be6", "#d99a62", "#d36f78", "#63b7ad",
    ];
    let index = id
        .bytes()
        .fold(0usize, |total, value| total.wrapping_add(value as usize))
        % palette.len();
    palette[index].into()
}

async fn encode_local_server_icon(path: &Path) -> Result<String> {
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|error| path_io_error("Nooki could not read that icon", path, error))?;
    if !metadata.is_file() {
        return Err(Error::Validation(
            "Choose an image file for the server icon.".into(),
        ));
    }
    if metadata.len() > MAX_SERVER_ICON_BYTES {
        return Err(Error::Validation(
            "Server icons must be 1 MB or smaller.".into(),
        ));
    }
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|error| path_io_error("Nooki could not read that icon", path, error))?;
    let mime = server_icon_mime(&bytes).ok_or_else(|| {
        Error::Validation("Choose a PNG, JPEG, or WebP image for the server icon.".into())
    })?;
    Ok(format!("data:{mime};base64,{}", BASE64.encode(bytes)))
}

fn validate_server_icon_data(value: Option<&str>) -> Result<Option<String>> {
    let Some(value) = value.filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if value.len() > (MAX_SERVER_ICON_BYTES as usize * 4 / 3) + 128 {
        return Err(Error::Validation(
            "Server icons must be 1 MB or smaller.".into(),
        ));
    }
    let (header, encoded) = value
        .split_once(";base64,")
        .ok_or_else(|| Error::Validation("The selected server icon is invalid.".into()))?;
    let declared_mime = header
        .strip_prefix("data:")
        .filter(|mime| matches!(*mime, "image/png" | "image/jpeg" | "image/webp"))
        .ok_or_else(|| {
            Error::Validation("Choose a PNG, JPEG, or WebP image for the server icon.".into())
        })?;
    let bytes = BASE64
        .decode(encoded)
        .map_err(|_| Error::Validation("The selected server icon is invalid.".into()))?;
    if bytes.len() as u64 > MAX_SERVER_ICON_BYTES || server_icon_mime(&bytes) != Some(declared_mime)
    {
        return Err(Error::Validation(
            "The selected server icon is invalid.".into(),
        ));
    }
    Ok(Some(format!(
        "data:{declared_mime};base64,{}",
        BASE64.encode(bytes)
    )))
}

fn server_icon_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

fn modern_management(version: &str) -> bool {
    let pieces = version
        .trim_start_matches(|value: char| !value.is_ascii_digit())
        .split('.')
        .filter_map(|part| part.parse::<u32>().ok())
        .collect::<Vec<_>>();
    pieces.first().is_some_and(|major| *major >= 26)
        || (pieces.first() == Some(&1) && pieces.get(1).is_some_and(|minor| *minor >= 7))
}

fn progress(
    channel: Option<&Channel<OperationEvent>>,
    id: &str,
    phase: &str,
    value: f32,
    message: &str,
) {
    if let Some(channel) = channel {
        let _ = channel.send(OperationEvent::Progress {
            operation_id: id.into(),
            phase: phase.into(),
            progress: value,
            message: message.into(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_computes_a_future_time() {
        let schedule = BackupSchedule {
            enabled: true,
            frequency: "daily".into(),
            time: "04:00".into(),
            ..Default::default()
        };
        assert!(next_schedule_time(&schedule).unwrap() > now_ms());
    }

    #[test]
    fn sanitizes_created_folder_names() {
        assert_eq!(unique_folder_name("My: Server?"), "My- Server-");
    }

    #[test]
    fn recognizes_supported_server_icon_formats() {
        assert_eq!(
            server_icon_mime(b"\x89PNG\r\n\x1a\nrest"),
            Some("image/png")
        );
        assert_eq!(
            server_icon_mime(&[0xff, 0xd8, 0xff, 0xe0]),
            Some("image/jpeg")
        );
        assert_eq!(
            server_icon_mime(b"RIFF\x04\x00\x00\x00WEBP"),
            Some("image/webp")
        );
        assert_eq!(server_icon_mime(b"<svg></svg>"), None);
    }

    #[test]
    fn validates_embedded_server_icons() {
        let bytes = b"\x89PNG\r\n\x1a\nrest";
        let value = format!("data:image/png;base64,{}", BASE64.encode(bytes));
        assert_eq!(
            validate_server_icon_data(Some(&value)).unwrap(),
            Some(value)
        );
        assert!(validate_server_icon_data(Some("data:image/svg+xml;base64,PHN2Zz4=")).is_err());
    }

    #[tokio::test]
    async fn copy_commit_preserves_nested_setup_files() {
        let temporary = tempfile::tempdir().unwrap();
        let source = temporary.path().join("staging");
        let destination = temporary.path().join("server");
        tokio::fs::create_dir_all(source.join("libraries"))
            .await
            .unwrap();
        tokio::fs::write(source.join("server.jar"), b"jar")
            .await
            .unwrap();
        tokio::fs::write(source.join("libraries").join("args.txt"), b"args")
            .await
            .unwrap();

        copy_directory_commit(&source, &destination).await.unwrap();

        assert_eq!(
            tokio::fs::read(destination.join("server.jar"))
                .await
                .unwrap(),
            b"jar"
        );
        assert_eq!(
            tokio::fs::read(destination.join("libraries").join("args.txt"))
                .await
                .unwrap(),
            b"args"
        );
        assert!(!source.exists());
    }
}
