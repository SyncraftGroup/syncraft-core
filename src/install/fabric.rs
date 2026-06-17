use crate::{Error, download::{Tempable, meta::{VersionMeta, fabric}}, install::Installer, paths::MinecraftPaths, version::Version};

#[async_trait::async_trait]
pub trait InstallFabricExt {
    async fn install_fabric(mut version: Version, vanilla_version: &str, loader_version: &str, paths: &MinecraftPaths) -> Result<Version, Error> {

        let manifest = fabric::Manifest::load_or(&paths.fabric().manifest_json(), async {
            fabric::fetch_manifest(vanilla_version).await
        }).await?;
        let tag = manifest.get_loader_tag(loader_version)?;
        let meta = VersionMeta::load_or(&paths.fabric().meta_json(), async {
            fabric::fetch_meta(vanilla_version, &tag).await
        }).await?;
        let fabric = Version::parse(meta, &paths).await?;
        version.merge(fabric);

        Installer::download_version(&version, paths).await?;
        Ok(version)
    }
}