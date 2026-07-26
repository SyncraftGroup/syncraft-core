use crate::{Error, download::{self, UiProgress, ResolvedFile, ResourceKind, Sources}, install::forge::installer::ForgeInstaller, paths::MinecraftPaths, version::Version};

pub enum NeoForgeInstallKind {
    Old,
    Neo
}

#[async_trait::async_trait]
pub trait InstallNeoForgeExt {
    async fn install_neoforge(
        version: Version,
        loader_version: &str,
        paths: &MinecraftPaths,
        progress: Option<&UiProgress>,
    ) -> Result<Version, Error> {

        let installer_path = paths.neoforge(loader_version).installer_path();

        let installer_file = ResolvedFile {
            url: Some(Sources::urls(ResourceKind::NeoForgeInstall {
                loader_version: loader_version.into(),
            }, &Sources::selected())?),
            path: installer_path.clone(),
            sha1: None, size: None
        };

        let pb = progress.map(|p| {
            p.add_download_bar(
                installer_path.file_name().and_then(|n| n.to_str()).unwrap_or("neoforge_install"),
                0,
            )
        });

        if !installer_file.verified().await? {
            download::download(&installer_file, pb.as_ref()).await?;
        }

        if let Some(pb) = pb {
            pb.finish_and_clear();
        }

        let neoforge_dir = paths.neoforge(loader_version).neoforge_dir();
        ForgeInstaller::install(version, neoforge_dir, installer_path, paths, progress).await
    }
}
