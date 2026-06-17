use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::download::{Tempable, meta::LibraryMeta};

#[derive(Debug, Deserialize, Serialize)]
pub struct InstallProfileMeta {
    #[serde(default)]
    pub spec: u32,
    // "forge"
    pub profile: String,
    pub version: String,
    // "1.20.1"
    pub minecraft: String,
    pub path: Option<String>,
    #[serde(rename = "serverJarPath")]
    pub server_jar_path: Option<String>,
    #[serde(default)]
    pub data: HashMap<String, DataEntry>,
    #[serde(default)]
    pub processors: Vec<ProcessorMeta>,
    #[serde(default)]
    pub libraries: Vec<LibraryMeta>,
    pub json: Option<String>,
    pub logo: Option<String>,
    #[serde(rename = "mirrorList")]
    pub mirror_list: Option<String>,
    pub welcome: Option<String>,
    pub icon: Option<String>,
}

impl Tempable for InstallProfileMeta {}

#[derive(Debug, Deserialize, Serialize)]
pub struct DataEntry {
    #[serde(default)]
    pub client: Option<String>,
    #[serde(default)]
    pub server: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ProcessorMeta {
    #[serde(default)]
    pub sides: Vec<String>,
    pub jar: String,
    #[serde(default)]
    pub classpath: Vec<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub outputs: HashMap<String, String>,
}