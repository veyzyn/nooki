use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
    sync::Arc,
    time::UNIX_EPOCH,
};

use serde::Serialize;
use tauri::State;
use uuid::Uuid;

use crate::{
    error::{AppError, CommandResult, Error, Result},
    recycle,
    state::AppState,
};

const MAX_EDITABLE_BYTES: u64 = 4 * 1024 * 1024;

type SharedState<'a> = State<'a, Arc<AppState>>;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerFileEntry {
    pub name: String,
    pub path: String,
    pub kind: &'static str,
    pub size: u64,
    pub modified_at: i64,
    pub editable: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerFileListing {
    pub path: String,
    pub entries: Vec<ServerFileEntry>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerTextFile {
    pub path: String,
    pub content: String,
    pub language: String,
    pub size: u64,
    pub modified_at: i64,
}

#[tauri::command]
pub async fn list_server_files(
    state: SharedState<'_>,
    server_id: String,
    path: String,
) -> CommandResult<ServerFileListing> {
    let server = state.server(&server_id).await.map_err(AppError::from)?;
    tokio::task::spawn_blocking(move || list_files(Path::new(&server.folder), &path))
        .await
        .map_err(|error| AppError::from(Error::Internal(error.to_string())))?
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn read_server_text_file(
    state: SharedState<'_>,
    server_id: String,
    path: String,
) -> CommandResult<ServerTextFile> {
    let server = state.server(&server_id).await.map_err(AppError::from)?;
    tokio::task::spawn_blocking(move || read_text_file(Path::new(&server.folder), &path))
        .await
        .map_err(|error| AppError::from(Error::Internal(error.to_string())))?
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn save_server_text_file(
    state: SharedState<'_>,
    server_id: String,
    path: String,
    content: String,
) -> CommandResult<ServerTextFile> {
    let server = state.server(&server_id).await.map_err(AppError::from)?;
    let lock = state.operation_lock(&server_id);
    let _guard = lock.lock().await;
    tokio::task::spawn_blocking(move || save_text_file(Path::new(&server.folder), &path, &content))
        .await
        .map_err(|error| AppError::from(Error::Internal(error.to_string())))?
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn create_server_file(
    state: SharedState<'_>,
    server_id: String,
    parent_path: String,
    name: String,
) -> CommandResult<()> {
    let server = state.server(&server_id).await.map_err(AppError::from)?;
    let lock = state.operation_lock(&server_id);
    let _guard = lock.lock().await;
    tokio::task::spawn_blocking(move || {
        let parent = resolve_existing(Path::new(&server.folder), &parent_path, true)?;
        let target = child_target(&parent, &name)?;
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target)?;
        Ok::<(), Error>(())
    })
    .await
    .map_err(|error| AppError::from(Error::Internal(error.to_string())))?
    .map_err(AppError::from)
}

#[tauri::command]
pub async fn create_server_folder(
    state: SharedState<'_>,
    server_id: String,
    parent_path: String,
    name: String,
) -> CommandResult<()> {
    let server = state.server(&server_id).await.map_err(AppError::from)?;
    let lock = state.operation_lock(&server_id);
    let _guard = lock.lock().await;
    tokio::task::spawn_blocking(move || {
        let parent = resolve_existing(Path::new(&server.folder), &parent_path, true)?;
        fs::create_dir(child_target(&parent, &name)?)?;
        Ok::<(), Error>(())
    })
    .await
    .map_err(|error| AppError::from(Error::Internal(error.to_string())))?
    .map_err(AppError::from)
}

#[tauri::command]
pub async fn rename_server_file(
    state: SharedState<'_>,
    server_id: String,
    path: String,
    name: String,
) -> CommandResult<()> {
    let server = state.server(&server_id).await.map_err(AppError::from)?;
    let lock = state.operation_lock(&server_id);
    let _guard = lock.lock().await;
    tokio::task::spawn_blocking(move || {
        let root = canonical_root(Path::new(&server.folder))?;
        let source = resolve_from_root(&root, &path, false)?;
        if source == root {
            return Err(Error::Validation(
                "The server root cannot be renamed.".into(),
            ));
        }
        let parent = source
            .parent()
            .ok_or_else(|| Error::Validation("This item cannot be renamed.".into()))?;
        let target = child_target(parent, &name)?;
        if target.exists() {
            return Err(Error::Conflict(
                "An item with that name already exists.".into(),
            ));
        }
        fs::rename(source, target)?;
        Ok(())
    })
    .await
    .map_err(|error| AppError::from(Error::Internal(error.to_string())))?
    .map_err(AppError::from)
}

#[tauri::command]
pub async fn delete_server_file(
    state: SharedState<'_>,
    server_id: String,
    path: String,
) -> CommandResult<()> {
    let server = state.server(&server_id).await.map_err(AppError::from)?;
    let lock = state.operation_lock(&server_id);
    let _guard = lock.lock().await;
    tokio::task::spawn_blocking(move || {
        let root = canonical_root(Path::new(&server.folder))?;
        let target = resolve_from_root(&root, &path, false)?;
        if target == root {
            return Err(Error::Validation(
                "The server root cannot be deleted.".into(),
            ));
        }
        recycle::move_to_recycle_bin(&target)
    })
    .await
    .map_err(|error| AppError::from(Error::Internal(error.to_string())))?
    .map_err(AppError::from)
}

fn list_files(root: &Path, relative: &str) -> Result<ServerFileListing> {
    let root = canonical_root(root)?;
    let directory = resolve_from_root(&root, relative, true)?;
    let normalized = relative_string(&root, &directory)?;
    let mut entries = Vec::new();
    for entry in fs::read_dir(&directory)? {
        let entry = entry?;
        let metadata = entry.path().symlink_metadata()?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        let is_directory = metadata.is_dir();
        if !is_directory && !metadata.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = relative_string(&root, &entry.path())?;
        entries.push(ServerFileEntry {
            editable: !is_directory && is_editable_path(&entry.path(), metadata.len()),
            name,
            path,
            kind: if is_directory { "directory" } else { "file" },
            size: if is_directory { 0 } else { metadata.len() },
            modified_at: modified_ms(&metadata),
        });
    }
    entries.sort_by(|left, right| {
        (left.kind != "directory")
            .cmp(&(right.kind != "directory"))
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    Ok(ServerFileListing {
        path: normalized,
        entries,
    })
}

fn read_text_file(root: &Path, relative: &str) -> Result<ServerTextFile> {
    let root = canonical_root(root)?;
    let target = resolve_from_root(&root, relative, false)?;
    let metadata = fs::metadata(&target)?;
    if !metadata.is_file() {
        return Err(Error::Validation(
            "Only text files can be opened in the editor.".into(),
        ));
    }
    if metadata.len() > MAX_EDITABLE_BYTES {
        return Err(Error::Unsupported(
            "This file is too large to edit in Nooki.".into(),
        ));
    }
    let bytes = fs::read(&target)?;
    if bytes.iter().take(8192).any(|byte| *byte == 0) {
        return Err(Error::Unsupported(
            "This appears to be a binary file.".into(),
        ));
    }
    let content = String::from_utf8(bytes)
        .map_err(|_| Error::Unsupported("This file is not valid UTF-8 text.".into()))?;
    Ok(ServerTextFile {
        path: relative_string(&root, &target)?,
        language: editor_language(&target).into(),
        size: metadata.len(),
        modified_at: modified_ms(&metadata),
        content,
    })
}

fn save_text_file(root: &Path, relative: &str, content: &str) -> Result<ServerTextFile> {
    if content.len() as u64 > MAX_EDITABLE_BYTES {
        return Err(Error::Validation(
            "This file is too large to save in Nooki.".into(),
        ));
    }
    let root = canonical_root(root)?;
    let target = resolve_from_root(&root, relative, false)?;
    if !target.is_file() {
        return Err(Error::Validation(
            "Only text files can be saved in the editor.".into(),
        ));
    }
    let parent = target
        .parent()
        .ok_or_else(|| Error::Validation("This file cannot be saved.".into()))?;
    let temporary = parent.join(format!(".nooki-save-{}.tmp", Uuid::new_v4()));
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    if let Err(error) = (|| -> std::io::Result<()> {
        output.write_all(content.as_bytes())?;
        output.sync_all()
    })() {
        let _ = fs::remove_file(&temporary);
        return Err(error.into());
    }
    drop(output);
    if let Err(error) = replace_file(&target, &temporary) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    read_text_file(&root, relative)
}

fn replace_file(target: &Path, temporary: &Path) -> Result<()> {
    let backup = target.with_file_name(format!(".nooki-save-{}.bak", Uuid::new_v4()));
    fs::rename(target, &backup)?;
    if let Err(error) = fs::rename(temporary, target) {
        let _ = fs::rename(&backup, target);
        return Err(error.into());
    }
    let _ = fs::remove_file(backup);
    Ok(())
}

fn canonical_root(root: &Path) -> Result<PathBuf> {
    let root = fs::canonicalize(root)?;
    if !root.is_dir() {
        return Err(Error::NotFound(
            "The registered server folder is missing.".into(),
        ));
    }
    Ok(root)
}

fn resolve_existing(root: &Path, relative: &str, directory: bool) -> Result<PathBuf> {
    let root = canonical_root(root)?;
    resolve_from_root(&root, relative, directory)
}

fn resolve_from_root(root: &Path, relative: &str, directory: bool) -> Result<PathBuf> {
    let relative = safe_relative(relative)?;
    let unresolved = root.join(relative);
    if fs::symlink_metadata(&unresolved)?.file_type().is_symlink() {
        return Err(Error::Unsupported(
            "Symbolic links are not managed by Nooki's file manager.".into(),
        ));
    }
    let target = fs::canonicalize(unresolved)?;
    if !target.starts_with(root) {
        return Err(Error::Validation(
            "That path is outside the server folder.".into(),
        ));
    }
    if directory && !target.is_dir() {
        return Err(Error::Validation("That path is not a folder.".into()));
    }
    Ok(target)
}

fn safe_relative(value: &str) -> Result<PathBuf> {
    let normalized = value.replace('/', "\\");
    let path = Path::new(&normalized);
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir if normalized.is_empty() => {}
            _ => {
                return Err(Error::Validation(
                    "That path is not inside the server folder.".into(),
                ))
            }
        }
    }
    Ok(path.to_path_buf())
}

fn child_target(parent: &Path, name: &str) -> Result<PathBuf> {
    let trimmed = name.trim();
    if trimmed.is_empty()
        || trimmed == "."
        || trimmed == ".."
        || trimmed.contains(['/', '\\'])
        || trimmed.chars().any(|character| character < ' ')
        || trimmed.ends_with(['.', ' '])
    {
        return Err(Error::Validation(
            "Enter a valid file or folder name.".into(),
        ));
    }
    #[cfg(windows)]
    if trimmed.contains(['<', '>', ':', '"', '|', '?', '*']) {
        return Err(Error::Validation(
            "That name contains characters Windows does not allow.".into(),
        ));
    }
    Ok(parent.join(trimmed))
}

fn relative_string(root: &Path, path: &Path) -> Result<String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| Error::Validation("That path is outside the server folder.".into()))?;
    Ok(relative
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/"))
}

fn modified_ms(metadata: &fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

fn is_editable_path(path: &Path, size: u64) -> bool {
    size <= MAX_EDITABLE_BYTES
        && matches!(
            path.extension()
                .and_then(|extension| extension.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase()
                .as_str(),
            "txt"
                | "properties"
                | "yml"
                | "yaml"
                | "json"
                | "json5"
                | "toml"
                | "cfg"
                | "conf"
                | "ini"
                | "xml"
                | "md"
                | "log"
                | "mcmeta"
                | "accesswidener"
                | "js"
                | "ts"
                | "css"
                | "html"
                | "sh"
                | "bat"
                | "cmd"
        )
}

fn editor_language(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "json" | "json5" | "mcmeta" => "json",
        "yml" | "yaml" => "yaml",
        "toml" => "ini",
        "properties" | "cfg" | "conf" | "ini" => "ini",
        "xml" => "xml",
        "md" => "markdown",
        "js" => "javascript",
        "ts" => "typescript",
        "css" => "css",
        "html" => "html",
        "sh" => "shell",
        "bat" | "cmd" => "bat",
        _ => "plaintext",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_parent_and_absolute_paths() {
        assert!(safe_relative("../outside").is_err());
        assert!(safe_relative(r"C:\outside").is_err());
        assert!(safe_relative("folder/file.txt").is_ok());
        assert!(safe_relative("").is_ok());
    }

    #[test]
    fn refuses_unsafe_child_names() {
        assert!(child_target(Path::new("server"), "../world").is_err());
        assert!(child_target(Path::new("server"), "world/").is_err());
        assert!(child_target(Path::new("server"), "server.properties").is_ok());
    }

    #[test]
    fn lists_reads_and_saves_text_files() {
        let temporary = tempfile::tempdir().unwrap();
        fs::create_dir(temporary.path().join("config")).unwrap();
        fs::write(temporary.path().join("server.properties"), b"motd=Hello\n").unwrap();
        fs::write(temporary.path().join("server.jar"), [0, 1, 2, 3]).unwrap();

        let listing = list_files(temporary.path(), "").unwrap();
        assert_eq!(listing.entries[0].name, "config");
        assert!(
            listing
                .entries
                .iter()
                .find(|entry| entry.name == "server.properties")
                .unwrap()
                .editable
        );
        assert!(
            !listing
                .entries
                .iter()
                .find(|entry| entry.name == "server.jar")
                .unwrap()
                .editable
        );

        let opened = read_text_file(temporary.path(), "server.properties").unwrap();
        assert_eq!(opened.language, "ini");
        assert_eq!(opened.content, "motd=Hello\n");

        let saved = save_text_file(temporary.path(), "server.properties", "motd=Nooki\n").unwrap();
        assert_eq!(saved.content, "motd=Nooki\n");
        assert_eq!(
            fs::read_to_string(temporary.path().join("server.properties")).unwrap(),
            "motd=Nooki\n"
        );
    }
}
