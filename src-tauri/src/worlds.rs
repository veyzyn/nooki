use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

use fastnbt::Value;
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::State;

use crate::{
    error::{AppError, CommandResult, Error, Result},
    models::{Server, ServerType, WorldEntry, WorldKind, WorldSettingsInput},
    paths::path_string,
    recycle,
    state::AppState,
};

type SharedState<'a> = State<'a, Arc<AppState>>;

#[derive(Clone)]
struct WorldTarget {
    entry: WorldEntry,
    level_dat: PathBuf,
    reset_paths: Vec<PathBuf>,
    delete_path: Option<PathBuf>,
}

#[derive(Default, Clone)]
struct LevelMetadata {
    seed: Option<i64>,
    version: Option<String>,
    data_version: Option<i32>,
    level_name: Option<String>,
    last_played: Option<i64>,
    spawn_x: Option<i32>,
    spawn_y: Option<i32>,
    spawn_z: Option<i32>,
    border_size: Option<f64>,
    day_time: Option<i64>,
    weather: String,
    game_mode: Option<String>,
    difficulty: Option<String>,
    hardcore: bool,
    allow_commands: bool,
    error: Option<String>,
}

#[tauri::command]
pub async fn list_worlds(
    state: SharedState<'_>,
    server_id: String,
) -> CommandResult<Vec<WorldEntry>> {
    let server = state.server(&server_id).await.map_err(AppError::from)?;
    scan_async(server).await.map_err(AppError::from)
}

#[tauri::command]
pub async fn save_world_settings(
    state: SharedState<'_>,
    server_id: String,
    world_id: String,
    input: WorldSettingsInput,
) -> CommandResult<Vec<WorldEntry>> {
    let lock = state.operation_lock(&server_id);
    let _guard = lock.lock().await;
    let server = stopped_server(state.inner(), &server_id)
        .await
        .map_err(AppError::from)?;
    validate_settings(&input).map_err(AppError::from)?;
    let target = find_target(&server, &world_id)
        .await
        .map_err(AppError::from)?;
    let level_dat = target.level_dat.clone();
    tokio::task::spawn_blocking(move || write_level_settings(&level_dat, &input))
        .await
        .map_err(|error| AppError::from(Error::Internal(error.to_string())))?
        .map_err(AppError::from)?;
    state
        .activity(
            "settings",
            Some(&server),
            format!("Updated world settings for {}", target.entry.name),
        )
        .await
        .map_err(AppError::from)?;
    scan_async(server).await.map_err(AppError::from)
}

#[tauri::command]
pub async fn regenerate_world(
    state: SharedState<'_>,
    server_id: String,
    world_id: String,
    reset_players: bool,
) -> CommandResult<Vec<WorldEntry>> {
    let lock = state.operation_lock(&server_id);
    let _guard = lock.lock().await;
    let server = stopped_server(state.inner(), &server_id)
        .await
        .map_err(AppError::from)?;
    let mut target = find_target(&server, &world_id)
        .await
        .map_err(AppError::from)?;
    if reset_players {
        if let Some(world_root) = target.level_dat.parent() {
            for name in ["playerdata", "advancements", "stats"] {
                target.reset_paths.push(world_root.join(name));
            }
            for name in ["data", "advancements", "stats"] {
                target
                    .reset_paths
                    .push(world_root.join("players").join(name));
            }
        }
    }
    let server_root = PathBuf::from(&server.folder);
    let paths = target.reset_paths.clone();
    tokio::task::spawn_blocking(move || recycle_paths(&server_root, &paths))
        .await
        .map_err(|error| AppError::from(Error::Internal(error.to_string())))?
        .map_err(AppError::from)?;
    state
        .activity(
            "settings",
            Some(&server),
            format!("Queued {} for regeneration", target.entry.name),
        )
        .await
        .map_err(AppError::from)?;
    scan_async(server).await.map_err(AppError::from)
}

#[tauri::command]
pub async fn delete_world(
    state: SharedState<'_>,
    server_id: String,
    world_id: String,
    confirmation: String,
) -> CommandResult<Vec<WorldEntry>> {
    let lock = state.operation_lock(&server_id);
    let _guard = lock.lock().await;
    let server = stopped_server(state.inner(), &server_id)
        .await
        .map_err(AppError::from)?;
    let target = find_target(&server, &world_id)
        .await
        .map_err(AppError::from)?;
    if !target.entry.custom || target.entry.primary {
        return Err(AppError::from(Error::Validation(
            "Built-in dimensions cannot be deleted. Regenerate their terrain instead.".into(),
        )));
    }
    if confirmation != target.entry.name {
        return Err(AppError::from(Error::Validation(
            "Type the world name exactly to delete it.".into(),
        )));
    }
    let delete_path = target.delete_path.ok_or_else(|| {
        AppError::from(Error::Validation(
            "This world does not have a separately removable folder.".into(),
        ))
    })?;
    let server_root = PathBuf::from(&server.folder);
    tokio::task::spawn_blocking(move || {
        validate_destructive_path(&server_root, &delete_path)?;
        recycle::move_to_recycle_bin(&delete_path)
    })
    .await
    .map_err(|error| AppError::from(Error::Internal(error.to_string())))?
    .map_err(AppError::from)?;
    state
        .activity(
            "settings",
            Some(&server),
            format!("Moved world {} to the Recycle Bin", target.entry.name),
        )
        .await
        .map_err(AppError::from)?;
    scan_async(server).await.map_err(AppError::from)
}

async fn stopped_server(state: &Arc<AppState>, server_id: &str) -> Result<Server> {
    let server = state.server(server_id).await?;
    if state.processes.is_running(server_id).await {
        return Err(Error::Conflict(
            "Stop the Minecraft server before changing world files.".into(),
        ));
    }
    Ok(server)
}

async fn scan_async(server: Server) -> Result<Vec<WorldEntry>> {
    tokio::task::spawn_blocking(move || scan_worlds(&server))
        .await
        .map_err(|error| Error::Internal(error.to_string()))?
}

async fn find_target(server: &Server, world_id: &str) -> Result<WorldTarget> {
    let server = server.clone();
    let id = world_id.to_owned();
    tokio::task::spawn_blocking(move || {
        scan_targets(&server)?
            .into_iter()
            .find(|target| target.entry.id == id)
            .ok_or_else(|| Error::NotFound("That world is no longer present on disk.".into()))
    })
    .await
    .map_err(|error| Error::Internal(error.to_string()))?
}

fn scan_worlds(server: &Server) -> Result<Vec<WorldEntry>> {
    Ok(scan_targets(server)?
        .into_iter()
        .map(|target| target.entry)
        .collect())
}

fn scan_targets(server: &Server) -> Result<Vec<WorldTarget>> {
    let server_root = PathBuf::from(&server.folder);
    let level_name = read_level_name(&server_root.join("server.properties"));
    // Imported servers can point directly at a world save. Prefer that root
    // when it owns level.dat instead of inventing a nested `world` folder.
    let primary_root = if server_root.join("level.dat").is_file() {
        server_root.clone()
    } else {
        server_root.join(&level_name)
    };
    let immediate_roots = discover_world_roots(&server_root)?;
    let nether_folder = format!("{level_name}_nether");
    let end_folder = format!("{level_name}_the_end");
    let separate_nether = immediate_roots
        .iter()
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == nether_folder)
        })
        .cloned();
    let separate_end = immediate_roots
        .iter()
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == end_folder)
        })
        .cloned();
    let primary_level = primary_root.join("level.dat");
    let primary_metadata = read_metadata(&primary_level);
    let canonical_dimensions = primary_root.join("dimensions").join("minecraft");
    let overworld_data =
        prefer_existing_dimension(canonical_dimensions.join("overworld"), primary_root.clone());
    let nether_fallback = separate_nether
        .as_ref()
        .map(|root| prefer_existing_dimension(root.join("DIM-1"), root.clone()))
        .unwrap_or_else(|| primary_root.join("DIM-1"));
    let nether_data =
        prefer_existing_dimension(canonical_dimensions.join("the_nether"), nether_fallback);
    let end_fallback = separate_end
        .as_ref()
        .map(|root| prefer_existing_dimension(root.join("DIM1"), root.clone()))
        .unwrap_or_else(|| primary_root.join("DIM1"));
    let end_data = prefer_existing_dimension(canonical_dimensions.join("the_end"), end_fallback);
    let mut used = HashSet::new();
    used.insert(normalized_key(&primary_root));
    if let Some(path) = &separate_nether {
        used.insert(normalized_key(path));
    }
    if let Some(path) = &separate_end {
        used.insert(normalized_key(path));
    }

    let mut targets = vec![
        build_target(
            server,
            "Overworld".into(),
            WorldKind::Overworld,
            overworld_data,
            primary_level.clone(),
            primary_metadata.clone(),
            true,
            false,
            None,
        ),
        build_target(
            server,
            "Nether".into(),
            WorldKind::Nether,
            nether_data,
            separate_nether
                .as_ref()
                .map(|path| path.join("level.dat"))
                .unwrap_or_else(|| primary_level.clone()),
            separate_nether
                .as_ref()
                .map(|path| read_metadata(&path.join("level.dat")))
                .unwrap_or_else(|| primary_metadata.clone()),
            true,
            false,
            None,
        ),
        build_target(
            server,
            "The End".into(),
            WorldKind::End,
            end_data,
            separate_end
                .as_ref()
                .map(|path| path.join("level.dat"))
                .unwrap_or_else(|| primary_level.clone()),
            separate_end
                .as_ref()
                .map(|path| read_metadata(&path.join("level.dat")))
                .unwrap_or_else(|| primary_metadata.clone()),
            true,
            false,
            None,
        ),
    ];

    if server.server_type != ServerType::Vanilla {
        for root in immediate_roots {
            if used.contains(&normalized_key(&root)) {
                continue;
            }
            let metadata = read_metadata(&root.join("level.dat"));
            let folder = root
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("Custom world");
            let name = metadata
                .level_name
                .clone()
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| folder.to_owned());
            let kind = if folder.ends_with("_nether") {
                WorldKind::Nether
            } else if folder.ends_with("_the_end") {
                WorldKind::End
            } else {
                WorldKind::Custom
            };
            targets.push(build_target(
                server,
                name,
                kind,
                root.clone(),
                root.join("level.dat"),
                metadata,
                false,
                true,
                Some(root),
            ));
        }
        targets.extend(discover_custom_dimensions(
            server,
            &primary_root,
            &primary_level,
            &primary_metadata,
        ));
    }

    targets.sort_by_key(|target| {
        let order = match target.entry.kind {
            WorldKind::Overworld if target.entry.primary => 0,
            WorldKind::Nether if target.entry.primary => 1,
            WorldKind::End if target.entry.primary => 2,
            _ => 3,
        };
        (order, target.entry.name.to_ascii_lowercase())
    });
    Ok(targets)
}

#[allow(clippy::too_many_arguments)]
fn build_target(
    server: &Server,
    name: String,
    kind: WorldKind,
    data_path: PathBuf,
    level_dat: PathBuf,
    metadata: LevelMetadata,
    primary: bool,
    custom: bool,
    delete_path: Option<PathBuf>,
) -> WorldTarget {
    let reset_paths = ["region", "entities", "poi"]
        .into_iter()
        .map(|name| data_path.join(name))
        .collect::<Vec<_>>();
    let (size, region_files) = directory_stats(&reset_paths);
    let player_files = if matches!(kind, WorldKind::Overworld)
        || (matches!(kind, WorldKind::Custom) && data_path.join("level.dat").is_file())
    {
        level_dat
            .parent()
            .map(count_player_files)
            .unwrap_or_default()
    } else {
        0
    };
    let generated = region_files > 0 || reset_paths.iter().any(|path| path.exists());
    let folder_name = data_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("world")
        .to_owned();
    let id = world_id(server, &data_path, &kind);
    WorldTarget {
        entry: WorldEntry {
            id,
            name,
            folder_name,
            kind,
            path: path_string(&data_path),
            generated,
            primary,
            custom,
            seed: metadata.seed.map(|seed| seed.to_string()),
            version: metadata.version,
            data_version: metadata.data_version,
            size,
            region_files,
            player_files,
            last_played: metadata.last_played,
            spawn_x: metadata.spawn_x,
            spawn_y: metadata.spawn_y,
            spawn_z: metadata.spawn_z,
            border_size: metadata.border_size,
            day_time: metadata.day_time,
            weather: metadata.weather,
            game_mode: metadata.game_mode,
            difficulty: metadata.difficulty,
            hardcore: metadata.hardcore,
            allow_commands: metadata.allow_commands,
            metadata_error: metadata.error,
        },
        level_dat,
        reset_paths,
        delete_path,
    }
}

fn discover_world_roots(server_root: &Path) -> Result<Vec<PathBuf>> {
    let mut roots = Vec::new();
    if server_root.join("level.dat").is_file() {
        roots.push(server_root.to_owned());
    }
    let entries = match std::fs::read_dir(server_root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(roots),
        Err(error) => return Err(error.into()),
    };
    for entry in entries {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_dir() && !file_type.is_symlink() && entry.path().join("level.dat").is_file()
        {
            roots.push(entry.path());
        }
    }
    Ok(roots)
}

fn discover_custom_dimensions(
    server: &Server,
    primary_root: &Path,
    level_dat: &Path,
    metadata: &LevelMetadata,
) -> Vec<WorldTarget> {
    let dimensions = primary_root.join("dimensions");
    if !dimensions.is_dir() {
        return Vec::new();
    }
    walkdir::WalkDir::new(&dimensions)
        .min_depth(2)
        .max_depth(3)
        .follow_links(false)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_dir() && entry.path().join("region").is_dir())
        .filter(|entry| {
            let relative = entry
                .path()
                .strip_prefix(&dimensions)
                .unwrap_or(entry.path());
            !is_builtin_dimension(relative)
        })
        .map(|entry| {
            let relative = entry
                .path()
                .strip_prefix(&dimensions)
                .unwrap_or(entry.path());
            let name = relative.to_string_lossy().replace(['\\', '/'], ":");
            build_target(
                server,
                name,
                WorldKind::Custom,
                entry.path().to_owned(),
                level_dat.to_owned(),
                metadata.clone(),
                false,
                true,
                Some(entry.path().to_owned()),
            )
        })
        .collect()
}

fn prefer_existing_dimension(preferred: PathBuf, fallback: PathBuf) -> PathBuf {
    if preferred.is_dir() {
        preferred
    } else {
        fallback
    }
}

fn is_builtin_dimension(relative: &Path) -> bool {
    let normalized = relative.to_string_lossy().replace('\\', "/");
    matches!(
        normalized.as_str(),
        "minecraft/overworld" | "minecraft/the_nether" | "minecraft/the_end"
    )
}

fn read_level_name(properties: &Path) -> String {
    std::fs::read_to_string(properties)
        .ok()
        .and_then(|contents| {
            contents.lines().find_map(|line| {
                let (key, value) = line.split_once('=')?;
                (key.trim() == "level-name").then(|| value.trim().to_owned())
            })
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "world".into())
}

fn read_metadata(level_dat: &Path) -> LevelMetadata {
    if !level_dat.is_file() {
        return LevelMetadata {
            weather: "clear".into(),
            ..Default::default()
        };
    }
    match read_level_value(level_dat).and_then(metadata_from_value) {
        Ok(metadata) => metadata,
        Err(error) => LevelMetadata {
            weather: "unknown".into(),
            error: Some(error.to_string()),
            ..Default::default()
        },
    }
}

fn read_level_value(path: &Path) -> Result<Value> {
    let file = File::open(path)?;
    let mut decoder = GzDecoder::new(file);
    let mut bytes = Vec::new();
    decoder.read_to_end(&mut bytes)?;
    fastnbt::from_bytes(&bytes)
        .map_err(|error| Error::Validation(format!("level.dat could not be read: {error}")))
}

fn metadata_from_value(root: Value) -> Result<LevelMetadata> {
    let root = compound(&root)
        .ok_or_else(|| Error::Validation("level.dat has no root compound.".into()))?;
    let data = root
        .get("Data")
        .and_then(compound)
        .ok_or_else(|| Error::Validation("level.dat has no Data compound.".into()))?;
    let seed = data
        .get("WorldGenSettings")
        .and_then(compound)
        .and_then(|settings| settings.get("seed"))
        .and_then(value_i64)
        .or_else(|| data.get("RandomSeed").and_then(value_i64));
    let weather = if data.get("thundering").and_then(value_bool).unwrap_or(false) {
        "thunder"
    } else if data.get("raining").and_then(value_bool).unwrap_or(false) {
        "rain"
    } else {
        "clear"
    };
    Ok(LevelMetadata {
        seed,
        version: data
            .get("Version")
            .and_then(compound)
            .and_then(|version| version.get("Name"))
            .and_then(value_string),
        data_version: data.get("DataVersion").and_then(value_i32),
        level_name: data.get("LevelName").and_then(value_string),
        last_played: data.get("LastPlayed").and_then(value_i64),
        spawn_x: data.get("SpawnX").and_then(value_i32),
        spawn_y: data.get("SpawnY").and_then(value_i32),
        spawn_z: data.get("SpawnZ").and_then(value_i32),
        border_size: data.get("BorderSize").and_then(value_f64),
        day_time: data.get("DayTime").and_then(value_i64),
        weather: weather.into(),
        game_mode: data.get("GameType").and_then(value_i32).map(game_mode),
        difficulty: data.get("Difficulty").and_then(value_i32).map(difficulty),
        hardcore: data.get("hardcore").and_then(value_bool).unwrap_or(false),
        allow_commands: data
            .get("allowCommands")
            .and_then(value_bool)
            .unwrap_or(false),
        error: None,
    })
}

fn write_level_settings(path: &Path, input: &WorldSettingsInput) -> Result<()> {
    let seed = input.seed.trim().parse::<i64>().map_err(|_| {
        Error::Validation(
            "Seed must be a whole number from -9223372036854775808 to 9223372036854775807.".into(),
        )
    })?;
    let mut root = read_level_value(path)?;
    let data = compound_mut(&mut root)
        .and_then(|root| root.get_mut("Data"))
        .and_then(compound_mut)
        .ok_or_else(|| Error::Validation("level.dat has no writable Data compound.".into()))?;
    data.insert("RandomSeed".into(), Value::Long(seed));
    if let Some(settings) = data.get_mut("WorldGenSettings").and_then(compound_mut) {
        settings.insert("seed".into(), Value::Long(seed));
    }
    data.insert("SpawnX".into(), Value::Int(input.spawn_x));
    data.insert("SpawnY".into(), Value::Int(input.spawn_y));
    data.insert("SpawnZ".into(), Value::Int(input.spawn_z));
    data.insert("BorderSize".into(), Value::Double(input.border_size));
    data.insert("DayTime".into(), Value::Long(input.day_time));
    let (raining, thundering) = match input.weather.as_str() {
        "clear" => (false, false),
        "rain" => (true, false),
        "thunder" => (true, true),
        _ => return Err(Error::Validation("Unknown weather selection.".into())),
    };
    data.insert("raining".into(), Value::Byte(i8::from(raining)));
    data.insert("thundering".into(), Value::Byte(i8::from(thundering)));
    data.insert(
        "rainTime".into(),
        Value::Int(if raining { 12000 } else { 0 }),
    );
    data.insert(
        "thunderTime".into(),
        Value::Int(if thundering { 12000 } else { 0 }),
    );

    let bytes = fastnbt::to_bytes(&root)
        .map_err(|error| Error::Validation(format!("level.dat could not be encoded: {error}")))?;
    let temporary = path.with_extension("dat.nooki-tmp");
    let backup = path.with_extension("dat.nooki-prev");
    {
        let file = File::create(&temporary)?;
        let mut encoder = GzEncoder::new(file, Compression::fast());
        encoder.write_all(&bytes)?;
        encoder.finish()?.sync_all()?;
    }
    let _ = std::fs::remove_file(&backup);
    std::fs::rename(path, &backup)?;
    if let Err(error) = std::fs::rename(&temporary, path) {
        let _ = std::fs::rename(&backup, path);
        return Err(error.into());
    }
    let _ = std::fs::remove_file(backup);
    Ok(())
}

fn validate_settings(input: &WorldSettingsInput) -> Result<()> {
    input
        .seed
        .trim()
        .parse::<i64>()
        .map_err(|_| Error::Validation("Seed must be a signed 64-bit whole number.".into()))?;
    if !(-2048..=2048).contains(&input.spawn_y) {
        return Err(Error::Validation(
            "Spawn Y must be between -2048 and 2048.".into(),
        ));
    }
    if !(1.0..=59_999_968.0).contains(&input.border_size) {
        return Err(Error::Validation(
            "World border size must be between 1 and 59,999,968 blocks.".into(),
        ));
    }
    if !(0..=23_999).contains(&input.day_time) {
        return Err(Error::Validation(
            "World time must be between 0 and 23,999 ticks.".into(),
        ));
    }
    Ok(())
}

fn recycle_paths(server_root: &Path, paths: &[PathBuf]) -> Result<()> {
    let existing = paths
        .iter()
        .filter(|path| path.exists())
        .collect::<Vec<_>>();
    if existing.is_empty() {
        return Err(Error::NotFound(
            "This world has no generated terrain to regenerate.".into(),
        ));
    }
    for path in existing {
        validate_destructive_path(server_root, path)?;
        recycle::move_to_recycle_bin(path)?;
    }
    Ok(())
}

fn validate_destructive_path(server_root: &Path, target: &Path) -> Result<()> {
    let server = std::fs::canonicalize(server_root)?;
    let target = std::fs::canonicalize(target)?;
    if target == server || !target.starts_with(&server) {
        return Err(Error::Validation(
            "Nooki refused an unsafe world path.".into(),
        ));
    }
    Ok(())
}

fn directory_stats(paths: &[PathBuf]) -> (u64, u32) {
    let mut size = 0_u64;
    let mut regions = 0_u32;
    for path in paths {
        if !path.exists() {
            continue;
        }
        for entry in walkdir::WalkDir::new(path)
            .follow_links(false)
            .into_iter()
            .flatten()
        {
            if entry.file_type().is_file() {
                size = size
                    .saturating_add(entry.metadata().map(|metadata| metadata.len()).unwrap_or(0));
                if entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "mca")
                {
                    regions = regions.saturating_add(1);
                }
            }
        }
    }
    (size, regions)
}

fn count_dat_files(path: &Path) -> u32 {
    std::fs::read_dir(path)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "dat")
        })
        .count() as u32
}

fn count_player_files(world_root: &Path) -> u32 {
    count_dat_files(&world_root.join("playerdata"))
        .saturating_add(count_dat_files(&world_root.join("players").join("data")))
}

fn world_id(server: &Server, path: &Path, kind: &WorldKind) -> String {
    #[derive(Serialize)]
    struct Key<'a> {
        server: &'a str,
        path: String,
        kind: &'a WorldKind,
    }
    let key = serde_json::to_vec(&Key {
        server: &server.id,
        path: normalized_key(path),
        kind,
    })
    .unwrap_or_default();
    hex::encode(Sha256::digest(key))[..20].into()
}

fn normalized_key(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase()
}

fn compound(value: &Value) -> Option<&HashMap<String, Value>> {
    match value {
        Value::Compound(value) => Some(value),
        _ => None,
    }
}

fn compound_mut(value: &mut Value) -> Option<&mut HashMap<String, Value>> {
    match value {
        Value::Compound(value) => Some(value),
        _ => None,
    }
}

fn value_i64(value: &Value) -> Option<i64> {
    match value {
        Value::Byte(value) => Some((*value).into()),
        Value::Short(value) => Some((*value).into()),
        Value::Int(value) => Some((*value).into()),
        Value::Long(value) => Some(*value),
        _ => None,
    }
}

fn value_i32(value: &Value) -> Option<i32> {
    value_i64(value).and_then(|value| i32::try_from(value).ok())
}

fn value_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Float(value) => Some((*value).into()),
        Value::Double(value) => Some(*value),
        value => value_i64(value).map(|value| value as f64),
    }
}

fn value_bool(value: &Value) -> Option<bool> {
    value_i64(value).map(|value| value != 0)
}

fn value_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        _ => None,
    }
}

fn game_mode(value: i32) -> String {
    match value {
        1 => "Creative",
        2 => "Adventure",
        3 => "Spectator",
        _ => "Survival",
    }
    .into()
}

fn difficulty(value: i32) -> String {
    match value {
        0 => "Peaceful",
        1 => "Easy",
        3 => "Hard",
        _ => "Normal",
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_modern_level_metadata() {
        let root = Value::Compound(HashMap::from([(
            "Data".into(),
            Value::Compound(HashMap::from([
                ("LevelName".into(), Value::String("Test world".into())),
                ("SpawnX".into(), Value::Int(12)),
                ("SpawnY".into(), Value::Int(70)),
                ("SpawnZ".into(), Value::Int(-8)),
                ("DataVersion".into(), Value::Int(4189)),
                (
                    "WorldGenSettings".into(),
                    Value::Compound(HashMap::from([("seed".into(), Value::Long(-42))])),
                ),
            ])),
        )]));
        let metadata = metadata_from_value(root).unwrap();
        assert_eq!(metadata.seed, Some(-42));
        assert_eq!(metadata.spawn_y, Some(70));
        assert_eq!(metadata.data_version, Some(4189));
    }

    #[test]
    fn reads_configured_level_name() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("server.properties");
        std::fs::write(&path, "motd=Hello\nlevel-name=parkour\n").unwrap();
        assert_eq!(read_level_name(&path), "parkour");
    }

    #[test]
    fn minecraft_dimensions_are_not_reported_as_custom_worlds() {
        assert!(is_builtin_dimension(Path::new("minecraft/overworld")));
        assert!(is_builtin_dimension(Path::new("minecraft/the_nether")));
        assert!(is_builtin_dimension(Path::new("minecraft/the_end")));
        assert!(!is_builtin_dimension(Path::new("my_pack/moon")));
    }

    #[test]
    fn writes_world_settings_without_losing_the_level_compound() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("level.dat");
        let root = Value::Compound(HashMap::from([
            (
                "Data".into(),
                Value::Compound(HashMap::from([
                    ("LevelName".into(), Value::String("Keep me".into())),
                    (
                        "WorldGenSettings".into(),
                        Value::Compound(HashMap::from([("seed".into(), Value::Long(1))])),
                    ),
                ])),
            ),
            ("NookiTest".into(), Value::String("preserved".into())),
        ]));
        let bytes = fastnbt::to_bytes(&root).unwrap();
        let file = File::create(&path).unwrap();
        let mut encoder = GzEncoder::new(file, Compression::fast());
        encoder.write_all(&bytes).unwrap();
        encoder.finish().unwrap();

        write_level_settings(
            &path,
            &WorldSettingsInput {
                seed: "-987654321".into(),
                spawn_x: 20,
                spawn_y: 80,
                spawn_z: -30,
                border_size: 15_000.0,
                day_time: 6_000,
                weather: "rain".into(),
            },
        )
        .unwrap();

        let updated = read_level_value(&path).unwrap();
        let root = compound(&updated).unwrap();
        assert_eq!(
            root.get("NookiTest").and_then(value_string).as_deref(),
            Some("preserved")
        );
        let metadata = metadata_from_value(updated).unwrap();
        assert_eq!(metadata.seed, Some(-987654321));
        assert_eq!(metadata.spawn_x, Some(20));
        assert_eq!(metadata.spawn_y, Some(80));
        assert_eq!(metadata.spawn_z, Some(-30));
        assert_eq!(metadata.day_time, Some(6_000));
        assert_eq!(metadata.weather, "rain");
        assert!(!path.with_extension("dat.nooki-prev").exists());
    }
}
