use std::{collections::HashMap, sync::OnceLock, time::Duration};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use tokio::sync::RwLock;

use crate::error::{AppError, CommandResult, Error, Result};

const MAX_AVATAR_BYTES: u64 = 256 * 1024;
static AVATAR_CACHE: OnceLock<RwLock<HashMap<String, Option<String>>>> = OnceLock::new();

#[tauri::command]
pub async fn load_player_avatar(identifier: String) -> CommandResult<Option<String>> {
    load_avatar(&identifier).await.map_err(AppError::from)
}

async fn load_avatar(identifier: &str) -> Result<Option<String>> {
    let identifier = identifier.trim();
    if !valid_identifier(identifier) {
        return Err(Error::Validation(
            "That Minecraft player name is invalid.".into(),
        ));
    }
    let cache_key = identifier.to_ascii_lowercase();
    let cache = AVATAR_CACHE.get_or_init(|| RwLock::new(HashMap::new()));
    if let Some(cached) = cache.read().await.get(&cache_key).cloned() {
        return Ok(cached);
    }

    let contact = option_env!("NOOKI_CONTACT_URL").unwrap_or("nooki@mints.wtf");
    let client = reqwest::Client::builder()
        .user_agent(format!("Nooki/{} ({contact})", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(8))
        .build()?;
    let url = format!(
        "https://mc-heads.net/avatar/{}/64.png",
        urlencoding::encode(identifier)
    );
    let response = match client.get(url).send().await {
        Ok(response) if response.status().is_success() => response,
        _ => {
            cache.write().await.insert(cache_key, None);
            return Ok(None);
        }
    };
    if response
        .content_length()
        .is_some_and(|length| length > MAX_AVATAR_BYTES)
    {
        cache.write().await.insert(cache_key, None);
        return Ok(None);
    }
    let bytes = response.bytes().await?;
    let avatar = (bytes.len() as u64 <= MAX_AVATAR_BYTES
        && bytes.starts_with(b"\x89PNG\r\n\x1a\n"))
    .then(|| format!("data:image/png;base64,{}", BASE64.encode(bytes)));
    cache.write().await.insert(cache_key, avatar.clone());
    Ok(avatar)
}

fn valid_identifier(value: &str) -> bool {
    let username = (3..=16).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
    let uuid_length = value.len() == 32 || value.len() == 36;
    let uuid = uuid_length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() || byte == b'-');
    username || uuid
}

#[cfg(test)]
mod tests {
    use super::valid_identifier;

    #[test]
    fn accepts_minecraft_names_and_uuids_only() {
        assert!(valid_identifier("Notch"));
        assert!(valid_identifier("069a79f444e94726a5befca90e38aaf5"));
        assert!(valid_identifier("069a79f4-44e9-4726-a5be-fca90e38aaf5"));
        assert!(!valid_identifier("ab"));
        assert!(!valid_identifier("name/../../player"));
    }
}
