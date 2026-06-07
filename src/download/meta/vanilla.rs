use serde::{Deserialize, Serialize};

use crate::{Error, download::{ResourceKind, Sources, Tempable, fetch, meta::VersionMeta}};


#[derive(Debug, Deserialize, Serialize)]
pub struct Manifest {
    pub latest: LatestTag,
    pub versions: Vec<VersionTag>
}

impl Tempable for Manifest {}

impl Manifest {
    pub fn get_version_tag(&self, vanilla_version: &str) -> Result<VersionTag, Error> {
        self.versions.iter().find(|v| v.id == vanilla_version).cloned()
        .ok_or(Error::Other(format!("version {vanilla_version} not found in manifest")))
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct LatestTag {
    pub release: String,
    pub snapshot: String
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct VersionTag {
    pub id: String,
    #[serde(rename = "type")]
    pub version_type: String,
    pub url: String,
    pub time: String,
    #[serde(rename = "releaseTime")]
    pub release_time: String,
}

pub async fn fetch_manifest() -> Result<Manifest, Error> {
    Ok(fetch(&Sources::urls(
        ResourceKind::VanillaManifest,
        &Sources::selected()
    )?).await?.json().await?)
}

pub async fn fetch_meta(tag: &VersionTag) -> Result<VersionMeta, Error> {
    Ok(fetch(&Sources::urls(
        ResourceKind::VanillaMeta(tag.clone()),
        &Sources::selected()
    )?).await?.json().await?)
}