pub mod downloader;
pub mod meta;

use std::path::PathBuf;

use serde::{Serialize, de::DeserializeOwned};
use tokio::{fs, io::AsyncWriteExt};

use crate::{Error, Source, config, download::meta::{AssetIndexMeta, vanilla}};

pub async fn fetch(urls: &Vec<String>) -> Result<reqwest::Response, Error> {
    let mut errs = Vec::new();
    for url in urls {
        match reqwest::get(url).await?.error_for_status() {
            Ok(res) => return Ok(res),
            Err(err) => errs.push(err),
        }
    }
    if !errs.is_empty() {
        Err(errs.remove(0))?
    } else {
        Err(Error::Other("empty url list".to_string()))
    }
}

pub enum ResourceKind {
    VanillaManifest,
    VanillaMeta(vanilla::VersionTag),
    FabricManifest {
        vanilla_version: String
    },
    FabricMeta {
        vanilla_version: String,
        loader_version: String
    },
    AssetIndex(AssetIndexMeta),
    Data {
        url: String
    },
    Library {
        path: String
    },
    AssetObject {
        identifier: String,
        sha1: String
    }
}

pub struct Sources {}
impl Sources {

    pub fn selected() -> Source {
        config().get().selected_source
    }

    pub fn parse_url(url: &str) -> Result<reqwest::Url, Error> {
        url.parse::<reqwest::Url>()
            .map_err(|err| Error::Parse(err.to_string()))
    }

    pub fn urls(kind: ResourceKind, source: &Source) -> Result<Vec<String>, Error> {
        Ok(match kind {
            ResourceKind::VanillaManifest => match source {
                Source::Default => vec![
                    "https://piston-meta.mojang.com/mc/game/version_manifest.json".to_string(),
                    "https://bmclapi2.bangbang93.com/mc/game/version_manifest.json".to_string()
                ],
                Source::BMCLAPI => vec![
                    "https://bmclapi2.bangbang93.com/mc/game/version_manifest.json".to_string(),
                    "https://piston-meta.mojang.com/mc/game/version_manifest.json".to_string(),
                ]
            }
            ResourceKind::VanillaMeta(..) | 
            ResourceKind::AssetIndex(..) |
            ResourceKind::Data { .. } => {
                let url = match &kind {
                    ResourceKind::VanillaMeta(tag) => &tag.url,
                    ResourceKind::AssetIndex(meta) => &meta.url,
                    ResourceKind::Data { url } => url,
                    _ => unreachable!(),
                };
                
                let url = Self::parse_url(url)?;
                match source {
                    Source::Default => vec![
                        format!("https://piston-meta.mojang.com{}", url.path()),
                        format!("https://bmclapi2.bangbang93.com{}", url.path())
                    ],
                    Source::BMCLAPI => vec![
                        format!("https://bmclapi2.bangbang93.com{}", url.path()),
                        format!("https://piston-meta.mojang.com{}", url.path())
                    ]
                }
            }
            ResourceKind::Library { path } => match source {
                Source::Default => vec![
                    format!("https://libraries.minecraft.net/{path}"),
                    format!("https://bmclapi2.bangbang93.com/maven/{path}")
                ],
                Source::BMCLAPI => vec![
                    format!("https://bmclapi2.bangbang93.com/maven/{path}"),
                    format!("https://libraries.minecraft.net/{path}")
                ]
            }
            ResourceKind::AssetObject { identifier, sha1 } => match source {
                Source::Default => vec![
                    format!("https://resources.download.minecraft.net/{identifier}/{sha1}"),
                    format!("https://bmclapi2.bangbang93.com/assets/{identifier}/{sha1}")
                ],
                Source::BMCLAPI => vec![
                    format!("https://bmclapi2.bangbang93.com/assets/{identifier}/{sha1}"),
                    format!("https://resources.download.minecraft.net/{identifier}/{sha1}")
                ]
            }
            ResourceKind::FabricManifest { vanilla_version } => match source {
                Source::Default => vec![
                    format!("https://meta.fabricmc.net/v2/versions/loader/{vanilla_version}"),
                    format!("https://bmclapi2.bangbang93.com/fabric-meta/v2/versions/loader/{vanilla_version}")
                ],
                Source::BMCLAPI => vec![
                    format!("https://bmclapi2.bangbang93.com/fabric-meta/v2/versions/loader/{vanilla_version}"),
                    format!("https://meta.fabricmc.net/v2/versions/loader/{vanilla_version}")
                ]
            }
            ResourceKind::FabricMeta { vanilla_version, loader_version } => match source {
                Source::Default => vec![
                    format!("https://meta.fabricmc.net/v2/versions/loader/{vanilla_version}/{loader_version}/profile/json"),
                    format!("https://bmclapi2.bangbang93.com/fabric-meta/v2/versions/loader/{vanilla_version}/{loader_version}/profile/json")
                ],
                Source::BMCLAPI => vec![
                    format!("https://bmclapi2.bangbang93.com/fabric-meta/v2/versions/loader/{vanilla_version}/{loader_version}/profile/json"),
                    format!("https://meta.fabricmc.net/v2/versions/loader/{vanilla_version}/{loader_version}/profile/json")
                ]
            }
        })
    }
}

#[async_trait::async_trait]
pub trait Tempable: DeserializeOwned + Serialize + Sized + Send + Sync {
    async fn temp(&self, path: &PathBuf) -> Result<(), Error> {
        let data = serde_json::to_string_pretty(&self)?;
        let mut f = fs::File::create(path).await?;
        f.write_all(&data.into_bytes()).await?;
        Ok(())
    }

    async fn load(path: &PathBuf) -> Result<Self, Error> {
        let text = fs::read_to_string(path).await?;
        Ok(serde_json::from_str(&text)?)
    }

    async fn load_or<Fut>(path: &PathBuf, future: Fut) -> Result<Self, Error>
    where Fut: Future<Output = Result<Self, Error>> + Send {
        if let Ok(s) = Self::load(path).await { Ok(s) }
        else {
            let s = future.await?;
            s.temp(path).await?;
            Ok(s)
        }
    }
}