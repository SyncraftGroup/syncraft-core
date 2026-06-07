use serde::{Deserialize, Serialize};

use crate::{Error, download::{ResourceKind, Sources, Tempable, fetch, meta::VersionMeta}};

#[derive(Debug, Deserialize, Serialize)]
pub struct Manifest(Vec<Tag>);

impl Tempable for Manifest {}

impl Manifest {
    pub fn get_loader_tag(&self, loader_version: &str) -> Result<LoaderTag, Error> {
        self.0.iter().find(|v| v.loader.version == loader_version).cloned()
        .and_then(|tag| Some(tag.loader))
        .ok_or(Error::Other(format!("loader version {loader_version} not found in manifest")))
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Tag {
    loader: LoaderTag
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LoaderTag {
    version: String,
    stable: bool
}

pub async fn fetch_manifest(vanilla_version: &str) -> Result<Manifest, Error> {
    Ok(fetch(&Sources::urls(
        ResourceKind::FabricManifest { vanilla_version: vanilla_version.to_string() },
        &Sources::selected()
    )?).await?.json().await?)
}

pub async fn fetch_meta(vanilla_version: &str, tag: &LoaderTag) -> Result<VersionMeta, Error> {
    Ok(fetch(&Sources::urls(
        ResourceKind::FabricMeta { vanilla_version: vanilla_version.to_string(), loader_version: tag.version.to_string() },
        &Sources::selected()
    )?).await?.json().await?)
}