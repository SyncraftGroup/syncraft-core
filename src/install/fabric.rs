use crate::{
    Error,
    download::{
        Tempable, UiProgress,
        meta::{VersionMeta, fabric},
    },
    install::Installer,
    paths::MinecraftPaths,
    version::Version,
};

#[async_trait::async_trait]
pub trait InstallFabricExt {
    async fn install_fabric(
        mut version: Version,
        vanilla_version: &str,
        loader_version: &str,
        paths: &MinecraftPaths,
        progress: Option<&UiProgress>,
    ) -> Result<Version, Error> {
        let spinner = progress.map(|p| p.add_spinner("Fetching Fabric manifest..."));
        let manifest = fabric::Manifest::load_or(&paths.fabric().manifest_json(), async {
            fabric::fetch_manifest(vanilla_version).await
        })
        .await?;
        if let Some(s) = spinner {
            s.finish_and_clear();
        }

        let tag = manifest.get_loader_tag(loader_version)?;

        let spinner = progress.map(|p| p.add_spinner("Fetching Fabric metadata..."));
        let meta = VersionMeta::load_or(&paths.fabric().meta_json(), async {
            fabric::fetch_meta(vanilla_version, &tag).await
        })
        .await?;
        if let Some(s) = spinner {
            s.finish_and_clear();
        }

        let fabric = Version::parse(meta, &paths).await?;
        // Fabric profile JSONs declare `inheritsFrom`; reject a mismatch instead
        // of installing a loader built for another Minecraft version.
        fabric.ensure_inherits_from(vanilla_version)?;
        version.merge(fabric);

        Installer::download_version(&version, paths, progress).await?;
        Ok(version)
    }
}
