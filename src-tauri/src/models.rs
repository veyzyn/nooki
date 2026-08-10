use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ServerType {
    Vanilla,
    Paper,
    Forge,
    NeoForge,
    Fabric,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ServerStatus {
    Running,
    #[default]
    Stopped,
    Crashed,
    Starting,
    Stopping,
    Restarting,
    Updating,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ResourceSample {
    pub at: i64,
    pub cpu: f32,
    pub memory: f32,
    pub players: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerAlert {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub detail: String,
    pub severity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAvailable {
    pub version: String,
    pub build: String,
    pub notes: String,
    #[serde(default)]
    pub experimental: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveOperation {
    pub id: String,
    pub kind: String,
    pub phase: String,
    pub progress: Option<f32>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Server {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub server_type: ServerType,
    pub version: String,
    pub build: String,
    pub status: ServerStatus,
    pub players: u32,
    pub max_players: u32,
    pub started_at: Option<i64>,
    pub memory: f64,
    pub min_memory: u32,
    pub max_memory: u32,
    pub cpu: f32,
    pub disk_used: f64,
    pub port: u16,
    pub folder: String,
    pub jar_path: String,
    pub accent: String,
    pub motd: String,
    pub game_mode: String,
    pub difficulty: String,
    pub pvp: bool,
    pub whitelist_enabled: bool,
    pub online_mode: bool,
    pub java_runtime_id: String,
    pub java_runtime: String,
    pub jvm_args: String,
    #[serde(default)]
    pub history: Vec<ResourceSample>,
    #[serde(default)]
    pub alerts: Vec<ServerAlert>,
    pub update_available: Option<UpdateAvailable>,
    pub last_exit: Option<String>,
    pub active_operation: Option<ActiveOperation>,
    #[serde(default = "default_rich_management")]
    pub rich_management: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HangarPluginMetadata {
    pub server_id: String,
    pub file_name: String,
    pub project_id: u64,
    pub namespace: String,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub author: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModMetadata {
    pub server_id: String,
    pub file_name: String,
    pub provider: String,
    pub project_id: String,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub author: String,
    pub version: String,
    pub icon_url: Option<String>,
    pub website_url: String,
}

fn default_rich_management() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Player {
    pub id: String,
    pub username: String,
    pub server_id: String,
    pub connected_at: i64,
    pub is_op: bool,
    pub avatar: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RosterEntry {
    pub id: String,
    pub username: String,
    pub avatar: String,
    pub added_at: i64,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ServerRoster {
    pub whitelist: Vec<RosterEntry>,
    pub operators: Vec<RosterEntry>,
    pub banned: Vec<RosterEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Backup {
    pub id: String,
    pub server_id: String,
    pub server_name: String,
    #[serde(rename = "type")]
    pub backup_type: String,
    pub created_at: i64,
    pub size: u64,
    pub version: String,
    pub notes: Option<String>,
    pub failed: Option<bool>,
    pub path: String,
    pub checksum: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupSchedule {
    pub enabled: bool,
    pub frequency: String,
    pub time: String,
    pub keep: u32,
    pub weekday: Option<u8>,
    pub last_run_at: Option<i64>,
    pub next_run_at: Option<i64>,
}

impl Default for BackupSchedule {
    fn default() -> Self {
        Self {
            enabled: false,
            frequency: "daily".into(),
            time: "04:00".into(),
            keep: 5,
            weekday: Some(0),
            last_run_at: None,
            next_run_at: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityEvent {
    pub id: String,
    pub kind: String,
    pub server_id: Option<String>,
    pub server_name: Option<String>,
    pub at: i64,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    pub id: String,
    pub at: i64,
    pub level: LogLevel,
    pub source: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogSession {
    pub id: String,
    pub server_id: String,
    pub started_at: i64,
    pub duration: i64,
    pub size: u64,
    pub outcome: String,
    pub path: String,
    #[serde(default)]
    pub lines: Vec<LogLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaRuntime {
    pub id: String,
    pub label: String,
    pub version: String,
    pub major: u32,
    pub path: String,
    pub bundled: bool,
    pub used_by: u32,
    pub architecture: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub server_folder: String,
    pub backup_folder: String,
    pub minimize_to_tray: bool,
    pub launch_on_login: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HostInfo {
    pub total_memory: f64,
    pub used_memory: f64,
    pub cpu: f32,
    pub disk_total: f64,
    pub disk_used: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    pub servers: Vec<Server>,
    pub players: Vec<Player>,
    pub rosters: HashMap<String, ServerRoster>,
    pub backups: Vec<Backup>,
    pub schedules: HashMap<String, BackupSchedule>,
    pub activity: Vec<ActivityEvent>,
    pub console_lines: HashMap<String, Vec<LogLine>>,
    pub settings: AppSettings,
    pub host: HostInfo,
    pub java_runtimes: Vec<JavaRuntime>,
    pub log_sessions: Vec<LogSession>,
    pub app_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionOption {
    pub id: String,
    pub version: String,
    pub build: String,
    pub release_type: String,
    pub experimental: bool,
    pub java_major: Option<u32>,
    pub published_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionCatalog {
    pub server_type: ServerType,
    pub versions: Vec<VersionOption>,
    pub fetched_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JarCandidate {
    pub path: String,
    pub file_name: String,
    pub server_type: Option<ServerType>,
    pub version: Option<String>,
    pub build: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportScan {
    pub folder: String,
    pub valid: bool,
    pub detected_name: String,
    pub detected_type: Option<ServerType>,
    pub detected_version: Option<String>,
    pub port: Option<u16>,
    pub eula_accepted: bool,
    pub candidates: Vec<JarCandidate>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateServerInput {
    pub name: String,
    #[serde(rename = "type")]
    pub server_type: ServerType,
    pub version: String,
    pub build: Option<String>,
    pub min_memory: u32,
    pub max_memory: u32,
    pub port: u16,
    pub parent_folder: String,
    pub eula: bool,
    pub java_runtime_id: Option<String>,
    #[serde(default)]
    pub experimental: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportServerInput {
    pub name: String,
    pub folder: String,
    pub jar_path: String,
    #[serde(rename = "type")]
    pub server_type: ServerType,
    pub version: String,
    pub build: String,
    pub min_memory: u32,
    pub max_memory: u32,
    pub port: u16,
    pub eula: bool,
    pub java_runtime_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerSettingsInput {
    pub name: String,
    pub motd: String,
    pub game_mode: String,
    pub difficulty: String,
    pub max_players: u32,
    pub pvp: bool,
    pub whitelist_enabled: bool,
    pub online_mode: bool,
    pub port: u16,
    pub min_memory: u32,
    pub max_memory: u32,
    pub java_runtime_id: String,
    pub jvm_args: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerActionInput {
    pub action: String,
    pub username: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeSoftwareInput {
    pub version: String,
    pub build: Option<String>,
    pub experimental: bool,
    pub confirmation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseKind {
    Mysql,
    Postgresql,
    Mongodb,
    Redis,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseStatus {
    Running,
    Stopped,
    Creating,
    Error,
    Missing,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedDatabase {
    pub id: String,
    pub server_id: String,
    pub kind: DatabaseKind,
    pub name: String,
    pub status: DatabaseStatus,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub database: String,
    pub connection_uri: String,
    pub container_name: String,
    pub volume_name: String,
    pub created_at: i64,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseEnvironment {
    pub available: bool,
    pub version: Option<String>,
    pub message: Option<String>,
    pub code: Option<String>,
    pub cli_path: Option<String>,
    pub context: Option<String>,
    pub details: Vec<String>,
    pub suggestions: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateDatabaseInput {
    pub kind: DatabaseKind,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WorldKind {
    Overworld,
    Nether,
    End,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldEntry {
    pub id: String,
    pub name: String,
    pub folder_name: String,
    pub kind: WorldKind,
    pub path: String,
    pub generated: bool,
    pub primary: bool,
    pub custom: bool,
    pub seed: Option<String>,
    pub version: Option<String>,
    pub data_version: Option<i32>,
    pub size: u64,
    pub region_files: u32,
    pub player_files: u32,
    pub last_played: Option<i64>,
    pub spawn_x: Option<i32>,
    pub spawn_y: Option<i32>,
    pub spawn_z: Option<i32>,
    pub border_size: Option<f64>,
    pub day_time: Option<i64>,
    pub weather: String,
    pub game_mode: Option<String>,
    pub difficulty: Option<String>,
    pub hardcore: bool,
    pub allow_commands: bool,
    pub metadata_error: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldSettingsInput {
    pub seed: String,
    pub spawn_x: i32,
    pub spawn_y: i32,
    pub spawn_z: i32,
    pub border_size: f64,
    pub day_time: i64,
    pub weather: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "event",
    content = "data",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum OperationEvent {
    Started {
        operation_id: String,
        message: String,
    },
    Progress {
        operation_id: String,
        phase: String,
        progress: f32,
        message: String,
    },
    Finished {
        operation_id: String,
        message: String,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "event",
    content = "data",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[allow(clippy::large_enum_variant)]
pub enum AppEvent {
    ServerChanged(Server),
    ServerRemoved {
        server_id: String,
    },
    ConsoleLine {
        server_id: String,
        line: LogLine,
    },
    ConsoleCleared {
        server_id: String,
    },
    PlayersChanged {
        server_id: String,
        players: Vec<Player>,
    },
    RostersChanged {
        server_id: String,
        roster: ServerRoster,
    },
    BackupChanged(Backup),
    BackupRemoved {
        backup_id: String,
    },
    ScheduleChanged {
        server_id: String,
        schedule: BackupSchedule,
    },
    ActivityAdded(ActivityEvent),
    HostMetrics(HostInfo),
    RuntimesChanged(Vec<JavaRuntime>),
    QuitRequested {
        running_servers: u32,
    },
}

#[cfg(test)]
mod event_serialization_tests {
    use super::*;

    #[test]
    fn app_event_struct_fields_use_frontend_casing() {
        let value = serde_json::to_value(AppEvent::ConsoleLine {
            server_id: "server-1".into(),
            line: LogLine {
                id: "line-1".into(),
                at: 1,
                level: LogLevel::Info,
                source: "Server".into(),
                text: "Ready".into(),
            },
        })
        .unwrap();

        assert_eq!(value["event"], "consoleLine");
        assert_eq!(value["data"]["serverId"], "server-1");
        assert!(value["data"].get("server_id").is_none());
    }

    #[test]
    fn operation_event_struct_fields_use_frontend_casing() {
        let value = serde_json::to_value(OperationEvent::Progress {
            operation_id: "operation-1".into(),
            phase: "download".into(),
            progress: 50.0,
            message: "Downloading".into(),
        })
        .unwrap();

        assert_eq!(value["event"], "progress");
        assert_eq!(value["data"]["operationId"], "operation-1");
        assert!(value["data"].get("operation_id").is_none());
    }
}
