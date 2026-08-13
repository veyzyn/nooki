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
use sha1::{Digest, Sha1};
use tauri::{ipc::Channel, AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::{
    error::{AppError, CommandResult, Error, Result},
    models::{now_ms, ModMetadata, OperationEvent, Server, ServerAlert, ServerStatus, ServerType},
    state::AppState,
};

const MODRINTH_API: &str = "https://api.modrinth.com/v2";
const CURSEFORGE_API: &str = "https://api.curseforge.com/v1";
const MAX_MOD_BYTES: u64 = 512 * 1024 * 1024;
const MAX_ICON_BYTES: u64 = 1024 * 1024;
const PAGE_SIZE: u32 = 20;

type SharedState<'a> = State<'a, Arc<AppState>>;

static ICON_CACHE: OnceLock<tokio::sync::RwLock<HashMap<String, Option<String>>>> = OnceLock::new();
static MANUAL_DOWNLOADS: OnceLock<tokio::sync::RwLock<HashMap<String, PendingManualDownload>>> =
    OnceLock::new();

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModFile {
    file_name: String,
    name: String,
    version: Option<String>,
    description: Option<String>,
    authors: Vec<String>,
    enabled: bool,
    size: u64,
    modified_at: i64,
    metadata: Option<ModMetadata>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModProject {
    provider: String,
    project_id: String,
    slug: String,
    name: String,
    description: String,
    author: String,
    downloads: u64,
    followers: u64,
    last_updated: i64,
    icon_url: Option<String>,
    website_url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModCatalog {
    projects: Vec<ModProject>,
    total: u64,
    offset: u32,
    has_more: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModVersionOption {
    id: String,
    version: String,
    release_type: String,
    published_at: i64,
    file_name: String,
    automatic: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualModDownload {
    token: String,
    project_name: String,
    file_name: String,
    download_url: String,
    downloads_folder: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModInstallResult {
    mods: Vec<ModFile>,
    manual_download: Option<ManualModDownload>,
}

#[derive(Clone)]
struct PendingManualDownload {
    manual: ManualModDownload,
    server_id: String,
    expected_sha1: Option<String>,
    created_at: i64,
    metadata: ModMetadata,
}

#[derive(Debug, Deserialize)]
struct ModrinthSearch {
    hits: Vec<ModrinthSearchProject>,
    offset: u32,
    limit: u32,
    total_hits: u64,
}

#[derive(Debug, Deserialize)]
struct ModrinthSearchProject {
    project_id: String,
    slug: String,
    title: String,
    description: String,
    author: String,
    downloads: u64,
    follows: u64,
    date_modified: String,
    icon_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ModrinthProjectDetail {
    id: String,
    slug: String,
    title: String,
    description: String,
    team: String,
    icon_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ModrinthTeamMember {
    user: ModrinthUser,
    is_owner: bool,
}

#[derive(Debug, Deserialize)]
struct ModrinthUser {
    username: String,
}

#[derive(Debug, Deserialize)]
struct ModrinthVersion {
    id: String,
    version_number: String,
    version_type: String,
    date_published: String,
    files: Vec<ModrinthFile>,
}

#[derive(Debug, Deserialize)]
struct ModrinthFile {
    hashes: HashMap<String, String>,
    url: String,
    filename: String,
    primary: bool,
}

#[derive(Debug, Deserialize)]
struct CurseResponse<T> {
    data: T,
    pagination: Option<CursePagination>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CursePagination {
    index: u32,
    page_size: u32,
    total_count: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurseProject {
    id: u64,
    name: String,
    slug: String,
    summary: String,
    download_count: u64,
    date_modified: String,
    authors: Vec<CurseAuthor>,
    logo: Option<CurseLogo>,
}

#[derive(Debug, Deserialize)]
struct CurseAuthor {
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurseLogo {
    thumbnail_url: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurseFile {
    id: u64,
    display_name: String,
    file_name: String,
    release_type: u8,
    file_date: String,
    download_url: Option<String>,
    hashes: Vec<CurseHash>,
}

#[derive(Debug, Deserialize)]
struct CurseHash {
    value: String,
    algo: u8,
}

#[tauri::command]
pub async fn list_mods(state: SharedState<'_>, server_id: String) -> CommandResult<Vec<ModFile>> {
    list_for_server(state.inner(), &server_id)
        .await
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn set_mod_enabled(
    state: SharedState<'_>,
    server_id: String,
    file_name: String,
    enabled: bool,
) -> CommandResult<Vec<ModFile>> {
    let lock = state.operation_lock(&server_id);
    let _guard = lock.lock().await;
    let server = editable_modded_server(state.inner(), &server_id).await?;
    let directory = mod_directory(&server);
    let source = safe_mod_path(&directory, &file_name)?;
    if !source.is_file() {
        return Err(Error::NotFound("That mod file no longer exists.".into()).into());
    }
    let target_name = toggle_file_name(&file_name, enabled)?;
    if target_name == file_name {
        return list_for_server(state.inner(), &server_id)
            .await
            .map_err(AppError::from);
    }
    let target = safe_mod_path(&directory, &target_name)?;
    if target.exists() {
        return Err(Error::Conflict(format!("A mod named {target_name} already exists.")).into());
    }
    tokio::fs::rename(&source, &target)
        .await
        .map_err(Error::from)?;
    if let Err(error) = state
        .db
        .rename_mod_metadata_file(&server_id, &file_name, &target_name)
        .await
    {
        let _ = tokio::fs::rename(&target, &source).await;
        return Err(error.into());
    }
    list_for_server(state.inner(), &server_id)
        .await
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn delete_mod(
    state: SharedState<'_>,
    server_id: String,
    file_name: String,
) -> CommandResult<Vec<ModFile>> {
    let lock = state.operation_lock(&server_id);
    let _guard = lock.lock().await;
    let server = editable_modded_server(state.inner(), &server_id).await?;
    let path = safe_mod_path(&mod_directory(&server), &file_name)?;
    if !path.is_file() {
        return Err(Error::NotFound("That mod file no longer exists.".into()).into());
    }
    tokio::task::spawn_blocking(move || crate::recycle::move_to_recycle_bin(&path))
        .await
        .map_err(|error| Error::Internal(error.to_string()))??;
    state
        .db
        .delete_mod_metadata_file(&server_id, &file_name)
        .await?;
    list_for_server(state.inner(), &server_id)
        .await
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn add_mod_files(
    state: SharedState<'_>,
    server_id: String,
    paths: Vec<String>,
) -> CommandResult<Vec<ModFile>> {
    if paths.is_empty() {
        return Err(Error::Validation("Choose at least one mod JAR.".into()).into());
    }
    let lock = state.operation_lock(&server_id);
    let _guard = lock.lock().await;
    let server = installable_modded_server(state.inner(), &server_id).await?;
    let directory = mod_directory(&server);
    tokio::fs::create_dir_all(&directory)
        .await
        .map_err(Error::from)?;
    let copied = tokio::task::spawn_blocking(move || {
        copy_mod_files(paths.into_iter().map(PathBuf::from).collect(), &directory)
    })
    .await
    .map_err(|error| Error::Internal(error.to_string()))??;
    if state.processes.is_running(&server_id).await {
        add_restart_alert(
            state.inner(),
            &server_id,
            "Restart the server to load the newly added mods.",
        )
        .await?;
    }
    state
        .activity(
            "settings",
            Some(&server),
            format!(
                "Added {copied} mod{} from local files",
                if copied == 1 { "" } else { "s" }
            ),
        )
        .await?;
    list_for_server(state.inner(), &server_id)
        .await
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn search_mods(
    provider: String,
    loader: String,
    game_version: String,
    query: String,
    offset: u32,
) -> CommandResult<ModCatalog> {
    validate_loader(&loader)?;
    match provider.as_str() {
        "modrinth" => search_modrinth(&loader, &game_version, &query, offset).await,
        "curseforge" => search_curseforge(&loader, &game_version, &query, offset).await,
        _ => Err(Error::Validation("Choose Modrinth or CurseForge.".into())),
    }
    .map_err(AppError::from)
}

#[tauri::command]
pub async fn load_mod_icon(provider: String, icon_url: String) -> CommandResult<Option<String>> {
    load_icon(&provider, &icon_url)
        .await
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn list_mod_versions(
    state: SharedState<'_>,
    server_id: String,
    provider: String,
    project_id: String,
) -> CommandResult<Vec<ModVersionOption>> {
    let server = modded_server(state.inner(), &server_id).await?;
    let versions: Vec<ModVersionOption> = match provider.as_str() {
        "modrinth" => compatible_modrinth_versions(&server, &project_id)
            .await?
            .into_iter()
            .filter_map(|version| {
                let file_name = preferred_modrinth_file(&version)?.filename.clone();
                Some(ModVersionOption {
                    id: version.id,
                    version: version.version_number,
                    release_type: version.version_type,
                    published_at: parse_date(&version.date_published),
                    file_name,
                    automatic: true,
                })
            })
            .collect(),
        "curseforge" => compatible_curse_files(&server, &project_id)
            .await?
            .into_iter()
            .map(|file| ModVersionOption {
                id: file.id.to_string(),
                version: file.display_name,
                release_type: curse_release_type(file.release_type).into(),
                published_at: parse_date(&file.file_date),
                file_name: file.file_name,
                automatic: file.download_url.is_some(),
            })
            .collect(),
        _ => return Err(Error::Validation("Choose Modrinth or CurseForge.".into()).into()),
    };
    if versions.is_empty() {
        return Err(Error::NotFound(format!(
            "No compatible releases were found for {} {}.",
            loader_name(&server)?,
            server.version
        ))
        .into());
    }
    Ok(versions)
}

#[tauri::command]
pub async fn install_mod(
    state: SharedState<'_>,
    server_id: String,
    provider: String,
    project_id: String,
    version_id: String,
    on_progress: Channel<OperationEvent>,
) -> CommandResult<ModInstallResult> {
    install_from_provider(
        state.inner().clone(),
        server_id,
        provider,
        project_id,
        version_id,
        &on_progress,
    )
    .await
    .map_err(AppError::from)
}

#[tauri::command]
pub async fn check_manual_mod_download(
    state: SharedState<'_>,
    token: String,
) -> CommandResult<ModInstallResult> {
    check_manual_download(state.inner().clone(), &token)
        .await
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn cancel_manual_mod_download(token: String) -> CommandResult<()> {
    manual_downloads().write().await.remove(&token);
    Ok(())
}

#[tauri::command]
pub async fn open_manual_mod_download(app: AppHandle, token: String) -> CommandResult<()> {
    let pending = manual_downloads()
        .read()
        .await
        .get(&token)
        .cloned()
        .ok_or_else(|| Error::NotFound("That manual download request has expired.".into()))?;
    app.opener()
        .open_url(pending.manual.download_url, None::<String>)
        .map_err(|error| Error::Internal(error.to_string()))?;
    Ok(())
}

async fn search_modrinth(
    loader: &str,
    version: &str,
    query: &str,
    offset: u32,
) -> Result<ModCatalog> {
    let facets = serde_json::to_string(&vec![
        vec!["project_type:mod"],
        vec![&format!("categories:{loader}")],
        vec![&format!("versions:{version}")],
        vec!["server_side!=unsupported"],
    ])?;
    let response: ModrinthSearch = http_client()?
        .get(format!("{MODRINTH_API}/search"))
        .query(&[
            ("query", query.trim().to_owned()),
            ("facets", facets),
            ("index", "downloads".into()),
            ("offset", offset.to_string()),
            ("limit", PAGE_SIZE.to_string()),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let projects = response
        .hits
        .into_iter()
        .map(|project| ModProject {
            provider: "modrinth".into(),
            website_url: format!("https://modrinth.com/mod/{}", project.slug),
            project_id: project.project_id,
            slug: project.slug,
            name: project.title,
            description: project.description,
            author: project.author,
            downloads: project.downloads,
            followers: project.follows,
            last_updated: parse_date(&project.date_modified),
            icon_url: project.icon_url,
        })
        .collect();
    Ok(ModCatalog {
        projects,
        total: response.total_hits,
        offset: response.offset,
        has_more: u64::from(response.offset + response.limit) < response.total_hits,
    })
}

async fn search_curseforge(
    loader: &str,
    version: &str,
    query: &str,
    offset: u32,
) -> Result<ModCatalog> {
    let response: CurseResponse<Vec<CurseProject>> =
        curse_request(format!("{CURSEFORGE_API}/mods/search"))?
            .query(&[
                ("gameId", "432".to_owned()),
                ("classId", "6".to_owned()),
                ("gameVersion", version.to_owned()),
                ("modLoaderType", curse_loader(loader).to_owned()),
                ("searchFilter", query.trim().to_owned()),
                ("sortField", "6".to_owned()),
                ("sortOrder", "desc".to_owned()),
                ("index", offset.to_string()),
                ("pageSize", PAGE_SIZE.to_string()),
            ])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
    let pagination = response.pagination.unwrap_or(CursePagination {
        index: offset,
        page_size: PAGE_SIZE,
        total_count: response.data.len() as u64,
    });
    let projects = response
        .data
        .into_iter()
        .map(|project| ModProject {
            provider: "curseforge".into(),
            project_id: project.id.to_string(),
            website_url: format!(
                "https://www.curseforge.com/minecraft/mc-mods/{}",
                project.slug
            ),
            slug: project.slug,
            name: project.name,
            description: project.summary,
            author: project
                .authors
                .first()
                .map_or_else(String::new, |author| author.name.clone()),
            downloads: project.download_count,
            followers: 0,
            last_updated: parse_date(&project.date_modified),
            icon_url: project.logo.map(|logo| logo.thumbnail_url),
        })
        .collect();
    Ok(ModCatalog {
        projects,
        total: pagination.total_count,
        offset: pagination.index,
        has_more: u64::from(pagination.index + pagination.page_size) < pagination.total_count,
    })
}

async fn compatible_modrinth_versions(
    server: &Server,
    project_id: &str,
) -> Result<Vec<ModrinthVersion>> {
    validate_project_id(project_id)?;
    let loader = loader_name(server)?;
    http_client()?
        .get(format!("{MODRINTH_API}/project/{project_id}/version"))
        .query(&[
            ("loaders", serde_json::to_string(&vec![loader])?),
            (
                "game_versions",
                serde_json::to_string(&vec![server.version.as_str()])?,
            ),
            ("include_changelog", "false".into()),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await
        .map_err(Error::from)
}

async fn compatible_curse_files(server: &Server, project_id: &str) -> Result<Vec<CurseFile>> {
    let project_number = project_id
        .parse::<u64>()
        .map_err(|_| Error::Validation("Invalid CurseForge project identifier.".into()))?;
    let loader = loader_name(server)?;
    let mut index = 0u32;
    let mut files = Vec::new();
    loop {
        let index_value = index.to_string();
        let response: CurseResponse<Vec<CurseFile>> =
            curse_request(format!("{CURSEFORGE_API}/mods/{project_number}/files"))?
                .query(&[
                    ("gameVersion", server.version.as_str()),
                    ("modLoaderType", curse_loader(loader)),
                    ("index", index_value.as_str()),
                    ("pageSize", "50"),
                ])
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
        let received = response.data.len() as u32;
        let total = response
            .pagination
            .as_ref()
            .map_or(u64::from(index + received), |page| page.total_count);
        files.extend(response.data);
        index = index.saturating_add(received);
        if received == 0 || u64::from(index) >= total {
            break;
        }
    }
    Ok(files)
}

fn preferred_modrinth_file(version: &ModrinthVersion) -> Option<&ModrinthFile> {
    version
        .files
        .iter()
        .find(|file| file.primary)
        .or_else(|| version.files.first())
}

fn curse_release_type(value: u8) -> &'static str {
    match value {
        1 => "release",
        2 => "beta",
        _ => "alpha",
    }
}

async fn install_from_provider(
    state: Arc<AppState>,
    server_id: String,
    provider: String,
    project_id: String,
    version_id: String,
    progress: &Channel<OperationEvent>,
) -> Result<ModInstallResult> {
    let lock = state.operation_lock(&server_id);
    let _guard = lock.lock().await;
    let server = installable_modded_server(&state, &server_id).await?;
    let operation_id = uuid::Uuid::new_v4().to_string();
    let _operation = crate::operations::begin(&operation_id);
    send_progress(
        progress,
        &operation_id,
        "resolve",
        5.0,
        "Finding a compatible mod release",
    );
    match provider.as_str() {
        "modrinth" => {
            install_modrinth(
                state,
                server,
                project_id,
                version_id,
                progress,
                &operation_id,
            )
            .await
        }
        "curseforge" => {
            install_curseforge(
                state,
                server,
                project_id,
                version_id,
                progress,
                &operation_id,
            )
            .await
        }
        _ => Err(Error::Validation("Choose Modrinth or CurseForge.".into())),
    }
}

async fn install_modrinth(
    state: Arc<AppState>,
    server: Server,
    project_id: String,
    version_id: String,
    progress: &Channel<OperationEvent>,
    operation_id: &str,
) -> Result<ModInstallResult> {
    validate_project_id(&project_id)?;
    validate_project_id(&version_id)?;
    let loader = loader_name(&server)?;
    let project: ModrinthProjectDetail = http_client()?
        .get(format!("{MODRINTH_API}/project/{project_id}"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let versions = compatible_modrinth_versions(&server, &project_id).await?;
    let version = versions
        .into_iter()
        .find(|version| version.id == version_id)
        .ok_or_else(|| {
            Error::NotFound(format!(
                "That {} release is not compatible with {} {}.",
                project.title, loader, server.version
            ))
        })?;
    let file = preferred_modrinth_file(&version)
        .ok_or_else(|| Error::NotFound("That release contains no downloadable JAR.".into()))?;
    validate_download_url("modrinth", &file.url)?;
    let author = modrinth_team_author(&project.team).await;
    let metadata = ModMetadata {
        server_id: server.id.clone(),
        file_name: file.filename.clone(),
        provider: "modrinth".into(),
        project_id: project.id,
        slug: project.slug.clone(),
        name: project.title,
        description: project.description,
        author,
        version: version.version_number.clone(),
        icon_url: project.icon_url,
        website_url: format!("https://modrinth.com/mod/{}", project.slug),
    };
    install_downloaded_mod(
        &state,
        &server,
        metadata,
        &file.url,
        file.hashes.get("sha1").map(String::as_str),
        progress,
        operation_id,
    )
    .await
}

async fn modrinth_team_author(team_id: &str) -> String {
    let response = match http_client() {
        Ok(client) => {
            client
                .get(format!("{MODRINTH_API}/team/{team_id}/members"))
                .send()
                .await
        }
        Err(_) => return String::new(),
    };
    let Ok(response) = response.and_then(reqwest::Response::error_for_status) else {
        return String::new();
    };
    let Ok(members) = response.json::<Vec<ModrinthTeamMember>>().await else {
        return String::new();
    };
    members
        .iter()
        .find(|member| member.is_owner)
        .or_else(|| members.first())
        .map_or_else(String::new, |member| member.user.username.clone())
}

async fn install_curseforge(
    state: Arc<AppState>,
    server: Server,
    project_id: String,
    version_id: String,
    progress: &Channel<OperationEvent>,
    operation_id: &str,
) -> Result<ModInstallResult> {
    let project_number = project_id
        .parse::<u64>()
        .map_err(|_| Error::Validation("Invalid CurseForge project identifier.".into()))?;
    let requested_file = version_id
        .parse::<u64>()
        .map_err(|_| Error::Validation("Invalid CurseForge version identifier.".into()))?;
    let loader = loader_name(&server)?;
    let project: CurseResponse<CurseProject> =
        curse_request(format!("{CURSEFORGE_API}/mods/{project_number}"))?
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
    let files = compatible_curse_files(&server, &project_id).await?;
    let file = files
        .iter()
        .find(|file| file.id == requested_file)
        .ok_or_else(|| {
            Error::NotFound(format!(
                "That {} release is not compatible with {} {}.",
                project.data.name, loader, server.version
            ))
        })?;
    let sha1 = file
        .hashes
        .iter()
        .find(|hash| hash.algo == 1)
        .map(|hash| hash.value.clone());
    let metadata = ModMetadata {
        server_id: server.id.clone(),
        file_name: file.file_name.clone(),
        provider: "curseforge".into(),
        project_id: project.data.id.to_string(),
        slug: project.data.slug.clone(),
        name: project.data.name.clone(),
        description: project.data.summary.clone(),
        author: project
            .data
            .authors
            .first()
            .map_or_else(String::new, |author| author.name.clone()),
        version: file.display_name.clone(),
        icon_url: project.data.logo.map(|logo| logo.thumbnail_url),
        website_url: format!(
            "https://www.curseforge.com/minecraft/mc-mods/{}",
            project.data.slug
        ),
    };
    if let Some(url) = &file.download_url {
        validate_download_url("curseforge", url)?;
        return install_downloaded_mod(
            &state,
            &server,
            metadata,
            url,
            sha1.as_deref(),
            progress,
            operation_id,
        )
        .await;
    }

    let downloads = dirs::download_dir().ok_or_else(|| {
        Error::NotFound(
            "Windows did not provide a Downloads folder for manual mod installation.".into(),
        )
    })?;
    let token = uuid::Uuid::new_v4().to_string();
    let manual = ManualModDownload {
        token: token.clone(),
        project_name: project.data.name,
        file_name: file.file_name.clone(),
        download_url: format!(
            "https://www.curseforge.com/minecraft/mc-mods/{}/download/{}",
            project.data.slug, file.id
        ),
        downloads_folder: downloads.to_string_lossy().into_owned(),
    };
    manual_downloads().write().await.insert(
        token,
        PendingManualDownload {
            manual: manual.clone(),
            server_id: server.id.clone(),
            expected_sha1: sha1,
            created_at: now_ms(),
            metadata,
        },
    );
    send_progress(
        progress,
        operation_id,
        "manual",
        100.0,
        "Waiting for a manual CurseForge download",
    );
    Ok(ModInstallResult {
        mods: list_for_server(&state, &server.id).await?,
        manual_download: Some(manual),
    })
}

async fn install_downloaded_mod(
    state: &Arc<AppState>,
    server: &Server,
    metadata: ModMetadata,
    url: &str,
    expected_sha1: Option<&str>,
    progress: &Channel<OperationEvent>,
    operation_id: &str,
) -> Result<ModInstallResult> {
    let directory = mod_directory(server);
    tokio::fs::create_dir_all(&directory).await?;
    let target = safe_mod_path(&directory, &metadata.file_name)?;
    let disabled = safe_mod_path(&directory, &format!("{}.disabled", metadata.file_name))?;
    if target.exists() || disabled.exists() {
        return Err(Error::Conflict(format!(
            "{} is already installed.",
            metadata.name
        )));
    }
    let temporary = directory.join(format!(".nooki-mod-{}.tmp", uuid::Uuid::new_v4()));
    if let Err(error) = download_mod(url, expected_sha1, &temporary, progress, operation_id).await {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(error);
    }
    if let Err(error) = tokio::fs::rename(&temporary, &target).await {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(error.into());
    }
    finish_install(state, server, metadata, &target).await
}

async fn check_manual_download(state: Arc<AppState>, token: &str) -> Result<ModInstallResult> {
    let pending = manual_downloads()
        .read()
        .await
        .get(token)
        .cloned()
        .ok_or_else(|| Error::NotFound("That manual download request has expired.".into()))?;
    let candidate = tokio::task::spawn_blocking({
        let pending = pending.clone();
        move || find_manual_candidate(&pending)
    })
    .await
    .map_err(|error| Error::Internal(error.to_string()))??;
    let Some(candidate) = candidate else {
        return Ok(ModInstallResult {
            mods: list_for_server(&state, &pending.server_id).await?,
            manual_download: Some(pending.manual),
        });
    };

    let lock = state.operation_lock(&pending.server_id);
    let _guard = lock.lock().await;
    let server = installable_modded_server(&state, &pending.server_id).await?;
    let directory = mod_directory(&server);
    tokio::fs::create_dir_all(&directory).await?;
    let target = safe_mod_path(&directory, &pending.metadata.file_name)?;
    let disabled = safe_mod_path(
        &directory,
        &format!("{}.disabled", pending.metadata.file_name),
    )?;
    if target.exists() || disabled.exists() {
        manual_downloads().write().await.remove(token);
        return Err(Error::Conflict(format!(
            "{} is already installed.",
            pending.metadata.name
        )));
    }
    tokio::fs::copy(&candidate, &target).await?;
    let result = finish_install(&state, &server, pending.metadata, &target).await;
    if result.is_ok() {
        manual_downloads().write().await.remove(token);
    }
    result
}

fn find_manual_candidate(pending: &PendingManualDownload) -> Result<Option<PathBuf>> {
    let downloads = PathBuf::from(&pending.manual.downloads_folder);
    if !downloads.is_dir() {
        return Ok(None);
    }
    for entry in std::fs::read_dir(downloads)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file()
            || !file_name_matches(
                &entry.file_name().to_string_lossy(),
                &pending.manual.file_name,
            )
        {
            continue;
        }
        if let Some(expected) = &pending.expected_sha1 {
            if sha1_file(&path)?.eq_ignore_ascii_case(expected) {
                return Ok(Some(path));
            }
        } else {
            let modified = entry
                .metadata()?
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |duration| duration.as_millis() as i64);
            if modified >= pending.created_at.saturating_sub(5_000) {
                return Ok(Some(path));
            }
        }
    }
    Ok(None)
}

fn file_name_matches(actual: &str, expected: &str) -> bool {
    if actual.eq_ignore_ascii_case(expected) {
        return true;
    }
    let simplify = |value: &str| {
        value
            .to_ascii_lowercase()
            .chars()
            .map(|character| {
                if matches!(character, '-' | '+' | '.' | '_') {
                    ' '
                } else {
                    character
                }
            })
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    simplify(actual) == simplify(expected)
}

fn sha1_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha1::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

async fn finish_install(
    state: &Arc<AppState>,
    server: &Server,
    metadata: ModMetadata,
    installed_path: &Path,
) -> Result<ModInstallResult> {
    if let Err(error) = state.db.save_mod_metadata(&metadata).await {
        let _ = tokio::fs::remove_file(installed_path).await;
        return Err(error);
    }
    if state.processes.is_running(&server.id).await {
        let mut live = state.server(&server.id).await?;
        if !live
            .alerts
            .iter()
            .any(|alert| alert.kind == "restart-required")
        {
            live.alerts.push(ServerAlert {
                id: uuid::Uuid::new_v4().to_string(),
                kind: "restart-required".into(),
                title: "Restart required".into(),
                detail: format!("Restart the server to load {}.", metadata.name),
                severity: "info".into(),
            });
            state.save_server(live).await?;
        }
    }
    state
        .activity(
            "settings",
            Some(server),
            format!(
                "Installed {} {} from {}",
                metadata.name,
                metadata.version,
                provider_label(&metadata.provider)
            ),
        )
        .await?;
    Ok(ModInstallResult {
        mods: list_for_server(state, &server.id).await?,
        manual_download: None,
    })
}

async fn download_mod(
    url: &str,
    expected_sha1: Option<&str>,
    destination: &Path,
    progress: &Channel<OperationEvent>,
    operation_id: &str,
) -> Result<()> {
    let response = http_client()?.get(url).send().await?.error_for_status()?;
    let total = response.content_length().unwrap_or(0);
    if total > MAX_MOD_BYTES {
        return Err(Error::Validation("That mod is larger than 512 MiB.".into()));
    }
    let mut stream = response.bytes_stream();
    let mut file = File::create(destination)?;
    let mut hasher = Sha1::new();
    let mut downloaded = 0u64;
    while let Some(chunk) = stream.next().await {
        if let Err(error) = crate::operations::check(operation_id) {
            drop(file);
            let _ = std::fs::remove_file(destination);
            return Err(error);
        }
        let chunk = chunk?;
        downloaded = downloaded.saturating_add(chunk.len() as u64);
        if downloaded > MAX_MOD_BYTES {
            return Err(Error::Validation("That mod is larger than 512 MiB.".into()));
        }
        hasher.update(&chunk);
        file.write_all(&chunk)?;
        let percent = if total > 0 {
            10.0 + downloaded as f32 / total as f32 * 84.0
        } else {
            45.0
        };
        send_progress(
            progress,
            operation_id,
            "download",
            percent,
            "Downloading mod",
        );
    }
    file.sync_all()?;
    crate::operations::check(operation_id)?;
    if let Some(expected) = expected_sha1 {
        let actual = hex::encode(hasher.finalize());
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(Error::Validation(
                "The mod checksum did not match its release data.".into(),
            ));
        }
    }
    send_progress(progress, operation_id, "install", 100.0, "Mod installed");
    Ok(())
}

async fn list_for_server(state: &Arc<AppState>, id: &str) -> Result<Vec<ModFile>> {
    let server = modded_server(state, id).await?;
    let directory = mod_directory(&server);
    tokio::fs::create_dir_all(&directory).await?;
    let mut files = tokio::task::spawn_blocking({
        let directory = directory.clone();
        move || read_mod_directory(&directory)
    })
    .await
    .map_err(|error| Error::Internal(error.to_string()))??;
    let mut metadata = state
        .db
        .load_mod_metadata(id)
        .await?
        .into_iter()
        .map(|item| (item.file_name.clone(), item))
        .collect::<HashMap<_, _>>();
    for file in &mut files {
        file.metadata = metadata.remove(&file.file_name);
    }
    for stale in metadata.into_values() {
        state
            .db
            .delete_mod_metadata_file(id, &stale.file_name)
            .await?;
    }
    Ok(files)
}

fn read_mod_directory(directory: &Path) -> Result<Vec<ModFile>> {
    let mut mods = Vec::new();
    if !directory.exists() {
        return Ok(mods);
    }
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !is_mod_file(file_name) || !path.is_file() {
            continue;
        }
        let file_metadata = entry.metadata()?;
        mods.push(ModFile {
            file_name: file_name.into(),
            name: display_name_from_file(file_name),
            version: None,
            description: None,
            authors: Vec::new(),
            enabled: file_name.to_ascii_lowercase().ends_with(".jar"),
            size: file_metadata.len(),
            modified_at: file_metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |duration| duration.as_millis() as i64),
            metadata: None,
        });
    }
    mods.sort_by_key(|file| file.name.to_lowercase());
    Ok(mods)
}

fn copy_mod_files(sources: Vec<PathBuf>, directory: &Path) -> Result<usize> {
    if sources.len() > 128 {
        return Err(Error::Validation(
            "Choose no more than 128 mod files at once.".into(),
        ));
    }
    let mut names = HashSet::new();
    let mut copies = Vec::with_capacity(sources.len());
    for source in sources {
        if !source.is_file() {
            return Err(Error::NotFound(format!(
                "Mod file not found: {}",
                source.display()
            )));
        }
        let name = source
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| Error::Validation("A selected mod has an invalid file name.".into()))?;
        if !name.to_ascii_lowercase().ends_with(".jar") {
            return Err(Error::Validation(format!("{name} is not a mod JAR.")));
        }
        if !names.insert(name.to_ascii_lowercase()) {
            return Err(Error::Conflict(format!(
                "The selection contains more than one file named {name}."
            )));
        }
        let size = std::fs::metadata(&source)?.len();
        if size > MAX_MOD_BYTES {
            return Err(Error::Validation(format!("{name} is larger than 512 MiB.")));
        }
        zip::ZipArchive::new(File::open(&source)?)
            .map_err(|_| Error::Validation(format!("{name} is not a valid JAR archive.")))?;
        let target = safe_mod_path(directory, name)?;
        let disabled = safe_mod_path(directory, &format!("{name}.disabled"))?;
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

async fn editable_modded_server(state: &Arc<AppState>, id: &str) -> Result<Server> {
    let server = modded_server(state, id).await?;
    if state.processes.is_running(id).await
        || !matches!(server.status, ServerStatus::Stopped | ServerStatus::Crashed)
    {
        return Err(Error::Conflict(
            "Stop the server before enabling, disabling, or deleting mods.".into(),
        ));
    }
    Ok(server)
}

async fn installable_modded_server(state: &Arc<AppState>, id: &str) -> Result<Server> {
    let server = modded_server(state, id).await?;
    if !matches!(
        server.status,
        ServerStatus::Running | ServerStatus::Stopped | ServerStatus::Crashed
    ) {
        return Err(Error::Conflict(
            "Wait for the current server operation to finish before installing a mod.".into(),
        ));
    }
    Ok(server)
}

async fn modded_server(state: &Arc<AppState>, id: &str) -> Result<Server> {
    let server = state.server(id).await?;
    if !matches!(
        server.server_type,
        ServerType::Forge | ServerType::NeoForge | ServerType::Fabric
    ) {
        return Err(Error::Unsupported(
            "Mods are only available for Fabric, Forge, and NeoForge servers.".into(),
        ));
    }
    Ok(server)
}

fn mod_directory(server: &Server) -> PathBuf {
    Path::new(&server.folder).join("mods")
}
fn loader_name(server: &Server) -> Result<&'static str> {
    match server.server_type {
        ServerType::Fabric => Ok("fabric"),
        ServerType::Forge => Ok("forge"),
        ServerType::NeoForge => Ok("neoforge"),
        _ => Err(Error::Unsupported(
            "This server does not use a supported mod loader.".into(),
        )),
    }
}
fn validate_loader(loader: &str) -> Result<()> {
    if matches!(loader, "fabric" | "forge" | "neoforge") {
        Ok(())
    } else {
        Err(Error::Validation(
            "Choose Fabric, Forge, or NeoForge.".into(),
        ))
    }
}
fn curse_loader(loader: &str) -> &'static str {
    match loader {
        "fabric" => "4",
        "neoforge" => "6",
        _ => "1",
    }
}

fn is_mod_file(file_name: &str) -> bool {
    let lower = file_name.to_ascii_lowercase();
    lower.ends_with(".jar") || lower.ends_with(".jar.disabled")
}
fn toggle_file_name(file_name: &str, enabled: bool) -> Result<String> {
    if !is_mod_file(file_name) {
        return Err(Error::Validation("Invalid mod file name.".into()));
    }
    if enabled {
        Ok(file_name
            .strip_suffix(".disabled")
            .unwrap_or(file_name)
            .into())
    } else if file_name.to_ascii_lowercase().ends_with(".jar.disabled") {
        Ok(file_name.into())
    } else {
        Ok(format!("{file_name}.disabled"))
    }
}
fn display_name_from_file(file_name: &str) -> String {
    file_name
        .strip_suffix(".disabled")
        .unwrap_or(file_name)
        .strip_suffix(".jar")
        .unwrap_or(file_name)
        .into()
}
fn safe_mod_path(directory: &Path, file_name: &str) -> Result<PathBuf> {
    let path = Path::new(file_name);
    if !is_mod_file(file_name)
        || path.components().count() != 1
        || !matches!(path.components().next(), Some(Component::Normal(_)))
        || file_name
            .chars()
            .any(|character| character.is_control() || r#"<>:"/\|?*"#.contains(character))
    {
        return Err(Error::Validation("Invalid mod file name.".into()));
    }
    if std::fs::symlink_metadata(directory).is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        return Err(Error::Validation(
            "The mods directory cannot be a symbolic link.".into(),
        ));
    }
    let joined = directory.join(path);
    if std::fs::symlink_metadata(&joined).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(Error::Validation(
            "Nooki will not modify a mod through a symbolic link.".into(),
        ));
    }
    Ok(joined)
}

pub(crate) async fn load_catalog_icon(provider: &str, icon_url: &str) -> Result<Option<String>> {
    load_icon(provider, icon_url).await
}

async fn load_icon(provider: &str, icon_url: &str) -> Result<Option<String>> {
    validate_icon_url(provider, icon_url)?;
    let cache = ICON_CACHE.get_or_init(|| tokio::sync::RwLock::new(HashMap::new()));
    if let Some(cached) = cache.read().await.get(icon_url).cloned() {
        return Ok(cached);
    }
    let response = http_client()?.get(icon_url).send().await?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let response = response.error_for_status()?;
    if response
        .content_length()
        .is_some_and(|size| size > MAX_ICON_BYTES)
    {
        return Err(Error::Validation(
            "That mod logo is unexpectedly large.".into(),
        ));
    }
    let bytes = response.bytes().await?;
    if bytes.len() as u64 > MAX_ICON_BYTES {
        return Err(Error::Validation(
            "That mod logo is unexpectedly large.".into(),
        ));
    }
    // Some catalog CDN responses omit their content type or label WebP bytes as PNG. The
    // server icon validator checks magic bytes, so derive the data URL from those same bytes.
    // Unsupported images simply fall back to the server software icon.
    let data = catalog_icon_mime(&bytes)
        .map(|mime| format!("data:{mime};base64,{}", BASE64.encode(bytes)));
    cache.write().await.insert(icon_url.into(), data.clone());
    Ok(data)
}

fn catalog_icon_mime(bytes: &[u8]) -> Option<&'static str> {
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

fn validate_icon_url(provider: &str, value: &str) -> Result<()> {
    let url = reqwest::Url::parse(value)
        .map_err(|_| Error::Validation("Invalid mod logo URL.".into()))?;
    let host = url.host_str().unwrap_or_default();
    let trusted = match provider {
        "modrinth" => host == "cdn.modrinth.com",
        "curseforge" => host.ends_with(".forgecdn.net") || host == "forgecdn.net",
        _ => false,
    };
    if url.scheme() != "https" || !trusted {
        return Err(Error::Validation("Untrusted mod logo URL.".into()));
    }
    Ok(())
}
fn validate_download_url(provider: &str, value: &str) -> Result<()> {
    let url = reqwest::Url::parse(value)
        .map_err(|_| Error::Validation("Invalid mod download URL.".into()))?;
    let host = url.host_str().unwrap_or_default();
    let trusted = match provider {
        "modrinth" => host == "cdn.modrinth.com",
        "curseforge" => host.ends_with(".forgecdn.net") || host == "forgecdn.net",
        _ => false,
    };
    if url.scheme() != "https" || !trusted {
        return Err(Error::Validation("Untrusted mod download URL.".into()));
    }
    Ok(())
}

fn curse_request(url: String) -> Result<reqwest::RequestBuilder> {
    let key = option_env!("NOOKI_CURSEFORGE_API_KEY")
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| Error::Unsupported(
            "CurseForge is not configured in this build. Set NOOKI_CURSEFORGE_API_KEY when building Nooki.".into()
        ))?;
    Ok(http_client()?.get(url).header("x-api-key", key))
}
fn http_client() -> Result<reqwest::Client> {
    let contact = option_env!("NOOKI_CONTACT_URL").unwrap_or("nooki@mints.wtf");
    reqwest::Client::builder()
        .user_agent(format!("Nooki/{} ({contact})", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(Error::from)
}
fn validate_project_id(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
    {
        return Err(Error::Validation(
            "Invalid Modrinth project identifier.".into(),
        ));
    }
    Ok(())
}
fn parse_date(value: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(value).map_or(0, |date| date.timestamp_millis())
}
fn provider_label(provider: &str) -> &str {
    if provider == "curseforge" {
        "CurseForge"
    } else {
        "Modrinth"
    }
}
fn manual_downloads() -> &'static tokio::sync::RwLock<HashMap<String, PendingManualDownload>> {
    MANUAL_DOWNLOADS.get_or_init(|| tokio::sync::RwLock::new(HashMap::new()))
}
fn send_progress(
    channel: &Channel<OperationEvent>,
    operation_id: &str,
    phase: &str,
    progress: f32,
    message: &str,
) {
    let _ = channel.send(OperationEvent::Progress {
        operation_id: operation_id.into(),
        phase: phase.into(),
        progress,
        message: message.into(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_icon_type_comes_from_image_bytes() {
        assert_eq!(
            catalog_icon_mime(b"\x89PNG\r\n\x1a\nrest"),
            Some("image/png")
        );
        assert_eq!(
            catalog_icon_mime(&[0xff, 0xd8, 0xff, 0xe0]),
            Some("image/jpeg")
        );
        assert_eq!(
            catalog_icon_mime(b"RIFF\x04\x00\x00\x00WEBP"),
            Some("image/webp")
        );
        assert_eq!(catalog_icon_mime(b"<html>not an image</html>"), None);
    }

    #[test]
    fn recognizes_enabled_and_disabled_mods() {
        assert!(is_mod_file("fabric-api.jar"));
        assert!(is_mod_file("fabric-api.jar.disabled"));
        assert!(!is_mod_file("config.toml"));
    }

    #[test]
    fn manual_download_matching_is_separator_and_case_insensitive() {
        assert!(file_name_matches(
            "Sodium-Fabric-0.6.0.jar",
            "sodium_fabric_0.6.0.jar"
        ));
        assert!(!file_name_matches("different.jar", "sodium.jar"));
    }

    #[test]
    fn rejects_unsafe_mod_paths() {
        let root = Path::new("mods");
        assert!(safe_mod_path(root, "../server.jar").is_err());
        assert!(safe_mod_path(root, "nested/mod.jar").is_err());
    }

    #[test]
    fn decodes_modrinth_project_detail_shape() {
        let project: ModrinthProjectDetail = serde_json::from_str(
            r#"{
                "id":"1bokaNcj",
                "slug":"xaeros-minimap",
                "title":"Xaero's Minimap",
                "description":"Displays a map",
                "team":"9lteWJca",
                "icon_url":"https://cdn.modrinth.com/data/1bokaNcj/icon.png"
            }"#,
        )
        .unwrap();
        assert_eq!(project.id, "1bokaNcj");
        assert_eq!(project.team, "9lteWJca");
    }

    #[test]
    fn manual_mod_import_copies_every_file_and_keeps_sources() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let mods = root.path().join("mods");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(&mods).unwrap();
        let first = source.join("First.jar");
        let second = source.join("Second.jar");
        for path in [&first, &second] {
            let file = File::create(path).unwrap();
            let mut jar = zip::ZipWriter::new(file);
            jar.start_file("fabric.mod.json", zip::write::SimpleFileOptions::default())
                .unwrap();
            jar.write_all(b"{}").unwrap();
            jar.finish().unwrap();
        }
        assert_eq!(
            copy_mod_files(vec![first.clone(), second.clone()], &mods).unwrap(),
            2
        );
        assert!(first.exists());
        assert!(second.exists());
        assert!(mods.join("First.jar").is_file());
        assert!(mods.join("Second.jar").is_file());
    }
}
