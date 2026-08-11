use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use parking_lot::Mutex as ParkingMutex;
use tauri::{ipc::Channel, AppHandle, Manager};
use tokio::sync::{Mutex, RwLock};

use crate::{
    catalog::CatalogClient,
    console::parse_console_line,
    db::Database,
    error::{Error, Result},
    java,
    models::{
        now_ms, ActivityEvent, AppEvent, AppSettings, AppSnapshot, Backup, BackupSchedule,
        HostInfo, JavaRuntime, LogLine, LogSession, Player, RosterEntry, Server, ServerRoster,
    },
    paths::normalize_path_string,
    process::ProcessManager,
    sharing::ShareManager,
};

pub struct AppState {
    pub db: Database,
    pub catalog: CatalogClient,
    pub servers: RwLock<HashMap<String, Server>>,
    pub players: RwLock<HashMap<String, Vec<Player>>>,
    pub rosters: RwLock<HashMap<String, ServerRoster>>,
    pub backups: RwLock<HashMap<String, Backup>>,
    pub schedules: RwLock<HashMap<String, BackupSchedule>>,
    pub activity: RwLock<Vec<ActivityEvent>>,
    pub console_lines: RwLock<HashMap<String, Vec<LogLine>>>,
    pub settings: RwLock<AppSettings>,
    pub host: RwLock<HostInfo>,
    pub runtimes: RwLock<HashMap<String, JavaRuntime>>,
    pub sessions: RwLock<HashMap<String, LogSession>>,
    pub processes: ProcessManager,
    pub shares: ShareManager,
    pub subscriber: ParkingMutex<Option<Channel<AppEvent>>>,
    pub operation_locks: ParkingMutex<HashMap<String, Arc<Mutex<()>>>>,
    pub app_data_dir: PathBuf,
    pub runtime_dir: PathBuf,
}

impl AppState {
    pub async fn new(app: AppHandle) -> Result<Arc<Self>> {
        let app_data_dir = app
            .path()
            .app_local_data_dir()
            .map_err(|error| Error::Internal(error.to_string()))?;
        let runtime_dir = app_data_dir.join("runtimes");
        tokio::fs::create_dir_all(&app_data_dir).await?;
        tokio::fs::create_dir_all(&runtime_dir).await?;
        let db = Database::open(&app_data_dir.join("nooki.db")).await?;

        let documents = app
            .path()
            .document_dir()
            .unwrap_or_else(|_| app_data_dir.clone());
        let nooki_documents = documents.join("Nooki");
        let defaults = AppSettings {
            server_folder: nooki_documents
                .join("Servers")
                .to_string_lossy()
                .into_owned(),
            backup_folder: nooki_documents
                .join("Backups")
                .to_string_lossy()
                .into_owned(),
            minimize_to_tray: true,
            launch_on_login: false,
        };
        let mut settings = db.load_settings(defaults).await?;
        settings.server_folder = normalize_path_string(&settings.server_folder);
        settings.backup_folder = normalize_path_string(&settings.backup_folder);
        db.save_settings(&settings).await?;
        tokio::fs::create_dir_all(&settings.server_folder).await?;
        tokio::fs::create_dir_all(&settings.backup_folder).await?;

        let ephemeral_dir = app_data_dir.join("ephemeral");
        if ephemeral_dir.exists() {
            tokio::fs::remove_dir_all(&ephemeral_dir).await?;
        }
        tokio::fs::create_dir_all(&ephemeral_dir).await?;
        let loaded_servers = db.load_servers().await?;
        for server in loaded_servers.iter().filter(|server| server.ephemeral) {
            db.delete_server(&server.id).await?;
        }
        let mut servers = loaded_servers
            .into_iter()
            .filter(|server| !server.ephemeral)
            .map(|server| (server.id.clone(), server))
            .collect::<HashMap<_, _>>();
        for server in servers.values_mut() {
            server.folder = normalize_path_string(&server.folder);
            server.jar_path = normalize_path_string(&server.jar_path);
            if !matches!(
                server.status,
                crate::models::ServerStatus::Stopped | crate::models::ServerStatus::Crashed
            ) {
                server.status = crate::models::ServerStatus::Stopped;
                server.started_at = None;
                server.players = 0;
                server.cpu = 0.0;
                server.memory = 0.0;
            }
            server.active_operation = None;
            server.sharing.status = crate::models::SharingStatus::Offline;
            server.sharing.address = None;
            server.sharing.last_error = None;
            db.save_server(server).await?;
        }
        let mut existing_runtimes = db.load_runtimes().await?;
        for runtime in &mut existing_runtimes {
            runtime.path = normalize_path_string(&runtime.path);
        }
        let mut detected = java::detect_runtimes(&runtime_dir, &existing_runtimes).await?;
        for runtime in &mut detected {
            runtime.used_by = servers
                .values()
                .filter(|server| server.java_runtime_id == runtime.id)
                .count() as u32;
            db.save_runtime(runtime).await?;
        }
        let runtimes = detected
            .into_iter()
            .map(|runtime| (runtime.id.clone(), runtime))
            .collect();
        let mut loaded_backups = db.load_backups().await?;
        for backup in &mut loaded_backups {
            backup.path = normalize_path_string(&backup.path);
            if !Path::new(&backup.path).is_file() {
                backup.failed = Some(true);
                backup.error_message = Some("The backup archive is missing from disk.".into());
                db.save_backup(backup).await?;
            } else if backup.error_message.as_deref()
                == Some("The backup archive is missing from disk.")
            {
                backup.failed = Some(false);
                backup.error_message = None;
            }
            db.save_backup(backup).await?;
        }
        let backups = loaded_backups
            .into_iter()
            .map(|backup| (backup.id.clone(), backup))
            .collect();
        let schedules = db.load_schedules().await?;
        let activity = db.load_activity(500).await?;
        let mut loaded_sessions = db.load_sessions().await?;
        for session in &mut loaded_sessions {
            session.path = normalize_path_string(&session.path);
            db.save_session(session).await?;
        }
        let sessions = loaded_sessions
            .into_iter()
            .map(|session| (session.id.clone(), session))
            .collect();
        let mut rosters = HashMap::new();
        let mut console_lines = HashMap::new();
        for server in servers.values() {
            rosters.insert(
                server.id.clone(),
                read_roster(Path::new(&server.folder))
                    .await
                    .unwrap_or_default(),
            );
            let lines = read_latest_console(server).await.unwrap_or_default();
            if !lines.is_empty() {
                console_lines.insert(server.id.clone(), lines);
            }
        }

        let state = Arc::new(Self {
            db,
            catalog: CatalogClient::new()?,
            servers: RwLock::new(servers),
            players: RwLock::new(HashMap::new()),
            rosters: RwLock::new(rosters),
            backups: RwLock::new(backups),
            schedules: RwLock::new(schedules),
            activity: RwLock::new(activity),
            console_lines: RwLock::new(console_lines),
            settings: RwLock::new(settings),
            host: RwLock::new(sample_host()),
            runtimes: RwLock::new(runtimes),
            sessions: RwLock::new(sessions),
            processes: ProcessManager::new()?,
            shares: ShareManager::new(&app_data_dir).await?,
            subscriber: ParkingMutex::new(None),
            operation_locks: ParkingMutex::new(HashMap::new()),
            app_data_dir,
            runtime_dir,
        });
        Ok(state)
    }

    pub async fn snapshot(&self) -> AppSnapshot {
        let mut servers = self
            .servers
            .read()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        let ephemeral_server = servers.iter().find(|server| server.ephemeral).cloned();
        servers.retain(|server| !server.ephemeral);
        servers.sort_by_key(|server| server.name.to_lowercase());
        let players = self
            .players
            .read()
            .await
            .values()
            .flatten()
            .cloned()
            .collect();
        let rosters = self.rosters.read().await.clone();
        let mut backups = self
            .backups
            .read()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        backups.sort_by_key(|backup| std::cmp::Reverse(backup.created_at));
        let schedules = self.schedules.read().await.clone();
        let activity = self.activity.read().await.clone();
        let console_lines = self.console_lines.read().await.clone();
        let settings = self.settings.read().await.clone();
        let relay_access = self.shares.access().await;
        let host = self.host.read().await.clone();
        let mut java_runtimes = self
            .runtimes
            .read()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        java_runtimes.sort_by_key(|runtime| std::cmp::Reverse(runtime.major));
        let mut log_sessions = self
            .sessions
            .read()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        log_sessions.sort_by_key(|session| std::cmp::Reverse(session.started_at));
        AppSnapshot {
            servers,
            ephemeral_server,
            players,
            rosters,
            backups,
            schedules,
            activity,
            console_lines,
            settings,
            relay_access,
            host,
            java_runtimes,
            log_sessions,
            app_version: env!("CARGO_PKG_VERSION").into(),
        }
    }

    pub fn subscribe(&self, channel: Channel<AppEvent>) {
        *self.subscriber.lock() = Some(channel);
    }

    pub fn emit(&self, event: AppEvent) {
        let mut subscriber = self.subscriber.lock();
        if subscriber
            .as_ref()
            .is_some_and(|channel| channel.send(event).is_err())
        {
            *subscriber = None;
        }
    }

    pub fn operation_lock(&self, server_id: &str) -> Arc<Mutex<()>> {
        self.operation_locks
            .lock()
            .entry(server_id.to_owned())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    pub async fn server(&self, id: &str) -> Result<Server> {
        self.servers
            .read()
            .await
            .get(id)
            .cloned()
            .ok_or_else(|| Error::NotFound("That server is no longer registered.".into()))
    }

    pub async fn save_server(&self, server: Server) -> Result<()> {
        self.db.save_server(&server).await?;
        self.servers
            .write()
            .await
            .insert(server.id.clone(), server.clone());
        self.emit(AppEvent::ServerChanged(server));
        Ok(())
    }

    pub async fn activity(
        &self,
        kind: &str,
        server: Option<&Server>,
        message: impl Into<String>,
    ) -> Result<ActivityEvent> {
        let event = ActivityEvent {
            id: uuid::Uuid::new_v4().to_string(),
            kind: kind.into(),
            server_id: server.map(|server| server.id.clone()),
            server_name: server.map(|server| server.name.clone()),
            at: now_ms(),
            message: message.into(),
        };
        self.db.save_activity(&event).await?;
        let mut activity = self.activity.write().await;
        activity.insert(0, event.clone());
        activity.truncate(500);
        drop(activity);
        self.emit(AppEvent::ActivityAdded(event.clone()));
        Ok(event)
    }

    pub async fn append_console(&self, server_id: &str, line: LogLine) {
        let mut lines = self.console_lines.write().await;
        let buffer = lines.entry(server_id.to_owned()).or_default();
        if buffer.last().is_some_and(|previous| {
            previous.level == line.level
                && previous.source == line.source
                && previous.text == line.text
                && line.at.saturating_sub(previous.at) <= 1_000
        }) {
            return;
        }
        buffer.push(line.clone());
        if buffer.len() > 2_000 {
            buffer.drain(..buffer.len() - 2_000);
        }
        drop(lines);
        self.emit(AppEvent::ConsoleLine {
            server_id: server_id.into(),
            line,
        });
    }

    pub async fn clear_console(&self, server_id: &str) {
        self.console_lines.write().await.remove(server_id);
        self.emit(AppEvent::ConsoleCleared {
            server_id: server_id.into(),
        });
    }

    pub async fn hydrate_console_from_disk(&self, server: &Server) {
        if self
            .console_lines
            .read()
            .await
            .get(&server.id)
            .is_some_and(|lines| !lines.is_empty())
        {
            return;
        }
        let Ok(lines) = read_latest_console(server).await else {
            return;
        };
        if lines.is_empty() {
            return;
        }
        self.console_lines
            .write()
            .await
            .insert(server.id.clone(), lines.clone());
        for line in lines {
            self.emit(AppEvent::ConsoleLine {
                server_id: server.id.clone(),
                line,
            });
        }
    }

    pub async fn refresh_roster(&self, server_id: &str) -> Result<ServerRoster> {
        let server = self.server(server_id).await?;
        let roster = read_roster(Path::new(&server.folder)).await?;
        self.rosters
            .write()
            .await
            .insert(server_id.into(), roster.clone());
        self.emit(AppEvent::RostersChanged {
            server_id: server_id.into(),
            roster: roster.clone(),
        });
        Ok(roster)
    }
}

async fn read_latest_console(server: &Server) -> Result<Vec<LogLine>> {
    let path = Path::new(&server.folder).join("logs").join("latest.log");
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let text = tokio::fs::read_to_string(&path).await?;
    let rows = text.lines().collect::<Vec<_>>();
    let start = rows.len().saturating_sub(2_000);
    Ok(rows[start..]
        .iter()
        .enumerate()
        .map(|(index, text)| {
            parse_console_line(
                format!("history-{}-{index}", server.id),
                (*text).to_owned(),
                false,
                now_ms(),
            )
        })
        .collect())
}

pub fn sample_host() -> HostInfo {
    use sysinfo::{Disks, System};
    let mut system = System::new_all();
    system.refresh_all();
    let disks = Disks::new_with_refreshed_list();
    let disk_total = disks
        .list()
        .iter()
        .map(|disk| disk.total_space())
        .sum::<u64>() as f64
        / 1_048_576.0;
    let disk_free = disks
        .list()
        .iter()
        .map(|disk| disk.available_space())
        .sum::<u64>() as f64
        / 1_048_576.0;
    HostInfo {
        total_memory: system.total_memory() as f64 / 1_048_576.0,
        used_memory: system.used_memory() as f64 / 1_048_576.0,
        cpu: system.global_cpu_usage(),
        disk_total,
        disk_used: (disk_total - disk_free).max(0.0),
    }
}

async fn read_roster(folder: &Path) -> Result<ServerRoster> {
    let whitelist = read_roster_file(&folder.join("whitelist.json"), None).await?;
    let operators = read_roster_file(&folder.join("ops.json"), None).await?;
    let banned = read_roster_file(&folder.join("banned-players.json"), Some("reason")).await?;
    Ok(ServerRoster {
        whitelist,
        operators,
        banned,
    })
}

async fn read_roster_file(path: &Path, reason_key: Option<&str>) -> Result<Vec<RosterEntry>> {
    let text = match tokio::fs::read_to_string(path).await {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let values: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap_or_default();
    Ok(values
        .into_iter()
        .filter_map(|value| {
            let username = value["name"].as_str()?.to_owned();
            Some(RosterEntry {
                id: value["uuid"].as_str().unwrap_or(&username).to_owned(),
                avatar: avatar_color(&username),
                username,
                added_at: value["created"]
                    .as_str()
                    .and_then(|created| chrono::DateTime::parse_from_rfc3339(created).ok())
                    .map(|date| date.timestamp_millis())
                    .unwrap_or(0),
                reason: reason_key
                    .and_then(|key| value[key].as_str())
                    .map(str::to_owned),
            })
        })
        .collect())
}

pub fn avatar_color(username: &str) -> String {
    let mut hash = 0u32;
    for byte in username.bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(byte as u32);
    }
    format!("hsl({}, 34%, 46%)", hash % 360)
}

pub async fn directory_size(path: &Path) -> f64 {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        walkdir::WalkDir::new(path)
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.metadata().ok())
            .filter(|metadata| metadata.is_file())
            .map(|metadata| metadata.len())
            .sum::<u64>() as f64
            / 1_048_576.0
    })
    .await
    .unwrap_or(0.0)
}
