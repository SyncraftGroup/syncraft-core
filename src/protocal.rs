use std::path::PathBuf;

use sha1::{Digest, Sha1};

use serde::{Deserialize, Serialize};

use crate::Error;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ResolvedFile {
    pub url: Option<Vec<String>>,
    pub path: PathBuf,
    pub sha1: Option<String>,
    pub size: Option<usize>,
}

pub fn sha1_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    hex::encode(digest)
}

impl ResolvedFile {
    pub async fn verified(&self) -> Result<bool, Error> {
        if self.path.exists() {
            if let Some(expected) = &self.sha1 {
                Ok(expected.eq_ignore_ascii_case(&self.sha1().await?))
            } else { Ok(true) }
        } else { Ok(false) }
    }

    pub async fn sha1(&self) -> Result<String, Error> {
        let bytes = tokio::fs::read(&self.path).await?;
        Ok(sha1_bytes(&bytes))
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub enum Loader {
    Vanilla,
    Fabric { loader_version: String },
    Forge { loader_version: String },
    NeoForge { loader_version: String }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct VersionInfo {
    pub name: String,
    pub vanilla_version: String,
    pub loader: Loader
}