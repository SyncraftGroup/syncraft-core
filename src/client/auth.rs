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

/// Generate an unsigned UUID (UUID v4 with dashes stripped).
pub fn unsigned_uuid() -> String {
    Uuid::new_v4().to_string().replace('-', "")
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
