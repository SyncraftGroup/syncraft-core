use std::{collections::HashMap, fs::File, path::PathBuf, str::FromStr, time::Duration};

use syncraft_protocol::{Loader, VersionInfo};

use crate::{Error, download::{Tempable, downloader::Downloader, meta::{VersionMeta, vanilla}}, install::{fabric::InstallFabricExt, forge::InstallForgeExt, neoforge::InstallNeoForgeExt}, paths::MinecraftPaths, version::Version};

pub mod fabric;
pub mod forge;
pub mod neoforge;

pub struct Installer { }

impl Installer {
    pub async fn restore_or_install(info: &VersionInfo, paths: &MinecraftPaths) -> Result<Version, Error> {

        paths.create_dirs().await?;

        let manifest = vanilla::Manifest::load_or(&paths.manifest_json(), async {
            vanilla::fetch_manifest().await
        }).await?;
        let tag = manifest.get_version_tag(&info.vanilla_version)?;
        let meta = VersionMeta::load_or(&paths.version_json(), async {
            vanilla::fetch_meta(&tag).await
        }).await?;
        let version = Version::parse(meta, paths).await?;
        match &info.loader {
            Loader::Vanilla => Self::install_vanilla(version, paths).await,
            Loader::Fabric { loader_version } => {
                Self::install_fabric(version, &info.vanilla_version, loader_version, paths).await
            }
            Loader::Forge { loader_version } => {
                Self::install_forge(version, &info.vanilla_version, loader_version, paths).await
            }
            Loader::NeoForge { loader_version } => {
                Self::install_neoforge(version, loader_version, paths).await
            }
        }
    }

    async fn install_vanilla(version: Version, paths: &MinecraftPaths) -> Result<Version, Error> {
        Self::download_version(&version, paths).await?;
        Ok(version)
    }

    async fn download_version(version: &Version, paths: &MinecraftPaths) -> Result<(), Error> {
        let downloader = Downloader::new(
            32,
            Duration::from_secs(30),
            5
        );
        downloader.ensure_all(version.files()).await?;
        for native in version.natives.values() {
            Self::unzip_native(&native.file.path, paths)?;
        }
        Ok(())
    }

    const NATIVE_EXTS: &[&str] = &["dll", "so", "dylib", "jnilib"];
    fn unzip_native(file_path: &PathBuf, paths: &MinecraftPaths) -> Result<(), Error> {
        let f = File::open(file_path)?;
        let mut archieve = zip::ZipArchive::new(f)?;
        for i in 0..archieve.len() {
            let mut f = archieve.by_index(i)?;
            if let Some(path) = f.enclosed_name()
            && let Some(ext) = path.extension().and_then(|ext| ext.to_str())
            && Self::NATIVE_EXTS.contains(&ext)
            && let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                let mut output = File::create(paths.native_path(name))?;
                std::io::copy(&mut f, &mut output)?;
            }
        }
        Ok(())
    }
}

impl InstallFabricExt for Installer {}
impl InstallForgeExt for Installer {}
impl InstallNeoForgeExt for Installer {}

/// A tool for pure-number version ids.
/// Note that you should make sure the compared ids are in a same format.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct VersionNum(Vec<usize>);

impl FromStr for VersionNum {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(Error::Parse("empty version".to_string()));
        }
        let mut parts = Vec::new();
        for part in s.split('.') {
            if part.is_empty() {
                return Err(Error::Parse(format!("invalid version: {s}")));
            }

            let value = part
                .parse::<usize>()
                .map_err(|_| Error::Parse(format!("invalid version component: {part}")))?;

            parts.push(value);
        }
        Ok(Self(parts))
    }
}

impl VersionNum {
    /// Examples:
    /// - "1.20.1" -> 1.20.1
    /// - "1.21-pre1" -> 1.21
    /// - "1.20.5-rc1" -> 1.20.5
    pub fn from_mc_version(s: &str) -> Result<Self, Error> {
        let numeric = s
            .split('-')
            .next()
            .ok_or_else(|| Error::Parse(format!("invalid version: {s}")))?;
        numeric.parse()
    }
}

pub fn expand_args<F: Clone + Fn(&str) -> String>(args: &[String], vars: &HashMap<String, String>, pattern: F) -> Vec<String> {
    args.iter().map(|arg| replace_vars(arg, vars, pattern.clone())).collect()
}

pub fn replace_vars<F: Fn(&str) -> String>(text: &str, vars: &HashMap<String, String>, pattern: F) -> String {
    let mut result = text.to_string();
    for (key, value) in vars {
        let pattern = pattern(key);
        result = result.replace(&pattern, value);
    }
    result
}