use std::{collections::HashMap, path::Path, process::Stdio, sync::Arc, time::Duration};

use regex::Regex;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin},
    sync::{Mutex, RwLock},
};

use crate::{
    console::parse_console_line,
    error::{Error, Result},
    models::{
        now_ms, AppEvent, LogLevel, LogLine, LogSession, Player, ServerAlert, ServerStatus,
        ServerType,
    },
    paths::path_string,
    state::{avatar_color, directory_size, AppState},
};

#[derive(Clone)]
struct ManagedProcess {
    child: Arc<Mutex<Child>>,
    stdin: Arc<Mutex<ChildStdin>>,
    pid: u32,
    port: u16,
    session_id: String,
    started_at: i64,
}

pub struct ProcessManager {
    processes: RwLock<HashMap<String, ManagedProcess>>,
    start_lock: Mutex<()>,
    #[cfg(windows)]
    job: WindowsJob,
}

impl ProcessManager {
    pub fn new() -> Result<Self> {
        Ok(Self {
            processes: RwLock::new(HashMap::new()),
            start_lock: Mutex::new(()),
            #[cfg(windows)]
            job: WindowsJob::new()?,
        })
    }

    pub async fn is_running(&self, server_id: &str) -> bool {
        self.processes.read().await.contains_key(server_id)
    }

    pub async fn start(&self, state: Arc<AppState>, server_id: &str) -> Result<()> {
        // Serialize startup so the process registry can reserve a port before
        // another server validates and starts with the same one.
        let _start_guard = self.start_lock.lock().await;
        if self.is_running(server_id).await {
            return Err(Error::Conflict("This server is already running.".into()));
        }
        let mut server = state.server(server_id).await?;
        let conflict_id = self
            .processes
            .read()
            .await
            .iter()
            .find_map(|(id, process)| (process.port == server.port).then(|| id.clone()));
        if let Some(conflict_id) = conflict_id {
            let conflict_name = state
                .servers
                .read()
                .await
                .get(&conflict_id)
                .map(|candidate| candidate.name.clone())
                .unwrap_or_else(|| "Another server".into());
            return Err(Error::Conflict(format!(
                "{} is already using port {}. Stop it before starting {}.",
                conflict_name, server.port, server.name
            )));
        }
        let folder = Path::new(&server.folder);
        let jar = Path::new(&server.jar_path);
        if !folder.is_dir() || !jar.is_file() {
            return Err(Error::NotFound(
                "The server folder or launch file no longer exists.".into(),
            ));
        }
        let runtime = state
            .runtimes
            .read()
            .await
            .get(&server.java_runtime_id)
            .cloned()
            .ok_or_else(|| Error::NotFound("The selected Java runtime is unavailable.".into()))?;
        if !Path::new(&runtime.path).is_file() {
            return Err(Error::NotFound(
                "The selected java.exe no longer exists.".into(),
            ));
        }
        validate_port(server.port)?;
        let mut command = tokio::process::Command::new(&runtime.path);
        command
            .current_dir(folder)
            .arg(format!("-Xms{}M", server.min_memory))
            .arg(format!("-Xmx{}M", server.max_memory));
        for argument in parse_jvm_args(&server.jvm_args)? {
            command.arg(argument);
        }
        if matches!(server.server_type, ServerType::Forge | ServerType::NeoForge)
            && jar
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("txt"))
        {
            command.arg(format!("@{}", jar.display())).arg("nogui");
        } else {
            command.arg("-jar").arg(jar).arg("nogui");
        }
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(false)
            .creation_flags(CREATE_NO_WINDOW);
        let mut child = command
            .spawn()
            .map_err(|error| Error::Process(format!("Java could not start: {error}")))?;
        let pid = child
            .id()
            .ok_or_else(|| Error::Process("Java started without a process id.".into()))?;
        #[cfg(windows)]
        self.job.assign(pid)?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| Error::Process("Java stdin was unavailable.".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::Process("Java stdout was unavailable.".into()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| Error::Process("Java stderr was unavailable.".into()))?;
        let session_id = uuid::Uuid::new_v4().to_string();
        let managed = ManagedProcess {
            child: Arc::new(Mutex::new(child)),
            stdin: Arc::new(Mutex::new(stdin)),
            pid,
            port: server.port,
            session_id: session_id.clone(),
            started_at: now_ms(),
        };
        self.processes
            .write()
            .await
            .insert(server_id.into(), managed.clone());
        state.clear_console(server_id).await;
        server.status = ServerStatus::Starting;
        server.started_at = Some(managed.started_at);
        server.history.clear();
        server.cpu = 0.0;
        server.memory = 0.0;
        server.last_exit = None;
        server.alerts.retain(|alert| {
            alert.kind != "crash"
                && alert.kind != "stop-timeout"
                && alert.kind != "restart-required"
        });
        state.save_server(server.clone()).await?;
        state
            .append_console(
                server_id,
                LogLine {
                    id: uuid::Uuid::new_v4().to_string(),
                    at: now_ms(),
                    level: LogLevel::Info,
                    source: "Nooki".into(),
                    text: format!("Starting {} with {}", server.name, runtime.label),
                },
            )
            .await;

        spawn_reader(state.clone(), server_id.into(), stdout, false);
        spawn_reader(state.clone(), server_id.into(), stderr, true);
        spawn_monitor(state.clone(), server_id.into(), managed);
        spawn_readiness_probe(state, server_id.into(), server.port);
        Ok(())
    }

    pub async fn send(&self, server_id: &str, command: &str) -> Result<()> {
        if command.contains(['\r', '\n']) {
            return Err(Error::Validation(
                "Console commands must be a single line.".into(),
            ));
        }
        if command.trim().is_empty() || command.len() > 2_048 {
            return Err(Error::Validation(
                "Enter a command between 1 and 2048 characters.".into(),
            ));
        }
        let process = self
            .processes
            .read()
            .await
            .get(server_id)
            .cloned()
            .ok_or_else(|| Error::Conflict("The server is not running.".into()))?;
        let mut stdin = process.stdin.lock().await;
        stdin
            .write_all(command.trim_start_matches('/').as_bytes())
            .await?;
        stdin.write_all(b"\n").await?;
        stdin.flush().await?;
        Ok(())
    }

    pub async fn stop(&self, state: Arc<AppState>, server_id: &str) -> Result<()> {
        let process = self
            .processes
            .read()
            .await
            .get(server_id)
            .cloned()
            .ok_or_else(|| Error::Conflict("The server is not running.".into()))?;
        let mut server = state.server(server_id).await?;
        server.status = ServerStatus::Stopping;
        state.save_server(server).await?;
        {
            let mut stdin = process.stdin.lock().await;
            stdin.write_all(b"stop\n").await?;
            stdin.flush().await?;
        }
        let state_for_timeout = state.clone();
        let id = server_id.to_owned();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(60)).await;
            if state_for_timeout.processes.is_running(&id).await {
                if let Ok(mut server) = state_for_timeout.server(&id).await {
                    if matches!(
                        server.status,
                        ServerStatus::Stopping | ServerStatus::Restarting
                    ) && !server
                        .alerts
                        .iter()
                        .any(|alert| alert.kind == "stop-timeout")
                    {
                        server.alerts.push(ServerAlert {
                            id: uuid::Uuid::new_v4().to_string(),
                            kind: "stop-timeout".into(),
                            title: "The server is not stopping".into(),
                            detail:
                                "Minecraft did not exit after 60 seconds. You can force stop it."
                                    .into(),
                            severity: "error".into(),
                        });
                        let _ = state_for_timeout.save_server(server).await;
                    }
                }
            }
        });
        Ok(())
    }

    pub async fn force_stop(&self, server_id: &str) -> Result<()> {
        let process = self
            .processes
            .read()
            .await
            .get(server_id)
            .cloned()
            .ok_or_else(|| Error::Conflict("The server is not running.".into()))?;
        process
            .child
            .lock()
            .await
            .kill()
            .await
            .map_err(|error| Error::Process(format!("Could not terminate Java: {error}")))?;
        Ok(())
    }

    pub async fn wait_for_exit(&self, server_id: &str, timeout: Duration) -> bool {
        let started = tokio::time::Instant::now();
        while self.is_running(server_id).await && started.elapsed() < timeout {
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        !self.is_running(server_id).await
    }

    pub async fn stop_all(&self, state: Arc<AppState>) -> bool {
        let ids = self
            .processes
            .read()
            .await
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for id in &ids {
            let _ = self.stop(state.clone(), id).await;
        }
        let started = tokio::time::Instant::now();
        while started.elapsed() < Duration::from_secs(60) {
            if self.processes.read().await.is_empty() {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        false
    }

    pub async fn pid(&self, server_id: &str) -> Option<u32> {
        self.processes
            .read()
            .await
            .get(server_id)
            .map(|process| process.pid)
    }
}

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;
#[cfg(not(windows))]
const CREATE_NO_WINDOW: u32 = 0;

fn spawn_reader<R>(state: Arc<AppState>, server_id: String, reader: R, stderr: bool)
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        while let Ok(Some(text)) = lines.next_line().await {
            handle_line(state.clone(), &server_id, text, stderr).await;
        }
    });
}

async fn handle_line(state: Arc<AppState>, server_id: &str, text: String, stderr: bool) {
    let line = parse_console_line(
        uuid::Uuid::new_v4().to_string(),
        text.clone(),
        stderr,
        now_ms(),
    );
    state.append_console(server_id, line).await;

    if text.contains("Done (") || text.contains("For help, type") {
        if let Ok(mut server) = state.server(server_id).await {
            if server.status == ServerStatus::Starting || server.status == ServerStatus::Restarting
            {
                server.status = ServerStatus::Running;
                server.started_at = Some(now_ms());
                let _ = state.save_server(server.clone()).await;
                if !server.ephemeral {
                    let _ = state
                        .activity("start", Some(&server), "Started and ready for players")
                        .await;
                }
                state.shares.start(state.clone(), server_id).await;
            }
        }
    }

    let joined = [
        Regex::new(r"\]: ([A-Za-z0-9_]{3,16})\[/[^\]]+\] logged in").ok(),
        Regex::new(r"\]: ([A-Za-z0-9_]{3,16}) joined the game").ok(),
    ]
    .into_iter()
    .flatten()
    .find_map(|regex| {
        regex
            .captures(&text)
            .and_then(|capture| capture.get(1).map(|value| value.as_str().to_owned()))
    });
    if let Some(username) = joined {
        let roster = state
            .rosters
            .read()
            .await
            .get(server_id)
            .cloned()
            .unwrap_or_default();
        let player = Player {
            id: format!("{server_id}:{}", username.to_lowercase()),
            username: username.clone(),
            server_id: server_id.into(),
            connected_at: now_ms(),
            is_op: roster
                .operators
                .iter()
                .any(|entry| entry.username.eq_ignore_ascii_case(&username)),
            avatar: avatar_color(&username),
        };
        let mut players = state.players.write().await;
        let list = players.entry(server_id.into()).or_default();
        if !list
            .iter()
            .any(|existing| existing.username.eq_ignore_ascii_case(&username))
        {
            list.push(player);
        }
        let current = list.clone();
        drop(players);
        update_player_count(&state, server_id, current.len() as u32).await;
        state.emit(AppEvent::PlayersChanged {
            server_id: server_id.into(),
            players: current,
        });
    }

    if let Some(names) = Regex::new(r"players online:\s*(.*)$")
        .ok()
        .and_then(|regex| regex.captures(&text))
        .and_then(|capture| capture.get(1))
        .map(|value| value.as_str().trim().to_owned())
    {
        let roster = state
            .rosters
            .read()
            .await
            .get(server_id)
            .cloned()
            .unwrap_or_default();
        let previous = state
            .players
            .read()
            .await
            .get(server_id)
            .cloned()
            .unwrap_or_default();
        let current = names
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(|username| {
                previous
                    .iter()
                    .find(|player| player.username.eq_ignore_ascii_case(username))
                    .cloned()
                    .unwrap_or_else(|| Player {
                        id: format!("{server_id}:{}", username.to_lowercase()),
                        username: username.into(),
                        server_id: server_id.into(),
                        connected_at: now_ms(),
                        is_op: roster
                            .operators
                            .iter()
                            .any(|entry| entry.username.eq_ignore_ascii_case(username)),
                        avatar: avatar_color(username),
                    })
            })
            .collect::<Vec<_>>();
        state
            .players
            .write()
            .await
            .insert(server_id.into(), current.clone());
        update_player_count(&state, server_id, current.len() as u32).await;
        state.emit(AppEvent::PlayersChanged {
            server_id: server_id.into(),
            players: current,
        });
    }

    if let Some(username) = Regex::new(r"\]: ([A-Za-z0-9_]{3,16}) left the game")
        .ok()
        .and_then(|regex| regex.captures(&text))
        .and_then(|capture| capture.get(1))
        .map(|value| value.as_str().to_owned())
    {
        let mut players = state.players.write().await;
        let list = players.entry(server_id.into()).or_default();
        list.retain(|player| !player.username.eq_ignore_ascii_case(&username));
        let current = list.clone();
        drop(players);
        update_player_count(&state, server_id, current.len() as u32).await;
        state.emit(AppEvent::PlayersChanged {
            server_id: server_id.into(),
            players: current,
        });
    }
}

async fn update_player_count(state: &AppState, server_id: &str, count: u32) {
    if let Ok(mut server) = state.server(server_id).await {
        server.players = count;
        let _ = state.save_server(server).await;
    }
}

fn spawn_monitor(state: Arc<AppState>, server_id: String, process: ManagedProcess) {
    tokio::spawn(async move {
        let exit = loop {
            let result = process.child.lock().await.try_wait();
            match result {
                Ok(Some(status)) => break Some(status),
                Ok(None) => tokio::time::sleep(Duration::from_millis(500)).await,
                Err(_) => break None,
            }
        };
        state.processes.processes.write().await.remove(&server_id);
        state.shares.stop_runtime(&server_id).await;
        state.players.write().await.remove(&server_id);
        state.emit(AppEvent::PlayersChanged {
            server_id: server_id.clone(),
            players: Vec::new(),
        });
        if let Ok(mut server) = state.server(&server_id).await {
            let graceful = server.status == ServerStatus::Stopping
                || server.status == ServerStatus::Restarting;
            server.status = if graceful {
                ServerStatus::Stopped
            } else {
                ServerStatus::Crashed
            };
            server.started_at = None;
            server.players = 0;
            server.cpu = 0.0;
            server.memory = 0.0;
            server.sharing.status = crate::models::SharingStatus::Offline;
            server.sharing.address = None;
            server.sharing.last_error = None;
            server.alerts.retain(|alert| alert.kind != "stop-timeout");
            let code = exit.and_then(|status| status.code());
            if !graceful {
                let message = format!(
                    "Java exited unexpectedly{}.",
                    code.map(|value| format!(" with code {value}"))
                        .unwrap_or_default()
                );
                server.last_exit = Some(message.clone());
                server.alerts.push(ServerAlert {
                    id: uuid::Uuid::new_v4().to_string(),
                    kind: "crash".into(),
                    title: "Server stopped unexpectedly".into(),
                    detail: message,
                    severity: "error".into(),
                });
            }
            server.disk_used = directory_size(Path::new(&server.folder)).await;
            let _ = state.save_server(server.clone()).await;
            if server.ephemeral {
                cleanup_ephemeral_server(&state, &server).await;
                return;
            }
            let _ = state
                .activity(
                    if graceful { "stop" } else { "crash" },
                    Some(&server),
                    if graceful {
                        "Stopped normally"
                    } else {
                        "Stopped unexpectedly"
                    },
                )
                .await;
            let session_dir = state.app_data_dir.join("logs").join(&server_id);
            let _ = tokio::fs::create_dir_all(&session_dir).await;
            let session_path = session_dir.join(format!("{}.log", process.session_id));
            let latest = Path::new(&server.folder).join("logs").join("latest.log");
            if latest.is_file() {
                let _ = tokio::fs::copy(&latest, &session_path).await;
            } else {
                let text = state
                    .console_lines
                    .read()
                    .await
                    .get(&server_id)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|line| line.text)
                    .collect::<Vec<_>>()
                    .join("\n");
                let _ = tokio::fs::write(&session_path, text).await;
            }
            let size = tokio::fs::metadata(&session_path)
                .await
                .map(|metadata| metadata.len())
                .unwrap_or(0);
            let session = LogSession {
                id: process.session_id.clone(),
                server_id: server_id.clone(),
                started_at: process.started_at,
                duration: now_ms() - process.started_at,
                size,
                outcome: if graceful {
                    "clean-stop".into()
                } else {
                    "crashed".into()
                },
                path: path_string(&session_path),
                lines: Vec::new(),
            };
            let _ = state.db.save_session(&session).await;
            state
                .sessions
                .write()
                .await
                .insert(session.id.clone(), session);
        }
    });
}

fn spawn_readiness_probe(state: Arc<AppState>, server_id: String, port: u16) {
    tokio::spawn(async move {
        let started = tokio::time::Instant::now();
        while started.elapsed() < Duration::from_secs(120)
            && state.processes.is_running(&server_id).await
        {
            if tokio::net::TcpStream::connect(("127.0.0.1", port))
                .await
                .is_ok()
            {
                if let Ok(mut server) = state.server(&server_id).await {
                    if server.status == ServerStatus::Starting
                        || server.status == ServerStatus::Restarting
                    {
                        server.status = ServerStatus::Running;
                        server.started_at = Some(now_ms());
                        let _ = state.save_server(server.clone()).await;
                        if !server.ephemeral {
                            let _ = state
                                .activity("start", Some(&server), "Started and ready for players")
                                .await;
                        }
                        state.shares.start(state.clone(), &server_id).await;
                    }
                }
                break;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    });
}

async fn cleanup_ephemeral_server(state: &AppState, server: &crate::models::Server) {
    let _ = state.db.delete_server(&server.id).await;
    state.servers.write().await.remove(&server.id);
    state.players.write().await.remove(&server.id);
    state.rosters.write().await.remove(&server.id);
    state.console_lines.write().await.remove(&server.id);
    let folder = Path::new(&server.folder);
    let ephemeral_root = state.app_data_dir.join("ephemeral");
    if folder.starts_with(&ephemeral_root) && folder != ephemeral_root {
        let _ = tokio::fs::remove_dir_all(folder).await;
    }
    state.emit(AppEvent::ServerRemoved {
        server_id: server.id.clone(),
    });
}

pub fn parse_jvm_args(input: &str) -> Result<Vec<String>> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for character in input.chars() {
        if escaped {
            current.push(character);
            escaped = false;
            continue;
        }
        match character {
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            value if value.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    result.push(std::mem::take(&mut current));
                }
            }
            value => current.push(value),
        }
    }
    if quoted {
        return Err(Error::Validation(
            "Java startup options contain an unmatched quote.".into(),
        ));
    }
    if !current.is_empty() {
        result.push(current);
    }
    for argument in &result {
        let lower = argument.to_ascii_lowercase();
        if lower.starts_with("-xms")
            || lower.starts_with("-xmx")
            || lower == "-jar"
            || lower == "nogui"
            || lower == "--nogui"
            || lower.starts_with('@')
        {
            return Err(Error::Validation(
                "Memory, server JAR, and nogui are managed by Nooki.".into(),
            ));
        }
    }
    Ok(result)
}

fn validate_port(port: u16) -> Result<()> {
    if port < 1024 {
        return Err(Error::Validation(
            "Pick a port between 1024 and 65535.".into(),
        ));
    }
    let listener = std::net::TcpListener::bind(("127.0.0.1", port))
        .map_err(|_| Error::Conflict(format!("Port {port} is already in use.")))?;
    drop(listener);
    Ok(())
}

#[cfg(windows)]
struct WindowsJob {
    handle: windows::Win32::Foundation::HANDLE,
}

#[cfg(windows)]
unsafe impl Send for WindowsJob {}
#[cfg(windows)]
unsafe impl Sync for WindowsJob {}

#[cfg(windows)]
impl WindowsJob {
    fn new() -> Result<Self> {
        use windows::Win32::System::JobObjects::{
            CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };
        unsafe {
            let handle =
                CreateJobObjectW(None, None).map_err(|error| Error::Process(error.to_string()))?;
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &info as *const _ as _,
                std::mem::size_of_val(&info) as u32,
            )
            .map_err(|error| Error::Process(error.to_string()))?;
            Ok(Self { handle })
        }
    }

    fn assign(&self, pid: u32) -> Result<()> {
        use windows::Win32::{
            Foundation::CloseHandle,
            System::{
                JobObjects::AssignProcessToJobObject,
                Threading::{OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE},
            },
        };
        unsafe {
            let process = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, false, pid)
                .map_err(|error| Error::Process(error.to_string()))?;
            let result = AssignProcessToJobObject(self.handle, process)
                .map_err(|error| Error::Process(error.to_string()));
            let _ = CloseHandle(process);
            result
        }
    }
}

#[cfg(windows)]
impl Drop for WindowsJob {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_quoted_jvm_arguments_without_using_a_shell() {
        assert_eq!(
            parse_jvm_args(r#"-XX:+UseG1GC -Dname="Sunday Server""#).unwrap(),
            vec!["-XX:+UseG1GC", "-Dname=Sunday Server"]
        );
    }

    #[test]
    fn rejects_owned_arguments() {
        assert!(parse_jvm_args("-Xmx4G").is_err());
        assert!(parse_jvm_args("-jar other.jar").is_err());
        assert!(parse_jvm_args("@unsafe-arguments.txt").is_err());
    }
}
