use crate::{
    Error,
    download::{self, ResolvedFile, ResourceKind, Sources},
    install::{forge::installer::ForgeInstaller, verify_installer},
    paths::MinecraftPaths,
    progress::Progress,
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
        progress: Option<&dyn Progress>,
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

        let installer_name = installer_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("neoforge_install");
        if let Some(p) = progress {
            p.file_start(installer_name, 0);
        }

        if !installer_file.verified().await? {
            download::download(&installer_file, progress, installer_name).await?;
        }
        verify_installer(&installer_file).await?;

        if let Some(p) = progress {
            p.file_done(installer_name);
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
