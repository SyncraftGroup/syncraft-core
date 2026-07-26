pub mod downloader;
pub mod meta;
pub mod reporter;

pub use reporter::UiProgress;

use std::{io, path::PathBuf, time::Duration};

use futures::stream::StreamExt;
use indicatif::ProgressBar;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha1::{Digest, Sha1};
use tokio::{fs, io::AsyncWriteExt};

use crate::{Error, Source, config, download::meta::{AssetIndexMeta, vanilla}, install::forge::ForgeInstallKind};

const USER_AGENT: &str = "syncraft-core/0.1";

pub async fn fetch(urls: &Vec<String>) -> Result<reqwest::Response, Error> {
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|e| Error::Other(e.to_string()))?;
    let mut errs = Vec::new();
    for url in urls {
        match client.get(url).timeout(Duration::from_secs(30)).send().await {
            Ok(resp) => match resp.error_for_status() {
                Ok(res) => return Ok(res),
                Err(err) => errs.push(err),
            },
            Err(err) => errs.push(err),
        }
    }
    if !errs.is_empty() {
        Err(errs.remove(0))?
    } else {
        Err(Error::Other("empty url list".to_string()))
    }
}

pub async fn download(file: &ResolvedFile, pb: Option<&ProgressBar>) -> Result<(), Error> {
    let url = file.url.as_ref()
        .ok_or_else(|| Error::Other("missing file url".to_string()))?;
    if let Some(parent) = file.path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let response = fetch(url).await?;
    let total_size = response.content_length().unwrap_or(0);
    if let Some(pb) = pb {
        pb.set_length(total_size);
    }
    let mut stream = response.bytes_stream();
    let mut dest = tokio::fs::File::create(&file.path).await?;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        dest.write_all(&chunk).await?;
        if let Some(pb) = pb {
            pb.inc(chunk.len() as u64);
        }
    }
    dest.sync_all().await?;
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
    ForgeInstall {
        vanilla_version: String,
        loader_version: String,
        kind: ForgeInstallKind
    },
    NeoForgeInstall {
        loader_version: String
    },
    AuthlibInjectorMeta,
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
            
            ResourceKind::ForgeInstall { ref vanilla_version, ref loader_version, ref kind } => {
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

            ResourceKind::NeoForgeInstall { loader_version } => {
                Urls {
                    default: &format!("https://maven.neoforged.net/releases/net/neoforged/neoforge/{loader_version}/neoforge-{loader_version}-installer.jar"),
                    bmclapi: &format!("https://bmclapi2.bangbang93.com/maven/net/neoforged/neoforge/{loader_version}/neoforge-{loader_version}-installer.jar"),
                }.build(source)
            }

            ResourceKind::AuthlibInjectorMeta => {
                Urls {
                    default: "https://authlib-injector.yushi.moe/artifact/latest.json",
                    bmclapi: "https://bmclapi2.bangbang93.com/mirrors/authlib-injector/artifact/latest.json",
                }.build(source)
            }
        })
    }
}


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
    pub async fn verified(&self) -> Result<bool, io::Error> {
        if self.path.exists() {
            if let Some(expected) = &self.sha1 {
                Ok(expected.eq_ignore_ascii_case(&self.sha1().await?))
            } else { Ok(true) }
        } else { Ok(false) }
    }

    pub async fn sha1(&self) -> Result<String, io::Error> {
        let bytes = tokio::fs::read(&self.path).await?;
        Ok(sha1_bytes(&bytes))
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