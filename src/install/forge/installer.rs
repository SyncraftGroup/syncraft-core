use std::{fs, path::PathBuf};

use crate::{Error, download::{UiProgress, ResolvedFile, ResourceKind, Sources, meta::{VersionMeta, forge::InstallProfileMeta}}, install::{Installer, forge::profile::InstallProfile}, package::PackageName, paths::MinecraftPaths, version::Version};

pub struct ForgeInstaller {}

impl ForgeInstaller {

    pub async fn install(mut version: Version, forge_dir: PathBuf, installer_path: PathBuf, paths: &MinecraftPaths, progress: Option<&UiProgress>) -> Result<Version, Error> {

        let (profile_meta, version_meta) = Self::inspect_installer(&forge_dir, &installer_path).await?;

        let install_libs = profile_meta.libraries.clone();
        let forge = Version::parse(version_meta, paths).await?;
        let profile = InstallProfile::parse(profile_meta, &forge_dir, paths)?;
        version.merge(forge);

        let installed_marker = forge_dir.join(".installed");
        if installed_marker.exists() {
            return Ok(version);
        }

        let source = Sources::selected();
        for lib_meta in &install_libs {
            let pkgname = lib_meta.name.parse::<PackageName>()?;
            if !lib_meta.natives.is_empty() {
                continue;
            }
            if let Some(downloads) = &lib_meta.downloads {
                if let Some(artifact) = &downloads.artifact {
                    let path = artifact.path.clone().unwrap_or_else(|| pkgname.to_maven_path());
                    version.files.push(ResolvedFile {
                        path: paths.library_path(&path),
                        url: Some(Sources::urls(ResourceKind::Library {
                            path: path.clone(),
                            repo_url: lib_meta.repo_url(),
                        }, &source)?),
                        sha1: artifact.sha1.clone(),
                        size: artifact.size,
                    });
                }
            }
        }

        Installer::download_version(&version, paths, progress).await?;

        let java = version.get_java_installation()?;
        for processor in profile.processors {
            processor.execute(&java.path)?;
        }

        fs::write(&installed_marker, "")?;

        Ok(version)
    }

    async fn inspect_installer(forge_dir: &PathBuf, installer_path: &PathBuf) -> Result<(InstallProfileMeta, VersionMeta), Error> {

        let install_profile_path = forge_dir.join("install_profile.json");
        let version_meta_path = forge_dir.join("version.json");

        if !install_profile_path.exists() || !version_meta_path.exists() {
            Self::unzip_installer(forge_dir, installer_path)?;
        }

        let install_profile_str = fs::read_to_string(&install_profile_path)?;
        let install_profile: InstallProfileMeta = serde_json::from_str(&install_profile_str)?;

        let version_meta_str = fs::read_to_string(&version_meta_path)?;
        let version_meta: VersionMeta = serde_json::from_str(&version_meta_str)?;

        Ok((install_profile, version_meta))
    }

    fn unzip_installer(forge_dir: &PathBuf, installer_path: &PathBuf) -> Result<(), Error> {
        let installer_file = fs::File::open(installer_path)?;
        let mut archive = zip::ZipArchive::new(installer_file)?;
        archive.extract(forge_dir)?;
        Ok(())
    }
}
