use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock, RwLock};

use java_detect::JavaInstallation;
use serde::{Deserialize, Serialize};

use crate::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Account {
    Offline {
        username: String,
        uuid: String,
    },
    Syncraft {
        username: String,
        uuid: String,
        email: String,
        client_token: String,
        access_token: String,
    },
}

impl std::fmt::Display for Account {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Offline { username, .. } => write!(f, "{} - Offline", username),
            Self::Syncraft {
                username, email, ..
            } => write!(f, "{} - {}", username, email),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClientConfig {
    #[serde(default)]
    pub syncraft_link: Option<String>,
    #[serde(default)]
    pub java: Option<JavaInstallation>,
    /// Explicit Java executable, overriding auto-detection (GUI setting).
    #[serde(default)]
    pub java_override: Option<PathBuf>,
    /// Manifest content hash of the last applied version.
    #[serde(default)]
    pub version_hash: Option<String>,
    /// Git commit of the last applied version, used to fetch the changelog.
    #[serde(default)]
    pub commit: Option<String>,
    /// Public key of the server's manifest signature (hex), pinned on first use.
    #[serde(default)]
    pub server_key: Option<String>,
    /// Override the parallel download count suggested by the server.
    #[serde(default)]
    pub download_concurrency: Option<usize>,
    #[serde(default)]
    pub account: Option<Account>,
    #[serde(default)]
    pub skin_hash: Option<String>,
    /// Name used when playing without a Syncraft account (desktop app).
    #[serde(default)]
    pub offline_username: Option<String>,
}

static CLIENT_CONFIG: OnceLock<ClientConfigManager> = OnceLock::new();

pub fn client_config() -> &'static ClientConfigManager {
    CLIENT_CONFIG.get().expect("client config not initialized")
}

pub fn init_client_config(path: impl Into<PathBuf>) {
    try_init_client_config(path).expect("failed to load client config");
}

/// Fallible variant of [`init_client_config`].
pub fn try_init_client_config(path: impl Into<PathBuf>) -> Result<(), Error> {
    let manager = ClientConfigManager::load(path)?;
    CLIENT_CONFIG
        .set(manager)
        .map_err(|_| Error::Other("client config already initialized".into()))
}

#[derive(Debug)]
pub struct ClientConfigManager {
    path: PathBuf,
    config: Arc<RwLock<ClientConfig>>,
}

impl ClientConfigManager {
    pub fn load(path: impl Into<PathBuf>) -> Result<Self, Error> {
        let path = path.into();
        let config = if path.exists() {
            serde_json::from_str(&std::fs::read_to_string(&path)?)?
        } else {
            ClientConfig::default()
        };
        Ok(Self {
            path,
            config: Arc::new(RwLock::new(config)),
        })
    }

    pub fn save(&self) -> Result<(), Error> {
        let config = self.get();
        if let Some(parent) = self.path.parent()
            && !parent.exists()
        {
            std::fs::create_dir_all(parent)?;
        }
        let mut f = std::fs::File::create(&self.path)?;
        f.write_all(serde_json::to_string_pretty(&config)?.as_bytes())?;
        Ok(())
    }

    pub fn get(&self) -> ClientConfig {
        self.config.read().unwrap().clone()
    }

    pub fn update<F: FnOnce(&mut ClientConfig)>(&self, f: F) -> Result<(), Error> {
        {
            let mut config = self.config.write().unwrap();
            f(&mut config);
        }
        self.save()?;
        Ok(())
    }
}
