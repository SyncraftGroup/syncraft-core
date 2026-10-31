use super::config::Account;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn is_valid_minecraft_username(name: &str) -> bool {
    let len = name.len();

    if !(3..=16).contains(&len) {
        return false;
    }

    name.bytes().all(|b| {
        matches!(b,
            b'a'..=b'z' |
            b'A'..=b'Z' |
            b'0'..=b'9' |
            b'_'
        )
    })
}

pub fn offline_uuid(name: &str) -> String {
    let hash = md5::compute(format!("OfflinePlayer:{}", name));
    Uuid::from_bytes(hash.into()).to_string().replace("-", "")
}

/// `POST /authserver/register` request body.
#[derive(Debug, Serialize)]
pub struct RegisterRequest {
    /// Email address used as the account identifier (Mojang legacy naming).
    pub username: String,
    pub password: String,
    /// The character name shown in-game.
    #[serde(rename = "characterName")]
    pub character_name: String,
}

/// Agent metadata sent by the client (always `"Minecraft"` / `1` for Minecraft).
#[derive(Debug, Serialize)]
pub struct Agent {
    #[allow(dead_code)]
    pub name: String,
    #[allow(dead_code)]
    pub version: i32,
}

/// `POST /authserver/authenticate` request body.
#[derive(Debug, Serialize)]
pub struct AuthenticateRequest {
    #[allow(dead_code)]
    pub agent: Agent,
    /// Email address used as the account identifier (Mojang legacy naming).
    pub username: String,
    pub password: String,
}

/// A user's character profile.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Profile {
    pub id: String,
    pub name: String,
}

/// `POST /authserver/refresh` request body.
#[derive(Debug, Serialize)]
pub struct RefreshRequest {
    #[serde(rename = "accessToken")]
    pub access_token: String,
    #[serde(rename = "clientToken")]
    pub client_token: String,
    #[serde(rename = "selectedProfile")]
    pub selected_profile: Option<Profile>,
}

/// `PUT /api/user/profile/{uuid}/{textureType}` — parsed multipart upload.
/// This is not a json request.
// #[derive(Debug)]
// pub struct TextureUpload {
//     pub skin_type: YggdrasilSkinType,
//     pub data: Vec<u8>,
// }

/// `POST /authserver/register` response body.
#[derive(Debug, Deserialize)]
pub struct RegisterResponse {
    pub id: String,
    #[allow(dead_code)]
    pub email: String,
}

/// Authenticate against a Syncraft Yggdrasil service.
pub async fn authenticate(
    link: &str,
    email: &str,
    password: &str,
) -> Result<Account, crate::Error> {
    let client = reqwest::Client::new();
    let request = AuthenticateRequest {
        agent: Agent {
            name: "Minecraft".into(),
            version: 1,
        },
        username: email.to_string(),
        password: password.to_string(),
    };
    let response = client
        .post(format!("{link}/yggdrasil/authserver/authenticate"))
        .json(&request)
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(crate::Error::Other(response.text().await?));
    }
    let auth: AuthenticateResponse = response.json().await?;
    let profile = auth
        .selected_profile
        .or_else(|| auth.available_profiles.into_iter().next())
        .ok_or_else(|| crate::Error::Other("no profile in auth response".into()))?;
    Ok(Account::Syncraft {
        username: profile.name,
        uuid: profile.id,
        email: email.to_string(),
        client_token: auth.client_token,
        access_token: auth.access_token,
    })
}

/// Register a Syncraft account. The account may still require e-mail
/// verification before it can log in.
pub async fn register(
    link: &str,
    username: &str,
    email: &str,
    password: &str,
) -> Result<RegisterResponse, crate::Error> {
    let client = reqwest::Client::new();
    let request = RegisterRequest {
        username: email.to_string(),
        password: password.to_string(),
        character_name: username.to_string(),
    };
    let response = client
        .post(format!("{link}/yggdrasil/register"))
        .json(&request)
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(crate::Error::Other(response.text().await?));
    }
    Ok(response.json().await?)
}

/// `POST /authserver/authenticate` response body.
#[derive(Debug, Deserialize)]
pub struct AuthenticateResponse {
    #[serde(rename = "accessToken")]
    pub access_token: String,
    #[serde(rename = "clientToken")]
    pub client_token: String,
    #[serde(rename = "availableProfiles")]
    pub available_profiles: Vec<Profile>,
    #[serde(rename = "selectedProfile")]
    pub selected_profile: Option<Profile>,
}

/// `POST /authserver/refresh` response body.
#[derive(Debug, Deserialize)]
pub struct RefreshResponse {
    #[serde(rename = "accessToken")]
    pub access_token: String,
    #[serde(rename = "clientToken")]
    pub client_token: String,
    #[serde(rename = "selectedProfile")]
    #[allow(dead_code)]
    pub selected_profile: Option<Profile>,
}

/// Refresh a Yggdrasil session.
///
/// Shared by the terminal client and the desktop app; the caller decides what
/// to do with a failure (the CLI re-prompts, the GUI falls back to offline).
pub async fn refresh_token(
    link: &str,
    access_token: &str,
    client_token: &str,
) -> Result<(String, String), crate::Error> {
    let client = reqwest::Client::new();
    let request = RefreshRequest {
        access_token: access_token.to_string(),
        client_token: client_token.to_string(),
        selected_profile: None,
    };
    let refresh_link = format!("{link}/yggdrasil/authserver/refresh");
    let response = client.post(&refresh_link).json(&request).send().await?;
    if !response.status().is_success() {
        return Err(crate::Error::Other(response.text().await?));
    }
    let result: RefreshResponse = response.json().await?;
    Ok((result.access_token, result.client_token))
}
