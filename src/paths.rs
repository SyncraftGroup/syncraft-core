use std::path::{Path, PathBuf};

use crate::{Error, config, install::forge::ForgeInstallKind};

pub struct MinecraftPaths {
    pub root: PathBuf,
    pub version_name: String
}

impl MinecraftPaths {
    pub fn new(
        version_name: impl Into<String>
    ) -> Result<Self, Error> {
        let root = PathBuf::from(config().get().game_dir);
        let abs_root = std::env::current_dir()?.join(root);
        Ok(Self {
            root: abs_root,
            version_name: version_name.into()
        })
    }

    pub async fn create_dirs(&self) -> Result<(), Error> {
        tokio::try_join!(
            tokio::fs::create_dir_all(self.root()),
            tokio::fs::create_dir_all(self.libraries_dir()),
            tokio::fs::create_dir_all(self.natives_dir()),
            tokio::fs::create_dir_all(self.asset_indexes_dir()),
            tokio::fs::create_dir_all(self.assets_dir()),
        )?;
        Ok(())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn manifest_json(&self) -> PathBuf {
        self.root().join("manifest.json")
    }

    pub fn libraries_dir(&self) -> PathBuf {
        self.root().join("libraries")
    }

    pub fn natives_dir(&self) -> PathBuf {
        self.root().join("natives")
    }

    pub fn assets_dir(&self) -> PathBuf {
        self.root().join("assets")
    }

    pub fn asset_object(&self, identifier: &str, sha1: &str) -> PathBuf {
        self.assets_dir().join("objects").join(identifier).join(sha1)
    }

    pub fn version_json(&self) -> PathBuf {
        self.root().join(format!("{}.json", self.version_name))
    }

    pub fn assets_json(&self) -> PathBuf {
        self.assets_dir().join("assets.json")
    }

    pub fn asset_indexes_dir(&self) -> PathBuf {
        self.assets_dir().join("indexes")
    }

    pub fn asset_indexes_json(&self, id: &str) -> PathBuf {
        self.assets_dir().join("indexes").join(format!("{id}.json"))
    }

    pub fn version_jar(&self) -> PathBuf {
        self.root().join(format!("{}.jar", self.version_name))
    }

    pub fn library_path(&self, path: &str) -> PathBuf {
        self.libraries_dir().join(path)
    }

    pub fn native_path(&self, path: &str) -> PathBuf {
        self.natives_dir().join(path)
    }

    pub fn fabric(&self) -> FabricPaths<'_> {
        FabricPaths { paths: self }
    }

    pub fn forge(&self) -> ForgePaths<'_> {
        ForgePaths { paths: self }
    }
}

pub struct FabricPaths<'a> {
    pub paths: &'a MinecraftPaths
}

impl FabricPaths<'_> {
    pub fn manifest_json(&self) -> PathBuf {
        self.paths.root().join("fabric_manifest.json")
    }

    pub fn meta_json(&self) -> PathBuf {
        self.paths.root().join(format!("{}_fabric.json", self.paths.version_name))
    }
}

pub struct ForgePaths<'a> {
    pub paths: &'a MinecraftPaths
}

impl ForgePaths<'_> {
    pub fn meta_path(&self, vanilla_version: &str, loader_version: &str, kind: ForgeInstallKind) -> PathBuf {
        let forge_dir = self.paths.libraries_dir().join("net").join("minecraftforge").join("forge").join(format!("{vanilla_version}-{loader_version}"));
        match kind {
            ForgeInstallKind::ClientZip => forge_dir.join(format!("forge-{vanilla_version}-{loader_version}-client.zip")),
            ForgeInstallKind::UniversalZip => forge_dir.join(format!("forge-{vanilla_version}-{loader_version}-universal.zip")),
            ForgeInstallKind::Installer => forge_dir.join(format!("forge-{vanilla_version}-{loader_version}-installer.jar")),
        }
    }

    pub fn meta_json(&self) -> PathBuf {
        self.paths.root().join(format!("{}_forge.json", self.paths.version_name))
    }
}