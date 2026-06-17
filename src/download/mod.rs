pub mod downloader;
pub mod meta;

use std::path::PathBuf;

use serde::{Serialize, de::DeserializeOwned};
use tokio::{fs, io::AsyncWriteExt};

use crate::{Error, Source, config, download::meta::{AssetIndexMeta, vanilla}, install::forge::ForgeInstallKind, protocal::ResolvedFile};

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

pub async fn download(file: &ResolvedFile) -> Result<(), Error> {
    let url = file.url.as_ref()
        .ok_or_else(|| Error::Other("missing file url".to_string()))?;
    if let Some(parent) = file.path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let bytes = fetch(url).await?.bytes().await?;
    tokio::fs::write(&file.path, &bytes).await?;
    Ok(())
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
        path: String,
        repo_url: Option<String>
    },
    AssetObject {
        identifier: String,
        sha1: String
    },
    ForgeMeta {
        vanilla_version: String,
        loader_version: String,
        kind: ForgeInstallKind
    }
}

pub struct Urls<'a> {
    pub default: &'a str,
    pub bmclapi: &'a str
}

impl Urls<'_> {
    pub fn build(self, source: &Source) -> Vec<String> {
        let primary = match source {
            Source::Default => self.default.to_string(),
            Source::BMCLAPI => self.bmclapi.to_string()
        };
        let mut urls = vec![primary];
        urls.extend(vec![
            self.default.to_string(),
            self.bmclapi.to_string()
        ]);
        urls.dedup();
        urls
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
            ResourceKind::VanillaManifest => {
                Urls {
                    default: "https://piston-meta.mojang.com/mc/game/version_manifest.json",
                    bmclapi: "https://bmclapi2.bangbang93.com/mc/game/version_manifest.json"
                }.build(source)
            }
            
            ResourceKind::VanillaMeta(..) | 
            ResourceKind::AssetIndex(..) |
            ResourceKind::Data { .. } => {
                let url_str = match &kind {
                    ResourceKind::VanillaMeta(tag) => &tag.url,
                    ResourceKind::AssetIndex(meta) => &meta.url,
                    ResourceKind::Data { url } => url,
                    _ => unreachable!(),
                };
                
                let parsed_url = Self::parse_url(url_str)?;
                let path = parsed_url.path();
                Urls {
                    default: &format!("https://piston-meta.mojang.com{}", path),
                    bmclapi: &format!("https://bmclapi2.bangbang93.com{}", path)
                }.build(source)
            }
            
            ResourceKind::Library { ref path, repo_url } => {
                let repo_url = repo_url.unwrap_or("https://libraries.minecraft.net".into());
                Urls {
                    default: &format!("{repo_url}/{path}"),
                    bmclapi: &format!("https://bmclapi2.bangbang93.com/maven/{path}")
                }.build(source)
            }
            
            ResourceKind::AssetObject { ref identifier, ref sha1 } => {
                Urls {
                    default: &format!("https://resources.download.minecraft.net/{identifier}/{sha1}"),
                    bmclapi: &format!("https://bmclapi2.bangbang93.com/assets/{identifier}/{sha1}")
                }.build(source)
            }
            
            ResourceKind::FabricManifest { ref vanilla_version } => {
                Urls {
                    default: &format!("https://meta.fabricmc.net/v2/versions/loader/{vanilla_version}"),
                    bmclapi: &format!("https://bmclapi2.bangbang93.com/fabric-meta/v2/versions/loader/{vanilla_version}")
                }.build(source)
            }
            
            ResourceKind::FabricMeta { ref vanilla_version, ref loader_version } => {
                Urls {
                    default: &format!("https://meta.fabricmc.net/v2/versions/loader/{vanilla_version}/{loader_version}/profile/json"),
                    bmclapi: &format!("https://bmclapi2.bangbang93.com/fabric-meta/v2/versions/loader/{vanilla_version}/{loader_version}/profile/json")
                }.build(source)
            }
            
            ResourceKind::ForgeMeta { ref vanilla_version, ref loader_version, ref kind } => {
                let suffix = match kind {
                    ForgeInstallKind::ClientZip => "client.zip",
                    ForgeInstallKind::UniversalZip => "universal.zip",
                    ForgeInstallKind::Installer => "installer.jar",
                };
                let version_part = format!("{vanilla_version}-{loader_version}");
                Urls {
                    default: &format!("https://maven.minecraftforge.net/net/minecraftforge/forge/{version_part}/forge-{version_part}-{suffix}"),
                    bmclapi: &format!("https://bmclapi2.bangbang93.com/maven/net/minecraftforge/forge/{version_part}/forge-{version_part}-{suffix}")
                }.build(source)
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