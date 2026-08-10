use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    process::Stdio,
};

use futures_util::StreamExt;
use regex::Regex;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tauri::ipc::Channel;
use tokio::io::AsyncWriteExt;

use crate::paths::{normalize_path, path_string};
use crate::{
    error::{Error, Result},
    models::{JavaRuntime, OperationEvent},
};

pub async fn detect_runtimes(
    managed_root: &Path,
    existing: &[JavaRuntime],
) -> Result<Vec<JavaRuntime>> {
    let mut candidates = Vec::new();
    if let Ok(home) = std::env::var("JAVA_HOME") {
        candidates.push(PathBuf::from(home).join("bin").join("java.exe"));
    }
    if let Ok(paths) = which::which_all("java") {
        candidates.extend(paths);
    }
    for base in [
        std::env::var_os("ProgramFiles").map(PathBuf::from),
        std::env::var_os("ProgramFiles(x86)").map(PathBuf::from),
        Some(managed_root.to_path_buf()),
    ]
    .into_iter()
    .flatten()
    {
        for vendor in [
            "Eclipse Adoptium",
            "Java",
            "Microsoft",
            "Amazon Corretto",
            "Zulu",
            "",
        ] {
            let root = base.join(vendor);
            if !root.exists() {
                continue;
            }
            for entry in walkdir::WalkDir::new(root)
                .max_depth(4)
                .into_iter()
                .flatten()
            {
                if entry.file_type().is_file()
                    && entry
                        .file_name()
                        .to_string_lossy()
                        .eq_ignore_ascii_case("java.exe")
                {
                    candidates.push(entry.path().to_path_buf());
                }
            }
        }
    }

    let mut seen = HashSet::new();
    let mut runtimes = Vec::new();
    for path in candidates {
        let canonical = normalize_path(std::fs::canonicalize(&path).unwrap_or(path));
        let key = canonical.to_string_lossy().to_lowercase();
        if !seen.insert(key) {
            continue;
        }
        if let Ok((version, major, architecture)) = inspect_java(&canonical).await {
            if architecture != "x64" {
                continue;
            }
            let bundled = canonical.starts_with(managed_root);
            let existing_runtime = existing
                .iter()
                .find(|runtime| Path::new(&runtime.path) == canonical);
            runtimes.push(JavaRuntime {
                id: existing_runtime
                    .map(|runtime| runtime.id.clone())
                    .unwrap_or_else(|| format!("java-{}-{}", major, uuid::Uuid::new_v4())),
                label: format!("Java {major}"),
                version,
                major,
                path: path_string(&canonical),
                bundled,
                used_by: existing_runtime.map(|runtime| runtime.used_by).unwrap_or(0),
                architecture,
            });
        }
    }
    runtimes.sort_by(|a, b| b.major.cmp(&a.major).then_with(|| a.path.cmp(&b.path)));
    Ok(runtimes)
}

pub async fn inspect_java(path: &Path) -> Result<(String, u32, String)> {
    let output = tokio::process::Command::new(path)
        .arg("-version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await?;
    if !output.status.success() {
        return Err(Error::Validation(format!(
            "{} could not be started.",
            path.display()
        )));
    }
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let regex = Regex::new(r#"version\s+\"([^\"]+)\""#)
        .map_err(|error| Error::Internal(error.to_string()))?;
    let version = regex
        .captures(&text)
        .and_then(|capture| capture.get(1))
        .map(|value| value.as_str().to_owned())
        .ok_or_else(|| {
            Error::Validation(format!(
                "Nooki could not determine the Java version at {}.",
                path.display()
            ))
        })?;
    let major = if version.starts_with("1.") {
        version
            .split('.')
            .nth(1)
            .and_then(|value| value.parse().ok())
            .unwrap_or(8)
    } else {
        version
            .split(['.', '+', '-'])
            .next()
            .and_then(|value| value.parse().ok())
            .unwrap_or(0)
    };
    let lower = text.to_ascii_lowercase();
    let architecture =
        if text.contains("64-Bit") || lower.contains("amd64") || lower.contains("x86_64") {
            "x64"
        } else if text.contains("32-Bit") || lower.contains("x86") {
            "x86"
        } else {
            "unknown"
        }
        .to_owned();
    Ok((version, major, architecture))
}

pub async fn install_temurin(
    major: u32,
    managed_root: &Path,
    channel: Option<&Channel<OperationEvent>>,
    operation_id: &str,
    progress_range: (f32, f32),
) -> Result<JavaRuntime> {
    let client = reqwest::Client::builder()
        .user_agent(format!("Nooki/{}", env!("CARGO_PKG_VERSION")))
        .build()?;
    let url = format!(
        "https://api.adoptium.net/v3/assets/latest/{major}/hotspot?architecture=x64&heap_size=normal&image_type=jre&jvm_impl=hotspot&os=windows&vendor=eclipse"
    );
    let assets: Value = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let package = assets
        .as_array()
        .and_then(|items| items.first())
        .map(|item| &item["binary"]["package"])
        .ok_or_else(|| {
            Error::NotFound(format!(
                "No Windows x64 Temurin Java {major} runtime is available."
            ))
        })?;
    let download_url = package["link"]
        .as_str()
        .ok_or_else(|| Error::Internal("Adoptium did not provide a runtime URL.".into()))?;
    let expected = package["checksum"].as_str().map(str::to_owned);
    tokio::fs::create_dir_all(managed_root).await?;
    let archive_path = managed_root.join(format!("java-{major}-{}.zip", uuid::Uuid::new_v4()));
    let response = client.get(download_url).send().await?.error_for_status()?;
    let total = response.content_length().unwrap_or(0);
    let mut stream = response.bytes_stream();
    let mut file = tokio::fs::File::create(&archive_path).await?;
    let mut received = 0u64;
    while let Some(chunk) = stream.next().await {
        if let Err(error) = crate::operations::check(operation_id) {
            drop(file);
            let _ = tokio::fs::remove_file(&archive_path).await;
            return Err(error);
        }
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        received += chunk.len() as u64;
        if let Some(channel) = channel {
            let transfer_progress = if total == 0 {
                0.0
            } else {
                received as f32 / total as f32 * 100.0
            };
            let progress = progress_range.0
                + (progress_range.1 - progress_range.0) * transfer_progress / 100.0;
            let _ = channel.send(OperationEvent::Progress {
                operation_id: operation_id.into(),
                phase: "java".into(),
                progress,
                message: if total == 0 {
                    format!(
                        "Downloading Java {major} · {} downloaded",
                        human_bytes(received)
                    )
                } else {
                    format!(
                        "Downloading Java {major} · {transfer_progress:.0}% · {} of {}",
                        human_bytes(received),
                        human_bytes(total)
                    )
                },
            });
        }
    }
    file.flush().await?;
    drop(file);
    if let Err(error) = crate::operations::check(operation_id) {
        let _ = tokio::fs::remove_file(&archive_path).await;
        return Err(error);
    }
    if let Some(expected) = expected {
        let bytes = tokio::fs::read(&archive_path).await?;
        let actual = hex::encode(Sha256::digest(bytes));
        if !actual.eq_ignore_ascii_case(&expected) {
            let _ = tokio::fs::remove_file(&archive_path).await;
            return Err(Error::Validation(
                "The Java runtime did not match Adoptium's checksum.".into(),
            ));
        }
    }
    let destination = managed_root.join(format!("java-{major}-{}", uuid::Uuid::new_v4()));
    let archive_for_task = archive_path.clone();
    let destination_for_task = destination.clone();
    let extract_operation = operation_id.to_owned();
    let extract_result = tokio::task::spawn_blocking(move || {
        extract_zip(&archive_for_task, &destination_for_task, &extract_operation)
    })
    .await
    .map_err(|error| Error::Internal(error.to_string()))?;
    if let Err(error) = extract_result {
        let _ = tokio::fs::remove_file(&archive_path).await;
        let _ = tokio::fs::remove_dir_all(&destination).await;
        return Err(error);
    }
    if let Err(error) = crate::operations::check(operation_id) {
        let _ = tokio::fs::remove_file(&archive_path).await;
        let _ = tokio::fs::remove_dir_all(&destination).await;
        return Err(error);
    }
    let _ = tokio::fs::remove_file(&archive_path).await;
    let java_path = walkdir::WalkDir::new(&destination)
        .max_depth(5)
        .into_iter()
        .flatten()
        .find(|entry| {
            entry.file_type().is_file()
                && entry
                    .file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case("java.exe")
        })
        .map(|entry| entry.into_path())
        .ok_or_else(|| Error::Archive("The Java archive did not contain java.exe.".into()))?;
    let (version, actual_major, architecture) = inspect_java(&java_path).await?;
    if actual_major != major || architecture != "x64" {
        let _ = tokio::fs::remove_dir_all(&destination).await;
        return Err(Error::Validation(format!("The downloaded runtime was Java {actual_major} ({architecture}), but Java {major} x64 was required.")));
    }
    Ok(JavaRuntime {
        id: format!("managed-java-{actual_major}-{}", uuid::Uuid::new_v4()),
        label: format!("Java {actual_major}"),
        version,
        major: actual_major,
        path: path_string(&java_path),
        bundled: true,
        used_by: 0,
        architecture,
    })
}

fn human_bytes(bytes: u64) -> String {
    const MIB: f64 = 1024.0 * 1024.0;
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / MIB)
    } else if bytes >= 1024 {
        format!("{:.0} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

fn extract_zip(archive: &Path, destination: &Path, operation_id: &str) -> Result<()> {
    std::fs::create_dir_all(destination)?;
    let file = std::fs::File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file)?;
    for index in 0..zip.len() {
        crate::operations::check(operation_id)?;
        let mut entry = zip.by_index(index)?;
        let Some(relative) = entry.enclosed_name() else {
            continue;
        };
        let target = destination.join(relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&target)?;
        } else {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut output = std::fs::File::create(target)?;
            std::io::copy(&mut entry, &mut output)?;
        }
    }
    Ok(())
}
