use crate::{
    Error,
    download::{self, ResolvedFile, ResourceKind, Sources, UiProgress},
    install::{forge::installer::ForgeInstaller, verify_installer},
    paths::MinecraftPaths,
    version::Version,
};

#[derive(Debug, Clone, Copy)]
pub enum NeoForgeInstallKind {
    Old,
    Neo,
}

#[async_trait::async_trait]
pub trait InstallNeoForgeExt {
    async fn install_neoforge(
        version: Version,
        vanilla_version: &str,
        loader_version: &str,
        paths: &MinecraftPaths,
        progress: Option<&UiProgress>,
    ) -> Result<Version, Error> {
        let installer_path = paths.neoforge(loader_version).installer_path();

        let urls = Sources::urls(
            ResourceKind::NeoForgeInstall {
                loader_version: loader_version.into(),
            },
            &Sources::selected(),
        )?;
        let sha1 = download::fetch_sha1(&urls[0]).await;
        let installer_file = ResolvedFile {
            url: Some(urls),
            path: installer_path.clone(),
            sha1,
            size: None,
        };

        let pb = progress.map(|p| {
            p.add_download_bar(
                installer_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("neoforge_install"),
                0,
            )
        });

        if !installer_file.verified().await? {
            download::download(&installer_file, pb.as_ref()).await?;
        }
        verify_installer(&installer_file).await?;

        if let Some(pb) = pb {
            pb.finish_and_clear();
        }

        let neoforge_dir = paths.neoforge(loader_version).neoforge_dir();
        ForgeInstaller::install(
            version,
            neoforge_dir,
            installer_path,
            paths,
            progress,
            vanilla_version,
            loader_version,
        )
        .await
    }
}
