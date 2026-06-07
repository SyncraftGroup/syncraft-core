use std::{fs::File, path::PathBuf, time::Duration};

use crate::{Error, download::{Tempable, downloader::Downloader, meta::{VersionMeta, fabric, vanilla}}, paths::MinecraftPaths, protocal::{Loader, VersionInfo}, version::Version};

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
        let mut version = Version::parse(meta, paths).await?;
        match &info.loader {
            Loader::Vanilla => (),
            Loader::Fabric { loader_version } => {
                Self::install_fabric(&mut version, &info.vanilla_version, loader_version, paths).await?
            }
        }

        println!("{:?}", version.jvm_args);
        
        let downloader = Downloader::new(
            32,
            Duration::from_secs(10),
            3
        );
        downloader.ensure_all(version.files()).await?;

        for native in version.natives.values() {
            Self::unzip_native(&native.file.path, paths)?;
        }

        Ok(version)
    }

    async fn install_fabric(version: &mut Version, vanilla_version: &str, loader_version: &str, paths: &MinecraftPaths) -> Result<(), Error> {

        let manifest = fabric::Manifest::load_or(&paths.fabric_manifest_json(), async {
            fabric::fetch_manifest(vanilla_version).await
        }).await?;
        let tag = manifest.get_loader_tag(loader_version)?;
        let meta = VersionMeta::load_or(&paths.fabric_meta_json(), async {
            fabric::fetch_meta(vanilla_version, &tag).await
        }).await?;
        let fabric = Version::parse(meta, &paths).await?;
        version.merge(fabric);

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