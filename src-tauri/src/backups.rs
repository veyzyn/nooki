use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::ipc::Channel;

use crate::{
    error::{Error, Result},
    models::{now_ms, AppEvent, Backup, OperationEvent},
    paths::path_string,
    properties::PropertiesFile,
    state::AppState,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackupManifest {
    schema_version: u32,
    server_id: String,
    server_name: String,
    version: String,
    created_at: i64,
    roots: Vec<String>,
}

pub async fn create_backup(
    state: Arc<AppState>,
    server_id: &str,
    backup_type: &str,
    notes: Option<String>,
    channel: Option<&Channel<OperationEvent>>,
) -> Result<Backup> {
    let operation_id = uuid::Uuid::new_v4().to_string();
    let _operation = crate::operations::begin(&operation_id);
    progress(
        channel,
        &operation_id,
        "prepare",
        2.0,
        "Preparing server data",
    );
    let server = state.server(server_id).await?;
    let running = state.processes.is_running(server_id).await;
    if running && !server.rich_management {
        return Err(Error::Unsupported(
            "This legacy server must be stopped before it can be backed up safely.".into(),
        ));
    }
    if running {
        state.processes.send(server_id, "save-off").await?;
        if let Err(error) = state.processes.send(server_id, "save-all flush").await {
            let _ = state.processes.send(server_id, "save-on").await;
            return Err(error);
        }
        tokio::time::sleep(std::time::Duration::from_millis(750)).await;
    }

    let result = async {
        let settings = state.settings.read().await.clone();
        let backup_root = PathBuf::from(&settings.backup_folder).join(&server.id);
        tokio::fs::create_dir_all(&backup_root).await?;
        let staging = state
            .app_data_dir
            .join("staging")
            .join(format!("backup-{operation_id}"));
        if tokio::fs::try_exists(&staging).await? {
            tokio::fs::remove_dir_all(&staging).await?;
        }
        tokio::fs::create_dir_all(&staging).await?;
        let roots = backup_roots(Path::new(&server.folder)).await?;
        progress(
            channel,
            &operation_id,
            "copy",
            18.0,
            "Copying recoverable server data",
        );
        let source = PathBuf::from(&server.folder);
        let stage_for_copy = staging.clone();
        let roots_for_copy = roots.clone();
        let copy_operation = operation_id.clone();
        let copy_result = tokio::task::spawn_blocking(move || {
            copy_roots(&source, &stage_for_copy, &roots_for_copy, &copy_operation)
        })
        .await
        .map_err(|error| Error::Internal(error.to_string()))?;
        if let Err(error) = copy_result {
            let _ = tokio::fs::remove_dir_all(&staging).await;
            return Err(error);
        }
        let manifest = BackupManifest {
            schema_version: 1,
            server_id: server.id.clone(),
            server_name: server.name.clone(),
            version: server.version.clone(),
            created_at: now_ms(),
            roots: roots.clone(),
        };
        tokio::fs::write(
            staging.join(".nooki-backup.json"),
            serde_json::to_vec_pretty(&manifest)?,
        )
        .await?;
        Ok::<_, Error>((backup_root, staging, manifest))
    }
    .await;

    if running {
        let _ = state.processes.send(server_id, "save-on").await;
    }
    let (backup_root, staging, manifest) = result?;
    progress(
        channel,
        &operation_id,
        "compress",
        55.0,
        "Compressing backup archive",
    );
    let safe_name = sanitize_filename(&server.name);
    let archive = backup_root.join(format!(
        "{}-{}-{}.zip",
        safe_name,
        manifest.created_at,
        &operation_id[..8]
    ));
    let staging_for_zip = staging.clone();
    let archive_for_zip = archive.clone();
    let zip_operation = operation_id.clone();
    if let Err(error) = tokio::task::spawn_blocking(move || {
        zip_folder(&staging_for_zip, &archive_for_zip, &zip_operation)
    })
    .await
    .map_err(|error| Error::Internal(error.to_string()))?
    {
        let _ = tokio::fs::remove_dir_all(&staging).await;
        let _ = tokio::fs::remove_file(&archive).await;
        return Err(error);
    }
    let _ = tokio::fs::remove_dir_all(&staging).await;
    let size = tokio::fs::metadata(&archive).await?.len();
    let checksum = sha256_file(&archive).await?;
    let backup = Backup {
        id: operation_id.clone(),
        server_id: server.id.clone(),
        server_name: server.name.clone(),
        backup_type: backup_type.into(),
        created_at: manifest.created_at,
        size,
        version: server.version.clone(),
        notes,
        failed: Some(false),
        path: path_string(&archive),
        checksum: Some(checksum),
        error_message: None,
    };
    state.db.save_backup(&backup).await?;
    state
        .backups
        .write()
        .await
        .insert(backup.id.clone(), backup.clone());
    state.emit(AppEvent::BackupChanged(backup.clone()));
    state
        .activity(
            "backup",
            Some(&server),
            format!("{} backup created", title_case(backup_type)),
        )
        .await?;
    if backup_type == "scheduled" {
        apply_retention(state.clone(), server_id).await?;
    }
    progress(channel, &operation_id, "done", 100.0, "Backup finished");
    Ok(backup)
}

pub async fn restore_backup(
    state: Arc<AppState>,
    backup_id: &str,
    channel: Option<&Channel<OperationEvent>>,
) -> Result<()> {
    let operation_id = uuid::Uuid::new_v4().to_string();
    let _operation = crate::operations::begin(&operation_id);
    let backup = state
        .backups
        .read()
        .await
        .get(backup_id)
        .cloned()
        .ok_or_else(|| Error::NotFound("That backup no longer exists.".into()))?;
    let server = state.server(&backup.server_id).await?;
    if state.processes.is_running(&server.id).await {
        return Err(Error::Conflict(
            "Stop the server before restoring a backup.".into(),
        ));
    }
    let archive = PathBuf::from(&backup.path);
    if !archive.is_file() {
        return Err(Error::NotFound(
            "The backup archive is missing from disk.".into(),
        ));
    }
    if let Some(expected) = &backup.checksum {
        if !sha256_file(&archive).await?.eq_ignore_ascii_case(expected) {
            return Err(Error::Archive(
                "The backup checksum is invalid. Restore was cancelled.".into(),
            ));
        }
    }
    progress(
        channel,
        &operation_id,
        "safety",
        5.0,
        "Creating a safety backup",
    );
    create_backup(
        state.clone(),
        &server.id,
        "safety",
        Some("Before restore".into()),
        channel,
    )
    .await?;
    let staging = PathBuf::from(&server.folder)
        .parent()
        .unwrap_or(Path::new(&server.folder))
        .join(format!(".nooki-restore-{operation_id}"));
    let rollback = PathBuf::from(&server.folder)
        .parent()
        .unwrap_or(Path::new(&server.folder))
        .join(format!(".nooki-rollback-{operation_id}"));
    let archive_for_task = archive.clone();
    let staging_for_task = staging.clone();
    let extract_operation = operation_id.clone();
    progress(
        channel,
        &operation_id,
        "extract",
        35.0,
        "Validating and extracting the backup",
    );
    let manifest = tokio::task::spawn_blocking(move || {
        extract_backup(&archive_for_task, &staging_for_task, &extract_operation)
    })
    .await
    .map_err(|error| Error::Internal(error.to_string()))??;
    if manifest.server_id != server.id {
        let _ = tokio::fs::remove_dir_all(&staging).await;
        return Err(Error::Validation(
            "This backup belongs to a different server.".into(),
        ));
    }
    progress(
        channel,
        &operation_id,
        "replace",
        70.0,
        "Replacing server data",
    );
    let server_folder = PathBuf::from(&server.folder);
    let mut roots = manifest.roots.clone();
    for current in backup_roots(&server_folder).await? {
        if !roots.contains(&current) {
            roots.push(current);
        }
    }
    let roots_for_replace = roots.clone();
    let replace_operation = operation_id.clone();
    let replace_result = tokio::task::spawn_blocking({
        let server_folder = server_folder.clone();
        let staging = staging.clone();
        let rollback = rollback.clone();
        move || {
            replace_roots(
                &server_folder,
                &staging,
                &rollback,
                &roots_for_replace,
                &replace_operation,
            )
        }
    })
    .await
    .map_err(|error| Error::Internal(error.to_string()))?;
    if let Err(error) = replace_result {
        let _ = tokio::task::spawn_blocking({
            let server_folder = server_folder.clone();
            let rollback = rollback.clone();
            move || rollback_roots(&server_folder, &rollback, &roots)
        })
        .await;
        return Err(error);
    }
    let _ = tokio::fs::remove_dir_all(&staging).await;
    let _ = tokio::fs::remove_dir_all(&rollback).await;
    state.refresh_roster(&server.id).await?;
    let mut refreshed = server.clone();
    refreshed.disk_used = crate::state::directory_size(Path::new(&server.folder)).await;
    state.save_server(refreshed).await?;
    state
        .activity("restore", Some(&server), "Backup restored successfully")
        .await?;
    progress(channel, &operation_id, "done", 100.0, "Restore finished");
    Ok(())
}

pub async fn delete_backup(state: Arc<AppState>, backup_id: &str) -> Result<()> {
    let backup = state
        .backups
        .read()
        .await
        .get(backup_id)
        .cloned()
        .ok_or_else(|| Error::NotFound("That backup no longer exists.".into()))?;
    match tokio::fs::remove_file(&backup.path).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    state.db.delete_backup(backup_id).await?;
    state.backups.write().await.remove(backup_id);
    state.emit(AppEvent::BackupRemoved {
        backup_id: backup_id.into(),
    });
    Ok(())
}

async fn apply_retention(state: Arc<AppState>, server_id: &str) -> Result<()> {
    let schedule = state
        .schedules
        .read()
        .await
        .get(server_id)
        .cloned()
        .unwrap_or_default();
    let mut scheduled = state
        .backups
        .read()
        .await
        .values()
        .filter(|backup| {
            backup.server_id == server_id
                && backup.backup_type == "scheduled"
                && backup.failed != Some(true)
        })
        .cloned()
        .collect::<Vec<_>>();
    scheduled.sort_by_key(|backup| std::cmp::Reverse(backup.created_at));
    for backup in scheduled.into_iter().skip(schedule.keep as usize) {
        delete_backup(state.clone(), &backup.id).await?;
    }
    Ok(())
}

async fn backup_roots(folder: &Path) -> Result<Vec<String>> {
    let properties = PropertiesFile::read(&folder.join("server.properties")).await?;
    let level_name = properties.get("level-name").unwrap_or("world").to_owned();
    let candidates = [
        level_name.clone(),
        format!("{level_name}_nether"),
        format!("{level_name}_the_end"),
        "server.properties".into(),
        "eula.txt".into(),
        "whitelist.json".into(),
        "ops.json".into(),
        "banned-players.json".into(),
        "banned-ips.json".into(),
        "plugins".into(),
        "mods".into(),
        "config".into(),
        "bukkit.yml".into(),
        "spigot.yml".into(),
        "paper.yml".into(),
        "paper-global.yml".into(),
        "paper-world-defaults.yml".into(),
    ];
    Ok(candidates
        .into_iter()
        .filter(|name| folder.join(name).exists())
        .collect())
}

fn copy_roots(
    source: &Path,
    destination: &Path,
    roots: &[String],
    operation_id: &str,
) -> Result<()> {
    for root in roots {
        crate::operations::check(operation_id)?;
        let from = source.join(root);
        let to = destination.join(root);
        copy_path(&from, &to, Some(operation_id))?;
    }
    Ok(())
}

fn copy_path(source: &Path, destination: &Path, operation_id: Option<&str>) -> Result<()> {
    if source.is_dir() {
        for entry in walkdir::WalkDir::new(source) {
            if let Some(operation_id) = operation_id {
                crate::operations::check(operation_id)?;
            }
            let entry = entry.map_err(|error| Error::Io(error.into()))?;
            let relative = entry
                .path()
                .strip_prefix(source)
                .map_err(|error| Error::Internal(error.to_string()))?;
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
    } else if source.is_file() {
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(source, destination)?;
    }
    Ok(())
}

fn zip_folder(source: &Path, destination: &Path, operation_id: &str) -> Result<()> {
    let file = File::create(destination)?;
    let mut archive = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for entry in walkdir::WalkDir::new(source).into_iter().flatten() {
        crate::operations::check(operation_id)?;
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(|error| Error::Internal(error.to_string()))?;
        if relative.as_os_str().is_empty() {
            continue;
        }
        let name = relative.to_string_lossy().replace('\\', "/");
        if entry.file_type().is_dir() {
            archive.add_directory(format!("{name}/"), options)?;
        } else {
            archive.start_file(name, options)?;
            let mut input = File::open(entry.path())?;
            std::io::copy(&mut input, &mut archive)?;
        }
    }
    archive.finish()?;
    Ok(())
}

fn extract_backup(
    archive_path: &Path,
    destination: &Path,
    operation_id: &str,
) -> Result<BackupManifest> {
    if destination.exists() {
        std::fs::remove_dir_all(destination)?;
    }
    std::fs::create_dir_all(destination)?;
    let mut archive = zip::ZipArchive::new(File::open(archive_path)?)?;
    for index in 0..archive.len() {
        crate::operations::check(operation_id)?;
        let mut entry = archive.by_index(index)?;
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| Error::Archive("The backup contains an unsafe path.".into()))?
            .to_path_buf();
        let target = destination.join(relative);
        if entry.is_dir() {
            std::fs::create_dir_all(target)?;
        } else {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut output = File::create(target)?;
            std::io::copy(&mut entry, &mut output)?;
        }
    }
    let mut manifest_text = String::new();
    File::open(destination.join(".nooki-backup.json"))?.read_to_string(&mut manifest_text)?;
    Ok(serde_json::from_str(&manifest_text)?)
}

fn replace_roots(
    server: &Path,
    staging: &Path,
    rollback: &Path,
    roots: &[String],
    operation_id: &str,
) -> Result<()> {
    std::fs::create_dir_all(rollback)?;
    for root in roots {
        crate::operations::check(operation_id)?;
        let current = server.join(root);
        if current.exists() {
            std::fs::rename(&current, rollback.join(root))?;
        }
        let replacement = staging.join(root);
        if replacement.exists() {
            copy_path(&replacement, &current, Some(operation_id))?;
        }
    }
    Ok(())
}

fn rollback_roots(server: &Path, rollback: &Path, roots: &[String]) -> Result<()> {
    for root in roots {
        let target = server.join(root);
        if target.is_dir() {
            std::fs::remove_dir_all(target)?;
        } else if target.is_file() {
            std::fs::remove_file(target)?;
        }
    }
    if !rollback.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(rollback)? {
        let entry = entry?;
        let target = server.join(entry.file_name());
        if target.exists() {
            if target.is_dir() {
                std::fs::remove_dir_all(&target)?;
            } else {
                std::fs::remove_file(&target)?;
            }
        }
        std::fs::rename(entry.path(), target)?;
    }
    Ok(())
}

async fn sha256_file(path: &Path) -> Result<String> {
    let data = tokio::fs::read(path).await?;
    Ok(hex::encode(Sha256::digest(data)))
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

fn sanitize_filename(value: &str) -> String {
    let value = value
        .chars()
        .map(|character| {
            if r#"<>:"/\|?*"#.contains(character) {
                '_'
            } else {
                character
            }
        })
        .collect::<String>();
    value.trim_matches([' ', '.']).to_owned()
}

fn title_case(value: &str) -> String {
    let mut characters = value.chars();
    characters
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + characters.as_str())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn selects_recoverable_data_and_excludes_runtime_files() {
        let temporary = tempfile::tempdir().unwrap();
        let folder = temporary.path();
        tokio::fs::write(folder.join("server.properties"), "level-name=home\n")
            .await
            .unwrap();
        tokio::fs::create_dir(folder.join("home")).await.unwrap();
        tokio::fs::create_dir(folder.join("plugins")).await.unwrap();
        tokio::fs::create_dir(folder.join("mods")).await.unwrap();
        tokio::fs::create_dir(folder.join("logs")).await.unwrap();
        tokio::fs::write(folder.join("server.jar"), b"jar")
            .await
            .unwrap();
        let roots = backup_roots(folder).await.unwrap();
        assert!(roots.contains(&"home".to_string()));
        assert!(roots.contains(&"plugins".to_string()));
        assert!(roots.contains(&"mods".to_string()));
        assert!(!roots.contains(&"logs".to_string()));
        assert!(!roots.contains(&"server.jar".to_string()));
    }

    #[test]
    fn rollback_removes_replacements_and_restores_original_roots() {
        let temporary = tempfile::tempdir().unwrap();
        let server = temporary.path().join("server");
        let rollback = temporary.path().join("rollback");
        std::fs::create_dir_all(server.join("world")).unwrap();
        std::fs::create_dir_all(rollback.join("world")).unwrap();
        std::fs::write(server.join("world").join("value.txt"), "new").unwrap();
        std::fs::write(rollback.join("world").join("value.txt"), "old").unwrap();
        rollback_roots(&server, &rollback, &["world".into()]).unwrap();
        assert_eq!(
            std::fs::read_to_string(server.join("world").join("value.txt")).unwrap(),
            "old"
        );
    }
}
