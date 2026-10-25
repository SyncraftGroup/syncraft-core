use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    Error,
    download::{
        ResolvedFile, ResourceKind, Sources, UiProgress,
        meta::{VersionMeta, forge::InstallProfileMeta},
    },
    install::{Installer, forge::profile::InstallProfile},
    package::PackageName,
    paths::MinecraftPaths,
    version::Version,
};

/// Records which processor outputs were produced, so a half-finished install
/// (or files deleted afterwards) is detected and repaired instead of skipped.
const MARKER: &str = "syncraft-install.json";

#[derive(Debug, Serialize, Deserialize)]
struct InstallMarker {
    loader_version: String,
    outputs: BTreeMap<PathBuf, String>,
}

pub struct ForgeInstaller {}

impl ForgeInstaller {
    #[allow(clippy::too_many_arguments)]
    pub async fn install(
        mut version: Version,
        forge_dir: PathBuf,
        installer_path: PathBuf,
        paths: &MinecraftPaths,
        progress: Option<&UiProgress>,
        minecraft_version: &str,
        loader_version: &str,
    ) -> Result<Version, Error> {
        let (profile_meta, version_meta) =
            Self::inspect_installer(&forge_dir, &installer_path).await?;

        let install_libs = profile_meta.libraries.clone();
        let forge = Version::parse(version_meta, paths).await?;
        // Catch a config/loader mismatch (e.g. Forge for 1.20.1 on a 1.20.2
        // server) before downloading anything.
        forge.ensure_inherits_from(minecraft_version)?;

        let profile = InstallProfile::parse(profile_meta, &forge_dir, paths)?;
        version.merge(forge);

        let expected_outputs = Self::expected_outputs(&profile);
        let marker_path = forge_dir.join(MARKER);
        // The marker only proves the processors ran; also require the vanilla
        // jar to still exist so a deleted client jar triggers a repair.
        if paths.version_jar().exists()
            && Self::marker_valid(&marker_path, loader_version, &expected_outputs)
        {
            return Ok(version);
        }

        let source = Sources::selected();
        for lib_meta in &install_libs {
            let pkgname = lib_meta.name.parse::<PackageName>()?;
            if !lib_meta.natives.is_empty() {
                continue;
            }
            if let Some(downloads) = &lib_meta.downloads
                && let Some(artifact) = &downloads.artifact
            {
                let path = artifact
                    .path
                    .clone()
                    .unwrap_or_else(|| pkgname.to_maven_path());
                version.files.push(ResolvedFile {
                    path: paths.library_path(&path),
                    url: Some(Sources::urls(
                        ResourceKind::Library {
                            path: path.clone(),
                            repo_url: lib_meta.repo_url(),
                        },
                        &source,
                    )?),
                    sha1: artifact.sha1.clone(),
                    size: artifact.size,
                });
            }
        }

        Installer::download_version(&version, paths, progress).await?;

        let java = version.get_java_installation()?;
        for processor in &profile.processors {
            processor.execute(&java.path)?;
        }

        // Processors can fail silently (exit code 0 but no output); verify the
        // declared outputs before caching the install as complete.
        Self::verify_outputs(&expected_outputs)?;
        Self::write_marker(&marker_path, loader_version, &expected_outputs)?;

        Ok(version)
    }

    fn expected_outputs(profile: &InstallProfile) -> BTreeMap<PathBuf, String> {
        let mut outputs = BTreeMap::new();
        for processor in &profile.processors {
            for (path, hash) in &processor.outputs {
                outputs.insert(PathBuf::from(path), hash.clone());
            }
        }
        outputs
    }

    fn marker_valid(
        marker_path: &Path,
        loader_version: &str,
        expected: &BTreeMap<PathBuf, String>,
    ) -> bool {
        let Ok(content) = fs::read_to_string(marker_path) else {
            return false;
        };
        let Ok(marker) = serde_json::from_str::<InstallMarker>(&content) else {
            return false;
        };
        if marker.loader_version != loader_version || marker.outputs != *expected {
            return false;
        }
        expected.iter().all(|(path, hash)| {
            crate::install::forge::profile::verify_file_hash(path, hash).unwrap_or(false)
        })
    }

    fn verify_outputs(expected: &BTreeMap<PathBuf, String>) -> Result<(), Error> {
        for (path, hash) in expected {
            if !crate::install::forge::profile::verify_file_hash(path, hash)? {
                return Err(Error::Other(format!(
                    "loader installer did not produce {}",
                    path.display()
                )));
            }
        }
        Ok(())
    }

    fn write_marker(
        marker_path: &Path,
        loader_version: &str,
        outputs: &BTreeMap<PathBuf, String>,
    ) -> Result<(), Error> {
        let marker = InstallMarker {
            loader_version: loader_version.to_string(),
            outputs: outputs.clone(),
        };
        fs::write(marker_path, serde_json::to_string_pretty(&marker)?)?;
        Ok(())
    }

    async fn inspect_installer(
        forge_dir: &PathBuf,
        installer_path: &PathBuf,
    ) -> Result<(InstallProfileMeta, VersionMeta), Error> {
        let install_profile_path = forge_dir.join("install_profile.json");
        let version_meta_path = forge_dir.join("version.json");

        if !install_profile_path.exists() || !version_meta_path.exists() {
            Self::unzip_installer(forge_dir, installer_path)?;
        }

        let install_profile_str = fs::read_to_string(&install_profile_path)?;
        let install_profile: InstallProfileMeta = serde_json::from_str(&install_profile_str)?;
        // Modern installers ship the version metadata as a sibling file.
        if !version_meta_path.exists() {
            return Err(Error::Other(format!(
                "installer {} is missing version.json",
                installer_path.display()
            )));
        }
        let version_meta: VersionMeta =
            serde_json::from_str(&fs::read_to_string(&version_meta_path)?)?;

        Ok((install_profile, version_meta))
    }

    fn unzip_installer(forge_dir: &PathBuf, installer_path: &PathBuf) -> Result<(), Error> {
        let installer_file = fs::File::open(installer_path)?;
        let mut archive = zip::ZipArchive::new(installer_file)?;
        archive.extract(forge_dir)?;
        Ok(())
    }
}
