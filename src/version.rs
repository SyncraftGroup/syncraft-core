use std::{collections::HashMap};

use crate::{Error, download::{ResolvedFile, ResourceKind, Sources, Tempable, meta::{ArgumentMeta, ArgumentValueMeta, AssetIndex, LibraryMeta, RuleAllowed, RuleContext, VersionMeta, fetch_asset_indexes}}, java::{self, JavaInstallation}, package::PackageName, paths::MinecraftPaths, platform::{ARCH_NAME, OS_NAME, current_native_classifier}};

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct LibKey {
    pub group: String,
    pub artifact: String,
    pub classifier: Option<String>,
    pub version: String
}

#[derive(Debug)]
pub struct Library {
    pub name: PackageName,
    pub file: ResolvedFile,
}

#[derive(Debug)]
pub struct Native {
    pub name: PackageName,
    pub file: ResolvedFile,
}

pub enum TypedLibrary {
    NativeJar(Native),
    Normal(Library)
}

impl TypedLibrary {
    /// recognise a lib's type
    pub fn resolve(
        lib: LibraryMeta,
        ctx: &RuleContext,
        paths: &MinecraftPaths
    ) -> Result<Self, Error> {

        let source = Sources::selected();

        // old-style natives
        if !lib.natives.is_empty() {
            let native_key = lib.natives.get(&ctx.os_name)
                .ok_or_else(|| {
                    Error::Parse(format!(
                        "missing native lib '{}' for platform '{}'",
                        lib.name, ctx.os_name
                    ))
                })?;
            let downloads = lib.downloads.as_ref()
                .ok_or_else(|| {
                    Error::Parse(format!(
                        "missing downloads for native lib '{}'",
                        lib.name
                    ))
                })?;
            let artifact = downloads.classifiers.get(native_key)
                .ok_or_else(|| {
                    Error::Parse(format!(
                        "missing classifier '{}' for native lib '{}'",
                        native_key, lib.name
                    ))
                })?;
            let pkgname = lib.name.parse::<PackageName>()?;
            let path = artifact.path.clone()
                .unwrap_or(pkgname.to_maven_path());
            Ok(Self::NativeJar(Native {
                name: pkgname,
                file: ResolvedFile {
                    path: paths.native_path(&path),
                    url: Some(Sources::urls(ResourceKind::Library { path, repo_url: lib.repo_url() }, &source)?),
                    sha1: artifact.sha1.clone(),
                    size: artifact.size
                }
            }))
        } else {
            // todo: handle mapping
            let pkgname = lib.name.parse::<PackageName>()?;
            if let Some(classifier) = &pkgname.classifier
            && classifier == current_native_classifier() {
                let downloads = lib.downloads.as_ref();
                let artifact = downloads.and_then(|d| d.artifact.as_ref());
                let path = artifact.and_then(|a| a.path.clone())
                    .unwrap_or(pkgname.to_maven_path());
                Ok(Self::NativeJar(Native {
                    name: pkgname,
                    file: ResolvedFile {
                        path: paths.native_path(&path),
                        url: Some(Sources::urls(ResourceKind::Library { path, repo_url: lib.repo_url() }, &source)?),
                        sha1: artifact.and_then(|a| a.sha1.clone()),
                        size: artifact.and_then(|a| a.size)
                    }
                }))
            } else {
                let downloads = lib.downloads.as_ref();
                let artifact = downloads.and_then(|d| d.artifact.as_ref());
                let path = artifact.and_then(|a| a.path.clone())
                    .unwrap_or(pkgname.to_maven_path());
                Ok(Self::Normal(Library {
                    name: pkgname,
                    file: ResolvedFile {
                        path: paths.library_path(&path),
                        url: Some(Sources::urls(ResourceKind::Library { path, repo_url: lib.repo_url() }, &source)?),
                        sha1: artifact.and_then(|a| a.sha1.clone()),
                        size: artifact.and_then(|a| a.size)
                    }
                }))
            }
        }
    }
}

#[derive(Debug)]
pub struct Assets {
    pub id: String,
    pub files: Vec<ResolvedFile>
}

#[derive(Debug)]
pub struct Version {
    pub main_class: Option<String>,
    pub java_version: Option<u8>,
    pub assets : Assets,
    pub version_jar: Option<ResolvedFile>,
    pub libraries: HashMap<LibKey, Library>,
    pub natives: HashMap<LibKey, Native>,
    pub game_args: Vec<String>,
    pub jvm_args: Vec<String>,
    pub files: Vec<ResolvedFile>
}

impl Version {
    pub async fn parse(meta: VersionMeta, paths: &MinecraftPaths) -> Result<Self, Error> {
        
        let source = Sources::selected();
        
        paths.create_dirs().await?;

        let main_class = meta.main_class;

        let java_version = meta.java_version.map(|v| v.major_version);
        
        let version_jar = meta
            .downloads.as_ref()
            .and_then(|d| d.get("client"))
            .and_then(|a| {
                let url = a.url.clone().and_then(|url| {
                    Sources::urls(ResourceKind::Data { url }, &source).ok()
                });
                Some(ResolvedFile {
                    url: url,
                    path: paths.version_jar(),
                    sha1: a.sha1.clone(),
                    size: a.size
                })
            });
        
        let mut assets = Assets { id: "".into(), files: Vec::new() };
        
        if let Some(asset_index_meta) = &meta.asset_index {
            let asset_index = AssetIndex::load_or(&paths.assets_json(), async move {
                fetch_asset_indexes(asset_index_meta).await
            }).await?;
            // todo: for older versions 'path' is needed
            // todo: pre-1.6?
            let mut assets_files = Vec::new();
            for (_path, object) in &asset_index.objects {
                let sha1 = &object.hash;
                let identifier = &sha1[..2];
                assets_files.push(ResolvedFile {
                    url: Some(Sources::urls(ResourceKind::AssetObject {
                        identifier: identifier.to_string(),
                        sha1: sha1.to_string()
                    }, &source)?),
                    path: paths.asset_object(identifier, sha1),
                    sha1: Some(sha1.to_string()),
                    size: Some(object.size)
                });
            }
            assets.files = assets_files;
            assets.id = asset_index_meta.id.clone().unwrap_or("assets".into());
            asset_index.temp(&&paths.asset_indexes_json(&assets.id)).await?;
        }
        

        let mut native_jars = HashMap::<LibKey, Native>::new();
        let mut libraries = HashMap::<LibKey, Library>::new();

        let ctx = RuleContext {
            os_name: OS_NAME.to_string(),
            os_arch: ARCH_NAME[0].to_string(),
            features: HashMap::new() // todo
        };

        

        for lib in meta.libraries {
            if !lib.rules.allowed(&ctx) {
                continue;
            }
            match TypedLibrary::resolve(lib, &ctx, &paths)? {
                TypedLibrary::NativeJar(native_jar) => {
                    native_jars.insert(native_jar.name.key(), native_jar);
                }
                TypedLibrary::Normal(library) => {
                    libraries.insert(library.name.key(), library);
                }
            }
        }

        let mut game_args = Vec::<String>::new();
        let mut jvm_args = Vec::<String>::new();

        if let Some(args) = meta.arguments {
            for arg in args.game {
                extend_argument(arg, &ctx, &mut game_args);
            }
            for arg in args.jvm {
                extend_argument(arg, &ctx, &mut jvm_args);
            }
        } else if !meta.minecraft_arguments.is_empty() {
            game_args = meta
                .minecraft_arguments
                .split_whitespace()
                .map(|s| s.to_string())
                .collect();
        }

        Ok(Self {
            main_class,
            java_version,
            assets,
            version_jar,
            libraries,
            natives: native_jars,
            game_args,
            jvm_args,
            files: Vec::new()
        })
    }

    pub fn merge(&mut self, child: Version) {
        if child.main_class.is_some() {
            self.main_class = child.main_class;
        }
        if child.java_version.is_some() {
            self.java_version = child.java_version;
        }
        if !child.assets.files.is_empty() {
            self.assets = child.assets;
        }
        if child.version_jar.is_some() {
            self.version_jar = child.version_jar;
        }

        // dedup by hash key
        self.libraries.extend(child.libraries);
        self.natives.extend(child.natives);
        
        self.game_args.extend(child.game_args);
        self.jvm_args.extend(child.jvm_args);
    }

    pub fn files(&self) -> impl Iterator<Item = &ResolvedFile> {
        self.version_jar.as_ref().into_iter()
        .chain(self.libraries.values().map(|l| &l.file))
        .chain(self.natives.values().map(|n| &n.file))
        .chain(self.assets.files.iter())
        .chain(self.files.iter())
    }

    pub fn get_java_installation(&self) -> Result<JavaInstallation, Error> {
        java::get_java(self.java_version.unwrap_or(8))
            .ok_or(Error::Other("cannot find java".to_string()))
    }
}

fn extend_argument(
    arg: ArgumentMeta,
    ctx: &RuleContext,
    out: &mut Vec<String>
) {
    match arg {
        ArgumentMeta::Simple(value) => {
            out.push(value);
        }
        ArgumentMeta::Complex { rules, value } => {
            if !rules.allowed(ctx) {
               return;
            }

            match value {
                ArgumentValueMeta::Single(value) => {
                    out.push(value);
                }
                ArgumentValueMeta::Multiple(values) => {
                    out.extend(values);
                }
            }
        }
    }
}