use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::{Arc, OnceLock},
};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{ipc::Channel, State};

use crate::{
    error::{AppError, CommandResult, Error, Result},
    models::{HangarPluginMetadata, OperationEvent, Server, ServerAlert, ServerStatus, ServerType},
    state::AppState,
};

const HANGAR_API: &str = "https://hangar.papermc.io/api/v1";
const MAX_PLUGIN_BYTES: u64 = 128 * 1024 * 1024;
const MAX_DESCRIPTOR_BYTES: u64 = 1024 * 1024;
const MAX_ICON_BYTES: u64 = 1024 * 1024;

static ICON_CACHE: OnceLock<tokio::sync::RwLock<HashMap<u64, Option<String>>>> = OnceLock::new();

type SharedState<'a> = State<'a, Arc<AppState>>;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginFile {
    pub file_name: String,
    pub name: String,
    pub version: Option<String>,
    pub description: Option<String>,
    pub authors: Vec<String>,
    pub enabled: bool,
    pub size: u64,
    pub modified_at: i64,
    pub hangar: Option<HangarPluginMetadata>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginProject {
    pub project_id: u64,
    pub namespace: String,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub author: String,
    pub downloads: u64,
    pub stars: u64,
    pub last_updated: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginCatalog {
    pub projects: Vec<PluginProject>,
    pub total: u64,
    pub offset: u32,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginVersionOption {
    pub id: String,
    pub version: String,
    pub release_type: String,
    pub published_at: i64,
    pub automatic: bool,
}

#[derive(Debug, Deserialize)]
struct Page<T> {
    pagination: Pagination,
    result: Vec<T>,
}

#[derive(Debug, Deserialize)]
struct Pagination {
    count: u64,
    limit: u32,
    offset: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HangarProject {
    id: u64,
    name: String,
    namespace: HangarNamespace,
    description: String,
    stats: HangarStats,
    last_updated: String,
}

#[derive(Debug, Deserialize)]
struct HangarNamespace {
    owner: String,
    slug: String,
}

#[derive(Debug, Deserialize)]
struct HangarStats {
    downloads: u64,
    stars: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HangarVersion {
    id: u64,
    name: String,
    created_at: String,
    channel: HangarChannel,
    downloads: std::collections::HashMap<String, HangarDownload>,
}

#[derive(Debug, Clone, Deserialize)]
struct HangarChannel {
    name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HangarDownload {
    file_info: Option<HangarFileInfo>,
    download_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HangarFileInfo {
    sha256_hash: String,
}

#[tauri::command]
pub async fn list_plugins(
    state: SharedState<'_>,
    server_id: String,
) -> CommandResult<Vec<PluginFile>> {
    list_for_server(state.inner(), &server_id)
        .await
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn set_plugin_enabled(
    state: SharedState<'_>,
    server_id: String,
    file_name: String,
    enabled: bool,
) -> CommandResult<Vec<PluginFile>> {
    let lock = state.operation_lock(&server_id);
    let _guard = lock.lock().await;
    let server = editable_paper_server(state.inner(), &server_id).await?;
    let plugins = plugin_directory(&server);
    let source = safe_plugin_path(&plugins, &file_name)?;
    if !source.is_file() {
        return Err(AppError::from(Error::NotFound(
            "That plugin file no longer exists.".into(),
        )));
    }
    let target_name = toggle_file_name(&file_name, enabled)?;
    if target_name == file_name {
        return list_for_server(state.inner(), &server_id)
            .await
            .map_err(AppError::from);
    }
    let target = safe_plugin_path(&plugins, &target_name)?;
    if target.exists() {
        return Err(AppError::from(Error::Conflict(format!(
            "A plugin named {target_name} already exists."
        ))));
    }
    tokio::fs::rename(&source, &target)
        .await
        .map_err(Error::from)
        .map_err(AppError::from)?;
    if let Err(error) = state
        .db
        .rename_plugin_metadata_file(&server_id, &file_name, &target_name)
        .await
    {
        let _ = tokio::fs::rename(&target, &source).await;
        return Err(AppError::from(error));
    }
    list_for_server(state.inner(), &server_id)
        .await
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn delete_plugin(
    state: SharedState<'_>,
    server_id: String,
    file_name: String,
) -> CommandResult<Vec<PluginFile>> {
    let lock = state.operation_lock(&server_id);
    let _guard = lock.lock().await;
    let server = editable_paper_server(state.inner(), &server_id).await?;
    let plugins = plugin_directory(&server);
    let path = safe_plugin_path(&plugins, &file_name)?;
    if !path.is_file() {
        return Err(AppError::from(Error::NotFound(
            "That plugin file no longer exists.".into(),
        )));
    }
    tokio::task::spawn_blocking(move || crate::recycle::move_to_recycle_bin(&path))
        .await
        .map_err(|error| AppError::from(Error::Internal(error.to_string())))?
        .map_err(AppError::from)?;
    state
        .db
        .delete_plugin_metadata_file(&server_id, &file_name)
        .await
        .map_err(AppError::from)?;
    list_for_server(state.inner(), &server_id)
        .await
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn add_plugin_files(
    state: SharedState<'_>,
    server_id: String,
    paths: Vec<String>,
) -> CommandResult<Vec<PluginFile>> {
    if paths.is_empty() {
        return Err(Error::Validation("Choose at least one plugin JAR.".into()).into());
    }
    let lock = state.operation_lock(&server_id);
    let _guard = lock.lock().await;
    let server = installable_paper_server(state.inner(), &server_id).await?;
    let directory = plugin_directory(&server);
    tokio::fs::create_dir_all(&directory)
        .await
        .map_err(Error::from)?;
    let copied = tokio::task::spawn_blocking(move || {
        copy_plugin_files(paths.into_iter().map(PathBuf::from).collect(), &directory)
    })
    .await
    .map_err(|error| Error::Internal(error.to_string()))??;
    if state.processes.is_running(&server_id).await {
        add_restart_alert(
            state.inner(),
            &server_id,
            "Restart the server to load the newly added plugins.",
        )
        .await?;
    }
    state
        .activity(
            "settings",
            Some(&server),
            format!(
                "Added {copied} plugin{} from local files",
                if copied == 1 { "" } else { "s" }
            ),
        )
        .await?;
    list_for_server(state.inner(), &server_id)
        .await
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn search_plugins(query: String, offset: u32) -> CommandResult<PluginCatalog> {
    search_hangar(query, offset).await.map_err(AppError::from)
}

#[tauri::command]
pub async fn load_plugin_icon(project_id: u64) -> CommandResult<Option<String>> {
    plugin_icon(project_id).await.map_err(AppError::from)
}

#[tauri::command]
pub async fn list_plugin_versions(
    state: SharedState<'_>,
    server_id: String,
    namespace: String,
    slug: String,
) -> CommandResult<Vec<PluginVersionOption>> {
    validate_project_part(&namespace)?;
    validate_project_part(&slug)?;
    let server = paper_server(state.inner(), &server_id).await?;
    let versions = compatible_hangar_versions(&namespace, &slug, &server.version).await?;
    let options = versions
        .into_iter()
        .filter(|version| installable_hangar_download(version).is_some())
        .map(|version| PluginVersionOption {
            id: version.id.to_string(),
            version: version.name,
            release_type: version.channel.name,
            published_at: chrono::DateTime::parse_from_rfc3339(&version.created_at)
                .map_or(0, |date| date.timestamp_millis()),
            automatic: true,
        })
        .collect::<Vec<_>>();
    if options.is_empty() {
        return Err(Error::NotFound(format!(
            "{slug} has no directly downloadable release compatible with Paper {}.",
            server.version
        ))
        .into());
    }
    Ok(options)
}

#[tauri::command]
pub async fn install_plugin(
    state: SharedState<'_>,
    server_id: String,
    namespace: String,
    slug: String,
    version_id: String,
    on_progress: Channel<OperationEvent>,
) -> CommandResult<Vec<PluginFile>> {
    install_from_hangar(
        state.inner().clone(),
        server_id,
        namespace,
        slug,
        version_id,
        &on_progress,
    )
    .await
    .map_err(AppError::from)
}

async fn editable_paper_server(state: &Arc<AppState>, id: &str) -> Result<Server> {
    let server = paper_server(state, id).await?;
    if state.processes.is_running(id).await
        || !matches!(server.status, ServerStatus::Stopped | ServerStatus::Crashed)
    {
        return Err(Error::Conflict(
            "Stop the server before enabling, disabling, or deleting plugins.".into(),
        ));
    }
    Ok(server)
}

async fn installable_paper_server(state: &Arc<AppState>, id: &str) -> Result<Server> {
    let server = paper_server(state, id).await?;
    if !matches!(
        server.status,
        ServerStatus::Running | ServerStatus::Stopped | ServerStatus::Crashed
    ) {
        return Err(Error::Conflict(
            "Wait for the current server operation to finish before installing a plugin.".into(),
        ));
    }
    Ok(server)
}

async fn paper_server(state: &Arc<AppState>, id: &str) -> Result<Server> {
    let server = state.server(id).await?;
    if server.server_type != ServerType::Paper {
        return Err(Error::Unsupported(
            "Plugins are only available for Paper servers.".into(),
        ));
    }
    Ok(server)
}

fn plugin_directory(server: &Server) -> PathBuf {
    Path::new(&server.folder).join("plugins")
}

async fn list_for_server(state: &Arc<AppState>, id: &str) -> Result<Vec<PluginFile>> {
    let server = state.server(id).await?;
    if server.server_type != ServerType::Paper {
        return Err(Error::Unsupported(
            "Plugins are only available for Paper servers.".into(),
        ));
    }
    let directory = plugin_directory(&server);
    tokio::fs::create_dir_all(&directory).await?;
    let mut plugins = list_plugin_directory(directory).await?;
    let mut metadata = state
        .db
        .load_plugin_metadata(id)
        .await?
        .into_iter()
        .map(|item| (item.file_name.clone(), item))
        .collect::<HashMap<_, _>>();
    for plugin in &mut plugins {
        plugin.hangar = metadata.remove(&plugin.file_name);
    }
    for stale in metadata.into_values() {
        state
            .db
            .delete_plugin_metadata_file(id, &stale.file_name)
            .await?;
    }
    Ok(plugins)
}

async fn list_plugin_directory(directory: PathBuf) -> Result<Vec<PluginFile>> {
    tokio::task::spawn_blocking(move || read_plugin_directory(&directory))
        .await
        .map_err(|error| Error::Internal(error.to_string()))?
}

fn read_plugin_directory(directory: &Path) -> Result<Vec<PluginFile>> {
    let mut plugins = Vec::new();
    if !directory.exists() {
        return Ok(plugins);
    }
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !is_plugin_file(file_name) || !path.is_file() {
            continue;
        }
        let metadata = entry.metadata()?;
        let enabled = file_name.to_ascii_lowercase().ends_with(".jar");
        let descriptor = read_plugin_descriptor(&path).unwrap_or_default();
        plugins.push(PluginFile {
            file_name: file_name.to_owned(),
            name: descriptor
                .name
                .unwrap_or_else(|| display_name_from_file(file_name)),
            version: descriptor.version,
            description: descriptor.description,
            authors: descriptor.authors,
            enabled,
            size: metadata.len(),
            modified_at: metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |duration| duration.as_millis() as i64),
            hangar: None,
        });
    }
    plugins.sort_by_key(|plugin| plugin.name.to_lowercase());
    Ok(plugins)
}

#[derive(Default)]
struct PluginDescriptor {
    name: Option<String>,
    version: Option<String>,
    description: Option<String>,
    authors: Vec<String>,
}

fn read_plugin_descriptor(path: &Path) -> Result<PluginDescriptor> {
    let file = File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let mut text = String::new();
    let mut found = false;
    for descriptor in ["paper-plugin.yml", "plugin.yml"] {
        if let Ok(mut entry) = archive.by_name(descriptor) {
            if entry.size() > MAX_DESCRIPTOR_BYTES {
                return Err(Error::Archive(
                    "The plugin descriptor is unexpectedly large.".into(),
                ));
            }
            entry.read_to_string(&mut text)?;
            found = true;
            break;
        }
    }
    if !found {
        return Err(Error::Archive(
            "The JAR does not contain a Paper plugin descriptor.".into(),
        ));
    }
    Ok(PluginDescriptor {
        name: yaml_scalar(&text, "name"),
        version: yaml_scalar(&text, "version"),
        description: yaml_scalar(&text, "description"),
        authors: yaml_list(&text, "authors")
            .or_else(|| yaml_scalar(&text, "author").map(|author| vec![author]))
            .unwrap_or_default(),
    })
}

fn copy_plugin_files(sources: Vec<PathBuf>, directory: &Path) -> Result<usize> {
    if sources.len() > 128 {
        return Err(Error::Validation(
            "Choose no more than 128 plugin files at once.".into(),
        ));
    }
    let mut names = HashSet::new();
    let mut copies = Vec::with_capacity(sources.len());
    for source in sources {
        if !source.is_file() {
            return Err(Error::NotFound(format!(
                "Plugin file not found: {}",
                source.display()
            )));
        }
        let name = source
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| {
                Error::Validation("A selected plugin has an invalid file name.".into())
            })?;
        if !name.to_ascii_lowercase().ends_with(".jar") {
            return Err(Error::Validation(format!("{name} is not a plugin JAR.")));
        }
        if !names.insert(name.to_ascii_lowercase()) {
            return Err(Error::Conflict(format!(
                "The selection contains more than one file named {name}."
            )));
        }
        let size = std::fs::metadata(&source)?.len();
        if size > MAX_PLUGIN_BYTES {
            return Err(Error::Validation(format!("{name} is larger than 128 MiB.")));
        }
        read_plugin_descriptor(&source)
            .map_err(|_| Error::Validation(format!("{name} is not a valid Paper plugin JAR.")))?;
        let target = safe_plugin_path(directory, name)?;
        let disabled = safe_plugin_path(directory, &format!("{name}.disabled"))?;
        if target.exists() || disabled.exists() {
            return Err(Error::Conflict(format!("{name} is already installed.")));
        }
        copies.push((source, target));
    }
    let mut created = Vec::with_capacity(copies.len());
    for (source, target) in copies {
        if let Err(error) = std::fs::copy(&source, &target) {
            for path in &created {
                let _ = std::fs::remove_file(path);
            }
            return Err(error.into());
        }
        created.push(target);
    }
    Ok(created.len())
}

async fn add_restart_alert(state: &Arc<AppState>, server_id: &str, detail: &str) -> Result<()> {
    let mut server = state.server(server_id).await?;
    if !server
        .alerts
        .iter()
        .any(|alert| alert.kind == "restart-required")
    {
        server.alerts.push(ServerAlert {
            id: uuid::Uuid::new_v4().to_string(),
            kind: "restart-required".into(),
            title: "Restart required".into(),
            detail: detail.into(),
            severity: "info".into(),
        });
        state.save_server(server).await?;
    }
    Ok(())
}

fn yaml_scalar(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let line = line.trim();
        let value = line.strip_prefix(&format!("{key}:"))?.trim();
        if value.is_empty() || value.starts_with('[') {
            return None;
        }
        Some(value.trim_matches(['\'', '"']).to_owned())
    })
}

fn yaml_list(text: &str, key: &str) -> Option<Vec<String>> {
    let value = text
        .lines()
        .find_map(|line| line.trim().strip_prefix(&format!("{key}:")).map(str::trim))?;
    let body = value.strip_prefix('[')?.strip_suffix(']')?;
    Some(
        body.split(',')
            .map(|item| item.trim().trim_matches(['\'', '"']).to_owned())
            .filter(|item| !item.is_empty())
            .collect(),
    )
}

fn is_plugin_file(file_name: &str) -> bool {
    let lower = file_name.to_ascii_lowercase();
    lower.ends_with(".jar") || lower.ends_with(".jar.disabled")
}

fn display_name_from_file(file_name: &str) -> String {
    file_name
        .strip_suffix(".disabled")
        .unwrap_or(file_name)
        .strip_suffix(".jar")
        .unwrap_or(file_name)
        .to_owned()
}

fn safe_plugin_path(directory: &Path, file_name: &str) -> Result<PathBuf> {
    let path = Path::new(file_name);
    if !is_plugin_file(file_name)
        || path.components().count() != 1
        || !matches!(path.components().next(), Some(Component::Normal(_)))
        || file_name
            .chars()
            .any(|character| character.is_control() || r#"<>:"/\|?*"#.contains(character))
    {
        return Err(Error::Validation("Invalid plugin file name.".into()));
    }
    if std::fs::symlink_metadata(directory).is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        return Err(Error::Validation(
            "The plugins directory cannot be a symbolic link.".into(),
        ));
    }
    let joined = directory.join(path);
    if std::fs::symlink_metadata(&joined).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(Error::Validation(
            "Nooki will not modify a plugin through a symbolic link.".into(),
        ));
    }
    Ok(joined)
}

fn toggle_file_name(file_name: &str, enabled: bool) -> Result<String> {
    if !is_plugin_file(file_name) {
        return Err(Error::Validation("Invalid plugin file name.".into()));
    }
    if enabled {
        Ok(file_name
            .strip_suffix(".disabled")
            .unwrap_or(file_name)
            .to_owned())
    } else if file_name.to_ascii_lowercase().ends_with(".jar.disabled") {
        Ok(file_name.to_owned())
    } else {
        Ok(format!("{file_name}.disabled"))
    }
}

fn http_client() -> Result<reqwest::Client> {
    let contact = option_env!("NOOKI_CONTACT_URL").unwrap_or("nooki@mints.wtf");
    reqwest::Client::builder()
        .user_agent(format!("Nooki/{} ({contact})", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(Error::from)
}

async fn search_hangar(query: String, offset: u32) -> Result<PluginCatalog> {
    let limit = 20u32;
    let page: Page<HangarProject> = http_client()?
        .get(format!("{HANGAR_API}/projects"))
        .query(&[
            ("limit", limit.to_string()),
            ("offset", offset.to_string()),
            ("platform", "PAPER".into()),
            ("sort", "-downloads".into()),
            ("query", query.trim().to_owned()),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let projects = page
        .result
        .into_iter()
        .map(|project| PluginProject {
            project_id: project.id,
            namespace: project.namespace.owner.clone(),
            slug: project.namespace.slug,
            name: project.name,
            description: project.description,
            author: project.namespace.owner,
            downloads: project.stats.downloads,
            stars: project.stats.stars,
            last_updated: chrono::DateTime::parse_from_rfc3339(&project.last_updated)
                .map_or(0, |date| date.timestamp_millis()),
        })
        .collect();
    Ok(PluginCatalog {
        projects,
        total: page.pagination.count,
        offset: page.pagination.offset,
        has_more: u64::from(page.pagination.offset + page.pagination.limit) < page.pagination.count,
    })
}

async fn plugin_icon(project_id: u64) -> Result<Option<String>> {
    let cache = ICON_CACHE.get_or_init(|| tokio::sync::RwLock::new(HashMap::new()));
    if let Some(cached) = cache.read().await.get(&project_id).cloned() {
        return Ok(cached);
    }
    let url = format!("https://hangarcdn.papermc.io/avatars/project/{project_id}.webp");
    let response = http_client()?.get(url).send().await?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        cache.write().await.insert(project_id, None);
        return Ok(None);
    }
    let response = response.error_for_status()?;
    if response
        .content_length()
        .is_some_and(|size| size > MAX_ICON_BYTES)
    {
        return Err(Error::Validation(
            "That plugin logo is unexpectedly large.".into(),
        ));
    }
    let mime = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .filter(|value| matches!(*value, "image/webp" | "image/png" | "image/jpeg"))
        .unwrap_or("image/webp")
        .to_owned();
    let bytes = response.bytes().await?;
    if bytes.len() as u64 > MAX_ICON_BYTES {
        return Err(Error::Validation(
            "That plugin logo is unexpectedly large.".into(),
        ));
    }
    let data_url = Some(format!("data:{mime};base64,{}", BASE64.encode(bytes)));
    cache.write().await.insert(project_id, data_url.clone());
    Ok(data_url)
}

async fn install_from_hangar(
    state: Arc<AppState>,
    server_id: String,
    namespace: String,
    slug: String,
    version_id: String,
    progress: &Channel<OperationEvent>,
) -> Result<Vec<PluginFile>> {
    validate_project_part(&namespace)?;
    validate_project_part(&slug)?;
    let lock = state.operation_lock(&server_id);
    let _guard = lock.lock().await;
    let server = installable_paper_server(&state, &server_id).await?;
    let operation_id = uuid::Uuid::new_v4().to_string();
    let _operation = crate::operations::begin(&operation_id);
    send_progress(
        progress,
        &operation_id,
        "resolve",
        5.0,
        "Finding a compatible Paper release",
    );

    let project: HangarProject = http_client()?
        .get(format!("{HANGAR_API}/projects/{namespace}/{slug}"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let requested_version = version_id
        .parse::<u64>()
        .map_err(|_| Error::Validation("Invalid Hangar version identifier.".into()))?;
    let versions = compatible_hangar_versions(&namespace, &slug, &server.version).await?;
    let version = versions
        .into_iter()
        .find(|version| version.id == requested_version)
        .ok_or_else(|| {
            Error::NotFound(format!(
                "That {slug} release is not compatible with Paper {}.",
                server.version
            ))
        })?;
    let download = version
        .downloads
        .get("PAPER")
        .ok_or_else(|| Error::NotFound(format!("{} has no Paper download.", version.name)))?;
    let url = download.download_url.as_deref().ok_or_else(|| {
        Error::Unsupported(
            "That release uses an external download and cannot be installed automatically.".into(),
        )
    })?;
    let file_info = download.file_info.as_ref().ok_or_else(|| {
        Error::Unsupported("That release does not provide a verifiable plugin file.".into())
    })?;
    let parsed = reqwest::Url::parse(url)
        .map_err(|_| Error::Validation("Hangar returned an invalid download URL.".into()))?;
    if parsed.scheme() != "https"
        || !parsed
            .host_str()
            .is_some_and(|host| host == "papermc.io" || host.ends_with(".papermc.io"))
    {
        return Err(Error::Validation(
            "Hangar returned an untrusted download location.".into(),
        ));
    }

    let plugins = plugin_directory(&server);
    tokio::fs::create_dir_all(&plugins).await?;
    let target_name = format!("{}.jar", safe_slug_file_name(&slug));
    let target = safe_plugin_path(&plugins, &target_name)?;
    let disabled_target = safe_plugin_path(&plugins, &format!("{target_name}.disabled"))?;
    if target.exists() || disabled_target.exists() {
        return Err(Error::Conflict(format!(
            "{slug} is already installed. Delete the existing copy before reinstalling it."
        )));
    }
    let temporary = plugins.join(format!(".nooki-plugin-{}.tmp", uuid::Uuid::new_v4()));
    let result = download_plugin(
        url,
        &file_info.sha256_hash,
        &temporary,
        progress,
        &operation_id,
    )
    .await;
    if let Err(error) = result {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(error);
    }
    let validation_path = temporary.clone();
    let validation = tokio::task::spawn_blocking(move || read_plugin_descriptor(&validation_path))
        .await
        .map_err(|error| Error::Internal(error.to_string()))?;
    if let Err(error) = validation {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(error);
    }
    if let Err(error) = tokio::fs::rename(&temporary, &target).await {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(error.into());
    }
    let metadata = HangarPluginMetadata {
        server_id: server_id.clone(),
        file_name: target_name,
        project_id: project.id,
        namespace: project.namespace.owner.clone(),
        slug: project.namespace.slug,
        name: project.name,
        description: project.description,
        author: project.namespace.owner,
        version: version.name.clone(),
    };
    if let Err(error) = state.db.save_plugin_metadata(&metadata).await {
        let _ = tokio::fs::remove_file(&target).await;
        return Err(error);
    }
    if state.processes.is_running(&server_id).await {
        let mut live_server = state.server(&server_id).await?;
        if !live_server
            .alerts
            .iter()
            .any(|alert| alert.kind == "restart-required")
        {
            live_server.alerts.push(ServerAlert {
                id: uuid::Uuid::new_v4().to_string(),
                kind: "restart-required".into(),
                title: "Restart required".into(),
                detail: format!("Restart the server to load {}.", metadata.name),
                severity: "info".into(),
            });
            state.save_server(live_server).await?;
        }
    }
    send_progress(
        progress,
        &operation_id,
        "install",
        100.0,
        "Plugin installed",
    );
    state
        .activity(
            "settings",
            Some(&server),
            format!("Installed {slug} {} from Hangar", version.name),
        )
        .await?;
    list_for_server(&state, &server_id).await
}

async fn compatible_hangar_versions(
    namespace: &str,
    slug: &str,
    paper_version: &str,
) -> Result<Vec<HangarVersion>> {
    let limit = 25u32;
    let mut offset = 0u32;
    let mut versions = Vec::new();
    loop {
        let page: Page<HangarVersion> = http_client()?
            .get(format!("{HANGAR_API}/projects/{namespace}/{slug}/versions"))
            .query(&[
                ("limit", limit.to_string()),
                ("offset", offset.to_string()),
                ("platform", "PAPER".into()),
                ("platformVersion", paper_version.to_owned()),
            ])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let received = page.result.len() as u32;
        versions.extend(page.result);
        offset = offset.saturating_add(received);
        if received == 0 || u64::from(offset) >= page.pagination.count {
            break;
        }
    }
    Ok(versions)
}

fn installable_hangar_download(version: &HangarVersion) -> Option<&HangarDownload> {
    version
        .downloads
        .get("PAPER")
        .filter(|download| download.download_url.is_some() && download.file_info.is_some())
}

fn validate_project_part(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 100
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err(Error::Validation(
            "Invalid Hangar project identifier.".into(),
        ));
    }
    Ok(())
}

fn safe_slug_file_name(slug: &str) -> String {
    slug.chars()
        .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
        .collect()
}

async fn download_plugin(
    url: &str,
    expected_sha256: &str,
    destination: &Path,
    progress: &Channel<OperationEvent>,
    operation_id: &str,
) -> Result<()> {
    let response = http_client()?.get(url).send().await?.error_for_status()?;
    let total = response.content_length().unwrap_or(0);
    if total > MAX_PLUGIN_BYTES {
        return Err(Error::Validation(
            "That plugin is larger than 128 MiB.".into(),
        ));
    }
    let mut stream = response.bytes_stream();
    let mut file = File::create(destination)?;
    let mut hasher = Sha256::new();
    let mut downloaded = 0u64;
    while let Some(chunk) = stream.next().await {
        if let Err(error) = crate::operations::check(operation_id) {
            drop(file);
            let _ = std::fs::remove_file(destination);
            return Err(error);
        }
        let chunk = chunk?;
        downloaded = downloaded.saturating_add(chunk.len() as u64);
        if downloaded > MAX_PLUGIN_BYTES {
            return Err(Error::Validation(
                "That plugin is larger than 128 MiB.".into(),
            ));
        }
        hasher.update(&chunk);
        file.write_all(&chunk)?;
        let percent = if total > 0 {
            10.0 + downloaded as f32 / total as f32 * 82.0
        } else {
            45.0
        };
        send_progress(
            progress,
            operation_id,
            "download",
            percent,
            "Downloading plugin",
        );
    }
    file.sync_all()?;
    drop(file);
    crate::operations::check(operation_id)?;
    let actual = hex::encode(hasher.finalize());
    if !actual.eq_ignore_ascii_case(expected_sha256) {
        return Err(Error::Validation(
            "The plugin checksum did not match Hangar's release data.".into(),
        ));
    }
    send_progress(
        progress,
        operation_id,
        "validate",
        95.0,
        "Validating plugin JAR",
    );
    Ok(())
}

fn send_progress(
    channel: &Channel<OperationEvent>,
    operation_id: &str,
    phase: &str,
    progress: f32,
    message: &str,
) {
    let _ = channel.send(OperationEvent::Progress {
        operation_id: operation_id.to_owned(),
        phase: phase.to_owned(),
        progress,
        message: message.to_owned(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_enabled_and_disabled_plugin_files() {
        assert!(is_plugin_file("LuckPerms.jar"));
        assert!(is_plugin_file("LuckPerms.jar.disabled"));
        assert!(!is_plugin_file("config.yml"));
        assert!(!is_plugin_file("plugin.disabled"));
    }

    #[test]
    fn toggles_only_the_disabled_suffix() {
        assert_eq!(
            toggle_file_name("Example.jar", false).unwrap(),
            "Example.jar.disabled"
        );
        assert_eq!(
            toggle_file_name("Example.jar.disabled", true).unwrap(),
            "Example.jar"
        );
    }

    #[test]
    fn rejects_paths_and_non_plugin_files() {
        let root = Path::new("plugins");
        assert!(safe_plugin_path(root, "../server.jar").is_err());
        assert!(safe_plugin_path(root, "nested/plugin.jar").is_err());
        assert!(safe_plugin_path(root, "notes.txt").is_err());
    }

    #[test]
    fn manual_plugin_import_copies_every_file_and_keeps_sources() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let plugins = root.path().join("plugins");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(&plugins).unwrap();
        let first = source.join("First.jar");
        let second = source.join("Second.jar");
        for path in [&first, &second] {
            let file = File::create(path).unwrap();
            let mut jar = zip::ZipWriter::new(file);
            jar.start_file("plugin.yml", zip::write::SimpleFileOptions::default())
                .unwrap();
            jar.write_all(b"name: Test\nversion: 1.0\n").unwrap();
            jar.finish().unwrap();
        }
        assert_eq!(
            copy_plugin_files(vec![first.clone(), second.clone()], &plugins).unwrap(),
            2
        );
        assert!(first.exists());
        assert!(second.exists());
        assert!(plugins.join("First.jar").is_file());
        assert!(plugins.join("Second.jar").is_file());
    }
}
