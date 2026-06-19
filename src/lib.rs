use std::{collections::HashMap, io::Write, path::PathBuf, sync::{Arc, OnceLock, RwLock}};

use serde::{Deserialize, Serialize};

pub mod platform;
pub mod package;
pub mod version;
pub mod paths;
pub mod download;
pub mod install;
pub mod launcher;
pub mod protocal;
pub mod java;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("(network) {0}")]
    Network(#[from] reqwest::Error),
    #[error("(io) {0}")]
    IO(#[from] std::io::Error),
    #[error("(parse) {0}")]
    Parse(String),
    #[error("(serialize) {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("(zip) {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("(zip) {0}")]
    Other(String)
}

static CONFIG: OnceLock<ConfigManager> = OnceLock::new();

pub fn config() -> &'static ConfigManager {
    CONFIG.get().expect("config not initialized")
}

pub fn set_config(path: impl Into<PathBuf>) {
    let manager = ConfigManager::load(path)
        .expect("failed to load config");
    CONFIG.set(manager).expect("failed to load config");
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum Source {
    Default,
    BMCLAPI
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub game_dir: String,
    pub java_paths: HashMap<u8, String>,
    pub selected_source: Source
}

impl Default for Config {
    fn default() -> Self {
        Self {
            game_dir: ".minecraft".into(),
            java_paths: HashMap::new(),
            selected_source: Source::Default
        }
    }
}

#[derive(Debug)]
pub struct ConfigManager {
    path: PathBuf,
    config: Arc<RwLock<Config>>
}

impl ConfigManager {
    pub fn load(path: impl Into<PathBuf>) -> Result<Self, Error> {
        let path = path.into();
        let config = if path.exists() {
            serde_json::from_str(&std::fs::read_to_string(&path)?)?
        } else {
            Config::default()
        };
        // skip unnecessary init saving
        Ok(Self {
            path,
            config: Arc::new(RwLock::new(config))
        })
    }

    pub fn save(&self) -> Result<(), Error> {
        let config = self.get();
        if let Some(parent) = self.path.parent()
        && !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
        let mut f = std::fs::File::create(&self.path)?;
        f.write_all(&serde_json::to_string_pretty(&config)?.into_bytes())?;
        Ok(())
    }

    pub fn get(&self) -> Config {
        self.config.read().unwrap().clone()
    }

    pub fn update<F: FnOnce(&mut Config)>(&self, f: F) -> Result<(), Error> {
        {
            let mut config = self.config.write().unwrap();
            f(&mut config);
        }
        self.save()?;
        Ok(())
    }
}

#[tokio::test]
async fn install_forge_test() -> Result<(), Error> {
    use protocal::{VersionInfo, Loader};
    use paths::MinecraftPaths;
    use install::Installer;

    let info = VersionInfo {
        name: "test".into(),
        vanilla_version: "1.20.1".into(),
        loader: Loader::Forge { loader_version: "47.4.0".into() }
    };

    set_config("config.json");
    config().update(|c| c.selected_source = Source::BMCLAPI)?;

    let paths = MinecraftPaths::new(&info.name)?;

    let version = Installer::restore_or_install(&info, &paths).await?;
    println!("{:?}", version);

    Ok(())
}

#[tokio::test]
async fn install_neoforge_test() -> Result<(), Error> {
    use protocal::{VersionInfo, Loader};
    use paths::MinecraftPaths;
    use install::Installer;

    let info = VersionInfo {
        name: "test".into(),
        vanilla_version: "1.21.1".into(),
        loader: Loader::NeoForge { loader_version: "21.1.233".into() }
    };

    set_config("config.json");
    config().update(|c| c.selected_source = Source::BMCLAPI)?;

    let paths = MinecraftPaths::new(&info.name)?;

    let version = Installer::restore_or_install(&info, &paths).await?;
    println!("{:?}", version);

    Ok(())
}

#[tokio::test]
async fn java_detect_test() -> Result<(), Error> {
    let javas = java::detect();
    println!("java count: {}", javas.len());
    println!("{:?}", javas);

    Ok(())
}