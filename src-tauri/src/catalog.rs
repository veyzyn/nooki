use std::{collections::HashMap, path::Path, sync::Arc};

use futures_util::StreamExt;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tauri::ipc::Channel;
use tokio::sync::RwLock;

use crate::{
    error::{Error, Result},
    models::{now_ms, OperationEvent, ServerType, VersionCatalog, VersionOption},
};

const MOJANG_MANIFEST: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const PAPER_BASE: &str = "https://fill.papermc.io/v3";
const FORGE_PROMOTIONS: &str =
    "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json";
const FORGE_MAVEN: &str = "https://maven.minecraftforge.net/net/minecraftforge/forge";
const NEOFORGE_MAVEN: &str = "https://maven.neoforged.net/releases/net/neoforged";
const FABRIC_META: &str = "https://meta.fabricmc.net/v2/versions";

#[derive(Debug, Clone)]
pub struct ResolvedDownload {
    pub url: String,
    pub checksum: Option<String>,
    pub checksum_kind: ChecksumKind,
    pub version: String,
    pub build: String,
    pub java_major: u32,
}

#[derive(Debug, Clone, Copy)]
pub enum ChecksumKind {
    Sha1,
    Sha256,
}

#[derive(Clone)]
pub struct CatalogClient {
    client: reqwest::Client,
    paper_enabled: bool,
    vanilla_release_cache: Arc<RwLock<Option<Vec<VersionOption>>>>,
    vanilla_full_cache: Arc<RwLock<Option<Vec<VersionOption>>>>,
    paper_cache: Arc<RwLock<Option<Vec<VersionOption>>>>,
    forge_cache: Arc<RwLock<Option<Vec<VersionOption>>>>,
    neoforge_cache: Arc<RwLock<Option<Vec<VersionOption>>>>,
    fabric_cache: Arc<RwLock<Option<Vec<VersionOption>>>>,
}

impl CatalogClient {
    pub fn new() -> Result<Self> {
        let contact = option_env!("NOOKI_CONTACT_URL")
            .filter(|value| value.starts_with("https://") || value.contains('@'));
        let user_agent = contact.map_or_else(
            || format!("Nooki/{}", env!("CARGO_PKG_VERSION")),
            |contact| format!("Nooki/{} ({contact})", env!("CARGO_PKG_VERSION")),
        );
        let client = reqwest::Client::builder().user_agent(user_agent).build()?;
        Ok(Self {
            client,
            paper_enabled: contact.is_some(),
            vanilla_release_cache: Arc::new(RwLock::new(None)),
            vanilla_full_cache: Arc::new(RwLock::new(None)),
            paper_cache: Arc::new(RwLock::new(None)),
            forge_cache: Arc::new(RwLock::new(None)),
            neoforge_cache: Arc::new(RwLock::new(None)),
            fabric_cache: Arc::new(RwLock::new(None)),
        })
    }

    pub async fn list_versions(
        &self,
        server_type: ServerType,
        include_experimental: bool,
    ) -> Result<VersionCatalog> {
        let versions = match server_type {
            ServerType::Vanilla => self.list_vanilla(include_experimental).await?,
            ServerType::Paper => self.list_paper(include_experimental).await?,
            ServerType::Forge => self.list_forge(include_experimental).await?,
            ServerType::NeoForge => self.list_neoforge(include_experimental).await?,
            ServerType::Fabric => self.list_fabric(include_experimental).await?,
        };
        Ok(VersionCatalog {
            server_type,
            versions,
            fetched_at: now_ms(),
        })
    }

    async fn list_vanilla(&self, include_experimental: bool) -> Result<Vec<VersionOption>> {
        let cache = if include_experimental {
            &self.vanilla_full_cache
        } else {
            &self.vanilla_release_cache
        };
        if let Some(cached) = cache.read().await.clone() {
            return Ok(cached);
        }
        let payload: Value = self
            .client
            .get(MOJANG_MANIFEST)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let versions = payload["versions"]
            .as_array()
            .ok_or_else(|| Error::Internal("Mojang returned an invalid version catalog.".into()))?;
        let client = self.client.clone();
        let candidates =
            versions.iter().cloned().enumerate().filter(|(_, entry)| {
                include_experimental || entry["type"].as_str() == Some("release")
            });
        let mut available = futures_util::stream::iter(candidates)
            .map(move |(index, entry)| {
                let client = client.clone();
                async move {
                    let metadata_url = entry["url"].as_str()?;
                    let metadata: Value = client
                        .get(metadata_url)
                        .send()
                        .await
                        .ok()?
                        .error_for_status()
                        .ok()?
                        .json()
                        .await
                        .ok()?;
                    metadata["downloads"]["server"]["url"].as_str()?;
                    let version = entry["id"].as_str()?.to_owned();
                    let release_type = entry["type"].as_str().unwrap_or("release").to_owned();
                    Some((
                        index,
                        VersionOption {
                            id: version.clone(),
                            version,
                            build: "release".into(),
                            experimental: release_type != "release",
                            release_type,
                            java_major: metadata["javaVersion"]["majorVersion"]
                                .as_u64()
                                .map(|major| major as u32),
                            published_at: entry["releaseTime"].as_str().map(str::to_owned),
                        },
                    ))
                }
            })
            .buffer_unordered(24)
            .filter_map(|item| async move { item })
            .collect::<Vec<_>>()
            .await;
        available.sort_by_key(|(index, _)| *index);
        let available = available
            .into_iter()
            .map(|(_, version)| version)
            .collect::<Vec<_>>();
        *cache.write().await = Some(available.clone());
        Ok(available)
    }

    async fn list_paper(&self, include_experimental: bool) -> Result<Vec<VersionOption>> {
        self.require_paper_contact()?;
        if let Some(cached) = self.paper_cache.read().await.clone() {
            return Ok(cached
                .into_iter()
                .filter(|version| include_experimental || !version.experimental)
                .collect());
        }
        let payload: Value = self
            .client
            .get(format!("{PAPER_BASE}/projects/paper"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let mut names = Vec::new();
        collect_version_strings(&payload["versions"], &mut names);
        names.sort_by(|a, b| version_cmp(b, a));
        names.dedup();
        let mut result = Vec::new();
        for version in names {
            if let Ok(builds) = self.paper_builds(&version).await {
                let stable = builds
                    .iter()
                    .find(|item| item["channel"].as_str() == Some("STABLE"));
                let unstable = builds
                    .iter()
                    .find(|item| item["channel"].as_str() != Some("STABLE"));
                for build in [stable, unstable].into_iter().flatten() {
                    let channel = build["channel"].as_str().unwrap_or("EXPERIMENTAL");
                    let experimental = channel != "STABLE";
                    let build_id = build["id"]
                        .as_i64()
                        .map(|v| v.to_string())
                        .or_else(|| build["build"].as_i64().map(|v| v.to_string()))
                        .unwrap_or_else(|| "latest".into());
                    result.push(VersionOption {
                        id: format!("{version}:{build_id}"),
                        version: version.clone(),
                        build: build_id,
                        release_type: channel.to_lowercase(),
                        experimental,
                        java_major: Some(paper_java_major(&version)),
                        published_at: build["time"].as_str().map(str::to_owned),
                    });
                }
            }
        }
        *self.paper_cache.write().await = Some(result.clone());
        Ok(result
            .into_iter()
            .filter(|version| include_experimental || !version.experimental)
            .collect())
    }

    async fn paper_builds(&self, version: &str) -> Result<Vec<Value>> {
        self.require_paper_contact()?;
        let payload: Value = self
            .client
            .get(format!(
                "{PAPER_BASE}/projects/paper/versions/{}/builds",
                urlencoding::encode(version)
            ))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        payload
            .as_array()
            .cloned()
            .or_else(|| payload["builds"].as_array().cloned())
            .ok_or_else(|| Error::Internal("Paper returned an invalid build catalog.".into()))
    }

    async fn forge_promotions(&self) -> Result<HashMap<String, String>> {
        let payload: Value = self
            .client
            .get(FORGE_PROMOTIONS)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        payload["promos"]
            .as_object()
            .ok_or_else(|| Error::Internal("Forge returned an invalid version catalog.".into()))
            .map(|promos| {
                promos
                    .iter()
                    .filter_map(|(key, value)| {
                        value.as_str().map(|value| (key.clone(), value.to_owned()))
                    })
                    .collect()
            })
    }

    async fn list_forge(&self, include_experimental: bool) -> Result<Vec<VersionOption>> {
        if let Some(cached) = self.forge_cache.read().await.clone() {
            return Ok(cached
                .into_iter()
                .filter(|version| include_experimental || !version.experimental)
                .collect());
        }
        let promotions = self.forge_promotions().await?;
        let mut builds: HashMap<String, (Option<String>, Option<String>)> = HashMap::new();
        for (key, build) in promotions {
            if let Some(version) = key.strip_suffix("-recommended") {
                builds.entry(version.into()).or_default().0 = Some(build);
            } else if let Some(version) = key.strip_suffix("-latest") {
                builds.entry(version.into()).or_default().1 = Some(build);
            }
        }
        let mut names = builds.keys().cloned().collect::<Vec<_>>();
        names.sort_by(|left, right| version_cmp(right, left));
        let mut result = Vec::new();
        for version in names {
            let Some((recommended, latest)) = builds.remove(&version) else {
                continue;
            };
            let primary = recommended.as_ref().or(latest.as_ref());
            if let Some(build) = primary {
                result.push(VersionOption {
                    id: format!("{version}:{build}"),
                    version: version.clone(),
                    build: build.clone(),
                    release_type: if recommended.is_some() {
                        "recommended".into()
                    } else {
                        "latest".into()
                    },
                    experimental: false,
                    java_major: None,
                    published_at: None,
                });
            }
            if let (Some(recommended), Some(latest)) = (recommended, latest) {
                if recommended != latest {
                    result.push(VersionOption {
                        id: format!("{version}:{latest}"),
                        version: version.clone(),
                        build: latest,
                        release_type: "latest".into(),
                        experimental: true,
                        java_major: None,
                        published_at: None,
                    });
                }
            }
        }
        *self.forge_cache.write().await = Some(result.clone());
        Ok(result
            .into_iter()
            .filter(|version| include_experimental || !version.experimental)
            .collect())
    }

    async fn list_fabric(&self, include_experimental: bool) -> Result<Vec<VersionOption>> {
        if let Some(cached) = self.fabric_cache.read().await.clone() {
            return Ok(cached
                .into_iter()
                .filter(|version| include_experimental || !version.experimental)
                .collect());
        }
        let games: Vec<Value> = self
            .client
            .get(format!("{FABRIC_META}/game"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let loaders: Vec<Value> = self
            .client
            .get(format!("{FABRIC_META}/loader"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let loader = loaders
            .iter()
            .find(|loader| loader["stable"].as_bool() == Some(true))
            .or_else(|| loaders.first())
            .and_then(|loader| loader["version"].as_str())
            .ok_or_else(|| Error::Internal("Fabric returned no loader versions.".into()))?;
        let result = games
            .into_iter()
            .filter_map(|game| {
                let version = game["version"].as_str()?.to_owned();
                let stable = game["stable"].as_bool().unwrap_or(false);
                Some(VersionOption {
                    id: format!("{version}:{loader}"),
                    version,
                    build: loader.into(),
                    release_type: if stable { "release" } else { "snapshot" }.into(),
                    experimental: !stable,
                    java_major: None,
                    published_at: None,
                })
            })
            .collect::<Vec<_>>();
        *self.fabric_cache.write().await = Some(result.clone());
        Ok(result
            .into_iter()
            .filter(|version| include_experimental || !version.experimental)
            .collect())
    }

    async fn list_neoforge(&self, include_experimental: bool) -> Result<Vec<VersionOption>> {
        if let Some(cached) = self.neoforge_cache.read().await.clone() {
            return Ok(cached
                .into_iter()
                .filter(|version| include_experimental || !version.experimental)
                .collect());
        }
        let modern = self
            .client
            .get(format!("{NEOFORGE_MAVEN}/neoforge/maven-metadata.xml"))
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        let legacy = self
            .client
            .get(format!("{NEOFORGE_MAVEN}/forge/maven-metadata.xml"))
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        let mut grouped: HashMap<String, Vec<String>> = HashMap::new();
        for build in extract_maven_versions(&modern)
            .into_iter()
            .chain(extract_maven_versions(&legacy))
        {
            if let Some(minecraft) = neoforge_minecraft_version(&build) {
                grouped.entry(minecraft).or_default().push(build);
            }
        }
        let mut minecraft_versions = grouped.keys().cloned().collect::<Vec<_>>();
        minecraft_versions.sort_by(|left, right| version_cmp(right, left));
        let mut result = Vec::new();
        for minecraft in minecraft_versions {
            let Some(mut builds) = grouped.remove(&minecraft) else {
                continue;
            };
            builds.sort_by(|left, right| neoforge_build_cmp(right, left));
            let stable = builds.iter().find(|build| !is_neoforge_experimental(build));
            let experimental = builds.iter().find(|build| is_neoforge_experimental(build));
            if let Some(build) = stable {
                result.push(VersionOption {
                    id: format!("{minecraft}:{build}"),
                    version: minecraft.clone(),
                    build: build.clone(),
                    release_type: "release".into(),
                    experimental: false,
                    java_major: Some(paper_java_major(&minecraft)),
                    published_at: None,
                });
            }
            if let Some(build) = experimental {
                result.push(VersionOption {
                    id: format!("{minecraft}:{build}"),
                    version: minecraft.clone(),
                    build: build.clone(),
                    release_type: "beta".into(),
                    experimental: true,
                    java_major: Some(paper_java_major(&minecraft)),
                    published_at: None,
                });
            }
        }
        *self.neoforge_cache.write().await = Some(result.clone());
        Ok(result
            .into_iter()
            .filter(|version| include_experimental || !version.experimental)
            .collect())
    }

    pub async fn resolve(
        &self,
        server_type: ServerType,
        version: &str,
        requested_build: Option<&str>,
        allow_experimental: bool,
    ) -> Result<ResolvedDownload> {
        match server_type {
            ServerType::Vanilla => self.resolve_vanilla(version).await,
            ServerType::Paper => {
                self.resolve_paper(version, requested_build, allow_experimental)
                    .await
            }
            ServerType::Forge => {
                self.resolve_forge(version, requested_build, allow_experimental)
                    .await
            }
            ServerType::NeoForge => {
                self.resolve_neoforge(version, requested_build, allow_experimental)
                    .await
            }
            ServerType::Fabric => {
                self.resolve_fabric(version, requested_build, allow_experimental)
                    .await
            }
        }
    }

    fn require_paper_contact(&self) -> Result<()> {
        if self.paper_enabled {
            Ok(())
        } else {
            Err(Error::Unsupported("Paper downloads are disabled until NOOKI_CONTACT_URL is set to a public support URL or contact email at build time.".into()))
        }
    }

    async fn resolve_vanilla(&self, version: &str) -> Result<ResolvedDownload> {
        let manifest: Value = self
            .client
            .get(MOJANG_MANIFEST)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let metadata_url = manifest["versions"]
            .as_array()
            .and_then(|versions| {
                versions
                    .iter()
                    .find(|entry| entry["id"].as_str() == Some(version))
            })
            .and_then(|entry| entry["url"].as_str())
            .ok_or_else(|| {
                Error::NotFound(format!("Minecraft {version} is not in Mojang's catalog."))
            })?;
        let metadata: Value = self
            .client
            .get(metadata_url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let server = &metadata["downloads"]["server"];
        let url = server["url"].as_str().ok_or_else(|| {
            Error::Unsupported(format!(
                "Minecraft {version} has no official server download."
            ))
        })?;
        Ok(ResolvedDownload {
            url: url.into(),
            checksum: server["sha1"].as_str().map(str::to_owned),
            checksum_kind: ChecksumKind::Sha1,
            version: version.into(),
            build: "release".into(),
            java_major: metadata["javaVersion"]["majorVersion"]
                .as_u64()
                .unwrap_or(8) as u32,
        })
    }

    async fn resolve_paper(
        &self,
        version: &str,
        requested_build: Option<&str>,
        allow_experimental: bool,
    ) -> Result<ResolvedDownload> {
        let builds = self.paper_builds(version).await?;
        let build = if let Some(requested) = requested_build {
            builds.iter().find(|item| {
                item["id"].as_i64().map(|v| v.to_string()) == Some(requested.to_owned())
                    || item["build"].as_i64().map(|v| v.to_string()) == Some(requested.to_owned())
            })
        } else {
            builds
                .iter()
                .find(|item| item["channel"].as_str() == Some("STABLE"))
        }
        .ok_or_else(|| Error::NotFound(format!("No matching Paper build exists for {version}.")))?;
        let experimental = build["channel"].as_str().unwrap_or("EXPERIMENTAL") != "STABLE";
        if experimental && !allow_experimental {
            return Err(Error::Validation(
                "Experimental Paper builds must be explicitly enabled.".into(),
            ));
        }
        let download = &build["downloads"]["server:default"];
        let url = download["url"].as_str().ok_or_else(|| {
            Error::Internal("Paper did not provide a server download URL.".into())
        })?;
        let build_id = build["id"]
            .as_i64()
            .map(|v| v.to_string())
            .or_else(|| build["build"].as_i64().map(|v| v.to_string()))
            .unwrap_or_else(|| "latest".into());
        Ok(ResolvedDownload {
            url: url.into(),
            checksum: download["checksums"]["sha256"].as_str().map(str::to_owned),
            checksum_kind: ChecksumKind::Sha256,
            version: version.into(),
            build: build_id,
            java_major: paper_java_major(version),
        })
    }

    async fn resolve_forge(
        &self,
        version: &str,
        requested_build: Option<&str>,
        allow_experimental: bool,
    ) -> Result<ResolvedDownload> {
        let promotions = self.forge_promotions().await?;
        let recommended = promotions.get(&format!("{version}-recommended"));
        let latest = promotions.get(&format!("{version}-latest"));
        let build = if let Some(requested) = requested_build {
            if requested.is_empty()
                || requested.len() > 64
                || !requested.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_')
                })
            {
                return Err(Error::Validation(
                    "The Forge build identifier is invalid.".into(),
                ));
            }
            requested.to_owned()
        } else {
            recommended.or(latest).cloned().ok_or_else(|| {
                Error::NotFound(format!("Forge is not available for Minecraft {version}."))
            })?
        };
        let experimental = recommended.is_some()
            && latest.is_some_and(|latest| latest == &build)
            && recommended.is_some_and(|recommended| recommended != &build);
        if experimental && !allow_experimental {
            return Err(Error::Validation(
                "Latest Forge builds must be explicitly enabled.".into(),
            ));
        }
        let artifact = format!("{version}-{build}");
        let url = format!("{FORGE_MAVEN}/{artifact}/forge-{artifact}-installer.jar");
        let checksum = match self.client.get(format!("{url}.sha1")).send().await {
            Ok(response) if response.status().is_success() => response
                .text()
                .await
                .ok()
                .map(|value| value.trim().to_owned()),
            _ => None,
        };
        if requested_build.is_some() && checksum.is_none() {
            return Err(Error::NotFound(format!(
                "Forge {build} is not available for Minecraft {version}."
            )));
        }
        let vanilla = self.resolve_vanilla(version).await?;
        Ok(ResolvedDownload {
            url,
            checksum,
            checksum_kind: ChecksumKind::Sha1,
            version: version.into(),
            build,
            java_major: vanilla.java_major,
        })
    }

    async fn resolve_neoforge(
        &self,
        version: &str,
        requested_build: Option<&str>,
        allow_experimental: bool,
    ) -> Result<ResolvedDownload> {
        let build = if let Some(build) = requested_build {
            validate_neoforge_build(build)?;
            let detected = neoforge_minecraft_version(build).ok_or_else(|| {
                Error::Validation("The NeoForge build identifier is invalid.".into())
            })?;
            if detected != version {
                return Err(Error::Validation(format!(
                    "NeoForge {build} targets Minecraft {detected}, not {version}."
                )));
            }
            build.to_owned()
        } else {
            self.list_neoforge(allow_experimental)
                .await?
                .into_iter()
                .find(|candidate| candidate.version == version)
                .map(|candidate| candidate.build)
                .ok_or_else(|| {
                    Error::NotFound(format!(
                        "NeoForge is not available for Minecraft {version}."
                    ))
                })?
        };
        let experimental = is_neoforge_experimental(&build);
        if experimental && !allow_experimental {
            return Err(Error::Validation(
                "Beta NeoForge builds must be explicitly enabled.".into(),
            ));
        }
        let (artifact, coordinate) = if version == "1.20.1" {
            ("forge", build.clone())
        } else {
            ("neoforge", build.clone())
        };
        let url = format!(
            "{NEOFORGE_MAVEN}/{artifact}/{coordinate}/{artifact}-{coordinate}-installer.jar"
        );
        let checksum = match self.client.get(format!("{url}.sha1")).send().await {
            Ok(response) if response.status().is_success() => response
                .text()
                .await
                .ok()
                .map(|value| value.trim().to_owned()),
            _ => None,
        };
        if checksum.is_none() {
            return Err(Error::NotFound(format!(
                "NeoForge {build} is no longer available from the official Maven."
            )));
        }
        let vanilla = self.resolve_vanilla(version).await?;
        Ok(ResolvedDownload {
            url,
            checksum,
            checksum_kind: ChecksumKind::Sha1,
            version: version.into(),
            build,
            java_major: vanilla.java_major,
        })
    }

    async fn resolve_fabric(
        &self,
        version: &str,
        requested_build: Option<&str>,
        allow_experimental: bool,
    ) -> Result<ResolvedDownload> {
        let games: Vec<Value> = self
            .client
            .get(format!("{FABRIC_META}/game"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let game = games
            .iter()
            .find(|game| game["version"].as_str() == Some(version))
            .ok_or_else(|| {
                Error::NotFound(format!("Fabric does not support Minecraft {version}."))
            })?;
        if game["stable"].as_bool() != Some(true) && !allow_experimental {
            return Err(Error::Validation(
                "Snapshot Fabric versions must be explicitly enabled.".into(),
            ));
        }
        let loaders: Vec<Value> = self
            .client
            .get(format!(
                "{FABRIC_META}/loader/{}",
                urlencoding::encode(version)
            ))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let loader = if let Some(requested) = requested_build {
            loaders
                .iter()
                .find(|entry| entry["loader"]["version"].as_str() == Some(requested))
        } else {
            loaders
                .iter()
                .find(|entry| entry["loader"]["stable"].as_bool() == Some(true))
                .or_else(|| loaders.first())
        }
        .ok_or_else(|| {
            Error::NotFound(format!(
                "No Fabric Loader is available for Minecraft {version}."
            ))
        })?;
        let loader_version = loader["loader"]["version"]
            .as_str()
            .ok_or_else(|| Error::Internal("Fabric returned an invalid loader version.".into()))?;
        let installers: Vec<Value> = self
            .client
            .get(format!("{FABRIC_META}/installer"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let installer = installers
            .iter()
            .find(|entry| entry["stable"].as_bool() == Some(true))
            .or_else(|| installers.first())
            .and_then(|entry| entry["version"].as_str())
            .ok_or_else(|| Error::Internal("Fabric returned no installer versions.".into()))?;
        let vanilla = self.resolve_vanilla(version).await?;
        Ok(ResolvedDownload {
            url: format!(
                "{FABRIC_META}/loader/{}/{}/{}/server/jar",
                urlencoding::encode(version),
                urlencoding::encode(loader_version),
                urlencoding::encode(installer)
            ),
            checksum: None,
            checksum_kind: ChecksumKind::Sha256,
            version: version.into(),
            build: loader_version.into(),
            java_major: vanilla.java_major,
        })
    }

    pub async fn download(
        &self,
        resolved: &ResolvedDownload,
        destination: &Path,
        channel: Option<&Channel<OperationEvent>>,
        operation_id: &str,
        progress_range: (f32, f32),
    ) -> Result<()> {
        let response = self
            .client
            .get(&resolved.url)
            .send()
            .await?
            .error_for_status()?;
        let total = response.content_length().unwrap_or(0);
        let temporary = destination.with_extension("download");
        let mut file = tokio::fs::File::create(&temporary).await?;
        let mut stream = response.bytes_stream();
        let mut received = 0u64;
        use tokio::io::AsyncWriteExt;
        while let Some(chunk) = stream.next().await {
            if let Err(error) = crate::operations::check(operation_id) {
                drop(file);
                let _ = tokio::fs::remove_file(&temporary).await;
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
                    phase: "download".into(),
                    progress,
                    message: if total == 0 {
                        format!(
                            "Downloading server software · {} downloaded",
                            human_bytes(received)
                        )
                    } else {
                        format!(
                            "Downloading server software · {transfer_progress:.0}% · {} of {}",
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
            let _ = tokio::fs::remove_file(&temporary).await;
            return Err(error);
        }
        verify_checksum(
            &temporary,
            resolved.checksum.as_deref(),
            resolved.checksum_kind,
        )
        .await?;
        tokio::fs::rename(temporary, destination).await?;
        Ok(())
    }
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

fn collect_version_strings(value: &Value, result: &mut Vec<String>) {
    match value {
        Value::String(value) => result.push(value.clone()),
        Value::Array(values) => values
            .iter()
            .for_each(|value| collect_version_strings(value, result)),
        Value::Object(values) => values
            .values()
            .for_each(|value| collect_version_strings(value, result)),
        _ => {}
    }
}

pub fn paper_java_major(version: &str) -> u32 {
    let normalized = version.trim_start_matches(|character: char| !character.is_ascii_digit());
    let parts: Vec<u32> = normalized
        .split('.')
        .filter_map(|part| part.parse().ok())
        .collect();
    if parts.first().copied().unwrap_or(0) >= 26 {
        return 25;
    }
    let minor = if parts.first() == Some(&1) {
        parts.get(1).copied().unwrap_or(0)
    } else {
        0
    };
    match minor {
        0..=11 => 8,
        12..=15 => 11,
        16 if parts.get(2).copied().unwrap_or(0) <= 4 => 11,
        16 => 16,
        17..=19 => 17,
        _ => 21,
    }
}

fn extract_maven_versions(metadata: &str) -> Vec<String> {
    regex::Regex::new(r"<version>([^<]+)</version>")
        .expect("the Maven version regex is valid")
        .captures_iter(metadata)
        .filter_map(|capture| capture.get(1).map(|value| value.as_str().trim().to_owned()))
        .collect()
}

pub(crate) fn neoforge_minecraft_version(build: &str) -> Option<String> {
    if let Some((minecraft, loader)) = build.split_once('-') {
        if minecraft.starts_with("1.20.1") && loader.starts_with("47.") {
            return Some("1.20.1".into());
        }
    }
    let numeric = build.split('-').next()?;
    let parts = numeric
        .split('.')
        .map(str::parse::<u32>)
        .collect::<std::result::Result<Vec<_>, _>>()
        .ok()?;
    if parts.len() < 3 {
        return None;
    }
    // NeoForge's legacy 1.20.1 Maven metadata contains one unprefixed 47.x
    // coordinate alongside the normal 1.20.1-47.x coordinates. The loader
    // major is 47; it is not a Minecraft 1.47 release.
    if parts[0] == 47 {
        return Some("1.20.1".into());
    }
    if parts[0] >= 26 {
        return Some(if parts[2] == 0 {
            format!("{}.{}", parts[0], parts[1])
        } else {
            format!("{}.{}.{}", parts[0], parts[1], parts[2])
        });
    }
    Some(if parts[1] == 0 {
        format!("1.{}", parts[0])
    } else {
        format!("1.{}.{}", parts[0], parts[1])
    })
}

fn neoforge_build_cmp(left: &str, right: &str) -> std::cmp::Ordering {
    version_cmp(
        neoforge_loader_version(left),
        neoforge_loader_version(right),
    )
    .then_with(|| left.cmp(right))
}

fn neoforge_loader_version(build: &str) -> &str {
    build
        .split_once('-')
        .filter(|(minecraft, _)| minecraft.starts_with("1.20.1"))
        .map(|(_, loader)| loader)
        .unwrap_or(build)
}

fn is_neoforge_experimental(build: &str) -> bool {
    let lower = build.to_ascii_lowercase();
    lower.contains("-beta") || lower.contains("-alpha")
}

fn validate_neoforge_build(build: &str) -> Result<()> {
    if build.is_empty()
        || build.len() > 80
        || !build
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '-'))
        || neoforge_minecraft_version(build).is_none()
    {
        return Err(Error::Validation(
            "The NeoForge build identifier is invalid.".into(),
        ));
    }
    Ok(())
}

pub(crate) fn version_cmp(left: &str, right: &str) -> std::cmp::Ordering {
    let tokens = |value: &str| {
        value
            .split(['.', '-', '_'])
            .map(|part| part.parse::<u32>().unwrap_or(0))
            .collect::<Vec<_>>()
    };
    tokens(left)
        .cmp(&tokens(right))
        .then_with(|| left.cmp(right))
}

async fn verify_checksum(path: &Path, expected: Option<&str>, kind: ChecksumKind) -> Result<()> {
    let Some(expected) = expected else {
        return Ok(());
    };
    let data = tokio::fs::read(path).await?;
    let actual = match kind {
        ChecksumKind::Sha256 => hex::encode(Sha256::digest(&data)),
        ChecksumKind::Sha1 => {
            use sha1::{Digest as _, Sha1};
            hex::encode(Sha1::digest(&data))
        }
    };
    if !actual.eq_ignore_ascii_case(expected) {
        return Err(Error::Validation(
            "The downloaded file did not match its published checksum.".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paper_java_mapping_covers_old_and_new_versions() {
        assert_eq!(paper_java_major("1.12.2"), 11);
        assert_eq!(paper_java_major("1.16.5"), 16);
        assert_eq!(paper_java_major("1.19.4"), 17);
        assert_eq!(paper_java_major("1.21.11"), 21);
        assert_eq!(paper_java_major("26.1"), 25);
    }

    #[test]
    fn version_sort_understands_new_scheme() {
        assert_eq!(version_cmp("26.1", "1.21.11"), std::cmp::Ordering::Greater);
    }

    #[test]
    fn neoforge_builds_map_to_minecraft_versions() {
        assert_eq!(
            neoforge_minecraft_version("1.20.1-47.1.106").as_deref(),
            Some("1.20.1")
        );
        assert_eq!(
            neoforge_minecraft_version("47.1.82").as_deref(),
            Some("1.20.1")
        );
        assert_eq!(
            neoforge_minecraft_version("20.4.251").as_deref(),
            Some("1.20.4")
        );
        assert_eq!(
            neoforge_minecraft_version("21.0.167").as_deref(),
            Some("1.21")
        );
        assert_eq!(
            neoforge_minecraft_version("21.11.45").as_deref(),
            Some("1.21.11")
        );
        assert_eq!(
            neoforge_minecraft_version("26.1.2.94").as_deref(),
            Some("26.1.2")
        );
        assert_eq!(
            neoforge_minecraft_version("26.2.0.57").as_deref(),
            Some("26.2")
        );
    }

    #[test]
    fn neoforge_legacy_builds_sort_by_loader_version() {
        let mut builds = [
            "1.20.1-47.1.81".to_owned(),
            "47.1.82".to_owned(),
            "1.20.1-47.1.106".to_owned(),
        ];
        builds.sort_by(|left, right| neoforge_build_cmp(right, left));
        assert_eq!(builds[0], "1.20.1-47.1.106");
        assert_eq!(builds[1], "47.1.82");
    }
}
