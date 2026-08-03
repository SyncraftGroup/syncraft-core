use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock, RwLock};

use java_detect::JavaInstallation;
use serde::{Deserialize, Serialize};

use syncraft_core::Error;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    pub syncraft_link: Option<String>,
    pub java: Option<JavaInstallation>,
    pub version_hash: Option<String>,
    pub account: Option<Account>,
    pub skin_hash: Option<String>,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            syncraft_link: None,
            java: None,
            version_hash: None,
            account: None,
            skin_hash: None,
        }
    }
}

static CLIENT_CONFIG: OnceLock<ClientConfigManager> = OnceLock::new();

pub fn client_config() -> &'static ClientConfigManager {
    CLIENT_CONFIG.get().expect("client config not initialized")
}

pub fn init_client_config(path: impl Into<PathBuf>) {
    let manager = ClientConfigManager::load(path).expect("failed to load client config");
    CLIENT_CONFIG.set(manager).ok();
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
