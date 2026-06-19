use syncraft_protocol::ResolvedFile;

use crate::{Error, download::{self, ResourceKind, Sources}, install::forge::installer::ForgeInstaller, paths::MinecraftPaths, version::Version};

pub mod installer;

pub enum NeoForgeInstallKind {
    Old,
    Neo
}

#[async_trait::async_trait]
pub trait InstallNeoForgeExt {
    async fn install_neoforge(version: Version, loader_version: &str, paths: &MinecraftPaths) -> Result<Version, Error> {

        let neoforge_dir = paths.neoforge(loader_version).neoforge_dir();
        let installer_path = paths.neoforge(loader_version).installer_path();

        let installer_file = ResolvedFile {
            url: Some(Sources::urls(ResourceKind::NeoForgeInstall {
                loader_version: loader_version.into(),
            }, &Sources::selected())?),
            path: installer_path.clone(),
            sha1: None, size: None
        };

        // sha1 is None: only to check if exists
        if !installer_file.verified().await? {
            download::download(&installer_file).await?;
        }

        ForgeInstaller::install(version, neoforge_dir, installer_path, paths).await
    }
}