pub mod vanilla;
pub mod fabric;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{Error, download::{ResourceKind, Sources, Tempable, fetch}};


pub struct RuleContext {
    pub os_name: String,
    pub os_arch: String,
    pub features: HashMap<String, bool>,
}

// for rules
pub trait RuleAllowed {
    fn allowed(&self, ctx: &RuleContext) -> bool;
}

#[derive(Debug, Deserialize, Serialize)]
pub struct OsRule {
    pub name: Option<String>,
    pub arch: Option<String>,
}

impl RuleAllowed for OsRule {
    fn allowed(&self, ctx: &RuleContext) -> bool {
        if let Some(name) = &self.name
        && name != &ctx.os_name {
            false
        } else if let Some(arch) = &self.arch
        && arch != &ctx.os_arch {
            false
        } else {
            true
        }
    }
}

impl RuleAllowed for HashMap<String, bool> {
    fn allowed(&self, ctx: &RuleContext) -> bool {
        !self.iter().any(|(key, expected)| {
            ctx.features.get(key).unwrap_or(&false) != expected
        })
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Rule {
    pub action: String,
    pub os: Option<OsRule>,
    #[serde(default)]
    pub features: HashMap<String, bool>
}

impl RuleAllowed for Rule {
    fn allowed(&self, ctx: &RuleContext) -> bool {
        if let Some(os_rule) = &self.os
        && !os_rule.allowed(ctx) {
            false
        } else if !self.features.allowed(ctx) {
            false
        } else {
            true
        }
    }
}

impl RuleAllowed for [Rule] {
    fn allowed(&self, ctx: &RuleContext) -> bool {
        if self.is_empty() {
            true
        } else {
            let mut allowed = false;
            for rule in self {
                if rule.allowed(ctx) {
                    allowed = rule.action == "allow";
                }
            }
            allowed
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ArgumentValueMeta {
    Multiple(Vec<String>),
    Single(String)
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ArgumentMeta {
    Complex {
        rules: Vec<Rule>,
        value: ArgumentValueMeta
    },
    Simple(String)
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ArgumentsMeta {
    pub game: Vec<ArgumentMeta>,
    pub jvm: Vec<ArgumentMeta>
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AssetIndexMeta {
    pub id: Option<String>,
    pub sha1: Option<String>,
    pub size: Option<usize>,
    #[serde(rename = "totalSize")]
    pub total_size: Option<usize>,
    pub url: String
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ArtifactMeta {
    pub path: Option<String>,
    pub sha1: Option<String>,
    pub size: Option<usize>,
    pub url: Option<String>
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct LibraryDownloadsMeta {
    #[serde(default)]
    pub artifact: Option<ArtifactMeta>,
    #[serde(default)]
    pub classifiers: HashMap<String, ArtifactMeta>
}

#[derive(Debug, Deserialize, Serialize)]
pub struct LibraryMeta {
    #[serde(default)]
    pub downloads: Option<LibraryDownloadsMeta>,
    pub name: String,
    #[serde(default)]
    pub rules: Vec<Rule>,
    // 仅旧版本；新版本已整合至 [`downloads`] 中，作为 Class Path 的一部分导入。
    #[serde(default)]
    pub natives: HashMap<String, String>,
    // 旧版本 / Modloader
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct JavaVersionMeta {
    // 该版本在官方启动器中的代号
    pub component: String,
    #[serde(rename = "majorVersion")]
    pub major_version: u8
}

#[derive(Debug, Deserialize, Serialize)]
pub struct VersionMeta {
    // >= 1.12
    #[serde(default)]
    #[serde(rename = "inheritsFrom")]
    pub inherits_from: Option<String>,
    // < 1.12
    #[serde(default)]
    #[serde(rename = "clientVersion")]
    pub client_version: Option<String>,
    #[serde(default)]
    #[serde(rename = "javaVersion")]
    pub java_version: Option<JavaVersionMeta>,
    // 新版本使用
    #[serde(default)]
    pub arguments: Option<ArgumentsMeta>,
    // 旧版本使用
    #[serde(default)]
    #[serde(rename = "minecraftArguments")]
    pub minecraft_arguments: String,
    #[serde(default)]
    #[serde(rename = "assetIndex")]
    pub asset_index: Option<AssetIndexMeta>,
    #[serde(default)]
    pub downloads: Option<HashMap<String, ArtifactMeta>>,
    #[serde(default)]
    pub libraries: Vec<LibraryMeta>,
    #[serde(rename = "mainClass")]
    pub main_class: Option<String>
}

impl Tempable for VersionMeta {}

#[derive(Debug, Deserialize, Serialize)]
pub struct AssetIndex {
    /// in pre-1.6
    #[serde(default)]
    pub map_to_resource: bool,
    /// key: file path, e.g. "minecraft/lang/ar_sa.json", only used in older versions
    pub objects: HashMap<String, AssetObject>
}

impl Tempable for AssetIndex {}

#[derive(Debug, Deserialize, Serialize)]
pub struct  AssetObject {
    pub hash: String,
    pub size: usize
}

pub async fn fetch_asset_indexes(asset_index: &AssetIndexMeta) -> Result<AssetIndex, Error> {
    Ok(fetch(&Sources::urls(
        ResourceKind::AssetIndex(asset_index.clone()),
        &Sources::selected()
    )?).await?.json().await?)
}