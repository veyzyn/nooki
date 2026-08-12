use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use ed25519_dalek::{Signer, SigningKey};
use futures_util::{SinkExt, Stream, StreamExt};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::{Mutex, RwLock},
    task::JoinHandle,
};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::{
    error::{Error, Result},
    models::{RelayAccess, Server, ServerStatus, SharingStatus},
    state::AppState,
};

const CONTROL_URL: &str = "wss://nooki-64f85d08d9.mints.wtf/v1/control";
const DATA_BASE_URL: &str = "wss://nooki-64f85d08d9.mints.wtf/v1/data";
const ACTIVATE_URL: &str = "https://nooki-64f85d08d9.mints.wtf/v1/activate";
const PROTOCOL_NAME: &str = "nooki-relay-v2";
const ACTIVATION_PROTOCOL: &str = "nooki-relay-activation-v1";

pub struct ShareManager {
    identity: Arc<SigningKey>,
    tasks: Mutex<HashMap<String, ShareRuntime>>,
    access: RwLock<RelayAccess>,
    access_path: PathBuf,
}

struct ShareRuntime {
    handle: JoinHandle<()>,
    route_token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RelayMessage {
    #[serde(rename = "type")]
    kind: String,
    nonce: Option<String>,
    address: Option<String>,
    device_id: Option<String>,
    connection_id: Option<String>,
    token: Option<String>,
    message: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Authentication<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    public_key: String,
    signature: String,
    server_id: &'a str,
    server_name: &'a str,
    route_token: &'a str,
    vanity: Option<&'a str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ActivationRequest<'a> {
    activation_code: &'a str,
    public_key: String,
    signature: String,
    timestamp: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ActivationError {
    message: String,
}

impl ShareManager {
    pub async fn new(app_data_dir: &Path) -> Result<Self> {
        let directory = app_data_dir.join("sharing");
        let identity = load_or_create_identity(&directory).await?;
        let access_path = directory.join("access.json");
        let access = load_access_receipt(&access_path).await.unwrap_or_default();
        Ok(Self {
            identity: Arc::new(identity),
            tasks: Mutex::new(HashMap::new()),
            access: RwLock::new(access),
            access_path,
        })
    }

    pub async fn access(&self) -> RelayAccess {
        self.access.read().await.clone()
    }

    pub async fn activate(&self, activation_code: &str) -> Result<RelayAccess> {
        let activation_code = activation_code.trim();
        if activation_code.is_empty() {
            return Err(Error::Validation("Enter an activation key.".into()));
        }
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Error::Internal("The system clock is invalid.".into()))?
            .as_secs() as i64;
        let proof = format!("{ACTIVATION_PROTOCOL}\n{activation_code}\n{timestamp}");
        let request = ActivationRequest {
            activation_code,
            public_key: URL_SAFE_NO_PAD.encode(self.identity.verifying_key().as_bytes()),
            signature: URL_SAFE_NO_PAD.encode(self.identity.sign(proof.as_bytes()).to_bytes()),
            timestamp,
        };
        let response = reqwest::Client::new()
            .post(ACTIVATE_URL)
            .json(&request)
            .send()
            .await?;
        if !response.status().is_success() {
            let message = response
                .json::<ActivationError>()
                .await
                .map(|error| error.message)
                .unwrap_or_else(|_| "Relay activation was rejected.".into());
            return Err(Error::NetworkMessage(message));
        }
        let access = response.json::<RelayAccess>().await?;
        if !access.activated || access.servers_allowed != 1 {
            return Err(Error::NetworkMessage(
                "The relay returned an invalid activation receipt.".into(),
            ));
        }
        save_access_receipt(&self.access_path, &access).await?;
        *self.access.write().await = access.clone();
        Ok(access)
    }

    pub async fn start(&self, state: Arc<AppState>, server_id: &str) {
        if !self.access.read().await.activated {
            return;
        }
        let Ok(server) = state.server(server_id).await else {
            return;
        };
        if !matches!(server.status, ServerStatus::Running)
            || !state.processes.is_running(server_id).await
        {
            return;
        }

        let mut tasks = self.tasks.lock().await;
        if tasks.contains_key(server_id) {
            return;
        }
        let id = server_id.to_owned();
        let mut route_bytes = [0_u8; 18];
        OsRng.fill_bytes(&mut route_bytes);
        let route_token = URL_SAFE_NO_PAD.encode(route_bytes);
        let handle = spawn_share_task(
            state,
            self.identity.clone(),
            id.clone(),
            route_token.clone(),
        );
        tasks.insert(
            id,
            ShareRuntime {
                handle,
                route_token,
            },
        );
    }

    pub async fn reconnect(&self, state: Arc<AppState>, server_id: &str) {
        let Some(runtime) = self.tasks.lock().await.remove(server_id) else {
            self.start(state, server_id).await;
            return;
        };
        runtime.handle.abort();

        let Ok(server) = state.server(server_id).await else {
            return;
        };
        if !matches!(server.status, ServerStatus::Running)
            || !state.processes.is_running(server_id).await
        {
            return;
        }

        let id = server_id.to_owned();
        let route_token = runtime.route_token;
        let handle = spawn_share_task(
            state,
            self.identity.clone(),
            id.clone(),
            route_token.clone(),
        );
        self.tasks.lock().await.insert(
            id,
            ShareRuntime {
                handle,
                route_token,
            },
        );
    }

    pub async fn stop_runtime(&self, server_id: &str) {
        if let Some(runtime) = self.tasks.lock().await.remove(server_id) {
            runtime.handle.abort();
        }
    }
}

async fn load_access_receipt(path: &Path) -> Result<RelayAccess> {
    let payload = tokio::fs::read(path).await?;
    serde_json::from_slice(&payload).map_err(Error::from)
}

async fn save_access_receipt(path: &Path, access: &RelayAccess) -> Result<()> {
    let directory = path
        .parent()
        .ok_or_else(|| Error::Internal("The relay access path is invalid.".into()))?;
    tokio::fs::create_dir_all(directory).await?;
    let temporary = directory.join(format!("access-{}.tmp", uuid::Uuid::new_v4()));
    tokio::fs::write(&temporary, serde_json::to_vec(access)?).await?;
    if path.is_file() {
        tokio::fs::remove_file(path).await?;
    }
    tokio::fs::rename(&temporary, path).await?;
    Ok(())
}

fn spawn_share_task(
    state: Arc<AppState>,
    identity: Arc<SigningKey>,
    server_id: String,
    route_token: String,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        run_share_loop(state, identity, server_id, route_token).await;
    })
}

async fn run_share_loop(
    state: Arc<AppState>,
    identity: Arc<SigningKey>,
    server_id: String,
    route_token: String,
) {
    let mut retry = Duration::from_secs(1);
    loop {
        let Ok(server) = state.server(&server_id).await else {
            break;
        };
        if !state.processes.is_running(&server_id).await {
            set_status(&state, &server_id, SharingStatus::Offline, None, None, None).await;
            break;
        }

        set_status(
            &state,
            &server_id,
            SharingStatus::Connecting,
            None,
            None,
            None,
        )
        .await;
        let mut slot_is_busy = false;
        match run_control_session(state.clone(), identity.clone(), server, &route_token).await {
            Ok(()) => retry = Duration::from_secs(1),
            Err(error) => {
                if !state.processes.is_running(&server_id).await {
                    set_status(&state, &server_id, SharingStatus::Offline, None, None, None).await;
                    break;
                }
                slot_is_busy = error.to_string().contains("relay slot");
                set_status(
                    &state,
                    &server_id,
                    SharingStatus::Error,
                    None,
                    None,
                    Some(error.to_string()),
                )
                .await;
            }
        }
        let wait = if slot_is_busy {
            Duration::from_secs(2)
        } else {
            retry
        };
        tokio::time::sleep(wait).await;
        retry = if slot_is_busy {
            Duration::from_secs(1)
        } else {
            (retry * 2).min(Duration::from_secs(30))
        };
    }
}

async fn run_control_session(
    state: Arc<AppState>,
    identity: Arc<SigningKey>,
    server: Server,
    route_token: &str,
) -> Result<()> {
    let (mut socket, _) = connect_async(CONTROL_URL)
        .await
        .map_err(|error| Error::NetworkMessage(format!("Relay connection failed: {error}")))?;
    let challenge = read_relay_message(&mut socket).await?;
    if challenge.kind != "challenge" {
        return Err(Error::NetworkMessage(
            "The relay did not send an identity challenge.".into(),
        ));
    }
    let nonce = challenge
        .nonce
        .ok_or_else(|| Error::NetworkMessage("The relay challenge was incomplete.".into()))?;
    let vanity = server.sharing.vanity.as_deref();
    let proof = authentication_proof(&nonce, &server.id, route_token, vanity);
    let auth = Authentication {
        kind: "authenticate",
        public_key: URL_SAFE_NO_PAD.encode(identity.verifying_key().as_bytes()),
        signature: URL_SAFE_NO_PAD.encode(identity.sign(proof.as_bytes()).to_bytes()),
        server_id: &server.id,
        server_name: &server.name,
        route_token,
        vanity,
    };
    socket
        .send(Message::Text(
            serde_json::to_string(&auth).map_err(Error::from)?.into(),
        ))
        .await
        .map_err(|error| Error::NetworkMessage(format!("Relay authentication failed: {error}")))?;

    let ready = read_relay_message(&mut socket).await?;
    if ready.kind == "error" {
        return Err(Error::NetworkMessage(
            ready
                .message
                .unwrap_or_else(|| "Identity proof was rejected.".into()),
        ));
    }
    if ready.kind != "ready" {
        return Err(Error::NetworkMessage(
            "The relay returned an unexpected response.".into(),
        ));
    }
    let address = ready
        .address
        .ok_or_else(|| Error::NetworkMessage("The relay did not assign an address.".into()))?;
    set_status(
        &state,
        &server.id,
        SharingStatus::Online,
        Some(address),
        ready.device_id,
        None,
    )
    .await;

    let mut keepalive = tokio::time::interval(Duration::from_secs(25));
    keepalive.tick().await;
    loop {
        tokio::select! {
            _ = keepalive.tick() => {
                socket.send(Message::Ping(Vec::new().into())).await
                    .map_err(|error| Error::NetworkMessage(format!("Relay keepalive failed: {error}")))?;
            }
            message = socket.next() => {
                let message = message
                    .ok_or_else(|| Error::NetworkMessage("The relay closed the connection.".into()))?
                    .map_err(|error| Error::NetworkMessage(format!("Relay connection failed: {error}")))?;
                match message {
                    Message::Text(text) => {
                        let event: RelayMessage = serde_json::from_str(&text)?;
                        if event.kind == "incoming" {
                            if let (Some(connection_id), Some(token)) = (event.connection_id, event.token) {
                                let port = server.port;
                                tokio::spawn(async move {
                                    let _ = bridge_player(connection_id, token, port).await;
                                });
                            }
                        } else if event.kind == "error" {
                            return Err(Error::NetworkMessage(event.message.unwrap_or_else(|| "The relay reported an error.".into())));
                        }
                    }
                    Message::Close(_) => return Err(Error::NetworkMessage("The relay closed the connection.".into())),
                    _ => {}
                }
            }
        }
    }
}

async fn read_relay_message<S>(socket: &mut S) -> Result<RelayMessage>
where
    S: Stream<Item = std::result::Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    let message = tokio::time::timeout(Duration::from_secs(15), socket.next())
        .await
        .map_err(|_| Error::NetworkMessage("The relay took too long to respond.".into()))?
        .ok_or_else(|| Error::NetworkMessage("The relay closed the connection.".into()))?
        .map_err(|error| Error::NetworkMessage(format!("Relay connection failed: {error}")))?;
    match message {
        Message::Text(text) => serde_json::from_str(&text).map_err(Error::from),
        _ => Err(Error::NetworkMessage(
            "The relay returned an unexpected message.".into(),
        )),
    }
}

async fn bridge_player(connection_id: String, token: String, port: u16) -> Result<()> {
    let local = tokio::time::timeout(
        Duration::from_secs(5),
        TcpStream::connect(("127.0.0.1", port)),
    )
    .await
    .map_err(|_| Error::NetworkMessage("The local Minecraft server did not respond.".into()))??;
    let url = format!("{DATA_BASE_URL}/{connection_id}?token={token}");
    let (socket, _) = connect_async(&url)
        .await
        .map_err(|error| Error::NetworkMessage(format!("Player tunnel failed: {error}")))?;
    let (mut ws_write, mut ws_read) = socket.split();
    let (mut local_read, mut local_write) = local.into_split();

    let relay_to_local = async {
        while let Some(message) = ws_read.next().await {
            match message.map_err(|error| Error::NetworkMessage(error.to_string()))? {
                Message::Binary(bytes) => local_write.write_all(&bytes).await?,
                Message::Close(_) => break,
                _ => {}
            }
        }
        Result::<()>::Ok(())
    };
    let local_to_relay = async {
        let mut buffer = vec![0_u8; 32 * 1024];
        loop {
            let read = local_read.read(&mut buffer).await?;
            if read == 0 {
                let _ = ws_write.close().await;
                break;
            }
            ws_write
                .send(Message::Binary(buffer[..read].to_vec().into()))
                .await
                .map_err(|error| Error::NetworkMessage(error.to_string()))?;
        }
        Result::<()>::Ok(())
    };
    tokio::select! {
        result = relay_to_local => result,
        result = local_to_relay => result,
    }
}

async fn set_status(
    state: &AppState,
    server_id: &str,
    status: SharingStatus,
    address: Option<String>,
    device_id: Option<String>,
    error: Option<String>,
) {
    if let Ok(mut server) = state.server(server_id).await {
        server.sharing.status = status;
        if !matches!(server.sharing.status, SharingStatus::Online) {
            server.sharing.address = None;
        }
        if address.is_some() {
            server.sharing.address = address;
        }
        if device_id.is_some() {
            server.sharing.device_id = device_id;
        }
        server.sharing.last_error = error;
        let _ = state.save_server(server).await;
    }
}

fn authentication_proof(
    nonce: &str,
    server_id: &str,
    route_token: &str,
    vanity: Option<&str>,
) -> String {
    format!(
        "{PROTOCOL_NAME}\n{nonce}\n{server_id}\n{route_token}\n{}",
        vanity.unwrap_or_default()
    )
}

pub(crate) fn normalize_vanity(value: Option<&str>) -> Result<Option<String>> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let value = value.to_ascii_lowercase();
    if value.len() < 3
        || value.len() > 32
        || !value.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
        || value.starts_with('-')
        || value.ends_with('-')
    {
        return Err(Error::Validation(
            "Use 3–32 letters, numbers, or hyphens for the vanity address.".into(),
        ));
    }
    Ok(Some(value))
}

async fn load_or_create_identity(directory: &Path) -> Result<SigningKey> {
    tokio::fs::create_dir_all(directory).await?;
    let path = directory.join("identity.key");
    if let Ok(bytes) = tokio::fs::read(&path).await {
        let (secret_bytes, migrate) = if bytes.len() == 32 {
            (bytes, true)
        } else {
            (unprotect_identity(&bytes)?, false)
        };
        let secret: [u8; 32] = secret_bytes
            .try_into()
            .map_err(|_| Error::Internal("The sharing identity file is invalid.".into()))?;
        if migrate {
            save_identity(&path, &secret).await?;
        }
        return Ok(SigningKey::from_bytes(&secret));
    }

    let identity = SigningKey::generate(&mut OsRng);
    save_identity(&path, &identity.to_bytes()).await?;
    Ok(identity)
}

async fn save_identity(path: &Path, secret: &[u8; 32]) -> Result<()> {
    let directory = path
        .parent()
        .ok_or_else(|| Error::Internal("The sharing identity path is invalid.".into()))?;
    let temporary = directory.join(format!("identity-{}.tmp", uuid::Uuid::new_v4()));
    tokio::fs::write(&temporary, protect_identity(secret)?).await?;
    if path.is_file() {
        tokio::fs::remove_file(path).await?;
    }
    tokio::fs::rename(temporary, path).await?;
    Ok(())
}

#[cfg(windows)]
fn protect_identity(secret: &[u8]) -> Result<Vec<u8>> {
    use windows::Win32::{
        Foundation::{LocalFree, HLOCAL},
        Security::Cryptography::{CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB},
    };

    let input = CRYPT_INTEGER_BLOB {
        cbData: secret.len() as u32,
        pbData: secret.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptProtectData(
            &input,
            windows::core::w!("Nooki relay identity"),
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
        .map_err(|error| {
            Error::Internal(format!("Could not protect the relay identity: {error}"))
        })?;
        let protected = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(output.pbData.cast())));
        Ok(protected)
    }
}

#[cfg(windows)]
fn unprotect_identity(protected: &[u8]) -> Result<Vec<u8>> {
    use windows::Win32::{
        Foundation::{LocalFree, HLOCAL},
        Security::Cryptography::{
            CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        },
    };

    let input = CRYPT_INTEGER_BLOB {
        cbData: protected.len() as u32,
        pbData: protected.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptUnprotectData(
            &input,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
        .map_err(|error| {
            Error::Internal(format!(
                "Could not unlock this device's relay identity: {error}"
            ))
        })?;
        let secret = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(output.pbData.cast())));
        Ok(secret)
    }
}

#[cfg(not(windows))]
fn protect_identity(secret: &[u8]) -> Result<Vec<u8>> {
    Ok(secret.to_vec())
}

#[cfg(not(windows))]
fn unprotect_identity(protected: &[u8]) -> Result<Vec<u8>> {
    Ok(protected.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn identity_is_stable_on_disk() {
        let directory = tempfile::tempdir().unwrap();
        let first = load_or_create_identity(directory.path()).await.unwrap();
        let second = load_or_create_identity(directory.path()).await.unwrap();
        assert_eq!(first.verifying_key(), second.verifying_key());
        let stored = tokio::fs::read(directory.path().join("identity.key"))
            .await
            .unwrap();
        #[cfg(windows)]
        assert_ne!(stored, first.to_bytes());
        #[cfg(not(windows))]
        assert_eq!(stored, first.to_bytes());
    }

    #[tokio::test]
    #[ignore = "contacts the deployed Nooki relay"]
    async fn live_relay_rejects_an_unactivated_installation() {
        let identity = SigningKey::generate(&mut OsRng);
        let server_id = format!("smoke-{}", uuid::Uuid::new_v4());
        let (mut socket, _) = connect_async(CONTROL_URL).await.unwrap();
        let challenge = read_relay_message(&mut socket).await.unwrap();
        let nonce = challenge.nonce.unwrap();
        let route_token = URL_SAFE_NO_PAD.encode([7_u8; 18]);
        let proof = authentication_proof(&nonce, &server_id, &route_token, None);
        let auth = Authentication {
            kind: "authenticate",
            public_key: URL_SAFE_NO_PAD.encode(identity.verifying_key().as_bytes()),
            signature: URL_SAFE_NO_PAD.encode(identity.sign(proof.as_bytes()).to_bytes()),
            server_id: &server_id,
            server_name: "Nooki smoke test",
            route_token: &route_token,
            vanity: None,
        };
        socket
            .send(Message::Text(serde_json::to_string(&auth).unwrap().into()))
            .await
            .unwrap();
        let response = read_relay_message(&mut socket).await.unwrap();
        assert_eq!(response.kind, "error");
        assert!(response.message.unwrap().contains("not activated"));
    }

    #[test]
    fn vanity_names_are_normalized_and_validated() {
        assert_eq!(
            normalize_vanity(Some(" Parkour-2 ")).unwrap().as_deref(),
            Some("parkour-2")
        );
        assert!(normalize_vanity(Some("-bad")).is_err());
        assert!(normalize_vanity(Some("two words")).is_err());
        assert_eq!(normalize_vanity(Some("  ")).unwrap(), None);
    }
}
