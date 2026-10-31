pub mod downloader;
pub mod meta;
pub mod reporter;

pub use reporter::UiProgress;

use std::{io, path::PathBuf, sync::OnceLock, time::Duration};

use futures::stream::StreamExt;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha1::{Digest, Sha1};
use tokio::{fs, io::AsyncWriteExt};

use crate::{
    Error, Source, config,
    download::meta::{AssetIndexMeta, vanilla},
    install::forge::ForgeInstallKind,
    progress::Progress,
};

const USER_AGENT: &str = "syncraft-core/0.1";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// Metadata requests are small; a generous total timeout still catches stalls.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
/// How often a single URL is retried when a mirror asks us to slow down.
const RATE_LIMIT_RETRIES: u32 = 4;

/// Shared HTTP client so metadata requests reuse connections.
pub(crate) fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .redirect(reqwest::redirect::Policy::limited(10))
            .connect_timeout(CONNECT_TIMEOUT)
            .read_timeout(Duration::from_secs(30))
            .timeout(REQUEST_TIMEOUT)
            .build()
            .expect("failed to build reqwest client")
    })
}

/// Parse a `Retry-After` header (seconds form).
fn retry_after(response: &reqwest::Response) -> Option<Duration> {
    response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok())
        .map(Duration::from_secs)
}

/// Send a request, backing off while a mirror rate-limits us.
///
/// BMCLAPI answers with `429` when more than ~10 requests are in flight in a
/// short window. Treating that as a plain failure pushes every download back to
/// the (slower) origin, so instead we wait and retry the same URL.
pub(crate) async fn send_retrying(
    build: impl Fn() -> reqwest::RequestBuilder,
) -> Result<reqwest::Response, reqwest::Error> {
    let mut backoff = Duration::from_millis(500);
    let mut attempt = 0;
    loop {
        let response = build().send().await?;
        let status = response.status();
        let rate_limited =
            status == StatusCode::TOO_MANY_REQUESTS || status == StatusCode::SERVICE_UNAVAILABLE;

        if rate_limited && attempt < RATE_LIMIT_RETRIES {
            let wait = retry_after(&response).unwrap_or(backoff);
            tokio::time::sleep(wait).await;
            backoff = (backoff * 2).min(Duration::from_secs(8));
            attempt += 1;
            continue;
        }

        return response.error_for_status();
    }
}

pub async fn fetch(urls: &Vec<String>) -> Result<reqwest::Response, Error> {
    let mut errs = Vec::new();
    for url in urls {
        match send_retrying(|| client().get(url)).await {
            Ok(resp) => return Ok(resp),
            Err(err) => errs.push(err),
        }
    }
    if !errs.is_empty() {
        Err(errs.remove(0))?
    } else {
        Err(Error::Other("empty url list".to_string()))
    }
}

/// Fetch the `.sha1` sidecar published next to a Maven artifact.
///
/// Returns `None` when the mirror does not provide one, in which case callers
/// fall back to structural validation (e.g. "is this a readable zip").
pub async fn fetch_sha1(url: &str) -> Option<String> {
    let response = client().get(format!("{url}.sha1")).send().await.ok()?;
    let text = response.error_for_status().ok()?.text().await.ok()?;
    let token = text.split_whitespace().next()?;
    (token.len() == 40 && token.chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| token.to_ascii_lowercase())
}

pub async fn download(
    file: &ResolvedFile,
    progress: Option<&dyn Progress>,
    name: &str,
) -> Result<(), Error> {
    let url = file
        .url
        .as_ref()
        .ok_or_else(|| Error::Other("missing file url".to_string()))?;
    if let Some(parent) = file.path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let response = fetch(url).await?;
    let total_size = response.content_length().unwrap_or(0);
    if let Some(progress) = progress {
        progress.file_total(name, total_size);
    }
    let mut stream = response.bytes_stream();
    let mut dest = tokio::fs::File::create(&file.path).await?;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        dest.write_all(&chunk).await?;
        if let Some(progress) = progress {
            progress.file_inc(name, chunk.len() as u64);
        }
    }
    dest.sync_all().await?;
    Ok(())
}

pub enum ResourceKind {
    VanillaManifest,
    VanillaMeta(vanilla::VersionTag),
    FabricManifest {
        vanilla_version: String,
    },
    FabricMeta {
        vanilla_version: String,
        loader_version: String,
    },
    AssetIndex(AssetIndexMeta),
    Data {
        url: String,
    },
    Library {
        path: String,
        repo_url: Option<String>,
    },
    AssetObject {
        identifier: String,
        sha1: String,
    },
    ForgeInstall {
        vanilla_version: String,
        loader_version: String,
        kind: ForgeInstallKind,
    },
    NeoForgeInstall {
        loader_version: String,
    },
    AuthlibInjectorMeta,
}

pub struct Urls<'a> {
    pub default: &'a str,
    pub bmclapi: &'a str,
}

impl Urls<'_> {
    pub fn build(self, source: &Source) -> Vec<String> {
        let primary = match source {
            Source::Default => self.default.to_string(),
            Source::BMCLAPI => self.bmclapi.to_string(),
        };
        let mut urls = vec![primary];
        // Fall back to both mirrors without duplicates (`Vec::dedup` only
        // removes adjacent duplicates).
        for candidate in [self.default.to_string(), self.bmclapi.to_string()] {
            if !urls.contains(&candidate) {
                urls.push(candidate);
            }
        }
        urls
    }
}

pub struct Sources {}
impl Sources {
    pub fn selected() -> Source {
        config().get().selected_source
    }

    /// How many concurrent requests the selected mirror tolerates.
    ///
    /// The official CDNs handle 32 just fine; BMCLAPI rate-limits to roughly
    /// ten requests per short window and answers `429` beyond that.
    pub fn max_concurrency(source: &Source) -> usize {
        match source {
            Source::Default => 32,
            Source::BMCLAPI => 4,
        }
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
            } else {
                Ok(true)
            }
        } else {
            Ok(false)
        }
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
    where
        Fut: Future<Output = Result<Self, Error>> + Send,
    {
        if let Ok(s) = Self::load(path).await {
            Ok(s)
        } else {
            let s = future.await?;
            s.temp(path).await?;
            Ok(s)
        }
    }
}
