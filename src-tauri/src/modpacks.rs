use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Write},
    net::IpAddr,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use sha2::{Digest, Sha512};
use tauri::{ipc::Channel, State};

use crate::{
    commands,
    error::{AppError, CommandResult, Error, Result},
    models::{CreateServerInput, OperationEvent, Server, ServerType},
    state::AppState,
};

const MODRINTH_API: &str = "https://api.modrinth.com/v2";
const CURSEFORGE_API: &str = "https://api.curseforge.com/v1";
const PAGE_SIZE: u32 = 20;
const MAX_ARCHIVE_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const MAX_EXTRACTED_BYTES: u64 = 12 * 1024 * 1024 * 1024;
const MAX_PACK_FILES: usize = 75_000;

type SharedState<'a> = State<'a, Arc<AppState>>;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModpackProject {
    provider: String,
    project_id: String,
    slug: String,
    name: String,
    description: String,
    author: String,
    downloads: u64,
    icon_url: Option<String>,
    website_url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModpackCatalog {
    projects: Vec<ModpackProject>,
    total: u64,
    offset: u32,
    has_more: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModpackVersionOption {
    id: String,
    name: String,
    version_number: String,
    minecraft_version: String,
    loader: String,
    release_type: String,
    published_at: i64,
    size: u64,
    automatic: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateModpackServerInput {
    provider: String,
    project_id: String,
    version_id: String,
    name: String,
    min_memory: u32,
    max_memory: u32,
    port: u16,
    parent_folder: String,
    eula: bool,
    java_runtime_id: Option<String>,
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
    icon_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ModrinthVersion {
    id: String,
    name: String,
    version_number: String,
    version_type: String,
    date_published: String,
    game_versions: Vec<String>,
    loaders: Vec<String>,
    files: Vec<ModrinthVersionFile>,
}

#[derive(Debug, Clone, Deserialize)]
struct ModrinthVersionFile {
    hashes: HashMap<String, String>,
    url: String,
    filename: String,
    primary: bool,
    size: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
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

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurseFile {
    id: u64,
    display_name: String,
    file_name: String,
    release_type: u8,
    file_date: String,
    file_length: u64,
    download_url: Option<String>,
    game_versions: Vec<String>,
    server_pack_file_id: Option<u64>,
    hashes: Vec<CurseHash>,
}

#[derive(Debug, Clone, Deserialize)]
struct CurseHash {
    value: String,
    algo: u8,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MrpackIndex {
    format_version: u32,
    game: String,
    dependencies: HashMap<String, String>,
    files: Vec<MrpackFile>,
}

#[derive(Debug, Deserialize)]
struct MrpackFile {
    path: String,
    hashes: HashMap<String, String>,
    downloads: Vec<String>,
    env: Option<MrpackEnvironment>,
    #[serde(rename = "fileSize")]
    file_size: u64,
}

#[derive(Debug, Deserialize)]
struct MrpackEnvironment {
    server: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CurseManifest {
    minecraft: CurseManifestMinecraft,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurseManifestMinecraft {
    version: String,
    mod_loaders: Vec<CurseManifestLoader>,
}

#[derive(Debug, Deserialize)]
struct CurseManifestLoader {
    id: String,
    primary: bool,
}

struct PreparedPack {
    overlay: PathBuf,
    server_type: ServerType,
    minecraft_version: String,
    loader_version: Option<String>,
}

#[tauri::command]
pub async fn search_modpacks(
    provider: String,
    query: String,
    offset: u32,
) -> CommandResult<ModpackCatalog> {
    match provider.as_str() {
        "modrinth" => search_modrinth(&query, offset).await,
        "curseforge" => search_curseforge(&query, offset).await,
        _ => Err(Error::Validation("Choose Modrinth or CurseForge.".into())),
    }
    .map_err(AppError::from)
}

#[tauri::command]
pub async fn list_modpack_versions(
    provider: String,
    project_id: String,
) -> CommandResult<Vec<ModpackVersionOption>> {
    match provider.as_str() {
        "modrinth" => list_modrinth_versions(&project_id).await,
        "curseforge" => list_curse_versions(&project_id).await,
        _ => Err(Error::Validation("Choose Modrinth or CurseForge.".into())),
    }
    .map_err(AppError::from)
}

#[tauri::command]
pub async fn create_modpack_server(
    state: SharedState<'_>,
    input: CreateModpackServerInput,
    on_progress: Channel<OperationEvent>,
) -> CommandResult<Server> {
    install_pack(state.inner().clone(), input, &on_progress)
        .await
        .map_err(AppError::from)
}

async fn search_modrinth(query: &str, offset: u32) -> Result<ModpackCatalog> {
    let facets = serde_json::to_string(&vec![vec!["project_type:modpack"]])?;
    let response: ModrinthSearch = http_client()?
        .get(format!("{MODRINTH_API}/search"))
        .query(&[
            ("query", query.trim().to_owned()),
            ("facets", facets),
            ("index", "downloads".to_owned()),
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
        .map(|project| ModpackProject {
            provider: "modrinth".into(),
            website_url: format!("https://modrinth.com/modpack/{}", project.slug),
            project_id: project.project_id,
            slug: project.slug,
            name: project.title,
            description: project.description,
            author: project.author,
            downloads: project.downloads,
            icon_url: project.icon_url,
        })
        .collect();
    Ok(ModpackCatalog {
        projects,
        total: response.total_hits,
        offset: response.offset,
        has_more: u64::from(response.offset + response.limit) < response.total_hits,
    })
}

async fn search_curseforge(query: &str, offset: u32) -> Result<ModpackCatalog> {
    let response: CurseResponse<Vec<CurseProject>> =
        curse_request(format!("{CURSEFORGE_API}/mods/search"))?
            .query(&[
                ("gameId", "432".to_owned()),
                ("classId", "4471".to_owned()),
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
        .map(|project| ModpackProject {
            provider: "curseforge".into(),
            project_id: project.id.to_string(),
            website_url: format!(
                "https://www.curseforge.com/minecraft/modpacks/{}",
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
            icon_url: project.logo.map(|logo| logo.thumbnail_url),
        })
        .collect();
    Ok(ModpackCatalog {
        projects,
        total: pagination.total_count,
        offset: pagination.index,
        has_more: u64::from(pagination.index + pagination.page_size) < pagination.total_count,
    })
}

async fn list_modrinth_versions(project_id: &str) -> Result<Vec<ModpackVersionOption>> {
    validate_identifier(project_id)?;
    let versions: Vec<ModrinthVersion> = http_client()?
        .get(format!("{MODRINTH_API}/project/{project_id}/version"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(versions
        .into_iter()
        .filter_map(|version| {
            let loader = supported_loader(&version.loaders)?;
            let file = select_mrpack_file(&version.files)?;
            Some(ModpackVersionOption {
                id: version.id,
                name: version.name,
                version_number: version.version_number,
                minecraft_version: version.game_versions.first()?.clone(),
                loader: loader.into(),
                release_type: version.version_type,
                published_at: parse_date(&version.date_published),
                size: file.size,
                automatic: true,
            })
        })
        .collect())
}

async fn list_curse_versions(project_id: &str) -> Result<Vec<ModpackVersionOption>> {
    let project_id = parse_curse_id(project_id)?;
    let files: CurseResponse<Vec<CurseFile>> =
        curse_request(format!("{CURSEFORGE_API}/mods/{project_id}/files"))?
            .query(&[("pageSize", "50")])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
    Ok(files
        .data
        .into_iter()
        .filter_map(|file| {
            let loader = supported_loader(&file.game_versions)?;
            let minecraft_version = minecraft_version(&file.game_versions)?;
            Some(ModpackVersionOption {
                id: file.id.to_string(),
                name: file.display_name,
                version_number: file.file_name,
                minecraft_version,
                loader: loader.into(),
                release_type: curse_release(file.release_type).into(),
                published_at: parse_date(&file.file_date),
                size: file.file_length,
                automatic: file.server_pack_file_id.is_some(),
            })
        })
        .collect())
}

async fn install_pack(
    state: Arc<AppState>,
    input: CreateModpackServerInput,
    channel: &Channel<OperationEvent>,
) -> Result<Server> {
    let operation_id = uuid::Uuid::new_v4().to_string();
    let _operation = crate::operations::begin(&operation_id);
    send_progress(
        channel,
        &operation_id,
        "resolve",
        2.0,
        "Resolving modpack release",
    );
    let workspace = state
        .app_data_dir
        .join("modpack-staging")
        .join(&operation_id);
    tokio::fs::create_dir_all(&workspace).await?;
    let result = async {
        let prepared = match input.provider.as_str() {
            "modrinth" => {
                prepare_modrinth_pack(
                    &input.project_id,
                    &input.version_id,
                    &workspace,
                    channel,
                    &operation_id,
                )
                .await?
            }
            "curseforge" => {
                prepare_curse_pack(
                    &input.project_id,
                    &input.version_id,
                    &workspace,
                    channel,
                    &operation_id,
                )
                .await?
            }
            _ => return Err(Error::Validation("Choose Modrinth or CurseForge.".into())),
        };
        crate::operations::check(&operation_id)?;
        send_progress(
            channel,
            &operation_id,
            "server",
            62.0,
            "Preparing the Minecraft installation",
        );
        let create_input = CreateServerInput {
            name: input.name,
            server_type: prepared.server_type,
            version: prepared.minecraft_version,
            build: prepared.loader_version,
            min_memory: input.min_memory,
            max_memory: input.max_memory,
            port: input.port,
            parent_folder: input.parent_folder,
            eula: input.eula,
            java_runtime_id: input.java_runtime_id,
            experimental: true,
        };
        let server = commands::create_with_overlay(
            state.clone(),
            create_input,
            Some(channel),
            Some(prepared.overlay),
            Some(commands::CreationProgress {
                operation_id: operation_id.clone(),
                start: 62.0,
                end: 99.0,
            }),
        )
        .await?;
        send_progress(
            channel,
            &operation_id,
            "done",
            100.0,
            "Modpack server is ready",
        );
        Ok(server)
    }
    .await;
    let _ = tokio::fs::remove_dir_all(&workspace).await;
    result
}

async fn prepare_modrinth_pack(
    project_id: &str,
    version_id: &str,
    workspace: &Path,
    channel: &Channel<OperationEvent>,
    operation_id: &str,
) -> Result<PreparedPack> {
    validate_identifier(project_id)?;
    validate_identifier(version_id)?;
    let version: ModrinthVersion = http_client()?
        .get(format!("{MODRINTH_API}/version/{version_id}"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let file = select_mrpack_file(&version.files)
        .ok_or_else(|| Error::NotFound("That release does not contain a .mrpack file.".into()))?;
    let archive = workspace.join("pack.mrpack");
    send_progress(
        channel,
        operation_id,
        "manifest",
        5.0,
        "Downloading the Modrinth pack manifest",
    );
    download_file(
        &file.url,
        &archive,
        operation_id,
        MAX_ARCHIVE_BYTES,
        Some(file.size),
        |received, total| {
            send_transfer_progress(
                channel,
                operation_id,
                "manifest",
                5.0,
                12.0,
                "Downloading Modrinth pack manifest",
                received,
                total,
            );
        },
    )
    .await?;
    send_progress(
        channel,
        operation_id,
        "verifyManifest",
        13.0,
        "Verifying the Modrinth pack manifest",
    );
    let verification_channel = channel.clone();
    let verification_id = operation_id.to_owned();
    verify_hash(
        archive.clone(),
        file.hashes.get("sha1").cloned(),
        file.hashes.get("sha512").cloned(),
        operation_id.to_owned(),
        move |checked, total| {
            send_transfer_progress(
                &verification_channel,
                &verification_id,
                "verifyManifest",
                12.0,
                14.0,
                "Checking Modrinth manifest integrity",
                checked,
                total,
            );
        },
    )
    .await?;
    send_progress(
        channel,
        operation_id,
        "manifest",
        15.0,
        "Reading the modpack manifest",
    );
    let archive_for_read = archive.clone();
    let index = tokio::task::spawn_blocking(move || read_mrpack_index(&archive_for_read))
        .await
        .map_err(|error| Error::Internal(error.to_string()))??;
    if index.format_version != 1 || index.game != "minecraft" {
        return Err(Error::Unsupported(
            "This is not a supported Minecraft .mrpack archive.".into(),
        ));
    }
    let (server_type, loader_version) = loader_from_dependencies(&index.dependencies)?;
    let minecraft_version = index
        .dependencies
        .get("minecraft")
        .cloned()
        .ok_or_else(|| {
            Error::Validation("The modpack does not declare a Minecraft version.".into())
        })?;
    let overlay = workspace.join("overlay");
    tokio::fs::create_dir_all(&overlay).await?;
    let installable = index
        .files
        .iter()
        .filter(|file| {
            file.env.as_ref().and_then(|env| env.server.as_deref()) != Some("unsupported")
        })
        .collect::<Vec<_>>();
    if installable.len() > MAX_PACK_FILES {
        return Err(Error::Validation(
            "This modpack contains too many files.".into(),
        ));
    }
    let declared_size = installable.iter().map(|file| file.file_size).sum::<u64>();
    if declared_size > MAX_EXTRACTED_BYTES {
        return Err(Error::Validation(
            "This modpack is too large to install safely.".into(),
        ));
    }
    let mut completed_bytes = 0_u64;
    for (index_number, pack_file) in installable.iter().enumerate() {
        let relative = safe_relative_path(&pack_file.path)?;
        let url = pack_file
            .downloads
            .first()
            .ok_or_else(|| Error::NotFound(format!("{} has no download URL.", pack_file.path)))?;
        validate_pack_download_url(url)?;
        let target = overlay.join(relative);
        if let Some(parent) = target.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        download_file(
            url,
            &target,
            operation_id,
            pack_file.file_size.max(1),
            Some(pack_file.file_size),
            |received, _| {
                let received_total = completed_bytes.saturating_add(received);
                let transfer_progress = if declared_size == 0 {
                    (index_number + 1) as f32 / installable.len().max(1) as f32 * 100.0
                } else {
                    received_total.min(declared_size) as f32 / declared_size as f32 * 100.0
                };
                send_progress(
                    channel,
                    operation_id,
                    "files",
                    16.0 + transfer_progress * 0.36,
                    &format!(
                        "Downloading pack files · {transfer_progress:.0}% · file {} of {}",
                        index_number + 1,
                        installable.len()
                    ),
                );
            },
        )
        .await?;
        let file_progress = if declared_size == 0 {
            16.0 + (index_number + 1) as f32 / installable.len().max(1) as f32 * 36.0
        } else {
            16.0 + completed_bytes
                .saturating_add(pack_file.file_size)
                .min(declared_size) as f32
                / declared_size as f32
                * 36.0
        };
        let verification_channel = channel.clone();
        let verification_id = operation_id.to_owned();
        let file_number = index_number + 1;
        let file_count = installable.len();
        verify_hash(
            target.clone(),
            pack_file.hashes.get("sha1").cloned(),
            pack_file.hashes.get("sha512").cloned(),
            operation_id.to_owned(),
            move |checked, total| {
                send_transfer_progress(
                    &verification_channel,
                    &verification_id,
                    "verifyPack",
                    file_progress,
                    file_progress,
                    &format!(
                        "Checking pack file integrity ({}/{})",
                        file_number, file_count
                    ),
                    checked,
                    total,
                );
            },
        )
        .await?;
        completed_bytes = completed_bytes.saturating_add(pack_file.file_size);
    }
    send_progress(
        channel,
        operation_id,
        "overrides",
        53.0,
        "Applying server overrides",
    );
    let archive_for_extract = archive.clone();
    let overlay_for_extract = overlay.clone();
    let extract_operation = operation_id.to_owned();
    tokio::task::spawn_blocking(move || {
        extract_mrpack_overrides(
            &archive_for_extract,
            &overlay_for_extract,
            &extract_operation,
        )
    })
    .await
    .map_err(|error| Error::Internal(error.to_string()))??;
    send_progress(
        channel,
        operation_id,
        "prepared",
        60.0,
        "Pack files are ready",
    );
    Ok(PreparedPack {
        overlay,
        server_type,
        minecraft_version,
        loader_version: Some(loader_version),
    })
}

async fn prepare_curse_pack(
    project_id: &str,
    version_id: &str,
    workspace: &Path,
    channel: &Channel<OperationEvent>,
    operation_id: &str,
) -> Result<PreparedPack> {
    let project_id = parse_curse_id(project_id)?;
    let version_id = parse_curse_id(version_id)?;
    let main: CurseResponse<CurseFile> = curse_request(format!(
        "{CURSEFORGE_API}/mods/{project_id}/files/{version_id}"
    ))?
    .send()
    .await?
    .error_for_status()?
    .json()
    .await?;
    let server_pack_id = main.data.server_pack_file_id.ok_or_else(|| Error::Unsupported(
        "This CurseForge release does not provide an automatic server pack. Choose a release marked Auto setup.".into(),
    ))?;
    let server_pack: CurseResponse<CurseFile> = curse_request(format!(
        "{CURSEFORGE_API}/mods/{project_id}/files/{server_pack_id}"
    ))?
    .send()
    .await?
    .error_for_status()?
    .json()
    .await?;
    let url = server_pack.data.download_url.as_deref().ok_or_else(|| Error::Unsupported(
        "CurseForge requires this server pack to be downloaded manually, so Nooki cannot finish automatic setup for this release.".into(),
    ))?;
    let mut server_type = supported_loader(&main.data.game_versions)
        .map(loader_type)
        .transpose()?
        .ok_or_else(|| {
            Error::Unsupported("This pack does not use Forge, NeoForge, or Fabric.".into())
        })?;
    let mut minecraft_version = minecraft_version(&main.data.game_versions).ok_or_else(|| {
        Error::Validation("CurseForge did not report the Minecraft version for this pack.".into())
    })?;
    let mut loader_version = None;
    if let Some(main_url) = main.data.download_url.as_deref() {
        send_progress(
            channel,
            operation_id,
            "manifest",
            4.0,
            "Reading CurseForge loader metadata",
        );
        let manifest_archive = workspace.join("client-pack.zip");
        if download_file(
            main_url,
            &manifest_archive,
            operation_id,
            MAX_ARCHIVE_BYTES,
            Some(main.data.file_length),
            |received, total| {
                send_transfer_progress(
                    channel,
                    operation_id,
                    "manifest",
                    4.0,
                    10.0,
                    "Reading CurseForge loader metadata",
                    received,
                    total,
                );
            },
        )
        .await
        .is_ok()
        {
            if let Ok(manifest) = read_curse_manifest(&manifest_archive) {
                minecraft_version = manifest.minecraft.version;
                if let Some(loader) = manifest
                    .minecraft
                    .mod_loaders
                    .iter()
                    .find(|loader| loader.primary)
                    .or_else(|| manifest.minecraft.mod_loaders.first())
                {
                    let (kind, version) = parse_curse_loader(&loader.id)?;
                    server_type = kind;
                    loader_version = Some(version);
                }
            }
        }
    }
    send_progress(
        channel,
        operation_id,
        "serverPack",
        12.0,
        "Downloading CurseForge server pack",
    );
    let archive = workspace.join("server-pack.zip");
    download_file(
        url,
        &archive,
        operation_id,
        MAX_ARCHIVE_BYTES,
        Some(server_pack.data.file_length),
        |received, total| {
            send_transfer_progress(
                channel,
                operation_id,
                "serverPack",
                12.0,
                50.0,
                "Downloading CurseForge server pack",
                received,
                total,
            );
        },
    )
    .await?;
    send_progress(
        channel,
        operation_id,
        "verifyPack",
        52.0,
        "Verifying the CurseForge server pack",
    );
    let sha1 = server_pack
        .data
        .hashes
        .iter()
        .find(|hash| hash.algo == 1)
        .map(|hash| hash.value.clone());
    let verification_channel = channel.clone();
    let verification_id = operation_id.to_owned();
    verify_hash(
        archive.clone(),
        sha1,
        None,
        operation_id.to_owned(),
        move |checked, total| {
            send_transfer_progress(
                &verification_channel,
                &verification_id,
                "verifyPack",
                50.0,
                54.0,
                "Checking CurseForge server pack integrity",
                checked,
                total,
            );
        },
    )
    .await?;
    let overlay = workspace.join("overlay");
    let archive_for_extract = archive.clone();
    let overlay_for_extract = overlay.clone();
    let extract_operation = operation_id.to_owned();
    send_progress(
        channel,
        operation_id,
        "extract",
        55.0,
        "Extracting server pack files",
    );
    tokio::task::spawn_blocking(move || {
        extract_curse_server_pack(
            &archive_for_extract,
            &overlay_for_extract,
            &extract_operation,
        )
    })
    .await
    .map_err(|error| Error::Internal(error.to_string()))??;
    send_progress(
        channel,
        operation_id,
        "prepared",
        60.0,
        "Server pack files are ready",
    );
    Ok(PreparedPack {
        overlay,
        server_type,
        minecraft_version,
        loader_version,
    })
}

fn read_mrpack_index(archive_path: &Path) -> Result<MrpackIndex> {
    let mut archive = zip::ZipArchive::new(File::open(archive_path)?)?;
    let mut entry = archive.by_name("modrinth.index.json").map_err(|_| {
        Error::Archive("The .mrpack archive has no modrinth.index.json file.".into())
    })?;
    if entry.size() > 8 * 1024 * 1024 {
        return Err(Error::Archive(
            "The modpack manifest is unexpectedly large.".into(),
        ));
    }
    let mut contents = String::new();
    entry.read_to_string(&mut contents)?;
    Ok(serde_json::from_str(&contents)?)
}

fn extract_mrpack_overrides(
    archive_path: &Path,
    destination: &Path,
    operation_id: &str,
) -> Result<()> {
    extract_archive_prefix(archive_path, destination, "overrides/", operation_id)?;
    extract_archive_prefix(archive_path, destination, "server-overrides/", operation_id)
}

fn extract_archive_prefix(
    archive_path: &Path,
    destination: &Path,
    prefix: &str,
    operation_id: &str,
) -> Result<()> {
    let mut archive = zip::ZipArchive::new(File::open(archive_path)?)?;
    let mut count = 0_usize;
    let mut size = 0_u64;
    for index in 0..archive.len() {
        crate::operations::check(operation_id)?;
        let mut entry = archive.by_index(index)?;
        let enclosed = entry
            .enclosed_name()
            .ok_or_else(|| Error::Archive("The modpack contains an unsafe path.".into()))?;
        let Ok(relative) = enclosed.strip_prefix(prefix.trim_end_matches('/')) else {
            continue;
        };
        if relative.as_os_str().is_empty() {
            continue;
        }
        reject_symlink(&entry)?;
        count += 1;
        size = size.saturating_add(entry.size());
        validate_extract_limits(count, size)?;
        write_zip_entry(&mut entry, &destination.join(relative))?;
    }
    Ok(())
}

fn extract_curse_server_pack(
    archive_path: &Path,
    destination: &Path,
    operation_id: &str,
) -> Result<()> {
    let mut archive = zip::ZipArchive::new(File::open(archive_path)?)?;
    let wrapper = common_archive_wrapper(&mut archive)?;
    std::fs::create_dir_all(destination)?;
    let mut count = 0_usize;
    let mut size = 0_u64;
    for index in 0..archive.len() {
        crate::operations::check(operation_id)?;
        let mut entry = archive.by_index(index)?;
        let enclosed = entry
            .enclosed_name()
            .ok_or_else(|| Error::Archive("The server pack contains an unsafe path.".into()))?;
        let relative = if let Some(wrapper) = wrapper.as_deref() {
            match enclosed.strip_prefix(wrapper) {
                Ok(path) => path.to_path_buf(),
                Err(_) => continue,
            }
        } else {
            enclosed
        };
        if relative.as_os_str().is_empty() {
            continue;
        }
        reject_symlink(&entry)?;
        count += 1;
        size = size.saturating_add(entry.size());
        validate_extract_limits(count, size)?;
        write_zip_entry(&mut entry, &destination.join(relative))?;
    }
    Ok(())
}

fn common_archive_wrapper(archive: &mut zip::ZipArchive<File>) -> Result<Option<PathBuf>> {
    let mut wrapper: Option<PathBuf> = None;
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        let path = entry
            .enclosed_name()
            .ok_or_else(|| Error::Archive("The server pack contains an unsafe path.".into()))?;
        let mut components = path.components();
        let Some(Component::Normal(first)) = components.next() else {
            return Ok(None);
        };
        if components.next().is_none() {
            return Ok(None);
        }
        let first = PathBuf::from(first);
        if wrapper.as_ref().is_some_and(|current| current != &first) {
            return Ok(None);
        }
        wrapper.get_or_insert(first);
    }
    Ok(wrapper)
}

fn write_zip_entry(entry: &mut zip::read::ZipFile<'_>, target: &Path) -> Result<()> {
    if entry.is_dir() {
        std::fs::create_dir_all(target)?;
    } else {
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut output = File::create(target)?;
        std::io::copy(entry, &mut output)?;
        output.flush()?;
    }
    Ok(())
}

fn reject_symlink(entry: &zip::read::ZipFile<'_>) -> Result<()> {
    if entry
        .unix_mode()
        .is_some_and(|mode| mode & 0o170000 == 0o120000)
    {
        return Err(Error::Archive(
            "Modpack archives containing symbolic links are not supported.".into(),
        ));
    }
    Ok(())
}

fn validate_extract_limits(files: usize, bytes: u64) -> Result<()> {
    if files > MAX_PACK_FILES {
        return Err(Error::Validation(
            "This modpack contains too many files.".into(),
        ));
    }
    if bytes > MAX_EXTRACTED_BYTES {
        return Err(Error::Validation(
            "This modpack is too large to install safely.".into(),
        ));
    }
    Ok(())
}

fn read_curse_manifest(path: &Path) -> Result<CurseManifest> {
    let mut archive = zip::ZipArchive::new(File::open(path)?)?;
    let mut entry = archive
        .by_name("manifest.json")
        .map_err(|_| Error::Archive("The CurseForge pack has no manifest.json file.".into()))?;
    let mut text = String::new();
    entry.read_to_string(&mut text)?;
    Ok(serde_json::from_str(&text)?)
}

async fn download_file<F>(
    url: &str,
    destination: &Path,
    operation_id: &str,
    max_bytes: u64,
    expected_bytes: Option<u64>,
    mut on_progress: F,
) -> Result<()>
where
    F: FnMut(u64, u64),
{
    let response = http_client()?.get(url).send().await?.error_for_status()?;
    let total = response.content_length().or(expected_bytes).unwrap_or(0);
    if Some(total).is_some_and(|length| length > max_bytes.min(MAX_ARCHIVE_BYTES)) {
        return Err(Error::Validation(
            "The download is larger than Nooki's safety limit.".into(),
        ));
    }
    let mut output = tokio::fs::File::create(destination).await?;
    let mut stream = response.bytes_stream();
    let mut written = 0_u64;
    let mut last_progress_marker = u64::MAX;
    while let Some(chunk) = stream.next().await {
        if let Err(error) = crate::operations::check(operation_id) {
            drop(output);
            let _ = tokio::fs::remove_file(destination).await;
            return Err(error);
        }
        let chunk = chunk?;
        written = written.saturating_add(chunk.len() as u64);
        if written > max_bytes.min(MAX_ARCHIVE_BYTES) {
            return Err(Error::Validation(
                "The download is larger than Nooki's safety limit.".into(),
            ));
        }
        tokio::io::AsyncWriteExt::write_all(&mut output, &chunk).await?;
        let progress_marker = written
            .min(total)
            .saturating_mul(100)
            .checked_div(total)
            .unwrap_or(written / (256 * 1024));
        if progress_marker != last_progress_marker {
            last_progress_marker = progress_marker;
            on_progress(written, total);
        }
    }
    tokio::io::AsyncWriteExt::flush(&mut output).await?;
    crate::operations::check(operation_id)?;
    on_progress(written, total);
    Ok(())
}

async fn verify_hash<F>(
    path: PathBuf,
    sha1: Option<String>,
    sha512: Option<String>,
    operation_id: String,
    on_progress: F,
) -> Result<()>
where
    F: FnMut(u64, u64) + Send + 'static,
{
    tokio::task::spawn_blocking(move || {
        verify_hash_blocking(
            &path,
            sha1.as_deref(),
            sha512.as_deref(),
            &operation_id,
            on_progress,
        )
    })
    .await
    .map_err(|error| Error::Internal(error.to_string()))?
}

fn verify_hash_blocking<F>(
    path: &Path,
    sha1: Option<&str>,
    sha512: Option<&str>,
    operation_id: &str,
    mut on_progress: F,
) -> Result<()>
where
    F: FnMut(u64, u64),
{
    if sha1.is_none() && sha512.is_none() {
        return Ok(());
    }
    let mut input = File::open(path)?;
    let total = input.metadata()?.len();
    let mut checked = 0_u64;
    let mut last_percentage = u64::MAX;
    let mut digest = if sha512.is_some() {
        PackDigest::Sha512(Sha512::new())
    } else {
        PackDigest::Sha1(Sha1::new())
    };
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        crate::operations::check(operation_id)?;
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
        checked = checked.saturating_add(count as u64);
        let percentage = checked
            .min(total)
            .saturating_mul(100)
            .checked_div(total)
            .unwrap_or(100);
        if percentage != last_percentage {
            last_percentage = percentage;
            on_progress(checked, total);
        }
    }
    on_progress(checked, total);
    let actual = digest.finalize();
    let (expected, algorithm) = if let Some(expected) = sha512 {
        (expected, "SHA-512")
    } else {
        (sha1.expect("a checksum was selected"), "SHA-1")
    };
    if !actual.eq_ignore_ascii_case(expected) {
        return Err(Error::Validation(format!(
            "A modpack file failed its {algorithm} checksum."
        )));
    }
    Ok(())
}

enum PackDigest {
    Sha1(Sha1),
    Sha512(Sha512),
}

impl PackDigest {
    fn update(&mut self, bytes: &[u8]) {
        match self {
            Self::Sha1(digest) => digest.update(bytes),
            Self::Sha512(digest) => digest.update(bytes),
        }
    }

    fn finalize(self) -> String {
        match self {
            Self::Sha1(digest) => hex::encode(digest.finalize()),
            Self::Sha512(digest) => hex::encode(digest.finalize()),
        }
    }
}

fn loader_from_dependencies(
    dependencies: &HashMap<String, String>,
) -> Result<(ServerType, String)> {
    if let Some(version) = dependencies.get("fabric-loader") {
        return Ok((ServerType::Fabric, version.clone()));
    }
    if let Some(version) = dependencies.get("neoforge") {
        return Ok((ServerType::NeoForge, version.clone()));
    }
    if let Some(version) = dependencies.get("forge") {
        return Ok((ServerType::Forge, version.clone()));
    }
    Err(Error::Unsupported(
        "Nooki currently supports Forge, NeoForge, and Fabric modpacks.".into(),
    ))
}

fn parse_curse_loader(value: &str) -> Result<(ServerType, String)> {
    if let Some(version) = value.strip_prefix("fabric-") {
        return Ok((ServerType::Fabric, version.into()));
    }
    if let Some(version) = value.strip_prefix("neoforge-") {
        return Ok((ServerType::NeoForge, version.into()));
    }
    if let Some(version) = value.strip_prefix("forge-") {
        return Ok((ServerType::Forge, version.into()));
    }
    Err(Error::Unsupported(format!(
        "The mod loader {value} is not supported."
    )))
}

fn supported_loader(values: &[String]) -> Option<&'static str> {
    if values
        .iter()
        .any(|value| value.eq_ignore_ascii_case("neoforge"))
    {
        Some("neoforge")
    } else if values
        .iter()
        .any(|value| value.eq_ignore_ascii_case("fabric"))
    {
        Some("fabric")
    } else if values
        .iter()
        .any(|value| value.eq_ignore_ascii_case("forge"))
    {
        Some("forge")
    } else {
        None
    }
}

fn loader_type(value: &str) -> Result<ServerType> {
    match value {
        "fabric" => Ok(ServerType::Fabric),
        "forge" => Ok(ServerType::Forge),
        "neoforge" => Ok(ServerType::NeoForge),
        _ => Err(Error::Unsupported("Unsupported mod loader.".into())),
    }
}

fn minecraft_version(values: &[String]) -> Option<String> {
    values
        .iter()
        .find(|value| {
            value
                .chars()
                .next()
                .is_some_and(|character| character.is_ascii_digit())
                && value.contains('.')
                && value.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_')
                })
        })
        .cloned()
}

fn select_mrpack_file(files: &[ModrinthVersionFile]) -> Option<&ModrinthVersionFile> {
    files
        .iter()
        .find(|file| file.primary && file.filename.to_ascii_lowercase().ends_with(".mrpack"))
        .or_else(|| {
            files
                .iter()
                .find(|file| file.filename.to_ascii_lowercase().ends_with(".mrpack"))
        })
}

fn safe_relative_path(value: &str) -> Result<PathBuf> {
    let path = Path::new(value);
    if path.is_absolute()
        || path.as_os_str().is_empty()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(Error::Archive(format!(
            "The modpack contains an unsafe path: {value}"
        )));
    }
    Ok(path.to_path_buf())
}

fn validate_pack_download_url(value: &str) -> Result<()> {
    let url = reqwest::Url::parse(value)
        .map_err(|_| Error::Validation("The modpack contains an invalid download URL.".into()))?;
    if url.scheme() != "https" {
        return Err(Error::Validation(
            "Modpack files must use HTTPS downloads.".into(),
        ));
    }
    let host = url.host_str().unwrap_or_default();
    if host.eq_ignore_ascii_case("localhost")
        || host.ends_with(".local")
        || host.parse::<IpAddr>().is_ok_and(|ip| !is_public_ip(ip))
    {
        return Err(Error::Validation(
            "The modpack contains a private-network download URL.".into(),
        ));
    }
    Ok(())
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            !(ip.is_private() || ip.is_loopback() || ip.is_link_local() || ip.is_unspecified())
        }
        IpAddr::V6(ip) => !(ip.is_loopback() || ip.is_unspecified() || ip.is_unique_local()),
    }
}

fn validate_identifier(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
    {
        return Err(Error::Validation(
            "The modpack identifier is invalid.".into(),
        ));
    }
    Ok(())
}

fn parse_curse_id(value: &str) -> Result<u64> {
    value
        .parse()
        .map_err(|_| Error::Validation("The CurseForge identifier is invalid.".into()))
}

fn curse_release(value: u8) -> &'static str {
    match value {
        1 => "release",
        2 => "beta",
        _ => "alpha",
    }
}
fn parse_date(value: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(value).map_or(0, |date| date.timestamp_millis())
}

fn curse_request(url: String) -> Result<reqwest::RequestBuilder> {
    let key = option_env!("NOOKI_CURSEFORGE_API_KEY")
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| Error::Unsupported("CurseForge is not configured in this build.".into()))?;
    Ok(http_client()?.get(url).header("x-api-key", key))
}

fn http_client() -> Result<reqwest::Client> {
    let contact = option_env!("NOOKI_CONTACT_URL").unwrap_or("nooki@mints.wtf");
    reqwest::Client::builder()
        .user_agent(format!("Nooki/{} ({contact})", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(Error::from)
}

fn send_progress(
    channel: &Channel<OperationEvent>,
    id: &str,
    phase: &str,
    value: f32,
    message: &str,
) {
    let _ = channel.send(OperationEvent::Progress {
        operation_id: id.into(),
        phase: phase.into(),
        progress: value,
        message: message.into(),
    });
}

#[allow(clippy::too_many_arguments)]
fn send_transfer_progress(
    channel: &Channel<OperationEvent>,
    id: &str,
    phase: &str,
    start: f32,
    end: f32,
    label: &str,
    received: u64,
    total: u64,
) {
    if total == 0 {
        send_progress(
            channel,
            id,
            phase,
            start,
            &format!("{label} · {} downloaded", human_bytes(received)),
        );
        return;
    }
    let transfer_progress = (received.min(total) as f32 / total as f32 * 100.0).clamp(0.0, 100.0);
    send_progress(
        channel,
        id,
        phase,
        start + (end - start) * transfer_progress / 100.0,
        &format!(
            "{label} · {transfer_progress:.0}% · {} of {}",
            human_bytes(received),
            human_bytes(total)
        ),
    );
}

fn human_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.0} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_supported_mrpack_loaders() {
        let fabric = HashMap::from([
            ("minecraft".into(), "1.21.1".into()),
            ("fabric-loader".into(), "0.16.9".into()),
        ]);
        assert!(matches!(
            loader_from_dependencies(&fabric),
            Ok((ServerType::Fabric, _))
        ));
        let forge = HashMap::from([
            ("minecraft".into(), "1.20.1".into()),
            ("forge".into(), "47.3.0".into()),
        ]);
        assert!(matches!(
            loader_from_dependencies(&forge),
            Ok((ServerType::Forge, _))
        ));
        let neoforge = HashMap::from([
            ("minecraft".into(), "1.21.1".into()),
            ("neoforge".into(), "21.1.248".into()),
        ]);
        assert!(matches!(
            loader_from_dependencies(&neoforge),
            Ok((ServerType::NeoForge, _))
        ));
        assert!(matches!(
            parse_curse_loader("neoforge-21.1.248"),
            Ok((ServerType::NeoForge, version)) if version == "21.1.248"
        ));
    }

    #[test]
    fn rejects_pack_path_traversal() {
        assert!(safe_relative_path("mods/example.jar").is_ok());
        assert!(safe_relative_path("../server.jar").is_err());
        assert!(safe_relative_path("/server.jar").is_err());
    }
}
