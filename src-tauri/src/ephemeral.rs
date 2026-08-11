use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

use flate2::read::GzDecoder;
use serde::Deserialize;
use tauri::{ipc::Channel, State};

use crate::{
    commands::ensure_runtime,
    error::{AppError, Error, Result},
    models::{
        CreateEphemeralServerInput, EphemeralWorldScan, OperationEvent, Server, ServerSharing,
        ServerStatus, ServerType, SharingStatus,
    },
    paths::{normalize_path, path_string},
    properties::PropertiesFile,
    state::{directory_size, AppState},
};

const MAX_WORLD_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const MAX_WORLD_FILES: usize = 100_000;

#[derive(Debug, Deserialize)]
struct LevelRoot {
    #[serde(rename = "Data")]
    data: LevelData,
}

#[derive(Debug, Deserialize)]
struct LevelData {
    #[serde(rename = "Version")]
    version: Option<LevelVersion>,
    #[serde(rename = "LevelName")]
    level_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LevelVersion {
    #[serde(rename = "Name")]
    name: Option<String>,
}

#[derive(Debug, Clone)]
enum WorldLocation {
    Folder(PathBuf),
    Zip { archive: PathBuf, root: PathBuf },
}

#[derive(Debug, Clone)]
struct InspectedWorld {
    scan: EphemeralWorldScan,
    location: WorldLocation,
}

#[tauri::command]
pub async fn scan_ephemeral_world(
    path: String,
) -> std::result::Result<EphemeralWorldScan, AppError> {
    tokio::task::spawn_blocking(move || inspect_world(PathBuf::from(path)).map(|world| world.scan))
        .await
        .map_err(|error| AppError::from(Error::Internal(error.to_string())))?
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn create_ephemeral_server(
    state: State<'_, Arc<AppState>>,
    input: CreateEphemeralServerInput,
    on_progress: Channel<OperationEvent>,
) -> std::result::Result<Server, AppError> {
    create(state.inner().clone(), input, Some(&on_progress))
        .await
        .map_err(AppError::from)
}

async fn create(
    state: Arc<AppState>,
    input: CreateEphemeralServerInput,
    channel: Option<&Channel<OperationEvent>>,
) -> Result<Server> {
    let operation_lock = state.operation_lock("ephemeral-world");
    let _guard = operation_lock.try_lock().map_err(|_| {
        Error::Conflict("Another temporary world is already being prepared.".into())
    })?;
    if state
        .servers
        .read()
        .await
        .values()
        .any(|server| server.ephemeral)
    {
        return Err(Error::Conflict(
            "Stop the current temporary world before starting another one.".into(),
        ));
    }
    if input.version.trim().is_empty() {
        return Err(Error::Validation("Choose a Minecraft version.".into()));
    }

    let operation_id = uuid::Uuid::new_v4().to_string();
    let _operation = crate::operations::begin(&operation_id);
    progress(
        channel,
        &operation_id,
        "inspect",
        2.0,
        "Checking world files",
    );
    let source = PathBuf::from(input.source_path);
    let inspected = tokio::task::spawn_blocking(move || inspect_world(source))
        .await
        .map_err(|error| Error::Internal(error.to_string()))??;
    let resolved = state
        .catalog
        .resolve(ServerType::Vanilla, input.version.trim(), None, false)
        .await?;
    let runtime = ensure_runtime(
        state.clone(),
        resolved.java_major,
        None,
        channel,
        &operation_id,
        (4.0, 14.0),
    )
    .await?;
    let port = available_port(&state).await?;
    let id = uuid::Uuid::new_v4().to_string();
    let ephemeral_root = state.app_data_dir.join("ephemeral");
    let staging = ephemeral_root.join(format!(".staging-{id}"));
    let final_folder = ephemeral_root.join(&id);
    tokio::fs::create_dir_all(&staging).await?;

    let setup = async {
        progress(channel, &operation_id, "world", 16.0, "Copying the world");
        let location = inspected.location.clone();
        let destination = staging.join("world");
        let copy_operation = operation_id.clone();
        tokio::task::spawn_blocking(move || {
            copy_world(&location, &destination, Some(&copy_operation))
        })
        .await
        .map_err(|error| Error::Internal(error.to_string()))??;

        progress(
            channel,
            &operation_id,
            "download",
            30.0,
            "Downloading the Minecraft server",
        );
        state
            .catalog
            .download(
                &resolved,
                &staging.join("server.jar"),
                channel,
                &operation_id,
                (30.0, 84.0),
            )
            .await?;
        crate::operations::check(&operation_id)?;
        progress(
            channel,
            &operation_id,
            "configure",
            86.0,
            "Configuring the temporary server",
        );
        tokio::fs::write(
            staging.join("eula.txt"),
            "# Accepted through Nooki temporary worlds\neula=true\n",
        )
        .await?;
        let mut properties = PropertiesFile::parse("");
        properties.update(&HashMap::from([
            ("server-port", port.to_string()),
            ("level-name", "world".into()),
            (
                "motd",
                format!("{} - shared with Nooki", inspected.scan.world_name),
            ),
            ("max-players", "20".into()),
            ("gamemode", "adventure".into()),
            ("difficulty", "peaceful".into()),
            ("pvp", "false".into()),
            ("white-list", "false".into()),
            ("online-mode", "true".into()),
            ("spawn-protection", "0".into()),
            ("enable-command-block", "true".into()),
            ("allow-flight", "true".into()),
            ("view-distance", "8".into()),
            ("simulation-distance", "6".into()),
        ]));
        properties
            .write_atomic(&staging.join("server.properties"))
            .await?;
        crate::operations::check(&operation_id)?;
        tokio::fs::rename(&staging, &final_folder).await?;
        Result::<()>::Ok(())
    }
    .await;
    if let Err(error) = setup {
        let _ = tokio::fs::remove_dir_all(&staging).await;
        return Err(error);
    }

    let server = Server {
        id: id.clone(),
        name: inspected.scan.world_name,
        server_type: ServerType::Vanilla,
        version: resolved.version,
        build: resolved.build,
        status: ServerStatus::Stopped,
        players: 0,
        max_players: 20,
        started_at: None,
        memory: 0.0,
        min_memory: 512,
        max_memory: 4096,
        cpu: 0.0,
        disk_used: directory_size(&final_folder).await,
        port,
        folder: path_string(&final_folder),
        jar_path: path_string(&final_folder.join("server.jar")),
        accent: "#5fb87f".into(),
        motd: "Temporary world shared with Nooki".into(),
        game_mode: "adventure".into(),
        difficulty: "peaceful".into(),
        pvp: false,
        whitelist_enabled: false,
        online_mode: true,
        java_runtime_id: runtime.id,
        java_runtime: runtime.label,
        jvm_args: "-XX:+UseG1GC".into(),
        history: Vec::new(),
        alerts: Vec::new(),
        update_available: None,
        last_exit: None,
        active_operation: None,
        rich_management: true,
        sharing: ServerSharing {
            status: SharingStatus::Offline,
            address: None,
            device_id: None,
            last_error: None,
            vanity: None,
        },
        ephemeral: true,
    };
    state.save_server(server.clone()).await?;
    progress(channel, &operation_id, "start", 94.0, "Starting the world");
    if let Err(error) = state.processes.start(state.clone(), &id).await {
        let _ = state.db.delete_server(&id).await;
        state.servers.write().await.remove(&id);
        let _ = tokio::fs::remove_dir_all(&final_folder).await;
        return Err(error);
    }
    progress(
        channel,
        &operation_id,
        "done",
        100.0,
        "Temporary world is starting",
    );
    state.server(&id).await
}

async fn available_port(state: &AppState) -> Result<u16> {
    for _ in 0..20 {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
        let port = listener.local_addr()?.port();
        drop(listener);
        if !state
            .servers
            .read()
            .await
            .values()
            .any(|server| server.port == port)
        {
            return Ok(port);
        }
    }
    Err(Error::Conflict(
        "Nooki could not reserve a local port for the temporary world.".into(),
    ))
}

fn inspect_world(path: PathBuf) -> Result<InspectedWorld> {
    let path = normalize_path(std::fs::canonicalize(path)?);
    if path.is_dir() {
        inspect_folder(path)
    } else if path.is_file()
        && path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
    {
        inspect_zip(path)
    } else {
        Err(Error::Validation(
            "Drop a Minecraft world folder or a .zip archive.".into(),
        ))
    }
}

fn inspect_folder(source: PathBuf) -> Result<InspectedWorld> {
    let mut candidates = walkdir::WalkDir::new(&source)
        .follow_links(false)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file() && entry.file_name() == "level.dat")
        .map(|entry| entry.into_path())
        .collect::<Vec<_>>();
    candidates.sort_by_key(|path| path.components().count());
    let level_dat = candidates.first().ok_or_else(|| {
        Error::Validation("That folder does not contain a Minecraft level.dat file.".into())
    })?;
    let root = level_dat
        .parent()
        .ok_or_else(|| Error::Validation("The world folder is invalid.".into()))?
        .to_path_buf();
    // A level.dat is enough to identify a world. Older, modded, or damaged
    // metadata should fall back to the version picker rather than rejecting
    // the source outright.
    let metadata = read_level_dat(File::open(level_dat)?).unwrap_or((None, None));
    let fallback = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Temporary world");
    let mut warnings = Vec::new();
    if candidates.len() > 1 {
        warnings.push(format!(
            "Found {} worlds and selected the least-nested one.",
            candidates.len()
        ));
    }
    Ok(InspectedWorld {
        scan: EphemeralWorldScan {
            source_path: path_string(&source),
            source_kind: "folder".into(),
            world_name: clean_world_name(metadata.1.as_deref().unwrap_or(fallback)),
            detected_version: metadata.0,
            warnings,
        },
        location: WorldLocation::Folder(root),
    })
}

fn inspect_zip(source: PathBuf) -> Result<InspectedWorld> {
    let mut archive = zip::ZipArchive::new(File::open(&source)?)?;
    let mut candidates = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let Some(path) = entry.enclosed_name() else {
            continue;
        };
        if path.file_name().is_some_and(|name| name == "level.dat") {
            let metadata = read_level_dat(&mut entry).unwrap_or((None, None));
            candidates.push((path, metadata));
        }
    }
    candidates.sort_by_key(|(path, _)| path.components().count());
    let (level_dat, metadata) = candidates.first().cloned().ok_or_else(|| {
        Error::Validation("That archive does not contain a Minecraft level.dat file.".into())
    })?;
    let root = level_dat.parent().unwrap_or(Path::new("")).to_path_buf();
    let fallback = root
        .file_name()
        .or_else(|| source.file_stem())
        .and_then(|name| name.to_str())
        .unwrap_or("Temporary world");
    let mut warnings = Vec::new();
    if candidates.len() > 1 {
        warnings.push(format!(
            "Found {} worlds and selected the least-nested one.",
            candidates.len()
        ));
    }
    Ok(InspectedWorld {
        scan: EphemeralWorldScan {
            source_path: path_string(&source),
            source_kind: "zip".into(),
            world_name: clean_world_name(metadata.1.as_deref().unwrap_or(fallback)),
            detected_version: metadata.0,
            warnings,
        },
        location: WorldLocation::Zip {
            archive: source,
            root,
        },
    })
}

fn read_level_dat(reader: impl Read) -> Result<(Option<String>, Option<String>)> {
    let mut decoder = GzDecoder::new(reader);
    let mut bytes = Vec::new();
    decoder.read_to_end(&mut bytes)?;
    let root: LevelRoot = fastnbt::from_bytes(&bytes)
        .map_err(|error| Error::Validation(format!("level.dat could not be read: {error}")))?;
    Ok((
        root.data.version.and_then(|version| version.name),
        root.data.level_name,
    ))
}

fn copy_world(
    location: &WorldLocation,
    destination: &Path,
    operation_id: Option<&str>,
) -> Result<()> {
    match location {
        WorldLocation::Folder(source) => copy_folder(source, destination, operation_id),
        WorldLocation::Zip { archive, root } => {
            extract_zip_world(archive, root, destination, operation_id)
        }
    }
}

fn copy_folder(source: &Path, destination: &Path, operation_id: Option<&str>) -> Result<()> {
    let mut total = 0_u64;
    let mut files = 0_usize;
    for entry in walkdir::WalkDir::new(source).follow_links(false) {
        if let Some(operation_id) = operation_id {
            crate::operations::check(operation_id)?;
        }
        let entry = entry.map_err(|error| Error::Io(error.into()))?;
        if entry.file_type().is_symlink() {
            return Err(Error::Validation(
                "World folders containing symbolic links are not supported.".into(),
            ));
        }
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(|error| Error::Internal(error.to_string()))?;
        let target = destination.join(relative);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(target)?;
            continue;
        }
        files += 1;
        total = total.saturating_add(
            entry
                .metadata()
                .map_err(|error| Error::Io(error.into()))?
                .len(),
        );
        validate_world_size(files, total)?;
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(entry.path(), target)?;
    }
    Ok(())
}

fn extract_zip_world(
    archive_path: &Path,
    root: &Path,
    destination: &Path,
    operation_id: Option<&str>,
) -> Result<()> {
    let mut archive = zip::ZipArchive::new(File::open(archive_path)?)?;
    let mut total = 0_u64;
    let mut files = 0_usize;
    for index in 0..archive.len() {
        if let Some(operation_id) = operation_id {
            crate::operations::check(operation_id)?;
        }
        let mut entry = archive.by_index(index)?;
        let relative = entry
            .enclosed_name()
            .and_then(|path| path.strip_prefix(root).ok().map(Path::to_path_buf));
        let Some(relative) = relative else {
            continue;
        };
        if relative.as_os_str().is_empty() {
            continue;
        }
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(Error::Archive(
                "World archives containing symbolic links are not supported.".into(),
            ));
        }
        let target = destination.join(relative);
        if entry.is_dir() {
            std::fs::create_dir_all(target)?;
            continue;
        }
        files += 1;
        total = total.saturating_add(entry.size());
        validate_world_size(files, total)?;
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut output = File::create(target)?;
        std::io::copy(&mut entry, &mut output)?;
        output.flush()?;
    }
    Ok(())
}

fn validate_world_size(files: usize, bytes: u64) -> Result<()> {
    if files > MAX_WORLD_FILES {
        return Err(Error::Validation(format!(
            "Temporary worlds can contain at most {MAX_WORLD_FILES} files."
        )));
    }
    if bytes > MAX_WORLD_BYTES {
        return Err(Error::Validation(
            "Temporary worlds can be at most 4 GB after extraction.".into(),
        ));
    }
    Ok(())
}

fn clean_world_name(value: &str) -> String {
    let cleaned = value
        .chars()
        .filter(|character| !character.is_control())
        .collect::<String>();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        "Temporary world".into()
    } else {
        cleaned.chars().take(80).collect()
    }
}

fn progress(
    channel: Option<&Channel<OperationEvent>>,
    operation_id: &str,
    phase: &str,
    value: f32,
    message: &str,
) {
    if let Some(channel) = channel {
        let _ = channel.send(OperationEvent::Progress {
            operation_id: operation_id.into(),
            phase: phase.into(),
            progress: value,
            message: message.into(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zip::write::SimpleFileOptions;

    #[test]
    fn nested_zip_world_is_selected_and_extracted_without_wrapper_folders() {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("map.zip");
        let level_dat = fixture_level_dat("1.21.4", "Parkour map");
        let file = File::create(&archive_path).unwrap();
        let mut archive = zip::ZipWriter::new(file);
        archive
            .start_file("download/map/world/level.dat", SimpleFileOptions::default())
            .unwrap();
        archive.write_all(&level_dat).unwrap();
        archive
            .start_file(
                "download/map/world/region/r.0.0.mca",
                SimpleFileOptions::default(),
            )
            .unwrap();
        archive.write_all(b"region").unwrap();
        archive.finish().unwrap();

        let inspected = inspect_world(archive_path).unwrap();
        assert_eq!(inspected.scan.detected_version.as_deref(), Some("1.21.4"));
        assert_eq!(inspected.scan.world_name, "Parkour map");
        let destination = temp.path().join("extracted");
        copy_world(&inspected.location, &destination, None).unwrap();
        assert!(destination.join("level.dat").is_file());
        assert!(destination.join("region/r.0.0.mca").is_file());
        assert!(!destination.join("download").exists());
    }

    #[test]
    fn rejects_zip_paths_that_escape_the_archive() {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("unsafe.zip");
        let file = File::create(&archive_path).unwrap();
        let mut archive = zip::ZipWriter::new(file);
        archive
            .start_file("../level.dat", SimpleFileOptions::default())
            .unwrap();
        archive
            .write_all(&fixture_level_dat("1.20.4", "Unsafe"))
            .unwrap();
        archive.finish().unwrap();
        assert!(inspect_world(archive_path).is_err());
    }

    #[test]
    fn unreadable_folder_metadata_falls_back_to_version_selection() {
        let temp = tempfile::tempdir().unwrap();
        let world = temp.path().join("old-parkour-map");
        std::fs::create_dir(&world).unwrap();
        std::fs::write(world.join("level.dat"), b"not valid nbt").unwrap();

        let inspected = inspect_world(world).unwrap();
        assert_eq!(inspected.scan.world_name, "old-parkour-map");
        assert_eq!(inspected.scan.detected_version, None);
    }

    fn fixture_level_dat(version: &str, name: &str) -> Vec<u8> {
        use flate2::{write::GzEncoder, Compression};
        use serde::Serialize;

        #[derive(Serialize)]
        struct FixtureRoot<'a> {
            #[serde(rename = "Data")]
            data: FixtureData<'a>,
        }
        #[derive(Serialize)]
        struct FixtureData<'a> {
            #[serde(rename = "Version")]
            version: FixtureVersion<'a>,
            #[serde(rename = "LevelName")]
            level_name: &'a str,
        }
        #[derive(Serialize)]
        struct FixtureVersion<'a> {
            #[serde(rename = "Name")]
            name: &'a str,
        }

        let bytes = fastnbt::to_bytes(&FixtureRoot {
            data: FixtureData {
                version: FixtureVersion { name: version },
                level_name: name,
            },
        })
        .unwrap();
        let mut gzip = GzEncoder::new(Vec::new(), Compression::default());
        gzip.write_all(&bytes).unwrap();
        gzip.finish().unwrap()
    }
}
