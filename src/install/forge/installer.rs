use std::{fs, path::PathBuf};

use crate::{Error, download::meta::{VersionMeta, forge::InstallProfileMeta}, install::{Installer, forge::profile::InstallProfile}, paths::MinecraftPaths, version::Version};

pub struct ForgeInstaller {}

impl ForgeInstaller {

    pub async fn install(mut version: Version, forge_dir: PathBuf, installer_path: PathBuf, paths: &MinecraftPaths) -> Result<Version, Error> {

        let (profile_meta, mut version_meta) = Self::inspect_installer(&forge_dir, &installer_path).await?;
        
        version_meta.libraries.extend(profile_meta.libraries.clone());
        let forge = Version::parse(version_meta, paths).await?;
        let profile = InstallProfile::parse(profile_meta, &forge_dir, paths)?;
        version.merge(forge);

        Installer::download_version(&version, paths).await?;

        let java = version.get_java_installation()?;
        for processor in profile.processors {
            processor.execute(&java.path)?;
        }

        Ok(version)
    }

    async fn inspect_installer(forge_dir: &PathBuf, installer_path: &PathBuf) -> Result<(InstallProfileMeta, VersionMeta), Error> {

        Self::unzip_installer(forge_dir, installer_path)?;

        let install_profile_path = forge_dir.join("install_profile.json");
        let install_profile_str = fs::read_to_string(&install_profile_path)?;
        let install_profile: InstallProfileMeta = serde_json::from_str(&install_profile_str)?;
        
        let version_meta_path = forge_dir.join("version.json");
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