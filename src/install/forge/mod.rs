use std::{
    fs::{self, File},
    path::PathBuf,
};

use crate::{
    Error,
    download::{self, ResolvedFile, ResourceKind, Sources, UiProgress},
    install::{Installer, VersionNum, forge::installer::ForgeInstaller, verify_installer},
    paths::MinecraftPaths,
    version::Version,
};

pub mod installer;
pub mod profile;

#[derive(Debug, Clone, Copy)]
pub enum ForgeInstallKind {
    ClientZip,
    UniversalZip,
    Installer,
}

#[async_trait::async_trait]
pub trait InstallForgeExt {
    async fn install_forge(
        version: Version,
        vanilla_version: &str,
        loader_version: &str,
        paths: &MinecraftPaths,
        progress: Option<&UiProgress>,
    ) -> Result<Version, Error> {
        let version_num = VersionNum::from_mc_version(vanilla_version)?;
        let install_kind = version_num.forge_install_kind();
        let forge_paths = paths.forge(vanilla_version, loader_version);
        let forge_dir = forge_paths.forge_dir();
        let install_file_path = forge_paths.install_file_path(install_kind);

        let urls = Sources::urls(
            ResourceKind::ForgeInstall {
                vanilla_version: vanilla_version.into(),
                loader_version: loader_version.into(),
                kind: install_kind,
            },
            &Sources::selected(),
        )?;
        let sha1 = download::fetch_sha1(&urls[0]).await;
        let install_file = ResolvedFile {
            url: Some(urls),
            path: install_file_path.clone(),
            sha1,
            size: None,
        };

        let pb = progress.map(|p| {
            p.add_download_bar(
                install_file_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("forge_install"),
                0,
            )
        });

        if !install_file.verified().await? {
            download::download(&install_file, pb.as_ref()).await?;
        }
        verify_installer(&install_file).await?;

        if let Some(pb) = pb {
            pb.finish_and_clear();
        }

        match install_kind {
            ForgeInstallKind::ClientZip | ForgeInstallKind::UniversalZip => {
                Self::install_forge_zip(version, install_file_path, paths, progress).await
            }
            ForgeInstallKind::Installer => {
                ForgeInstaller::install(
                    version,
                    forge_dir,
                    install_file_path,
                    paths,
                    progress,
                    vanilla_version,
                    loader_version,
                )
                .await
            }
        }
    }

    async fn install_forge_zip(
        version: Version,
        meta_path: PathBuf,
        paths: &MinecraftPaths,
        progress: Option<&UiProgress>,
    ) -> Result<Version, Error> {
        Installer::download_version(&version, paths, progress).await?;

        let version_jar = paths.version_jar();
        let temp_jar = paths.root().join("temp_client.jar.tmp");

        let meta_file = fs::File::options().read(true).open(&meta_path)?;
        let mut meta_zip = zip::ZipArchive::new(meta_file)?;

        let version_jar_file = fs::File::options().read(true).open(&version_jar)?;
        let mut version_jar_zip = zip::ZipArchive::new(version_jar_file)?;

        let temp_jar_file = fs::File::options()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&temp_jar)?;
        let mut temp_zip = zip::ZipWriter::new(temp_jar_file);

        merge_client_jar(&mut meta_zip, &mut temp_zip)?;
        merge_client_jar(&mut version_jar_zip, &mut temp_zip)?;

        temp_zip.finish()?.sync_all()?;

        fs::remove_file(&version_jar)?;
        fs::rename(&temp_jar, &version_jar)?;
        fs::remove_file(&temp_jar)?;

        Ok(version)
    }
}

fn merge_client_jar(
    from: &mut zip::ZipArchive<File>,
    to: &mut zip::ZipWriter<File>,
) -> Result<(), Error> {
    for index in 0..from.len() {
        if let Ok(entry) = from.by_index(index) {
            if entry.name().starts_with("META-INF") {
                continue;
            }
            if entry.is_file() {
                to.raw_copy_file(entry)?;
            } else if entry.is_dir() {
                to.add_directory(entry.name(), zip::write::SimpleFileOptions::default())?;
            }
        }
    }
    Ok(())
}

impl VersionNum {
    fn forge_install_kind(&self) -> ForgeInstallKind {
        if *self <= VersionNum(vec![1, 2, 5]) {
            ForgeInstallKind::ClientZip
        } else if *self <= VersionNum(vec![1, 5, 2]) {
            ForgeInstallKind::UniversalZip
        } else {
            ForgeInstallKind::Installer
        }
    }
}
