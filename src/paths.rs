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

    pub fn forge(&self, vanilla_version: &str, loader_version: &str) -> ForgePaths<'_> {
        ForgePaths {
            paths: self,
            vanilla_version: vanilla_version.into(),
            loader_version: loader_version.into()
        }
    }

    pub fn neoforge(&self, loader_version: &str) -> NeoForgePaths<'_> {
        NeoForgePaths {
            paths: self,
            loader_version: loader_version.into()
        }
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
    pub paths: &'a MinecraftPaths,
    pub vanilla_version: String,
    pub loader_version: String
}

impl ForgePaths<'_> {
    pub fn forge_dir(&self) -> PathBuf {
        self.paths.libraries_dir()
            .join("net")
            .join("minecraftforge")
            .join("forge")
            .join(format!("{}-{}", self.vanilla_version, self.loader_version))
    }

    pub fn install_file_path(&self, kind: ForgeInstallKind) -> PathBuf {
        let forge_dir = self.forge_dir();
        match kind {
            ForgeInstallKind::ClientZip => forge_dir.join(format!("forge-{}-{}-client.zip", self.vanilla_version, self.loader_version)),
            ForgeInstallKind::UniversalZip => forge_dir.join(format!("forge-{}-{}-universal.zip", self.vanilla_version, self.loader_version)),
            ForgeInstallKind::Installer => forge_dir.join(format!("forge-{}-{}-installer.jar", self.vanilla_version, self.loader_version)),
        }
    }

    pub fn meta_json(&self) -> PathBuf {
        self.paths.root().join(format!("{}_forge.json", self.paths.version_name))
    }
}

pub struct NeoForgePaths<'a> {
    pub paths: &'a MinecraftPaths,
    pub loader_version: String
}

impl NeoForgePaths<'_> {
    pub fn neoforge_dir(&self) -> PathBuf {
        self.paths.libraries_dir()
            .join("net")
            .join("neoforged")
            .join("neoforge")
            .join(&self.loader_version)
    }

    pub fn installer_path(&self) -> PathBuf {
        self.neoforge_dir().join(format!("neoforge-{}-installer.jar", self.loader_version))
    }
}